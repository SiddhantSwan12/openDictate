//! Microphone and system-audio capture through WASAPI (cpal). Each stream lives on its own thread,
//! and audio callbacks only append samples, never touch UI state.

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use opendictate_core::model::AudioInputDevice;
use opendictate_speech::audio::{has_speech, rms, to_speech_format, VoiceLevelMeter};
use parking_lot::Mutex;
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

pub enum Source {
    Microphone(Option<AudioInputDevice>),
    /// Everything other apps play, captured as WASAPI loopback of the default output.
    SystemOutput,
}

fn device_id(device: &cpal::Device) -> Option<String> {
    device.name().ok()
}

/// Connected microphones, keyed by their Windows name.
pub fn microphones() -> Vec<AudioInputDevice> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|devices| devices.filter_map(|d| device_id(&d)).map(|name| AudioInputDevice { id: name.clone(), name }).collect())
        .unwrap_or_default()
}

pub fn default_microphone() -> Option<AudioInputDevice> {
    cpal::default_host().default_input_device().and_then(|d| device_id(&d)).map(|name| AudioInputDevice { id: name.clone(), name })
}

/// A chosen microphone is used only while it is connected; otherwise the Windows default.
pub fn resolve(chosen: Option<&AudioInputDevice>, available: &[AudioInputDevice], system_default: Option<AudioInputDevice>) -> Option<AudioInputDevice> {
    chosen.and_then(|c| available.iter().find(|a| a.id == c.id).cloned()).or(system_default)
}

fn find_device(source: &Source) -> Result<cpal::Device> {
    let host = cpal::default_host();
    match source {
        Source::SystemOutput => host.default_output_device().ok_or_else(|| anyhow!("No speakers or headphones are available to capture.")),
        Source::Microphone(choice) => {
            let resolved = resolve(choice.as_ref(), &microphones(), default_microphone());
            let device = resolved.and_then(|r| {
                host.input_devices().ok()?.find(|d| device_id(d).as_deref() == Some(r.id.as_str()))
            });
            device
                .or_else(|| host.default_input_device())
                .ok_or_else(|| anyhow!("No microphone found. Connect one, or allow microphone access in Windows Settings → Privacy & security → Microphone."))
        }
    }
}

/// A running capture. Dropping it stops the stream.
pub struct Capture {
    stop: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
    pub rate: u32,
    pub device_name: String,
}

impl Capture {
    /// Delivers mono samples at `rate` to `on_audio` from the audio thread.
    pub fn open(source: Source, mut on_audio: impl FnMut(&[f32]) + Send + 'static) -> Result<Self> {
        let (ready_tx, ready_rx) = channel::<Result<(u32, String)>>();
        let (stop_tx, stop_rx) = channel::<()>();
        let thread = std::thread::Builder::new().name("audio-capture".into()).spawn(move || {
            let build = || -> Result<(cpal::Stream, u32, String)> {
                let loopback = matches!(source, Source::SystemOutput);
                let device = find_device(&source)?;
                let name = device_id(&device).unwrap_or_default();
                let config = if loopback { device.default_output_config() } else { device.default_input_config() }
                    .context("Couldn't read the audio device format")?;
                let channels = config.channels() as usize;
                let rate = config.sample_rate().0;
                let format = config.sample_format();
                let stream_config: cpal::StreamConfig = config.into();
                let mut mono = Vec::new();
                let mut push = move |data: &[f32]| {
                    mono.clear();
                    if channels == 1 {
                        mono.extend_from_slice(data);
                    } else {
                        mono.extend(data.chunks_exact(channels).map(|f| f.iter().sum::<f32>() / channels as f32));
                    }
                    on_audio(&mono);
                };
                let error = |e| log::warn!("audio stream error: {e}");
                let stream = match format {
                    cpal::SampleFormat::F32 => device.build_input_stream(&stream_config, move |d: &[f32], _| push(d), error, None),
                    cpal::SampleFormat::I16 => {
                        let mut buf = Vec::new();
                        device.build_input_stream(
                            &stream_config,
                            move |d: &[i16], _| {
                                buf.clear();
                                buf.extend(d.iter().map(|s| *s as f32 / 32768.0));
                                push(&buf)
                            },
                            error,
                            None,
                        )
                    }
                    cpal::SampleFormat::I32 => {
                        let mut buf = Vec::new();
                        device.build_input_stream(
                            &stream_config,
                            move |d: &[i32], _| {
                                buf.clear();
                                buf.extend(d.iter().map(|s| *s as f32 / 2147483648.0));
                                push(&buf)
                            },
                            error,
                            None,
                        )
                    }
                    cpal::SampleFormat::U16 => {
                        let mut buf = Vec::new();
                        device.build_input_stream(
                            &stream_config,
                            move |d: &[u16], _| {
                                buf.clear();
                                buf.extend(d.iter().map(|s| (*s as f32 - 32768.0) / 32768.0));
                                push(&buf)
                            },
                            error,
                            None,
                        )
                    }
                    other => return Err(anyhow!("Unsupported audio format {other:?}")),
                }
                .map_err(|e| anyhow!("Couldn't open {name}: {e}. Check Windows Settings → Privacy & security → Microphone."))?;
                stream.play().map_err(|e| anyhow!("Couldn't start {name}: {e}"))?;
                Ok((stream, rate, name))
            };
            match build() {
                Ok((stream, rate, name)) => {
                    let _ = ready_tx.send(Ok((rate, name)));
                    let _ = stop_rx.recv();
                    drop(stream);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            }
        })?;
        let (rate, device_name) = ready_rx.recv().map_err(|_| anyhow!("The audio thread stopped unexpectedly."))??;
        Ok(Self { stop: Some(stop_tx), thread: Some(thread), rate, device_name })
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub struct RecordedAudio {
    /// 16 kHz mono.
    pub samples: Vec<f32>,
    pub duration: f64,
    pub has_speech: bool,
}

/// A dictation recording held in memory; audio never touches the disk.
pub struct Recorder {
    capture: Capture,
    buffer: Arc<Mutex<Vec<f32>>>,
    started: Instant,
}

impl Recorder {
    /// `on_level` receives waveform levels (0...1) about 20 times per second.
    pub fn start(microphone: Option<AudioInputDevice>, on_level: impl Fn(f32) + Send + 'static) -> Result<Self> {
        let buffer = Arc::new(Mutex::new(Vec::with_capacity(48_000 * 30)));
        let sink = buffer.clone();
        let mut meter = VoiceLevelMeter::default();
        let mut pending: Vec<f32> = Vec::new();
        let rate_cell = Arc::new(std::sync::atomic::AtomicU32::new(48_000));
        let rate_reader = rate_cell.clone();
        let capture = Capture::open(Source::Microphone(microphone), move |mono| {
            sink.lock().extend_from_slice(mono);
            let rate = rate_reader.load(std::sync::atomic::Ordering::Relaxed) as f32;
            pending.extend_from_slice(mono);
            let slice = (rate / 20.0) as usize;
            while pending.len() >= slice {
                let level = meter.update(rms(&pending[..slice]), 0.05);
                on_level(level);
                pending.drain(..slice);
            }
        })?;
        rate_cell.store(capture.rate, std::sync::atomic::Ordering::Relaxed);
        Ok(Self { capture, buffer, started: Instant::now() })
    }

    pub fn elapsed(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    /// Stops capture and converts to 16 kHz mono.
    pub fn stop(self, silence_threshold: f32) -> RecordedAudio {
        let rate = self.capture.rate;
        drop(self.capture);
        let raw = std::mem::take(&mut *self.buffer.lock());
        let samples = to_speech_format(&raw, 1, rate);
        let duration = samples.len() as f64 / opendictate_speech::SAMPLE_RATE as f64;
        let has_speech = has_speech(&samples, silence_threshold);
        RecordedAudio { samples, duration, has_speech }
    }
}
