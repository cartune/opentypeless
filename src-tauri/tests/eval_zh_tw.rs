//! End-to-end evaluation against the real OpenAI API using synthesized
//! Taiwanese-Mandarin clips (scripts/gen-eval-clips.sh).
//!
//! Ignored by default: run with
//!   OPENAI_API_KEY=sk-... cargo test --manifest-path src-tauri/Cargo.toml --test eval_zh_tw -- --ignored --nocapture
//! Each clip costs a fraction of a cent.

use opentypeless_lib::app_detector::types::{
    BrowserAccessStatus, ContextFamily, ContextProfileSummary,
};
use opentypeless_lib::llm::post_process::{post_process_final_text, PostProcessOptions};
use opentypeless_lib::llm::{ChineseScript, DictionaryTerm, LlmConfig, PolishRequest};
use opentypeless_lib::stt::config::{build_known_whisper_config, normalize_stt_language};
use opentypeless_lib::stt::prompt::build_stt_prompt;
use opentypeless_lib::stt::whisper_compat::WhisperCompatProvider;
use opentypeless_lib::stt::{SttConfig, SttProvider, UploadFormat};
use opentypeless_lib::voice_intent::{VoiceIntent, VoiceIntentKind, VoiceOutputPlacement};

fn api_key() -> Option<String> {
    std::env::var("OPENAI_API_KEY")
        .ok()
        .or_else(|| std::env::var("OPENAI_API").ok())
        .filter(|k| !k.trim().is_empty())
}

fn fixture_pcm(name: &str) -> Option<Vec<u8>> {
    let path = format!(
        "{}/tests/fixtures/audio/{name}.wav",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = std::fs::read(path).ok()?;
    // 44-byte canonical WAV header written by afconvert; data chunk follows.
    let data_pos = bytes.windows(4).position(|w| w == b"data")? + 8;
    Some(bytes[data_pos..].to_vec())
}

fn fixtures_present() -> bool {
    ["list", "filler", "mixed", "abbr"]
        .iter()
        .all(|name| fixture_pcm(name).is_some())
}

async fn transcribe(model: &str, name: &str, key: &str) -> String {
    let pcm = fixture_pcm(name).expect("fixture checked by fixtures_present()");
    let provider_config = build_known_whisper_config("openai-whisper").unwrap();
    let mut provider = WhisperCompatProvider::new(provider_config);
    let language = normalize_stt_language("openai-whisper", model, "zh-TW");
    let config = SttConfig {
        api_key: key.to_string(),
        language,
        prompt: build_stt_prompt(
            Some("zh-TW"),
            &["Kubernetes".to_string(), "OKR".to_string()],
        ),
        model_override: Some(model.to_string()),
        upload_format: UploadFormat::Auto,
        ..SttConfig::default()
    };
    provider.connect(&config).await.expect("connect");
    for chunk in pcm.chunks(640) {
        provider.send_audio(chunk).await.expect("send");
    }
    provider
        .disconnect()
        .await
        .expect("transcribe")
        .unwrap_or_default()
}

fn has_simplified_only_chars(text: &str) -> bool {
    // Characters that only exist in Simplified Chinese; enough to catch a script slip.
    const SIMPLIFIED: &[char] = &[
        '们', '开', '标', '进', '问', '题', '预', '专', '务', '项', '时', '发',
    ];
    text.chars().any(|c| SIMPLIFIED.contains(&c))
}

#[tokio::test]
#[ignore]
async fn stt_eval_traditional_chinese_clips() {
    let Some(key) = api_key() else {
        eprintln!("OPENAI_API_KEY not set; skipping");
        return;
    };
    if !fixtures_present() {
        eprintln!("audio fixtures missing; run scripts/gen-eval-clips.sh first; skipping");
        return;
    }
    for model in ["gpt-4o-mini-transcribe", "whisper-1"] {
        let list = transcribe(model, "list", &key).await;
        println!("[{model}] list:   {list}");
        assert!(list.contains("第一") && list.contains("第二") && list.contains("第三"));
        assert!(
            !has_simplified_only_chars(&list),
            "{model} emitted Simplified chars: {list}"
        );

        let mixed = transcribe(model, "mixed", &key).await;
        println!("[{model}] mixed:  {mixed}");
        let lower = mixed.to_lowercase();
        assert!(lower.contains("api") && lower.contains("merge") && lower.contains("kubernetes"));

        let abbr = transcribe(model, "abbr", &key).await;
        println!("[{model}] abbr:   {abbr}");
        assert!(abbr.contains("OKR") && abbr.contains("KPI"));

        let filler = transcribe(model, "filler", &key).await;
        println!("[{model}] filler: {filler}");
        assert!(!filler.trim().is_empty());
    }
}

async fn polish(raw: &str, key: &str) -> String {
    let config = LlmConfig {
        provider: "openai".to_string(),
        api_key: key.to_string(),
        model: std::env::var("EVAL_LLM_MODEL").unwrap_or_else(|_| "gpt-4.1-mini".to_string()),
        base_url: "https://api.openai.com/v1".to_string(),
        max_tokens: 1024,
        temperature: 0.3,
    };
    let provider = opentypeless_lib::llm::create_provider("openai", None);
    let req = PolishRequest {
        raw_text: raw.to_string(),
        context: ContextProfileSummary {
            profile_id: "general.native".to_string(),
            family: ContextFamily::General,
            app_label: "General".to_string(),
            icon_key: "general".to_string(),
            override_id: None,
            browser_access_status: BrowserAccessStatus::NotApplicable,
            browser_target: None,
        },
        dictionary: vec![DictionaryTerm::word("Kubernetes")],
        correction_rules: vec![],
        chinese_script: ChineseScript::Traditional,
        polish_style: "clean".to_string(),
        mapped_scene_prompt: String::new(),
        active_scene_prompt: String::new(),
        polish_custom_prompt: String::new(),
        translate_enabled: false,
        target_lang: String::new(),
        selected_text: None,
        operation_id: None,
        voice_intent: VoiceIntent {
            kind: VoiceIntentKind::DictateInsert,
            placement: VoiceOutputPlacement::InsertAtCursor,
            confidence: 1.0,
            search_provider: None,
            payload: None,
            grammar_locale: None,
            fallback_reason: None,
        },
    };
    let response = provider.polish(&config, &req, None).await.expect("polish");
    post_process_final_text(
        &response.polished_text,
        &PostProcessOptions {
            chinese_script: ChineseScript::Traditional,
            ..Default::default()
        },
    )
}

#[tokio::test]
#[ignore]
async fn polish_eval_traditional_lists_fillers_and_latin_terms() {
    let Some(key) = api_key() else {
        eprintln!("OPENAI_API_KEY not set; skipping");
        return;
    };
    if !fixtures_present() {
        eprintln!("audio fixtures missing; run scripts/gen-eval-clips.sh first; skipping");
        return;
    }
    let list = polish(
        &transcribe("gpt-4o-mini-transcribe", "list", &key).await,
        &key,
    )
    .await;
    println!("polished list:\n{list}");
    assert!(
        list.contains("1.") && list.contains("2.") && list.contains("3."),
        "numbered list expected: {list}"
    );
    assert!(
        !has_simplified_only_chars(&list),
        "Simplified chars leaked: {list}"
    );

    let filler = polish(
        &transcribe("gpt-4o-mini-transcribe", "filler", &key).await,
        &key,
    )
    .await;
    println!("polished filler: {filler}");
    for tic in ["那個", "就是說", "對啊", "嗯"] {
        assert!(!filler.contains(tic), "filler {tic} survived: {filler}");
    }

    let mixed = polish(
        &transcribe("gpt-4o-mini-transcribe", "mixed", &key).await,
        &key,
    )
    .await;
    println!("polished mixed: {mixed}");
    for term in ["API", "PR", "merge", "standup", "Kubernetes", "deployment"] {
        assert!(
            mixed.to_lowercase().contains(&term.to_lowercase()),
            "{term} lost: {mixed}"
        );
    }
    assert!(
        !has_simplified_only_chars(&mixed),
        "Simplified chars leaked: {mixed}"
    );
}
