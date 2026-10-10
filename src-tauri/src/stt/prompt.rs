//! Builds the free-text `prompt` sent to Whisper-family transcription models.
//!
//! The prompt biases recognition towards a script (Traditional vs Simplified
//! Chinese) and towards the user's dictionary terms. whisper-1 only reads the
//! last 224 tokens, so the hint goes first and the dictionary is truncated.

/// Rough budget for dictionary text. CJK characters cost more tokens than
/// Latin words in the Whisper tokenizer, so the caps differ.
const DICTIONARY_CJK_CHAR_BUDGET: usize = 100;
const DICTIONARY_LATIN_CHAR_BUDGET: usize = 400;

fn script_hint(language: Option<&str>) -> Option<&'static str> {
    let language = language?.trim().to_ascii_lowercase();
    match language.as_str() {
        "zh-tw" | "zh-hant" | "zh_hant" | "zh-hk" => {
            Some("以下是繁體中文的語音內容，請使用台灣慣用的繁體字。內容會中英夾雜，英文單字、縮寫與產品名稱請保持英文原文，不要翻成中文。")
        }
        "zh" | "zh-cn" | "zh-hans" | "zh_hans" => {
            Some("以下是中文的语音内容。内容会中英夹杂，英文单词、缩写与产品名称请保持英文原文。")
        }
        "ja" => Some("以下は日本語の音声です。"),
        _ => None,
    }
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3000..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF | 0xAC00..=0xD7AF)
}

fn join_dictionary(dictionary: &[String], cjk: bool) -> String {
    let separator = if cjk { "、" } else { ", " };
    let budget = if cjk {
        DICTIONARY_CJK_CHAR_BUDGET
    } else {
        DICTIONARY_LATIN_CHAR_BUDGET
    };
    let mut out = String::new();
    let mut used = 0usize;
    for word in dictionary {
        let word = word.trim();
        if word.is_empty() {
            continue;
        }
        let cost = word.chars().count() + separator.chars().count();
        if used + cost > budget {
            break;
        }
        if !out.is_empty() {
            out.push_str(separator);
        }
        out.push_str(word);
        used += cost;
    }
    out
}

/// Build the STT prompt from the configured language and dictionary words.
/// Returns `None` when there is nothing useful to send.
/// The language whose script hint the prompt should carry. `multi` (automatic
/// detection) says nothing about the script, so the UI language stands in:
/// a Traditional-Chinese user who leaves detection on still wants 繁體 output.
pub fn effective_prompt_language<'a>(
    stt_language: &'a str,
    ui_language: &'a str,
) -> Option<&'a str> {
    let stt = stt_language.trim();
    if !stt.is_empty() && !stt.eq_ignore_ascii_case("multi") && !stt.eq_ignore_ascii_case("auto") {
        return Some(stt);
    }
    let ui = ui_language.trim().to_ascii_lowercase();
    (ui.starts_with("zh") || ui.starts_with("ja")).then_some(ui_language.trim())
}

/// `language` parameter for the request: ISO 639-1 when the prompt language
/// is known (whisper and the gpt-4o transcribers both take `zh`), else none.
pub fn request_language_for(prompt_language: Option<&str>) -> Option<String> {
    let language = prompt_language?.trim().to_ascii_lowercase();
    if language.starts_with("zh") {
        Some("zh".to_string())
    } else if language.starts_with("ja") {
        Some("ja".to_string())
    } else if language.len() == 2 {
        Some(language)
    } else {
        None
    }
}

fn is_instruction_following_model(model: &str) -> bool {
    let model = model.trim().to_ascii_lowercase();
    model.starts_with("gpt-4o") || model.starts_with("gpt-transcribe")
}

fn instruction_hint(language: Option<&str>) -> Option<&'static str> {
    let language = language?.trim().to_ascii_lowercase();
    match language.as_str() {
        "zh-tw" | "zh-hant" | "zh_hant" | "zh-hk" => Some(
            "請逐字轉錄這段台灣國語語音，使用繁體中文與台灣用語。內容會中英夾雜，英文單字、縮寫與產品名稱保留英文原文，不要翻成中文。不要省略、合併或摘要任何句子，也不要加入沒有說出的內容。",
        ),
        "zh" | "zh-cn" | "zh-hans" | "zh_hans" => Some(
            "请逐字转录这段中文语音。内容会中英夹杂，英文单词、缩写与产品名称保留英文原文。不要省略、合并或摘要任何句子，也不要加入没有说出的内容。",
        ),
        "ja" => Some("この日本語の音声を一字一句そのまま書き起こしてください。省略や要約はしないでください。"),
        _ => None,
    }
}

/// Like `build_stt_prompt`, but the gpt-4o transcribers read the prompt as
/// an instruction rather than as sample text, and they tend to tidy speech
/// up (dropping clauses, switching script), so they get told not to.
pub fn build_stt_prompt_for_model(
    model: &str,
    language: Option<&str>,
    dictionary: &[String],
) -> Option<String> {
    if !is_instruction_following_model(model) {
        return build_stt_prompt(language, dictionary);
    }
    let hint = instruction_hint(language);
    let cjk = hint.is_some_and(|h| h.chars().any(is_cjk))
        || dictionary.iter().any(|w| w.chars().any(is_cjk));
    let words = join_dictionary(dictionary, cjk);
    let mut prompt = String::new();
    if let Some(hint) = hint {
        prompt.push_str(hint);
    }
    if !words.is_empty() {
        if !prompt.is_empty() {
            prompt.push(' ');
        }
        if cjk {
            prompt.push_str("專有名詞與常用詞彙：");
            prompt.push_str(&words);
            prompt.push('。');
        } else {
            prompt.push_str("Proper nouns and vocabulary: ");
            prompt.push_str(&words);
            prompt.push('.');
        }
    }
    (!prompt.is_empty()).then_some(prompt)
}

pub fn build_stt_prompt(language: Option<&str>, dictionary: &[String]) -> Option<String> {
    let hint = script_hint(language);
    let cjk = hint.is_some_and(|h| h.chars().any(is_cjk))
        || dictionary.iter().any(|w| w.chars().any(is_cjk));
    let words = join_dictionary(dictionary, cjk);

    let mut prompt = String::new();
    if let Some(hint) = hint {
        prompt.push_str(hint);
    }
    if !words.is_empty() {
        if !prompt.is_empty() {
            prompt.push(' ');
        }
        if cjk {
            prompt.push_str("常用詞彙：");
            prompt.push_str(&words);
            prompt.push('。');
        } else {
            prompt.push_str("Vocabulary: ");
            prompt.push_str(&words);
            prompt.push('.');
        }
    }
    (!prompt.is_empty()).then_some(prompt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn automatic_detection_falls_back_to_the_ui_language_for_the_script() {
        assert_eq!(effective_prompt_language("multi", "zh-TW"), Some("zh-TW"));
        assert_eq!(effective_prompt_language("multi", "en"), None);
        assert_eq!(effective_prompt_language("zh-TW", "en"), Some("zh-TW"));
        assert_eq!(request_language_for(Some("zh-TW")), Some("zh".to_string()));
        assert_eq!(request_language_for(Some("en")), Some("en".to_string()));
        assert_eq!(request_language_for(None), None);
    }

    #[test]
    fn gpt_4o_transcribers_get_an_instruction_not_sample_text() {
        let prompt =
            build_stt_prompt_for_model("gpt-4o-transcribe", Some("zh-TW"), &words(&["Cartune"]))
                .unwrap();
        assert!(prompt.starts_with("請逐字轉錄"));
        assert!(prompt.contains("不要省略"));
        assert!(prompt.contains("Cartune"));
        let whisper = build_stt_prompt_for_model("whisper-1", Some("zh-TW"), &words(&["Cartune"]));
        assert_eq!(
            whisper,
            build_stt_prompt(Some("zh-TW"), &words(&["Cartune"]))
        );
    }

    #[test]
    fn empty_inputs_give_no_prompt() {
        assert_eq!(build_stt_prompt(None, &[]), None);
        assert_eq!(build_stt_prompt(Some("en"), &[]), None);
        assert_eq!(build_stt_prompt(Some("multi"), &[]), None);
    }

    #[test]
    fn traditional_chinese_gets_a_script_hint_first() {
        let prompt = build_stt_prompt(Some("zh-TW"), &words(&["Cartune", "GIGAPRESS"])).unwrap();
        assert!(prompt.starts_with("以下是繁體中文"));
        assert!(prompt.contains("Cartune、GIGAPRESS"));
    }

    #[test]
    fn latin_dictionary_uses_comma_separator() {
        let prompt = build_stt_prompt(Some("en"), &words(&["Kubernetes", "OKR"])).unwrap();
        assert_eq!(prompt, "Vocabulary: Kubernetes, OKR.");
    }

    #[test]
    fn dictionary_is_truncated_but_hint_is_kept() {
        let many: Vec<String> = (0..200).map(|i| format!("詞彙{i}")).collect();
        let prompt = build_stt_prompt(Some("zh-TW"), &many).unwrap();
        assert!(prompt.starts_with("以下是繁體中文"));
        let dict_part = prompt.split("常用詞彙：").nth(1).unwrap();
        assert!(dict_part.chars().count() <= DICTIONARY_CJK_CHAR_BUDGET + 2);
        assert!(prompt.contains("詞彙0"));
        assert!(!prompt.contains("詞彙199"));
    }

    #[test]
    fn blank_words_are_skipped() {
        let prompt = build_stt_prompt(None, &words(&["  ", "Alpha", ""])).unwrap();
        assert_eq!(prompt, "Vocabulary: Alpha.");
    }
}
