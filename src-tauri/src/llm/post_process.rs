//! Deterministic post-processing applied to the final polished text right
//! before it is typed into the target application. The capsule still shows
//! the streamed preview; only the single final insertion goes through here.
//!
//! Order: script conversion first (so users can write correction rules in
//! the script they read), then exact correction-rule replacement.

use super::{ChineseScript, CorrectionRule};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PostProcessOptions {
    pub chinese_script: ChineseScript,
    pub correction_rules: Vec<CorrectionRule>,
    /// Apply enabled correction rules as literal replacements.
    pub apply_correction_rules: bool,
}

// zhhz converters keep internal scratch buffers (not Sync). Building one
// takes seconds (the OpenCC tables are large), and a per-thread cache meant
// every tokio worker paid that price on its first dictation: the first run
// after launch averaged 3 s of output time. One shared converter behind a
// mutex, built once at startup (`warm_up`), keeps conversion to microseconds.
use std::sync::{LazyLock, Mutex};

static S2TWP: LazyLock<Mutex<zhhz::Converter>> =
    LazyLock::new(|| Mutex::new(zhhz::Converter::new(zhhz::Config::S2twp)));
static TW2SP: LazyLock<Mutex<zhhz::Converter>> =
    LazyLock::new(|| Mutex::new(zhhz::Converter::new(zhhz::Config::Tw2sp)));

/// Build the converters now (call once from a background thread at startup).
pub fn warm_up() {
    let started = std::time::Instant::now();
    let _ = to_traditional("預熱");
    let _ = to_simplified("预热");
    tracing::info!(
        "Script converters ready in {}ms",
        started.elapsed().as_millis()
    );
}

/// Simplified -> Traditional (Taiwan phrases). Latin text passes through.
pub fn to_traditional(text: &str) -> String {
    S2TWP
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .convert(text)
}

/// Traditional (Taiwan) -> Simplified. Latin text passes through.
pub fn to_simplified(text: &str) -> String {
    TW2SP
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .convert(text)
}

fn replace_ascii_case_insensitive(text: &str, pattern: &str, replacement: &str) -> String {
    let lower_text = text.to_ascii_lowercase();
    let lower_pattern = pattern.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut search = 0;
    while let Some(pos) = lower_text[search..].find(&lower_pattern) {
        let start = search + pos;
        let end = start + pattern.len();
        out.push_str(&text[last..start]);
        out.push_str(replacement);
        last = end;
        search = end;
        if lower_pattern.is_empty() {
            break;
        }
    }
    out.push_str(&text[last..]);
    out
}

/// Apply enabled correction rules literally. Longer patterns win so that a
/// rule for "open type less" is applied before a rule for "type less".
pub fn apply_correction_rules(text: &str, rules: &[CorrectionRule]) -> String {
    let mut ordered: Vec<&CorrectionRule> = rules
        .iter()
        .filter(|rule| rule.enabled && !rule.pattern.trim().is_empty())
        .collect();
    ordered.sort_by_key(|rule| std::cmp::Reverse(rule.pattern.chars().count()));

    let mut out = text.to_string();
    for rule in ordered {
        let pattern = rule.pattern.trim();
        let replacement = rule.replacement.trim();
        if pattern == replacement {
            continue;
        }
        if pattern.is_ascii() {
            out = replace_ascii_case_insensitive(&out, pattern, replacement);
        } else {
            out = out.replace(pattern, replacement);
        }
    }
    out
}

pub fn post_process_final_text(text: &str, options: &PostProcessOptions) -> String {
    let mut out = match options.chinese_script {
        ChineseScript::Traditional => to_traditional(text),
        ChineseScript::Simplified => to_simplified(text),
        ChineseScript::Preserve => text.to_string(),
    };
    if options.apply_correction_rules && !options.correction_rules.is_empty() {
        out = apply_correction_rules(&out, &options.correction_rules);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(pattern: &str, replacement: &str, enabled: bool) -> CorrectionRule {
        CorrectionRule {
            id: 0,
            pattern: pattern.to_string(),
            replacement: replacement.to_string(),
            enabled,
        }
    }

    #[test]
    fn preserve_is_identity() {
        let input = "第一，買牛奶\n第二，洗衣服";
        assert_eq!(
            post_process_final_text(input, &PostProcessOptions::default()),
            input
        );
    }

    #[test]
    fn traditional_converts_simplified_with_taiwan_phrases() {
        let options = PostProcessOptions {
            chinese_script: ChineseScript::Traditional,
            ..Default::default()
        };
        assert_eq!(
            post_process_final_text("我们把软件的信息发到网络上", &options),
            "我們把軟體的資訊發到網路上"
        );
        // Latin terms and already-Traditional text are untouched.
        assert_eq!(
            post_process_final_text("我們把 API 的 PR 先 merge", &options),
            "我們把 API 的 PR 先 merge"
        );
    }

    #[test]
    fn simplified_converts_traditional() {
        let options = PostProcessOptions {
            chinese_script: ChineseScript::Simplified,
            ..Default::default()
        };
        assert_eq!(
            post_process_final_text("軟體與資料", &options),
            "软件与数据"
        );
    }

    #[test]
    fn correction_rules_apply_literally_longest_first() {
        let rules = vec![
            rule("type less", "Typeless", true),
            rule("open type less", "OpenTypeless", true),
            rule("disabled", "nope", false),
            rule("卡通", "Cartune", true),
        ];
        assert_eq!(
            apply_correction_rules("we use Open Type Less and 卡通 daily, disabled", &rules),
            "we use OpenTypeless and Cartune daily, disabled"
        );
    }

    #[test]
    fn correction_rules_run_after_script_conversion() {
        let options = PostProcessOptions {
            chinese_script: ChineseScript::Traditional,
            correction_rules: vec![rule("資料庫", "DB", true)],
            apply_correction_rules: true,
        };
        assert_eq!(
            post_process_final_text("请检查数据库", &options),
            "請檢查DB"
        );
    }

    #[test]
    fn correction_rules_can_be_disabled_globally() {
        let options = PostProcessOptions {
            chinese_script: ChineseScript::Preserve,
            correction_rules: vec![rule("a", "b", true)],
            apply_correction_rules: false,
        };
        assert_eq!(post_process_final_text("a", &options), "a");
    }
}
