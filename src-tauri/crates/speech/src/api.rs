//! Port of BetterWispr `APISpeechProvider.swift`: optional, explicitly selected cloud speech APIs.
//! Never used as a fallback. Requests reject redirects and carry no cookies or cache.

use crate::audio::wav;
use crate::SAMPLE_RATE;
use anyhow::{anyhow, bail, Result};
use opendictate_core::cleaner::base_language;
use opendictate_core::model::{SpeechApi, SpeechConnection};
use reqwest::blocking::{multipart, Client};
use std::time::Duration;

fn client() -> Result<Client> {
    Ok(Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(20))
        .build()?)
}

fn http_error(status: u16) -> anyhow::Error {
    anyhow!(match status {
        401 | 403 => "The provider rejected your API key. Check the key and its permissions in Models.".to_string(),
        429 => "The provider's rate limit or account quota was reached. Check your account and try again later.".to_string(),
        300..=399 => "The endpoint redirected the request. Enter its final URL in Models; audio and keys are never forwarded to redirects.".to_string(),
        code => format!("The speech endpoint returned HTTP {code}. Check the endpoint, model and selected language."),
    })
}

/// Validates configuration and key without any network request.
pub fn prepare(connection: &SpeechConnection, key: &str, language: Option<&str>) -> Result<()> {
    connection.validate().map_err(|e| anyhow!(e))?;
    connection.validate_key(key).map_err(|e| anyhow!(e))?;
    connection.validate_language(language).map_err(|e| anyhow!(e))?;
    Ok(())
}

/// Uploads 16 kHz mono audio to the selected connection. Sarvam receives sequential 25-second clips.
pub fn transcribe(connection: &SpeechConnection, key: &str, samples: &[f32], language: Option<&str>) -> Result<String> {
    prepare(connection, key, language)?;
    let seconds = samples.len() as f64 / SAMPLE_RATE as f64;
    if seconds > 125.0 {
        bail!("Use an audio clip of at most two minutes for this connection.");
    }
    let chunk_seconds = if connection.api == SpeechApi::Sarvam { 25 } else { 125 };
    let client = client()?;
    let language = language.filter(|l| *l != "auto").map(base_language);
    let mut transcripts = Vec::new();
    for chunk in samples.chunks(chunk_seconds * SAMPLE_RATE as usize) {
        let text = send(&client, connection, key, wav(chunk), language.as_deref())?;
        if !text.is_empty() {
            transcripts.push(text);
        }
    }
    Ok(transcripts.join(" "))
}

fn send(client: &Client, connection: &SpeechConnection, key: &str, wav: Vec<u8>, language: Option<&str>) -> Result<String> {
    let mut request = client.post(&connection.endpoint).header("Accept", "application/json");
    if !key.is_empty() {
        request = match connection.api {
            SpeechApi::Sarvam => request.header("api-subscription-key", key),
            _ => request.bearer_auth(key),
        };
    }
    request = match connection.api {
        SpeechApi::Smallest => {
            let mut query = vec![("model", connection.model_id.clone())];
            if let Some(language) = language {
                query.push(("language", language.to_string()));
            }
            request.query(&query).header("Content-Type", "application/octet-stream").body(wav)
        }
        SpeechApi::Sarvam | SpeechApi::OpenAiCompatible => {
            let mut form = multipart::Form::new().text("model", connection.model_id.clone());
            if connection.api == SpeechApi::Sarvam {
                form = form
                    .text("language_code", language.map_or("unknown".to_string(), |l| format!("{l}-IN")))
                    .text("mode", "transcribe");
            } else {
                form = form.text("response_format", "json");
                if let Some(language) = language {
                    form = form.text("language", language.to_string());
                }
            }
            let file = multipart::Part::bytes(wav).file_name("audio.wav").mime_str("audio/wav")?;
            request.multipart(form.part("file", file))
        }
    };
    let response = request.send().map_err(|e| anyhow!("Couldn't reach the speech endpoint: {e}"))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(http_error(status));
    }
    let field = match connection.api {
        SpeechApi::Sarvam => "transcript",
        SpeechApi::Smallest => "transcription",
        SpeechApi::OpenAiCompatible => "text",
    };
    let json: serde_json::Value = response
        .json()
        .map_err(|_| anyhow!("The endpoint did not return a supported transcription response. Check its API format and model."))?;
    json.get(field)
        .and_then(|v| v.as_str())
        .map(|t| t.trim().to_string())
        .ok_or_else(|| anyhow!("The endpoint did not return a supported transcription response. Check its API format and model."))
}
