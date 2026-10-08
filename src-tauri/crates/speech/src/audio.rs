//! Sample-rate conversion, levels and WAV encoding for 16 kHz mono speech audio.

use crate::SAMPLE_RATE;

/// Converts interleaved samples at any rate and channel count to 16 kHz mono.
pub fn to_speech_format(interleaved: &[f32], channels: usize, rate: u32) -> Vec<f32> {
    let channels = channels.max(1);
    let mono: Vec<f32> = if channels == 1 {
        interleaved.to_vec()
    } else {
        interleaved.chunks_exact(channels).map(|frame| frame.iter().sum::<f32>() / channels as f32).collect()
    };
    resample(&mono, rate, SAMPLE_RATE)
}

/// Windowed-sinc resampling; quality is well above what speech models need.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = to as f64 / from as f64;
    let out_len = (input.len() as f64 * ratio).floor() as usize;
    // Low-pass at the lower Nyquist frequency when downsampling.
    let cutoff = ratio.min(1.0) * 0.95;
    let half_width = 16usize;
    let mut out = Vec::with_capacity(out_len);
    for n in 0..out_len {
        let center = n as f64 / ratio;
        let first = (center.floor() as isize - half_width as isize).max(0) as usize;
        let last = ((center.floor() as usize) + half_width).min(input.len() - 1);
        let mut acc = 0.0f64;
        let mut norm = 0.0f64;
        for (i, sample) in input.iter().enumerate().take(last + 1).skip(first) {
            let x = (i as f64 - center) * cutoff;
            let sinc = if x.abs() < 1e-9 { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) };
            let t = (i as f64 - center) / (half_width as f64 + 1.0);
            let window = if t.abs() >= 1.0 { 0.0 } else { 0.5 * (1.0 + (std::f64::consts::PI * t).cos()) };
            let weight = sinc * window;
            acc += *sample as f64 * weight;
            norm += weight;
        }
        out.push(if norm.abs() > 1e-9 { (acc / norm) as f32 } else { 0.0 });
    }
    out
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Block size used for the amplitude gate, close to an audio-engine buffer.
pub const GATE_BLOCK: usize = 1024;

/// Seconds of audio whose block RMS reaches the threshold. An amplitude gate, not a semantic VAD.
pub fn active_seconds(samples: &[f32], threshold: f32) -> f64 {
    let threshold = if threshold.is_finite() { threshold.max(0.0) } else { 0.002 };
    let active: usize = samples.chunks(GATE_BLOCK).filter(|c| rms(c) >= threshold).map(<[f32]>::len).sum();
    active as f64 / SAMPLE_RATE as f64
}

/// A dictation has speech when at least 0.12 s rises above the silence threshold.
pub fn has_speech(samples: &[f32], threshold: f32) -> bool {
    active_seconds(samples, threshold) >= 0.12
}

/// When a meeting chunk closes: at the maximum length, or after the minimum at a pause.
#[derive(Clone, Copy, Debug)]
pub struct ChunkPolicy {
    pub minimum: f64,
    pub maximum: f64,
    pub pause: f64,
}

impl Default for ChunkPolicy {
    fn default() -> Self {
        Self { minimum: 10.0, maximum: 30.0, pause: 0.6 }
    }
}

impl ChunkPolicy {
    pub fn should_rotate(&self, duration: f64, trailing_silence: f64) -> bool {
        duration >= self.maximum || (duration >= self.minimum && trailing_silence >= self.pause)
    }
}

/// Maps RMS over a stretch of audio to a 0...1 waveform level relative to the room's
/// noise floor and the speaker's recent peak. Port of BetterWispr's `VoiceLevelMeter`.
#[derive(Clone, Debug, Default)]
pub struct VoiceLevelMeter {
    pub level: f32,
    floor: Option<f32>,
    peak: Option<f32>,
}

impl VoiceLevelMeter {
    const TICK: f32 = 0.1;
    const FLOOR_RISE: f32 = 0.1;
    const PEAK_FALL: f32 = 0.12;
    const MINIMUM_RANGE: f32 = 15.0;
    const GATE: f32 = 3.0;
    const CURVE: f32 = 0.7;
    const ATTACK: f32 = 0.8;
    const RELEASE: f32 = 0.6;

    /// Rates are tuned per 0.1 s and scaled by `duration`, so the meter moves at the same speed for any slice length.
    pub fn update(&mut self, rms: f32, duration: f32) -> f32 {
        let ticks = duration.max(0.0) / Self::TICK;
        let mut target = 0.0;
        if rms.is_finite() && rms > 0.0 {
            let decibels = (20.0 * rms.log10()).max(-60.0);
            let floor = (self.floor.unwrap_or(decibels) + Self::FLOOR_RISE * ticks).min(decibels);
            let peak = decibels
                .max(self.peak.unwrap_or(-60.0) - Self::PEAK_FALL * ticks)
                .max(floor + Self::MINIMUM_RANGE);
            let span = peak - floor - Self::GATE;
            target = ((decibels - floor - Self::GATE) / span).clamp(0.0, 1.0).powf(Self::CURVE);
            self.floor = Some(floor);
            self.peak = Some(peak);
        }
        let rate = if target > self.level { Self::ATTACK } else { Self::RELEASE };
        self.level += (target - self.level) * (1.0 - (1.0 - rate).powf(ticks));
        self.level
    }
}

/// 16-bit PCM WAV bytes for 16 kHz mono samples.
pub fn wav(samples: &[f32]) -> Vec<u8> {
    let size = (samples.len() * 2) as u32;
    let mut data = Vec::with_capacity(44 + size as usize);
    data.extend_from_slice(b"RIFF");
    data.extend_from_slice(&(size + 36).to_le_bytes());
    data.extend_from_slice(b"WAVEfmt ");
    data.extend_from_slice(&16u32.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    data.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    data.extend_from_slice(&2u16.to_le_bytes());
    data.extend_from_slice(&16u16.to_le_bytes());
    data.extend_from_slice(b"data");
    data.extend_from_slice(&size.to_le_bytes());
    for sample in samples {
        let clamped = if sample.is_finite() { sample.clamp(-1.0, 1.0) } else { 0.0 };
        data.extend_from_slice(&((clamped * i16::MAX as f32).round() as i16).to_le_bytes());
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampling_keeps_duration_and_tone() {
        let rate = 48_000;
        let tone: Vec<f32> = (0..rate).map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin() * 0.5).collect();
        let out = resample(&tone, rate, 16_000);
        assert_eq!(out.len(), 16_000);
        assert!((rms(&out) - rms(&tone)).abs() < 0.02);
    }

    #[test]
    fn wav_header_is_valid() {
        let bytes = wav(&[0.0; 160]);
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(bytes.len(), 44 + 320);
    }
}
