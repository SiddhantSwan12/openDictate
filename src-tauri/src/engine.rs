//! Owns the loaded local speech model, shared by dictation and meetings.
//! Loading reads local files only; downloads happen solely through an explicit install.

use crate::credentials::{self, Purpose};
use crate::paths;
use anyhow::{anyhow, Result};
use opendictate_core::model::{AppSettings, SpeechConnection};
use opendictate_speech::{find_model, is_installed, EngineKind, LoadedModel, ModelInfo};
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::Arc;

/// What the selected model ID resolves to.
#[derive(Clone)]
pub enum Selected {
    Local(ModelInfo),
    Api(SpeechConnection),
}

impl Selected {
    pub fn resolve(settings: &AppSettings, id: &str) -> Option<Self> {
        if let Some(model) = find_model(id) {
            return Some(Self::Local(model));
        }
        settings.speech_connections.iter().find(|c| c.speech_model_id() == id).cloned().map(Self::Api)
    }

    pub fn name(&self) -> String {
        match self {
            Self::Local(m) => m.name.clone(),
            Self::Api(c) => c.name.clone(),
        }
    }

    pub fn is_api(&self) -> bool {
        matches!(self, Self::Api(_))
    }

    pub fn is_installed(&self) -> bool {
        match self {
            Self::Local(m) => is_installed(&paths::models_dir(), m),
            Self::Api(_) => true,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelView {
    pub id: String,
    pub name: String,
    pub detail: String,
    pub size_label: String,
    pub engine: EngineKind,
    pub installed: bool,
    pub languages: Vec<&'static str>,
}

pub fn model_views(settings: &AppSettings) -> Vec<ModelView> {
    let root = paths::models_dir();
    let mut views: Vec<ModelView> = opendictate_speech::catalog()
        .into_iter()
        .map(|m| ModelView {
            installed: is_installed(&root, &m),
            id: m.id,
            name: m.name,
            detail: m.detail,
            size_label: m.size_label,
            engine: m.engine,
            languages: m.languages,
        })
        .collect();
    views.extend(settings.speech_connections.iter().map(|c| ModelView {
        id: c.speech_model_id(),
        name: c.name.clone(),
        detail: format!("{} · {}", c.api.name(), c.endpoint),
        size_label: "Uses your endpoint".into(),
        engine: EngineKind::Api,
        installed: true,
        languages: vec![],
    }));
    views
}

#[derive(Default)]
pub struct Engine {
    loaded: Arc<Mutex<Option<LoadedModel>>>,
}

impl Engine {
    pub fn loaded_id(&self) -> Option<String> {
        self.loaded.try_lock().and_then(|m| m.as_ref().map(|m| m.id.clone()))
    }

    pub fn unload(&self) {
        *self.loaded.lock() = None;
    }

    /// Loads the model once and warms it up so the first dictation is fast.
    pub fn ensure_loaded(&self, model: &ModelInfo, use_gpu: bool) -> Result<()> {
        let mut slot = self.loaded.lock();
        if slot.as_ref().is_some_and(|m| m.id == model.id) {
            return Ok(());
        }
        *slot = None;
        let mut loaded = LoadedModel::load(&paths::models_dir(), model, use_gpu)?;
        let _ = loaded.transcribe(&vec![0.0; 16_000], Some("en"), &[]);
        *slot = Some(loaded);
        Ok(())
    }

    /// Transcribes with whichever model is selected. API connections upload only here.
    pub fn transcribe(
        &self,
        selected: &Selected,
        samples: &[f32],
        language: Option<&str>,
        vocabulary: &[String],
        use_gpu: bool,
    ) -> Result<String> {
        match selected {
            Selected::Local(model) => {
                self.ensure_loaded(model, use_gpu)?;
                let mut slot = self.loaded.lock();
                let loaded = slot.as_mut().ok_or_else(|| anyhow!("Prepare a speech model before starting dictation."))?;
                let language = language.filter(|_| model.engine == EngineKind::Whisper || !model.languages.is_empty());
                loaded.transcribe(samples, language, vocabulary)
            }
            Selected::Api(connection) => {
                let key = credentials::read(Purpose::Speech, connection).map_err(|e| anyhow!(e))?.unwrap_or_default();
                opendictate_speech::api::transcribe(connection, &key, samples, language)
            }
        }
    }

    /// Validates without network access, loading a local model if needed.
    pub fn prepare(&self, selected: &Selected, language: Option<&str>, use_gpu: bool) -> Result<()> {
        match selected {
            Selected::Local(model) => self.ensure_loaded(model, use_gpu),
            Selected::Api(connection) => {
                let key = credentials::read(Purpose::Speech, connection).map_err(|e| anyhow!(e))?.unwrap_or_default();
                opendictate_speech::api::prepare(connection, &key, language)
            }
        }
    }
}
