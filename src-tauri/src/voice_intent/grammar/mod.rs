mod en;
mod zh_hans;
mod zh_hant;

use super::normalize::NormalizedUtterance;
use super::{CommandLocale, SearchProvider};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CommandMatch<T> {
    NoMatch,
    MissingPayload,
    Matched(T),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchMatch {
    pub provider: SearchProvider,
    pub query: String,
}

pub(crate) fn match_draft(
    locale: CommandLocale,
    view: &NormalizedUtterance<'_>,
) -> CommandMatch<String> {
    match locale {
        CommandLocale::En => en::match_draft(view),
        CommandLocale::ZhHans => zh_hans::match_draft(view),
        CommandLocale::ZhHant => zh_hant::match_draft(view),
    }
}

/// The reply prefixes of a locale, longest first, for the command-signal and
/// reported-speech guards.
pub(crate) fn reply_prefixes(locale: CommandLocale) -> &'static [&'static str] {
    match locale {
        CommandLocale::En => en::REPLY_PREFIXES,
        CommandLocale::ZhHans => zh_hans::REPLY_PREFIXES,
        CommandLocale::ZhHant => zh_hant::REPLY_PREFIXES,
    }
}

/// Reply-shaped drafts ("跟他說…", "tell them…"). Ask routes only.
pub(crate) fn match_reply(
    locale: CommandLocale,
    view: &NormalizedUtterance<'_>,
) -> CommandMatch<String> {
    match locale {
        CommandLocale::En => en::match_reply(view),
        CommandLocale::ZhHans => zh_hans::match_reply(view),
        CommandLocale::ZhHant => zh_hant::match_reply(view),
    }
}

pub(crate) fn matches_rewrite(locale: CommandLocale, view: &NormalizedUtterance<'_>) -> bool {
    match locale {
        CommandLocale::En => en::matches_rewrite(view),
        CommandLocale::ZhHans => zh_hans::matches_rewrite(view),
        CommandLocale::ZhHant => zh_hant::matches_rewrite(view),
    }
}

pub(crate) fn matches_translation(locale: CommandLocale, view: &NormalizedUtterance<'_>) -> bool {
    match locale {
        CommandLocale::En => en::matches_translation(view),
        CommandLocale::ZhHans => zh_hans::matches_translation(view),
        CommandLocale::ZhHant => zh_hant::matches_translation(view),
    }
}

pub(crate) fn matches_informational(locale: CommandLocale, view: &NormalizedUtterance<'_>) -> bool {
    match locale {
        CommandLocale::En => en::matches_informational(view),
        CommandLocale::ZhHans => zh_hans::matches_informational(view),
        CommandLocale::ZhHant => zh_hant::matches_informational(view),
    }
}

pub(crate) fn match_search(
    locale: CommandLocale,
    view: &NormalizedUtterance<'_>,
) -> CommandMatch<SearchMatch> {
    match locale {
        CommandLocale::En => en::match_search(view),
        CommandLocale::ZhHans => zh_hans::match_search(view),
        CommandLocale::ZhHant => zh_hant::match_search(view),
    }
}

/// True when the utterance reads as a question rather than an instruction:
/// it ends with a question mark or a sentence-final question particle.
/// Used by Ask command mode to keep questions on the nondestructive popup.
pub(crate) fn is_question_shaped(view: &NormalizedUtterance<'_>) -> bool {
    let text = view
        .match_text()
        .trim_end_matches(['.', '。', '!', '！', ' ']);
    if text.ends_with('?') || text.ends_with('？') {
        return true;
    }
    ["嗎", "吗", "呢", "麼", "么"]
        .iter()
        .any(|particle| text.ends_with(particle))
}

pub(crate) fn exact_confidence(view: &NormalizedUtterance<'_>) -> f32 {
    if view
        .match_text()
        .chars()
        .last()
        .is_some_and(|character| matches!(character, '.' | '。' | '!' | '！' | '?' | '？'))
    {
        0.9
    } else {
        1.0
    }
}
