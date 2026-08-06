use anyhow::{Context, Result};
use reqwest::blocking::multipart::{Form, Part};
use reqwest::blocking::Client;
use reqwest::Url;
use serde::Deserialize;

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_TRANSCRIBE_MODEL: &str = "gpt-transcribe";
const DEFAULT_TRANSLATE_MODEL: &str = "whisper-1";

#[derive(Debug, Clone)]
pub struct OpenAiConfig {
    pub api_key: String,
    pub base_url: Url,
    pub transcribe_model: String,
    pub translate_model: String,
}

impl OpenAiConfig {
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .context("OPENAI_API_KEY is not set. Provide it via environment or .env file.")?;
        let base_url =
            std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        let base_url = format!("{}/", base_url.trim_end_matches('/'));
        let base_url = Url::parse(&base_url).context("OPENAI_BASE_URL is not a valid URL")?;
        let transcribe_model = std::env::var("WHISPER_WRITE_TRANSCRIBE_MODEL")
            .unwrap_or_else(|_| DEFAULT_TRANSCRIBE_MODEL.to_string());
        let translate_model = std::env::var("WHISPER_WRITE_TRANSLATE_MODEL")
            .unwrap_or_else(|_| DEFAULT_TRANSLATE_MODEL.to_string());

        Ok(Self {
            api_key,
            base_url,
            transcribe_model,
            translate_model,
        })
    }
}

#[derive(Clone)]
pub struct OpenAiClient {
    client: Client,
    config: OpenAiConfig,
}

impl OpenAiClient {
    pub fn new(config: OpenAiConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .context("Failed to build HTTP client")?;
        Ok(Self { client, config })
    }

    pub fn transcribe_audio(&self, file: AudioFile) -> Result<String> {
        self.send_audio(
            AudioEndpoint::Transcriptions,
            &self.config.transcribe_model,
            file,
        )
    }

    pub fn translate_audio(&self, file: AudioFile) -> Result<String> {
        self.send_audio(
            AudioEndpoint::Translations,
            &self.config.translate_model,
            file,
        )
    }

    fn send_audio(&self, endpoint: AudioEndpoint, model: &str, file: AudioFile) -> Result<String> {
        if file.bytes.is_empty() {
            return Ok(String::new());
        }

        let part = Part::bytes(file.bytes)
            .file_name(file.name)
            .mime_str("audio/wav")
            .context("Failed to set audio MIME type")?;

        let form = Form::new()
            .part("file", part)
            .text("model", model.to_string())
            .text("response_format", "json".to_string());

        let url = self
            .config
            .base_url
            .join(endpoint.path())
            .context("Failed to build OpenAI request URL")?;

        let response = self
            .client
            .post(url)
            .bearer_auth(&self.config.api_key)
            .multipart(form)
            .send()
            .context("Failed to send request to OpenAI")?
            .error_for_status()
            .context("OpenAI request failed")?;

        let body: AudioResponse = response.json().context("Failed to parse OpenAI response")?;
        Ok(body.text.unwrap_or_default().trim().to_string())
    }
}

#[derive(Debug)]
pub struct AudioFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

impl AudioFile {
    pub fn new(name: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            bytes,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum AudioEndpoint {
    Transcriptions,
    Translations,
}

impl AudioEndpoint {
    fn path(self) -> &'static str {
        match self {
            AudioEndpoint::Transcriptions => "audio/transcriptions",
            AudioEndpoint::Translations => "audio/translations",
        }
    }
}

#[derive(Debug, Deserialize)]
struct AudioResponse {
    text: Option<String>,
}
