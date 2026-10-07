pub mod aliyun_qwen3_asr;
pub mod apple_speech;
pub mod assemblyai;
pub mod capabilities;
pub mod cloud;
pub mod config;
pub mod deepgram;
pub mod managed_audio;
pub mod prompt;
pub mod volcengine;
pub mod whisper_compat;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

use whisper_compat::{WhisperCompatConfig, WhisperCompatProvider};

/// How file-upload STT providers should package the recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UploadFormat {
    /// Ogg/Opus when the provider accepts it, WAV otherwise.
    #[default]
    Auto,
    Wav,
    Opus,
}

impl UploadFormat {
    pub fn from_config_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "wav" => Self::Wav,
            "opus" | "ogg" => Self::Opus,
            _ => Self::Auto,
        }
    }

    pub fn as_config_value(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Wav => "wav",
            Self::Opus => "opus",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttConfig {
    pub api_key: String,
    pub language: Option<String>,
    pub smart_format: bool,
    pub sample_rate: u32,
    pub resource_id: Option<String>,
    pub operation_id: Option<String>,
    pub managed_audio: Option<managed_audio::ManagedAudioEncodingConfig>,
    pub provider_region: Option<String>,
    /// Vocabulary / script hint for providers that accept a text prompt
    /// (Whisper-family `prompt`). Built by `prompt::build_stt_prompt`.
    pub prompt: Option<String>,
    /// Overrides the provider's default model (e.g. `gpt-4o-mini-transcribe`).
    pub model_override: Option<String>,
    pub upload_format: UploadFormat,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            language: None,
            smart_format: true,
            sample_rate: 16000,
            resource_id: None,
            operation_id: None,
            managed_audio: None,
            provider_region: None,
            prompt: None,
            model_override: None,
            upload_format: UploadFormat::Auto,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TranscriptEvent {
    Partial { text: String },
    Final { text: String, confidence: f32 },
    SpeechStarted,
    SpeechEnded,
    Error { message: String },
}

#[async_trait]
pub trait SttProvider: Send + Sync {
    async fn connect(&mut self, config: &SttConfig) -> Result<(), AppError>;
    async fn send_audio(&mut self, chunk: &[u8]) -> Result<(), AppError>;
    async fn recv_transcript(&mut self) -> Result<Option<TranscriptEvent>, AppError>;
    /// Disconnect and optionally return a final transcript (for file-based providers).
    async fn disconnect(&mut self) -> Result<Option<String>, AppError>;
    fn recording_limit_override_seconds(&self) -> Option<u32> {
        None
    }
    fn recording_limit_override_explanation_key(&self) -> Option<&'static str> {
        None
    }
    fn name(&self) -> &str;
}

/// Phrases Whisper-family models emit for silence or noise-only audio. They
/// come from subtitle/credits text in the training data and never from the
/// user, so a transcript that is essentially just one of them is dropped.
const KNOWN_STT_HALLUCINATIONS: &[&str] = &[
    "请不吝点赞",
    "請不吝點贊",
    "點贊 訂閱 轉發",
    "点赞 订阅 转发",
    "点赞订阅转发",
    "打赏支持明镜",
    "打賞支持明鏡",
    "明镜与点点栏目",
    "明鏡與點點欄目",
    "字幕由",
    "字幕提供",
    "谢谢观看",
    "謝謝觀看",
    "感谢观看",
    "感謝觀看",
    "请订阅",
    "請訂閱",
    "下期再见",
    "下期再見",
    "thank you for watching",
    "thanks for watching",
    "subtitles by",
    "subscribe to my channel",
    "please subscribe",
    "amara.org",
];

/// Returns true when the transcript is a known no-speech hallucination.
/// Only short transcripts (<= 40 chars) qualify so a real sentence that merely mentions
/// one of these phrases is kept.
pub fn is_known_hallucination(text: &str) -> bool {
    let normalized: String = text
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| !matches!(c, ',' | '，' | '。' | '.' | '!' | '！' | '、' | ' '))
        .collect();
    if normalized.is_empty() {
        return false;
    }
    if normalized.chars().count() > 40 {
        return false;
    }
    KNOWN_STT_HALLUCINATIONS.iter().any(|phrase| {
        let phrase: String = phrase
            .to_lowercase()
            .chars()
            .filter(|c| *c != ' ')
            .collect();
        normalized.contains(&phrase)
    })
}

pub fn create_provider(
    provider_name: &str,
    custom_whisper_config: Option<WhisperCompatConfig>,
    client: Option<reqwest::Client>,
) -> Result<Box<dyn SttProvider>, AppError> {
    match provider_name {
        "cloud" => {
            let api_base_url = crate::api_base_url();
            Ok(match client {
                Some(ref c) => Box::new(cloud::CloudSttProvider::with_client(
                    api_base_url,
                    c.clone(),
                )),
                None => Box::new(cloud::CloudSttProvider::new(api_base_url)),
            })
        }
        "assemblyai" => Ok(Box::new(assemblyai::AssemblyAiProvider::new())),
        "deepgram" => Ok(Box::new(deepgram::DeepgramProvider::new())),
        aliyun_qwen3_asr::ALIYUN_QWEN3_ASR_PROVIDER => {
            Ok(Box::new(aliyun_qwen3_asr::AliyunQwen3AsrProvider::new()))
        }
        apple_speech::APPLE_SPEECH_PROVIDER => {
            Ok(Box::new(apple_speech::AppleSpeechProvider::new()))
        }
        volcengine::VOLCENGINE_DOUBAO_PROVIDER => {
            Ok(Box::new(volcengine::VolcengineDoubaoProvider::new()))
        }
        config::CUSTOM_WHISPER_PROVIDER => {
            let wc = custom_whisper_config.ok_or_else(|| {
                AppError::Config("Local / Custom Whisper is missing base URL or model".to_string())
            })?;
            Ok(match client {
                Some(ref c) => Box::new(WhisperCompatProvider::with_client(wc, c.clone())),
                None => Box::new(WhisperCompatProvider::new(wc)),
            })
        }
        name => {
            // All Whisper-compatible providers share the same HTTP upload logic.
            // Config is centralised in config::build_known_whisper_config.
            let wc = config::build_known_whisper_config(name)
                .ok_or_else(|| AppError::Config(format!("Unknown STT provider: {}", name)))?;
            Ok(match client {
                Some(ref c) => Box::new(WhisperCompatProvider::with_client(wc, c.clone())),
                None => Box::new(WhisperCompatProvider::new(wc)),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_whisper_requires_explicit_config() {
        let result = create_provider(config::CUSTOM_WHISPER_PROVIDER, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn custom_whisper_uses_explicit_config() {
        let cfg = config::build_custom_whisper_config(
            "http://localhost:8000/v1",
            "Systran/faster-whisper-large-v3",
        )
        .unwrap();

        let provider = create_provider(config::CUSTOM_WHISPER_PROVIDER, Some(cfg), None).unwrap();
        assert_eq!(provider.name(), config::CUSTOM_WHISPER_PROVIDER);
    }

    #[test]
    fn creates_volcengine_doubao_realtime_provider() {
        let provider = create_provider("volcengine-doubao", None, None).unwrap();
        assert_eq!(provider.name(), "Volcengine Doubao Realtime ASR");
    }

    #[test]
    fn creates_aliyun_qwen3_realtime_provider() {
        let provider = create_provider("aliyun-qwen3-asr", None, None).unwrap();
        assert_eq!(provider.name(), "Aliyun Qwen3 Realtime ASR");
    }

    #[test]
    fn creates_apple_speech_builtin_local_provider() {
        let provider = create_provider("apple-speech", None, None).unwrap();
        assert_eq!(provider.name(), "Apple Speech");
    }

    #[test]
    fn unknown_stt_provider_returns_error() {
        let result = create_provider("not-a-provider", None, None);
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod hallucination_tests {
    use super::is_known_hallucination;

    #[test]
    fn detects_chinese_subtitle_credits() {
        assert!(is_known_hallucination(
            "请不吝点赞 订阅 转发 打赏支持明镜与点点栏目"
        ));
        assert!(is_known_hallucination(
            "請不吝點贊、訂閱、轉發、打賞支持明鏡與點點欄目"
        ));
        assert!(is_known_hallucination("字幕由 Amara.org 社区提供"));
        assert!(is_known_hallucination("謝謝觀看！"));
    }

    #[test]
    fn detects_english_credits() {
        assert!(is_known_hallucination("Thank you for watching."));
        assert!(is_known_hallucination("Thanks for watching!"));
    }

    #[test]
    fn keeps_real_speech() {
        assert!(!is_known_hallucination("我們明天開會討論預算"));
        assert!(!is_known_hallucination("Please send the report by Friday"));
        assert!(!is_known_hallucination(""));
        // Long real sentences that merely mention a phrase are kept.
        assert!(!is_known_hallucination(
            "影片最後記得提醒觀眾謝謝觀看，然後把贊助商的連結放在說明欄，再檢查一次字幕有沒有對齊時間軸，最後匯出一千零八十p的版本上傳"
        ));
    }
}
