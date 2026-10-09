use super::{CommandMatch, SearchMatch};
use crate::voice_intent::normalize::{trim_command_payload, NormalizedUtterance};
use crate::voice_intent::SearchProvider;

pub(super) fn match_draft(view: &NormalizedUtterance<'_>) -> CommandMatch<String> {
    for prefix in [
        "写一封",
        "帮我写",
        "回复说",
        "写个",
        "起草",
        "草拟",
        "帮我拟",
        "拟一封",
        "拟一份",
        "帮我回复",
        "回一封",
        "写一段",
        "帮我生成",
        "生成一段",
        "生成一封",
        // Reply-shaped requests: "tell them that…" is a draft of the reply.
        "帮我跟他说",
        "帮我跟她说",
        "帮我跟他们说",
        "帮我告诉他",
        "帮我告诉她",
        "帮我回他",
        "帮我回她",
        "帮我回应",
        "帮我回",
        "跟他们说",
        "跟对方说",
        "跟他说",
        "跟她说",
        "跟他讲",
        "跟她讲",
        "告诉他们",
        "告诉对方",
        "告诉他",
        "告诉她",
        "回复他",
        "回复她",
        "回复对方",
        "回他说",
        "回她说",
        "回他",
        "回她",
        "回应说",
        "回说",
    ] {
        if !view.starts_with_prefix(prefix, false) {
            continue;
        }
        return view
            .payload_after_prefix(prefix)
            .map(CommandMatch::Matched)
            .unwrap_or(CommandMatch::MissingPayload);
    }
    for prefix in [
        "写一份",
        "我想写一份",
        "我想写一封",
        "帮我写一封邮件",
        "帮我写一份邮件",
    ] {
        if !view.starts_with_prefix(prefix, false) {
            continue;
        }
        let Some(payload) = view.payload_after_prefix(prefix) else {
            return CommandMatch::MissingPayload;
        };
        if looks_like_draft_artifact(&payload) {
            return CommandMatch::Matched(payload);
        }
    }
    CommandMatch::NoMatch
}

fn looks_like_draft_artifact(payload: &str) -> bool {
    ["邮件", "封信", "消息", "通知", "回复", "邀请"]
        .iter()
        .any(|marker| payload.contains(marker))
}

/// Positive imperatives that edit the selection in place (see zh_hant.rs).
pub(super) const REWRITE_PREFIXES: &[&str] = &[
    "改写这段",
    "润色这段",
    "把这段写得",
    "精简这段",
    "扩写这段",
    "修正这段",
    "把这段改成",
    "改成",
    "改为",
    "改写",
    "改用",
    "改一下",
    "改错字",
    "帮我改",
    "请改",
    "请帮我改",
    "把它改",
    "把它变",
    "把这句改",
    "把这个改",
    "把这些改",
    "把选中",
    "润饰",
    "润色",
    "精简",
    "缩短",
    "简化",
    "扩写",
    "扩充",
    "修正",
    "校对",
    "校正",
    "订正",
    "重写",
    "重新写",
    "重新整理",
    "整理成",
    "整理一下",
    "整理这段",
    "换句话说",
    "换个说法",
    "写成",
    "弄成",
    "排成",
    "列成",
    "转成",
    "转换成",
    "加上标点",
    "补上标点",
    "加标点",
    "调整语气",
    "让它",
    "让这段",
    "让这句",
    "让这个",
    "这段改成",
    "这段要改",
    "帮这段",
    "帮它",
    "正式一点",
    "口语一点",
    "简短一点",
    "简洁一点",
    "精简一点",
    "更正式",
    "更简洁",
    "更口语",
    "更简短",
];

pub(super) const TRANSLATE_PREFIXES: &[&str] = &[
    "把这段翻译成",
    "翻译这段到",
    "将选中文字翻译成",
    "翻译这段成",
    "翻译成",
    "翻成",
    "翻译为",
    "翻为",
    "帮我翻成",
    "帮我翻译成",
    "把它翻成",
    "把它翻译成",
    "把这段翻成",
    "把这句翻成",
    "把这句翻译成",
    "译成",
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
        "总结这段",
        "解释这段",
        "比较这段",
        "这段是什么意思",
        "为什么",
        "怎么",
        "什么",
        "谁",
        "何时",
        "哪里",
    ]
    .iter()
    .any(|prefix| view.starts_with_prefix(prefix, false))
}

pub(super) fn match_search(view: &NormalizedUtterance<'_>) -> CommandMatch<SearchMatch> {
    match_search_with_verbs(view, &["搜索", "搜"])
}

fn match_search_with_verbs(
    view: &NormalizedUtterance<'_>,
    verbs: &[&str],
) -> CommandMatch<SearchMatch> {
    let text = view.match_text();
    for (name, provider) in provider_names() {
        for verb in verbs {
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
