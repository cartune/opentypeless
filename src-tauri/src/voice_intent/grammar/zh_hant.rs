use super::{CommandMatch, SearchMatch};
use crate::voice_intent::normalize::{trim_command_payload, NormalizedUtterance};
use crate::voice_intent::SearchProvider;

pub(super) fn match_draft(view: &NormalizedUtterance<'_>) -> CommandMatch<String> {
    for prefix in [
        "寫一封",
        "幫我寫",
        "回覆說",
        "寫個",
        "起草",
        "草擬",
        "幫我擬",
        "擬一封",
        "擬一份",
        "幫我回覆",
        "回一封",
        "寫一段",
        "寫一份",
        "幫我產生",
        "幫我生成",
        "生成一段",
        "生成一封",
        // Reply-shaped requests: "tell them that…" is a draft of the reply.
        "幫我跟他說",
        "幫我跟她說",
        "幫我跟他們說",
        "幫我告訴他",
        "幫我告訴她",
        "幫我回他",
        "幫我回她",
        "幫我回應",
        "幫我回",
        "跟他們說",
        "跟對方說",
        "跟他說",
        "跟她說",
        "跟他講",
        "跟她講",
        "告訴他們",
        "告訴對方",
        "告訴他",
        "告訴她",
        "回覆他",
        "回覆她",
        "回覆對方",
        "回他說",
        "回她說",
        "回他",
        "回她",
        "回應說",
        "回說",
    ] {
        if !view.starts_with_prefix(prefix, false) {
            continue;
        }
        return view
            .payload_after_prefix(prefix)
            .map(CommandMatch::Matched)
            .unwrap_or(CommandMatch::MissingPayload);
    }
    CommandMatch::NoMatch
}

/// Positive imperatives that edit the selection in place. Every entry is a
/// verb-first command; questions and comments about the text never start
/// with one of these.
pub(super) const REWRITE_PREFIXES: &[&str] = &[
    "改寫這段",
    "潤色這段",
    "把這段寫得",
    "精簡這段",
    "擴寫這段",
    "修正這段",
    "把這段改成",
    "改成",
    "改為",
    "改寫",
    "改用",
    "改一下",
    "改錯字",
    "幫我改",
    "請改",
    "請幫我改",
    "把它改",
    "把它變",
    "把這句改",
    "把這個改",
    "把這些改",
    "把選取",
    "潤飾",
    "潤色",
    "精簡",
    "縮短",
    "簡化",
    "擴寫",
    "擴充",
    "修正",
    "校對",
    "校正",
    "訂正",
    "重寫",
    "重新寫",
    "重新整理",
    "整理成",
    "整理一下",
    "整理這段",
    "換句話說",
    "換個說法",
    "寫成",
    "弄成",
    "排成",
    "列成",
    "轉成",
    "轉換成",
    "加上標點",
    "補上標點",
    "加標點",
    "調整語氣",
    "讓它",
    "讓這段",
    "讓這句",
    "讓這個",
    "這段改成",
    "這段要改",
    "幫這段",
    "幫它",
    "正式一點",
    "口語一點",
    "簡短一點",
    "簡潔一點",
    "精簡一點",
    "更正式",
    "更簡潔",
    "更口語",
    "更簡短",
];

pub(super) const TRANSLATE_PREFIXES: &[&str] = &[
    "把這段翻譯成",
    "翻譯這段到",
    "將選取文字翻譯成",
    "翻譯這段成",
    "翻譯成",
    "翻成",
    "翻譯為",
    "翻為",
    "幫我翻成",
    "幫我翻譯成",
    "把它翻成",
    "把它翻譯成",
    "把這段翻成",
    "把這句翻成",
    "把這句翻譯成",
    "譯成",
];

pub(super) fn matches_rewrite(view: &NormalizedUtterance<'_>) -> bool {
    REWRITE_PREFIXES
        .iter()
        .any(|prefix| view.starts_with_prefix(prefix, false))
}

pub(super) fn matches_translation(view: &NormalizedUtterance<'_>) -> bool {
    TRANSLATE_PREFIXES.iter().any(|prefix| {
        view.starts_with_prefix(prefix, false) && view.payload_after_prefix(prefix).is_some()
    })
}

pub(super) fn matches_informational(view: &NormalizedUtterance<'_>) -> bool {
    [
        "總結這段",
        "解釋這段",
        "比較這段",
        "這段是什麼意思",
        "為什麼",
        "怎麼",
        "什麼",
        "誰",
        "何時",
        "哪裡",
    ]
    .iter()
    .any(|prefix| view.starts_with_prefix(prefix, false))
}

pub(super) fn match_search(view: &NormalizedUtterance<'_>) -> CommandMatch<SearchMatch> {
    let text = view.match_text();
    for (name, provider) in provider_names() {
        for verb in ["搜尋", "搜"] {
            let leading = format!("在 {name} {verb}");
            if view.starts_with_prefix(&leading, false) {
                return view
                    .payload_after_prefix(&leading)
                    .map(|query| CommandMatch::Matched(SearchMatch { provider, query }))
                    .unwrap_or(CommandMatch::MissingPayload);
            }

            let start = format!("{verb} ");
            let suffix = format!(" 在 {name}");
            if text.starts_with(&start) && text.ends_with(&suffix) {
                let query_start = start.len();
                let query_end = text.len() - suffix.len();
                return view
                    .original_for_match_range(query_start, query_end)
                    .as_deref()
                    .and_then(trim_command_payload)
                    .map(|query| {
                        CommandMatch::Matched(SearchMatch {
                            provider,
                            query: query.to_string(),
                        })
                    })
                    .unwrap_or(CommandMatch::MissingPayload);
            }
        }
    }
    CommandMatch::NoMatch
}

fn provider_names() -> [(&'static str, SearchProvider); 4] {
    [
        ("google", SearchProvider::Google),
        ("youtube", SearchProvider::YouTube),
        ("amazon", SearchProvider::Amazon),
        ("github", SearchProvider::GitHub),
    ]
}
