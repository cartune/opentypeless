/// Shared STT provider configuration constants.
///
/// Eliminates the triple duplication of endpoint/model/extra_fields across:
/// - `stt::create_provider`
/// - `lib::test_stt_connection`
/// - `lib::bench_stt_connection`
///
use super::whisper_compat::WhisperCompatConfig;

pub const APPLE_SPEECH_PROVIDER: &str = "apple-speech";
pub const CUSTOM_WHISPER_PROVIDER: &str = "custom-whisper";
pub const CUSTOM_WHISPER_PRESET_SPEACHES: &str = "speaches";
pub const CUSTOM_WHISPER_PRESET_CUSTOM: &str = "custom";
pub const DEFAULT_CUSTOM_WHISPER_BASE_URL: &str = "http://localhost:8000/v1";
pub const DEFAULT_CUSTOM_WHISPER_MODEL: &str = "Systran/faster-whisper-large-v3";

/// Configuration for a Whisper-compatible STT provider.
#[allow(clippy::doc_lazy_continuation)]
pub struct SttProviderConfig {
    pub endpoint: &'static str,
    pub model: &'static str,
    pub extra_fields: &'static [(&'static str, &'static str)],
    /// The endpoint decodes Ogg/Opus uploads (verified for OpenAI and Groq).
    pub accepts_ogg_opus: bool,
    /// The endpoint honours a free-text `prompt` field.
    pub supports_prompt: bool,
}

/// Returns the endpoint, model name, and any extra form fields for a given
/// Whisper-compatible STT provider.
pub fn get_whisper_config(provider: &str) -> Option<SttProviderConfig> {
    match provider {
        "glm-asr" => Some(SttProviderConfig {
            endpoint: "https://open.bigmodel.cn/api/paas/v4/audio/transcriptions",
            model: "glm-asr-2512",
            extra_fields: &[("stream", "false")],
            accepts_ogg_opus: false,
            supports_prompt: false,
        }),
        "openai-whisper" => Some(SttProviderConfig {
            endpoint: "https://api.openai.com/v1/audio/transcriptions",
            model: "whisper-1",
            extra_fields: &[],
            accepts_ogg_opus: true,
            supports_prompt: true,
        }),
        "groq-whisper" => Some(SttProviderConfig {
            endpoint: "https://api.groq.com/openai/v1/audio/transcriptions",
            model: "whisper-large-v3-turbo",
            extra_fields: &[],
            accepts_ogg_opus: true,
            supports_prompt: true,
        }),
        "siliconflow" => Some(SttProviderConfig {
            endpoint: "https://api.siliconflow.cn/v1/audio/transcriptions",
            model: "FunAudioLLM/SenseVoiceSmall",
            extra_fields: &[],
            accepts_ogg_opus: false,
            supports_prompt: false,
        }),
        _ => None,
    }
}

pub fn normalize_custom_whisper_endpoint(base_url: &str) -> Result<String, String> {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return Err("Base URL is required for Local / Custom Whisper".to_string());
    }

    let mut parsed =
        url::Url::parse(trimmed).map_err(|_| "Base URL must be a valid URL".to_string())?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("Base URL must start with http:// or https://".to_string());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("Base URL must not include credentials".to_string());
    }
    if parsed.fragment().is_some() {
        return Err("Base URL must not include a fragment".to_string());
    }

    let normalized_path = parsed.path().trim_end_matches('/').to_string();
    if normalized_path.ends_with("/audio/transcriptions") {
        parsed.set_path(&normalized_path);
    } else {
        parsed.set_path(&format!("{normalized_path}/audio/transcriptions"));
    }

    Ok(parsed.to_string())
}

pub fn build_custom_whisper_config(
    base_url: &str,
    model: &str,
) -> Result<WhisperCompatConfig, String> {
    let model = model.trim();
    if model.is_empty() {
        return Err("Model is required for Local / Custom Whisper".to_string());
    }

    Ok(WhisperCompatConfig {
        provider_name: CUSTOM_WHISPER_PROVIDER.to_string(),
        endpoint: normalize_custom_whisper_endpoint(base_url)?,
        model: model.to_string(),
        extra_fields: vec![],
        api_key_required: false,
        accepts_ogg_opus: false,
        supports_prompt: true,
    })
}

pub fn build_known_whisper_config(provider: &str) -> Option<WhisperCompatConfig> {
    let cfg = get_whisper_config(provider)?;
    Some(WhisperCompatConfig {
        provider_name: provider.to_string(),
        endpoint: cfg.endpoint.to_string(),
        model: cfg.model.to_string(),
        extra_fields: cfg
            .extra_fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        api_key_required: true,
        accepts_ogg_opus: cfg.accepts_ogg_opus,
        supports_prompt: cfg.supports_prompt,
    })
}

/// OpenAI transcription models that accept regional codes such as `zh-tw`.
/// `whisper-1` only accepts ISO-639-1 (`zh`), verified against the live API.
pub fn stt_model_accepts_regional_language(model: &str) -> bool {
    model.trim().to_ascii_lowercase().starts_with("gpt-")
}

/// Reduce a language tag to its ISO-639-1 base (`zh-TW` -> `zh`).
fn base_language(language: &str) -> String {
    language
        .split(['-', '_'])
        .next()
        .unwrap_or(language)
        .to_ascii_lowercase()
}

/// Map the configured `stt_language` to what a provider/model combination
/// actually accepts. `None` means "let the provider auto-detect".
pub fn normalize_stt_language(provider: &str, model: &str, language: &str) -> Option<String> {
    let language = language.trim();
    if language.is_empty() || language.eq_ignore_ascii_case("multi") {
        return None;
    }
    let lower = language.to_ascii_lowercase();
    let is_regional = lower.contains('-') || lower.contains('_');
    Some(match provider {
        // Whisper-compatible HTTP uploads.
        "openai-whisper" => {
            if is_regional && !stt_model_accepts_regional_language(model) {
                base_language(&lower)
            } else {
                lower
            }
        }
        "groq-whisper" | "siliconflow" | "glm-asr" | CUSTOM_WHISPER_PROVIDER | "cloud" => {
            base_language(&lower)
        }
        // Streaming providers with their own tag conventions.
        "deepgram" | APPLE_SPEECH_PROVIDER => language.to_string(),
        "volcengine-doubao" => {
            if lower.starts_with("zh") {
                "zh-CN".to_string()
            } else {
                base_language(&lower)
            }
        }
        _ => base_language(&lower),
    })
}

pub fn stt_provider_requires_api_key(provider: &str) -> bool {
    !matches!(
        provider,
        "cloud" | CUSTOM_WHISPER_PROVIDER | APPLE_SPEECH_PROVIDER
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glm_asr_config() {
        let cfg = get_whisper_config("glm-asr").unwrap();
        assert!(cfg.endpoint.contains("bigmodel.cn"));
        assert_eq!(cfg.model, "glm-asr-2512");
        assert!(cfg.extra_fields.contains(&("stream", "false")));
    }

    #[test]
    fn test_openai_whisper_config() {
        let cfg = get_whisper_config("openai-whisper").unwrap();
        assert!(cfg.endpoint.contains("api.openai.com"));
        assert_eq!(cfg.model, "whisper-1");
        assert!(cfg.extra_fields.is_empty());
    }

    #[test]
    fn test_groq_whisper_config() {
        let cfg = get_whisper_config("groq-whisper").unwrap();
        assert!(cfg.endpoint.contains("api.groq.com"));
        assert_eq!(cfg.model, "whisper-large-v3-turbo");
        assert!(cfg.extra_fields.is_empty());
    }

    #[test]
    fn test_siliconflow_config() {
        let cfg = get_whisper_config("siliconflow").unwrap();
        assert!(cfg.endpoint.contains("siliconflow"));
        assert_eq!(cfg.model, "FunAudioLLM/SenseVoiceSmall");
        assert!(cfg.extra_fields.is_empty());
    }

    #[test]
    fn test_unknown_provider_returns_none() {
        assert!(get_whisper_config("unknown").is_none());
    }

    #[test]
    fn test_deepgram_not_in_whisper_config() {
        assert!(get_whisper_config("deepgram").is_none());
    }

    #[test]
    fn test_assemblyai_not_in_whisper_config() {
        assert!(get_whisper_config("assemblyai").is_none());
    }

    #[test]
    fn test_cloud_not_in_whisper_config() {
        assert!(get_whisper_config("cloud").is_none());
    }

    #[test]
    fn apple_speech_is_builtin_local_and_does_not_require_api_key() {
        assert!(!stt_provider_requires_api_key(APPLE_SPEECH_PROVIDER));
        assert!(get_whisper_config(APPLE_SPEECH_PROVIDER).is_none());
    }

    #[test]
    fn test_normalize_custom_whisper_base_url() {
        let endpoint = normalize_custom_whisper_endpoint("http://localhost:8000/v1").unwrap();
        assert_eq!(endpoint, "http://localhost:8000/v1/audio/transcriptions");
    }

    #[test]
    fn test_normalize_custom_whisper_full_endpoint() {
        let endpoint =
            normalize_custom_whisper_endpoint("http://localhost:8000/v1/audio/transcriptions")
                .unwrap();
        assert_eq!(endpoint, "http://localhost:8000/v1/audio/transcriptions");
    }

    #[test]
    fn custom_whisper_appends_transcription_path_before_query() {
        let endpoint =
            normalize_custom_whisper_endpoint("https://example.com/openai?api-version=2026-01-01")
                .unwrap();

        assert_eq!(
            endpoint,
            "https://example.com/openai/audio/transcriptions?api-version=2026-01-01"
        );
    }

    #[test]
    fn custom_whisper_preserves_query_on_full_endpoint() {
        let endpoint = normalize_custom_whisper_endpoint(
            "https://example.com/v1/audio/transcriptions?api-version=2026-01-01",
        )
        .unwrap();

        assert_eq!(
            endpoint,
            "https://example.com/v1/audio/transcriptions?api-version=2026-01-01"
        );
    }

    #[test]
    fn custom_whisper_rejects_embedded_credentials_and_fragments() {
        let credentials =
            normalize_custom_whisper_endpoint("https://user:secret@example.com/v1").unwrap_err();
        let fragment =
            normalize_custom_whisper_endpoint("https://example.com/v1#section").unwrap_err();

        assert!(credentials.contains("credentials"));
        assert!(fragment.contains("fragment"));
    }

    #[test]
    fn test_custom_whisper_rejects_empty_base_url() {
        let err = normalize_custom_whisper_endpoint("   ").unwrap_err();
        assert!(err.contains("Base URL is required"));
    }

    #[test]
    fn test_custom_whisper_rejects_non_http_url() {
        let err = normalize_custom_whisper_endpoint("file:///tmp/server").unwrap_err();
        assert!(err.contains("http://"));
    }

    #[test]
    fn test_build_custom_whisper_config() {
        let cfg = build_custom_whisper_config(
            "http://localhost:8000/v1",
            "Systran/faster-whisper-large-v3",
        )
        .unwrap();
        assert_eq!(cfg.provider_name, CUSTOM_WHISPER_PROVIDER);
        assert_eq!(
            cfg.endpoint,
            "http://localhost:8000/v1/audio/transcriptions"
        );
        assert_eq!(cfg.model, "Systran/faster-whisper-large-v3");
        assert!(!cfg.api_key_required);
    }

    #[test]
    fn test_build_custom_whisper_config_requires_model() {
        let err = build_custom_whisper_config("http://localhost:8000/v1", "  ").unwrap_err();
        assert!(err.contains("Model is required"));
    }
}

#[cfg(test)]
mod language_tests {
    use super::*;

    #[test]
    fn multi_and_empty_mean_auto_detect() {
        assert_eq!(
            normalize_stt_language("openai-whisper", "whisper-1", "multi"),
            None
        );
        assert_eq!(
            normalize_stt_language("openai-whisper", "whisper-1", ""),
            None
        );
    }

    #[test]
    fn whisper_1_only_gets_iso_639_1() {
        assert_eq!(
            normalize_stt_language("openai-whisper", "whisper-1", "zh-TW").as_deref(),
            Some("zh")
        );
        assert_eq!(
            normalize_stt_language("openai-whisper", "whisper-1", "en").as_deref(),
            Some("en")
        );
    }

    #[test]
    fn gpt_transcribe_models_keep_regional_codes() {
        for model in [
            "gpt-4o-mini-transcribe",
            "gpt-4o-transcribe",
            "gpt-transcribe",
        ] {
            assert_eq!(
                normalize_stt_language("openai-whisper", model, "zh-TW").as_deref(),
                Some("zh-tw"),
                "{model}"
            );
        }
    }

    #[test]
    fn other_whisper_compat_providers_use_base_language() {
        assert_eq!(
            normalize_stt_language("groq-whisper", "whisper-large-v3-turbo", "zh-TW").as_deref(),
            Some("zh")
        );
        assert_eq!(
            normalize_stt_language("custom-whisper", "large-v3", "zh-TW").as_deref(),
            Some("zh")
        );
    }

    #[test]
    fn streaming_providers_keep_or_remap_regional_codes() {
        assert_eq!(
            normalize_stt_language("deepgram", "nova-3", "zh-TW").as_deref(),
            Some("zh-TW")
        );
        assert_eq!(
            normalize_stt_language("apple-speech", "", "zh-TW").as_deref(),
            Some("zh-TW")
        );
        assert_eq!(
            normalize_stt_language("volcengine-doubao", "", "zh-TW").as_deref(),
            Some("zh-CN")
        );
    }
}
