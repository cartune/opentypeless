use async_trait::async_trait;

use crate::error::AppError;

use super::managed_audio::{ManagedAudioEncoderWorker, ManagedAudioEncodingConfig};
use super::{SttConfig, SttProvider, TranscriptEvent, UploadFormat};

/// Configuration for a Whisper-compatible HTTP file-upload STT provider.
#[derive(Debug)]
pub struct WhisperCompatConfig {
    pub provider_name: String,
    pub endpoint: String,
    pub model: String,
    /// Extra form text fields (e.g. GLM-ASR needs "stream"="false").
    pub extra_fields: Vec<(String, String)>,
    /// Local OpenAI-compatible servers often do not require authentication.
    pub api_key_required: bool,
    /// The endpoint decodes Ogg/Opus uploads; otherwise WAV is sent.
    pub accepts_ogg_opus: bool,
    /// The endpoint honours a free-text `prompt` field.
    pub supports_prompt: bool,
}

/// Ogg byte cap for live Opus encoding of a file upload: far above any real
/// recording (12.5 min at 48 kbit/s is ~4.5 MB) but below the 25 MB API limit.
const OPUS_UPLOAD_MAX_BYTES: u64 = 20 * 1024 * 1024;

/// Decide whether a recording should be uploaded as Ogg/Opus.
pub fn should_upload_opus(format: UploadFormat, accepts_ogg_opus: bool) -> bool {
    match format {
        UploadFormat::Wav => false,
        UploadFormat::Opus | UploadFormat::Auto => accepts_ogg_opus,
    }
}

/// The fields of one transcription request, independent of reqwest so the
/// decision logic can be unit tested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptionRequestFields {
    pub model: String,
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub file_name: &'static str,
    pub mime_type: &'static str,
}

/// One transcription request without retries (the shadow path).
async fn post_transcription(
    client: &reqwest::Client,
    endpoint: &str,
    api_key: &str,
    fields: &TranscriptionRequestFields,
    payload: Vec<u8>,
    extra_fields: &[(String, String)],
) -> Result<String, String> {
    let file_part = reqwest::multipart::Part::bytes(payload)
        .file_name(fields.file_name)
        .mime_str(fields.mime_type)
        .map_err(|e| e.to_string())?;
    let mut form = reqwest::multipart::Form::new()
        .text("model", fields.model.clone())
        .part("file", file_part);
    if let Some(lang) = fields.language.clone() {
        form = form.text("language", lang);
    }
    if let Some(prompt) = fields.prompt.clone() {
        form = form.text("prompt", prompt);
    }
    for (key, value) in extra_fields {
        form = form.text(key.clone(), value.clone());
    }
    let mut request = client
        .post(endpoint)
        .multipart(form)
        .timeout(std::time::Duration::from_secs(60));
    if !api_key.trim().is_empty() {
        request = request.header("Authorization", format!("Bearer {api_key}"));
    }
    let resp = request.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        let end = body
            .char_indices()
            .take_while(|&(i, _)| i < 200)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(body.len());
        return Err(format!("HTTP {}: {}", status, &body[..end]));
    }
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    Ok(v["text"].as_str().unwrap_or("").trim().to_string())
}

pub fn transcription_request_fields(
    provider: &WhisperCompatConfig,
    config: &SttConfig,
    opus: bool,
) -> TranscriptionRequestFields {
    let model = config
        .model_override
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .unwrap_or(provider.model.as_str())
        .to_string();
    let language = config
        .language
        .as_deref()
        .filter(|lang| *lang != "multi" && !lang.trim().is_empty())
        .map(|lang| lang.to_string());
    let prompt = if provider.supports_prompt {
        config
            .prompt
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(|p| p.to_string())
    } else {
        None
    };
    let (file_name, mime_type) = if opus {
        ("audio.ogg", "audio/ogg")
    } else {
        ("audio.wav", "audio/wav")
    };
    TranscriptionRequestFields {
        model,
        language,
        prompt,
        file_name,
        mime_type,
    }
}

/// A 4xx that looks like the endpoint rejected the container/codec rather
/// than the request itself. Used to retry an Opus upload as WAV once.
pub fn is_unsupported_format_rejection(status: u16, body: &str) -> bool {
    if !(400..500).contains(&status) || status == 401 || status == 403 || status == 429 {
        return false;
    }
    let body = body.to_ascii_lowercase();
    if body.contains("language") {
        return false;
    }
    [
        "format",
        "codec",
        "decode",
        "unsupported",
        "invalid file",
        "file type",
        "ogg",
        "opus",
    ]
    .iter()
    .any(|needle| body.contains(needle))
}

/// Max audio buffer: ~24 MB PCM ≈ 12.5 min at 16kHz 16-bit mono.
/// Keeps the resulting WAV under 25 MB (OpenAI/Groq limit).
const MAX_AUDIO_BYTES: usize = 24 * 1024 * 1024;

/// Generic provider for any OpenAI Whisper-compatible transcription API.
/// Works with: OpenAI, Groq, SiliconFlow, GLM-ASR.
pub struct WhisperCompatProvider {
    provider_config: WhisperCompatConfig,
    stt_config: Option<SttConfig>,
    audio_buffer: Vec<u8>,
    client: reqwest::Client,
    /// Live Ogg/Opus encoder fed during recording so stop() only finalizes.
    opus_worker: Option<ManagedAudioEncoderWorker>,
    opus_failed: bool,
}

impl WhisperCompatProvider {
    pub fn new(provider_config: WhisperCompatConfig) -> Self {
        Self {
            provider_config,
            stt_config: None,
            audio_buffer: Vec::new(),
            client: reqwest::Client::new(),
            opus_worker: None,
            opus_failed: false,
        }
    }

    pub fn with_client(provider_config: WhisperCompatConfig, client: reqwest::Client) -> Self {
        Self {
            provider_config,
            stt_config: None,
            audio_buffer: Vec::new(),
            client,
            opus_worker: None,
            opus_failed: false,
        }
    }

    fn opus_encoding_config() -> ManagedAudioEncodingConfig {
        ManagedAudioEncodingConfig {
            preferred_wav_max_bytes: 0,
            max_audio_bytes: OPUS_UPLOAD_MAX_BYTES,
            ..ManagedAudioEncodingConfig::default()
        }
    }

    fn start_opus_worker(&mut self, config: &SttConfig) {
        self.opus_worker = None;
        self.opus_failed = false;
        if !should_upload_opus(config.upload_format, self.provider_config.accepts_ogg_opus) {
            return;
        }
        if config.sample_rate != 16_000 {
            tracing::warn!(
                "{}: Opus upload requires 16 kHz input; using WAV",
                self.provider_config.provider_name
            );
            return;
        }
        match ManagedAudioEncoderWorker::start(
            super::cloud::stream_serial(config.operation_id.as_deref()),
            Self::opus_encoding_config(),
        ) {
            Ok(worker) => self.opus_worker = Some(worker),
            Err(error) => {
                tracing::warn!(
                    "{}: Opus encoder failed to start; using WAV: {}",
                    self.provider_config.provider_name,
                    error
                );
                self.opus_failed = true;
            }
        }
    }

    /// Finalize the live Opus stream, or `None` when WAV must be used.
    async fn finish_opus(&mut self) -> Option<Vec<u8>> {
        let worker = self.opus_worker.take()?;
        if self.opus_failed {
            return None;
        }
        match worker.finish().await {
            Ok(encoded) => Some(encoded.bytes),
            Err(error) => {
                tracing::warn!(
                    "{}: Opus finalize failed; falling back to WAV: {}",
                    self.provider_config.provider_name,
                    error
                );
                None
            }
        }
    }

    /// Build a WAV file from raw PCM 16-bit mono audio. Public so test helpers can reuse it.
    pub fn build_wav(pcm: &[u8], sample_rate: u32) -> Vec<u8> {
        let data_len = pcm.len() as u32;
        let channels: u16 = 1;
        let bits_per_sample: u16 = 16;
        let byte_rate = sample_rate * (channels as u32) * (bits_per_sample as u32) / 8;
        let block_align = channels * bits_per_sample / 8;
        let file_size = 36 + data_len;

        let mut wav = Vec::with_capacity(44 + pcm.len());
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&file_size.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&channels.to_le_bytes());
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&byte_rate.to_le_bytes());
        wav.extend_from_slice(&block_align.to_le_bytes());
        wav.extend_from_slice(&bits_per_sample.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.extend_from_slice(pcm);
        wav
    }
}

#[async_trait]
impl SttProvider for WhisperCompatProvider {
    async fn connect(&mut self, config: &SttConfig) -> Result<(), AppError> {
        if self.provider_config.api_key_required && config.api_key.is_empty() {
            return Err(AppError::Auth(format!(
                "{} API key is empty",
                self.provider_config.provider_name
            )));
        }
        self.stt_config = Some(config.clone());
        self.audio_buffer.clear();
        self.start_opus_worker(config);
        tracing::info!(
            "{} provider ready (buffering mode, upload={})",
            self.provider_config.provider_name,
            if self.opus_worker.is_some() {
                "ogg/opus"
            } else {
                "wav"
            }
        );
        Ok(())
    }

    async fn send_audio(&mut self, chunk: &[u8]) -> Result<(), AppError> {
        if self.audio_buffer.len() + chunk.len() > MAX_AUDIO_BYTES {
            return Err(AppError::Config(format!(
                "{}: audio exceeds maximum length (~12 min)",
                self.provider_config.provider_name
            )));
        }
        self.audio_buffer.extend_from_slice(chunk);
        if let Some(worker) = self.opus_worker.as_ref() {
            if let Err(error) = worker.try_send_pcm(chunk) {
                tracing::warn!(
                    "{}: Opus worker fell behind; retaining PCM for a WAV upload: {}",
                    self.provider_config.provider_name,
                    error
                );
                self.opus_failed = true;
                self.opus_worker.take();
            }
        }
        Ok(())
    }

    async fn recv_transcript(&mut self) -> Result<Option<TranscriptEvent>, AppError> {
        // File-based providers transcribe in disconnect(); keep this future
        // pending so the pipeline select loop does not busy-spin while recording.
        std::future::pending().await
    }

    async fn disconnect(&mut self) -> Result<Option<String>, AppError> {
        let config = match &self.stt_config {
            Some(c) => c.clone(),
            None => return Ok(None),
        };

        if self.audio_buffer.is_empty() {
            self.opus_worker.take();
            tracing::info!(
                "{}: no audio buffered, skipping",
                self.provider_config.provider_name
            );
            return Ok(None);
        }

        let pcm = std::mem::take(&mut self.audio_buffer);
        let audio_len_secs = pcm.len() as f64 / (config.sample_rate as f64 * 2.0);
        let encode_start = std::time::Instant::now();
        let mut opus_data = self.finish_opus().await;
        let mut wav_data = if opus_data.is_none() {
            Some(Self::build_wav(&pcm, config.sample_rate))
        } else {
            None
        };
        let mut fields =
            transcription_request_fields(&self.provider_config, &config, opus_data.is_some());
        tracing::info!(
            "{}: sending {:.1}s of audio as {} ({} bytes, encode {}ms, model {})",
            self.provider_config.provider_name,
            audio_len_secs,
            fields.mime_type,
            opus_data
                .as_ref()
                .or(wav_data.as_ref())
                .map(Vec::len)
                .unwrap_or(0),
            encode_start.elapsed().as_millis(),
            fields.model
        );

        if let (Some(shadow_model), Some(sink)) = (&config.shadow_model, &config.shadow_sink) {
            let mut shadow_config = config.clone();
            shadow_config.model_override = Some(shadow_model.clone());
            let shadow_fields = transcription_request_fields(
                &self.provider_config,
                &shadow_config,
                opus_data.is_some(),
            );
            let payload = opus_data
                .clone()
                .unwrap_or_else(|| Self::build_wav(&pcm, config.sample_rate));
            let (tx, rx) = tokio::sync::oneshot::channel();
            *sink.lock().unwrap_or_else(|e| e.into_inner()) = Some(rx);
            let client = self.client.clone();
            let endpoint = self.provider_config.endpoint.clone();
            let api_key = config.api_key.clone();
            let extra = self.provider_config.extra_fields.clone();
            let model = shadow_model.clone();
            tokio::spawn(async move {
                let started = std::time::Instant::now();
                let result = post_transcription(
                    &client,
                    &endpoint,
                    &api_key,
                    &shadow_fields,
                    payload,
                    &extra,
                )
                .await;
                let _ = tx.send(super::ShadowTranscript {
                    model,
                    text: result.as_ref().ok().cloned().filter(|t| !t.is_empty()),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    error: result.err(),
                });
            });
        }

        let mut attempt = 0u32;
        loop {
            let payload = match opus_data.as_ref() {
                Some(bytes) => bytes.clone(),
                None => wav_data
                    .get_or_insert_with(|| Self::build_wav(&pcm, config.sample_rate))
                    .clone(),
            };
            let file_part = reqwest::multipart::Part::bytes(payload)
                .file_name(fields.file_name)
                .mime_str(fields.mime_type)
                .map_err(|e| AppError::Config(e.to_string()))?;

            let mut form = reqwest::multipart::Form::new()
                .text("model", fields.model.clone())
                .part("file", file_part);

            if let Some(lang) = fields.language.clone() {
                form = form.text("language", lang);
            }
            if let Some(prompt) = fields.prompt.clone() {
                form = form.text("prompt", prompt);
            }

            // Provider-specific extra fields
            for (key, value) in &self.provider_config.extra_fields {
                form = form.text(key.clone(), value.clone());
            }

            let mut request = self
                .client
                .post(&self.provider_config.endpoint)
                .multipart(form)
                .timeout(std::time::Duration::from_secs(60));

            if !config.api_key.trim().is_empty() {
                request = request.header("Authorization", format!("Bearer {}", config.api_key));
            }

            let resp_result = request.send().await;

            match resp_result {
                Ok(resp) => {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();

                    if status.is_success() {
                        let v: serde_json::Value = serde_json::from_str(&body)
                            .map_err(|e| AppError::Config(e.to_string()))?;
                        let text = v["text"].as_str().unwrap_or("").trim().to_string();

                        tracing::info!(
                            "{} transcription: {} chars",
                            self.provider_config.provider_name,
                            text.len()
                        );

                        return Ok(if text.is_empty() { None } else { Some(text) });
                    } else if opus_data.is_some()
                        && is_unsupported_format_rejection(status.as_u16(), &body)
                    {
                        tracing::warn!(
                            "{}: endpoint rejected Ogg/Opus ({}); retrying once as WAV",
                            self.provider_config.provider_name,
                            status
                        );
                        opus_data = None;
                        fields =
                            transcription_request_fields(&self.provider_config, &config, false);
                        continue;
                    } else if status.as_u16() >= 500 && attempt < 2 {
                        let truncate_at = body
                            .char_indices()
                            .take_while(|&(i, _)| i < 200)
                            .last()
                            .map(|(i, c)| i + c.len_utf8())
                            .unwrap_or(body.len());
                        tracing::warn!(
                            "{} server error {} (attempt {}/3): {}",
                            self.provider_config.provider_name,
                            status,
                            attempt + 1,
                            &body[..truncate_at]
                        );
                        attempt += 1;
                        tokio::time::sleep(std::time::Duration::from_millis(
                            1000 * 2u64.pow(attempt - 1),
                        ))
                        .await;
                        continue;
                    } else {
                        // Truncate at a valid UTF-8 char boundary to avoid panic on multi-byte chars
                        let truncate_at = body
                            .char_indices()
                            .take_while(|&(i, _)| i < 200)
                            .last()
                            .map(|(i, c)| i + c.len_utf8())
                            .unwrap_or(body.len());
                        let sanitized = &body[..truncate_at];
                        tracing::error!(
                            "{} HTTP {}: {}",
                            self.provider_config.provider_name,
                            status,
                            sanitized
                        );
                        return Err(AppError::Api {
                            status: status.as_u16(),
                            body: sanitized.to_string(),
                        });
                    }
                }
                Err(e) if e.is_timeout() && attempt < 2 => {
                    tracing::warn!(
                        "{} timeout (attempt {}/3)",
                        self.provider_config.provider_name,
                        attempt + 1
                    );
                    attempt += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(
                        1000 * 2u64.pow(attempt - 1),
                    ))
                    .await;
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn name(&self) -> &str {
        &self.provider_config.provider_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connect_allows_empty_api_key_when_not_required() {
        let mut provider = WhisperCompatProvider::new(WhisperCompatConfig {
            provider_name: "custom-whisper".to_string(),
            endpoint: "http://localhost:8000/v1/audio/transcriptions".to_string(),
            model: "test-model".to_string(),
            extra_fields: vec![],
            api_key_required: false,
            accepts_ogg_opus: false,
            supports_prompt: true,
        });

        let result = provider
            .connect(&SttConfig {
                api_key: String::new(),
                language: None,
                smart_format: true,
                sample_rate: 16000,
                resource_id: None,
                operation_id: None,
                managed_audio: None,
                provider_region: None,
                ..SttConfig::default()
            })
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn recv_transcript_waits_for_file_based_provider() {
        let mut provider = WhisperCompatProvider::new(WhisperCompatConfig {
            provider_name: "test-whisper".to_string(),
            endpoint: "https://example.test/transcriptions".to_string(),
            model: "test-model".to_string(),
            extra_fields: vec![],
            api_key_required: true,
            accepts_ogg_opus: false,
            supports_prompt: true,
        });

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(20),
            provider.recv_transcript(),
        )
        .await;

        assert!(result.is_err());
    }
}

#[cfg(test)]
mod upload_tests {
    use super::*;

    fn provider(accepts_ogg_opus: bool, supports_prompt: bool) -> WhisperCompatConfig {
        WhisperCompatConfig {
            provider_name: "test".to_string(),
            endpoint: "https://example.invalid/v1/audio/transcriptions".to_string(),
            model: "whisper-1".to_string(),
            extra_fields: vec![],
            api_key_required: true,
            accepts_ogg_opus,
            supports_prompt,
        }
    }

    #[test]
    fn opus_only_when_provider_accepts_it() {
        assert!(should_upload_opus(UploadFormat::Auto, true));
        assert!(!should_upload_opus(UploadFormat::Auto, false));
        assert!(!should_upload_opus(UploadFormat::Wav, true));
        assert!(should_upload_opus(UploadFormat::Opus, true));
        assert!(!should_upload_opus(UploadFormat::Opus, false));
    }

    #[test]
    fn request_fields_use_override_model_language_and_prompt() {
        let config = SttConfig {
            language: Some("zh-tw".to_string()),
            prompt: Some("以下是繁體中文。".to_string()),
            model_override: Some("gpt-4o-mini-transcribe".to_string()),
            ..SttConfig::default()
        };
        let fields = transcription_request_fields(&provider(true, true), &config, true);
        assert_eq!(fields.model, "gpt-4o-mini-transcribe");
        assert_eq!(fields.language.as_deref(), Some("zh-tw"));
        assert_eq!(fields.prompt.as_deref(), Some("以下是繁體中文。"));
        assert_eq!(fields.file_name, "audio.ogg");
        assert_eq!(fields.mime_type, "audio/ogg");
    }

    #[test]
    fn request_fields_fall_back_to_provider_model_and_wav() {
        let config = SttConfig {
            language: Some("multi".to_string()),
            prompt: Some("hint".to_string()),
            model_override: Some("   ".to_string()),
            ..SttConfig::default()
        };
        let fields = transcription_request_fields(&provider(false, false), &config, false);
        assert_eq!(fields.model, "whisper-1");
        assert_eq!(fields.language, None);
        assert_eq!(fields.prompt, None, "prompt dropped when unsupported");
        assert_eq!(fields.file_name, "audio.wav");
    }

    #[test]
    fn format_rejections_are_detected_but_auth_errors_are_not() {
        assert!(is_unsupported_format_rejection(
            400,
            "Unsupported file format: ogg"
        ));
        assert!(is_unsupported_format_rejection(
            415,
            "could not decode audio"
        ));
        assert!(!is_unsupported_format_rejection(
            401,
            "invalid api key format"
        ));
        assert!(!is_unsupported_format_rejection(
            400,
            "Invalid language 'zh-tw'. Language parameter must be specified in ISO-639-1 format."
        ));
        assert!(!is_unsupported_format_rejection(500, "format"));
    }
}
