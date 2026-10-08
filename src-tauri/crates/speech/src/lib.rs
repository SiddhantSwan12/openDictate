//! Local speech engines (whisper.cpp and Parakeet ONNX), model installation and
//! user-configured API connections. Mirrors BetterWispr's `Speech/` folder.

pub mod api;
pub mod audio;

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use transcribe_rs::onnx::parakeet::ParakeetModel;
use transcribe_rs::onnx::Quantization;
use transcribe_rs::whisper_cpp::{WhisperEngine, WhisperInferenceParams, WhisperLoadParams};
use transcribe_rs::{SpeechModel as _, TranscribeOptions};

pub const SAMPLE_RATE: u32 = 16_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EngineKind {
    Whisper,
    Parakeet,
    Api,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFile {
    pub url: String,
    pub name: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub detail: String,
    pub size_label: String,
    pub engine: EngineKind,
    /// Language codes the model handles; empty means "many" (Whisper).
    pub languages: Vec<&'static str>,
    #[serde(skip)]
    pub files: Vec<RemoteFile>,
}

const PARAKEET_V3_LANGUAGES: &[&str] = &[
    "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv", "lt", "mt", "pl", "pt", "ro",
    "sk", "sl", "es", "sv", "ru", "uk",
];

fn hf(repo: &str, name: &str, bytes: u64) -> RemoteFile {
    RemoteFile { url: format!("https://huggingface.co/{repo}/resolve/main/{name}"), name: name.into(), bytes }
}

fn parakeet_files(repo: &str, decoder_bytes: u64) -> Vec<RemoteFile> {
    vec![
        hf(repo, "encoder-model.int8.onnx", 652_183_999),
        hf(repo, "decoder_joint-model.int8.onnx", decoder_bytes),
        hf(repo, "nemo128.onnx", 0),
        hf(repo, "vocab.txt", 0),
        hf(repo, "config.json", 0),
    ]
}

/// Built-in local models. IDs are persisted, so renaming one needs a migration.
pub fn catalog() -> Vec<ModelInfo> {
    let whisper = "ggerganov/whisper.cpp";
    vec![
        ModelInfo {
            id: "parakeet-v3".into(),
            name: "Parakeet TDT v3".into(),
            detail: "NVIDIA's multilingual model for 25 European languages, with punctuation. Very fast.".into(),
            size_label: "~680 MB".into(),
            engine: EngineKind::Parakeet,
            languages: PARAKEET_V3_LANGUAGES.to_vec(),
            files: parakeet_files("istupakov/parakeet-tdt-0.6b-v3-onnx", 18_202_004),
        },
        ModelInfo {
            id: "parakeet-v2".into(),
            name: "Parakeet TDT v2".into(),
            detail: "NVIDIA's English-only model, tuned for English accuracy, with punctuation.".into(),
            size_label: "~670 MB".into(),
            engine: EngineKind::Parakeet,
            languages: vec!["en"],
            files: parakeet_files("istupakov/parakeet-tdt-0.6b-v2-onnx", 9_004_004),
        },
        ModelInfo {
            id: "whisper-turbo".into(),
            name: "Whisper Large v3 Turbo".into(),
            detail: "Broadest language coverage, including languages Parakeet does not support. Fast on NVIDIA GPUs.".into(),
            size_label: "~1.6 GB".into(),
            engine: EngineKind::Whisper,
            languages: vec![],
            files: vec![hf(whisper, "ggml-large-v3-turbo.bin", 1_624_555_275)],
        },
        ModelInfo {
            id: "whisper-turbo-compressed".into(),
            name: "Whisper Large v3 Turbo (compressed)".into(),
            detail: "The languages of Whisper Large v3 Turbo in a compressed build that takes less disk space.".into(),
            size_label: "~575 MB".into(),
            engine: EngineKind::Whisper,
            languages: vec![],
            files: vec![hf(whisper, "ggml-large-v3-turbo-q5_0.bin", 574_041_195)],
        },
        ModelInfo {
            id: "whisper-small".into(),
            name: "Whisper Small".into(),
            detail: "A lighter multilingual Whisper model with a smaller download.".into(),
            size_label: "~490 MB".into(),
            engine: EngineKind::Whisper,
            languages: vec![],
            files: vec![hf(whisper, "ggml-small.bin", 487_601_967)],
        },
    ]
}

pub fn find_model(id: &str) -> Option<ModelInfo> {
    catalog().into_iter().find(|m| m.id == id)
}

const INSTALLED_MARKER: &str = ".opendictate-installed";

pub fn model_dir(models_root: &Path, model: &ModelInfo) -> PathBuf {
    models_root.join(&model.id)
}

/// A model counts as installed only after every file downloaded and loaded once.
pub fn is_installed(models_root: &Path, model: &ModelInfo) -> bool {
    let dir = model_dir(models_root, model);
    dir.join(INSTALLED_MARKER).exists() && model.files.iter().all(|f| dir.join(&f.name).exists())
}

pub fn uninstall(models_root: &Path, model: &ModelInfo) -> Result<()> {
    let dir = model_dir(models_root, model);
    if dir.exists() {
        fs::remove_dir_all(&dir).with_context(|| format!("Couldn't remove {}", dir.display()))?;
    }
    Ok(())
}

/// Downloads every file of a model, reporting progress in 0..=1. Only runs on an explicit install action.
pub fn install(
    models_root: &Path,
    model: &ModelInfo,
    use_gpu: bool,
    cancel: &AtomicBool,
    progress: &dyn Fn(f64),
) -> Result<()> {
    let dir = model_dir(models_root, model);
    fs::create_dir_all(&dir)?;
    let _ = fs::remove_file(dir.join(INSTALLED_MARKER));
    let client = reqwest::blocking::Client::builder()
        .user_agent("OpenDictate")
        .timeout(None)
        .build()?;
    let total: u64 = model.files.iter().map(|f| f.bytes.max(1)).sum();
    let mut done: u64 = 0;
    for file in &model.files {
        let target = dir.join(&file.name);
        if target.exists() && (file.bytes == 0 || fs::metadata(&target)?.len() == file.bytes) {
            done += file.bytes.max(1);
            continue;
        }
        let partial = dir.join(format!("{}.part", file.name));
        let mut response = client
            .get(&file.url)
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|e| anyhow!("Hugging Face didn't send {}: {e}. Check your internet connection and try again.", file.name))?;
        let expected = response.content_length().unwrap_or(file.bytes);
        let mut out = fs::File::create(&partial)?;
        let mut buffer = vec![0u8; 1 << 20];
        let mut received: u64 = 0;
        loop {
            if cancel.load(Ordering::Relaxed) {
                drop(out);
                let _ = fs::remove_file(&partial);
                bail!("Download cancelled.");
            }
            let n = response.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            out.write_all(&buffer[..n])?;
            received += n as u64;
            let scale = file.bytes.max(1) as f64 / expected.max(1) as f64;
            progress(((done as f64 + received as f64 * scale) / total as f64 * 0.9).min(0.9));
        }
        out.sync_all()?;
        drop(out);
        if expected > 0 && received != expected {
            let _ = fs::remove_file(&partial);
            bail!("{} downloaded incompletely ({received} of {expected} bytes). Try again.", file.name);
        }
        fs::rename(&partial, &target)?;
        done += file.bytes.max(1);
    }
    progress(0.92);
    // Load once to prove the files work before marking the model installed.
    LoadedModel::load(models_root, model, use_gpu).context("The downloaded model could not be loaded")?;
    fs::write(dir.join(INSTALLED_MARKER), b"ok")?;
    progress(1.0);
    Ok(())
}

enum Inner {
    Whisper(Box<WhisperEngine>),
    Parakeet(Box<ParakeetModel>),
}

/// A loaded local model. Loading reads local files only and never touches the network.
pub struct LoadedModel {
    pub id: String,
    inner: Inner,
}

impl LoadedModel {
    pub fn load(models_root: &Path, model: &ModelInfo, use_gpu: bool) -> Result<Self> {
        let dir = model_dir(models_root, model);
        if model.files.iter().any(|f| !dir.join(&f.name).exists()) {
            bail!("Download {} in Models first. Transcription never downloads files.", model.name);
        }
        let inner = match model.engine {
            EngineKind::Whisper => {
                let path = dir.join(&model.files[0].name);
                let params = WhisperLoadParams { use_gpu, ..Default::default() };
                let engine = WhisperEngine::load_with_params(&path, params.clone())
                    .or_else(|e| {
                        // Flash attention or the GPU can be unavailable on some drivers; fall back to CPU.
                        log::warn!("Whisper GPU load failed ({e}); retrying on CPU");
                        WhisperEngine::load_with_params(&path, WhisperLoadParams { use_gpu: false, flash_attn: false, ..params })
                    })
                    .map_err(|e| anyhow!("Couldn't load {}: {e}", model.name))?;
                Inner::Whisper(Box::new(engine))
            }
            EngineKind::Parakeet => {
                transcribe_rs::set_ort_accelerator(if use_gpu {
                    transcribe_rs::OrtAccelerator::DirectMl
                } else {
                    transcribe_rs::OrtAccelerator::CpuOnly
                });
                let model_result = ParakeetModel::load(&dir, &Quantization::Int8).or_else(|e| {
                    log::warn!("Parakeet DirectML load failed ({e}); retrying on CPU");
                    transcribe_rs::set_ort_accelerator(transcribe_rs::OrtAccelerator::CpuOnly);
                    ParakeetModel::load(&dir, &Quantization::Int8)
                });
                Inner::Parakeet(Box::new(model_result.map_err(|e| anyhow!("Couldn't load {}: {e}", model.name))?))
            }
            EngineKind::Api => bail!("API connections are not local models."),
        };
        Ok(Self { id: model.id.clone(), inner })
    }

    /// Transcribes 16 kHz mono samples. Vocabulary becomes a Whisper prompt; Parakeet relies on
    /// the vocabulary replacement pass that runs afterwards.
    pub fn transcribe(&mut self, samples: &[f32], language: Option<&str>, vocabulary: &[String]) -> Result<String> {
        if samples.is_empty() {
            bail!("The recording contains no audio.");
        }
        let text = match &mut self.inner {
            Inner::Whisper(engine) => {
                let prompt = (!vocabulary.is_empty()).then(|| {
                    let mut terms: Vec<&str> = vocabulary.iter().map(String::as_str).take(60).collect();
                    terms.dedup();
                    format!("Glossary: {}.", terms.join(", "))
                });
                let params = WhisperInferenceParams {
                    language: language.map(String::from),
                    initial_prompt: prompt,
                    n_threads: std::thread::available_parallelism().map_or(4, |n| n.get().min(8) as i32),
                    ..Default::default()
                };
                // Whisper needs at least one second of audio.
                let mut padded = samples.to_vec();
                if padded.len() < SAMPLE_RATE as usize + SAMPLE_RATE as usize / 4 {
                    padded.resize(SAMPLE_RATE as usize + SAMPLE_RATE as usize / 4, 0.0);
                }
                engine.transcribe_with(&padded, &params).map_err(|e| anyhow!("{e}"))?.text
            }
            Inner::Parakeet(model) => {
                let options = TranscribeOptions { language: language.map(String::from), ..Default::default() };
                model.transcribe(samples, &options).map_err(|e| anyhow!("{e}"))?.text
            }
        };
        Ok(text.trim().to_string())
    }
}
