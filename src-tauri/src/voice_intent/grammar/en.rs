use super::{CommandMatch, SearchMatch};
use crate::voice_intent::normalize::{trim_command_payload, NormalizedUtterance};
use crate::voice_intent::SearchProvider;

pub(super) fn match_draft(view: &NormalizedUtterance<'_>) -> CommandMatch<String> {
    for prefix in [
        "reply with",
        "reply that",
        "reply saying",
        "respond that",
        "respond with",
        "respond saying",
        "tell him",
        "tell her",
        "tell them",
        "let him know",
        "let her know",
        "let them know",
        "say that",
        "compose",
        "draft",
        "write",
    ] {
        if !view.starts_with_prefix(prefix, true) {
            continue;
        }
        return view
            .payload_after_prefix(prefix)
            .map(CommandMatch::Matched)
            .unwrap_or(CommandMatch::MissingPayload);
    }
    CommandMatch::NoMatch
}

/// Positive imperatives that edit the selection in place. Every entry is
/// verb-first and matched on a token boundary, so questions ("is this
/// formal?") and comments ("this reads well") never match.
pub(super) const REWRITE_PREFIXES: &[&str] = &[
    "rewrite",
    "rephrase",
    "reword",
    "make this",
    "make it",
    "make the",
    "fix this",
    "fix the",
    "fix it",
    "fix grammar",
    "fix typos",
    "fix spelling",
    "format this",
    "format it",
    "turn this into",
    "turn it into",
    "shorten",
    "simplify",
    "expand this",
    "expand it",
    "elaborate on this",
    "tighten",
    "polish this",
    "polish it",
    "proofread",
    "clean this up",
    "clean it up",
    "clean up",
    "correct this",
    "correct the",
    "edit this",
    "improve this",
    "improve the",
    "change this to",
    "change it to",
    "change the tone",
    "convert this",
    "convert it",
    "put this in",
    "put it in",
    "add punctuation",
    "capitalize",
    "formalize",
    "paraphrase",
    "condense",
];

pub(super) const TRANSLATE_PREFIXES: &[&str] = &[
    "translate this to",
    "translate this into",
    "translate the selection to",
    "translate the selection into",
    "translate it to",
    "translate it into",
    "translate to",
    "translate into",
];

pub(super) fn matches_rewrite(view: &NormalizedUtterance<'_>) -> bool {
    REWRITE_PREFIXES
        .iter()
        .any(|prefix| view.starts_with_prefix(prefix, true))
}

pub(super) fn matches_translation(view: &NormalizedUtterance<'_>) -> bool {
    TRANSLATE_PREFIXES.iter().any(|prefix| {
        view.starts_with_prefix(prefix, true) && view.payload_after_prefix(prefix).is_some()
    })
}

pub(super) fn matches_informational(view: &NormalizedUtterance<'_>) -> bool {
    [
        "summarize this",
        "explain this",
        "compare this",
        "what ",
        "why ",
        "how ",
        "who ",
        "when ",
        "where ",
    ]
    .iter()
    .any(|prefix| view.match_text().starts_with(prefix))
}

pub(super) fn match_search(view: &NormalizedUtterance<'_>) -> CommandMatch<SearchMatch> {
    for command in ["search", "find"] {
        let Some(rest) = view.payload_after_prefix(command) else {
            if view.starts_with_prefix(command, true) {
                return CommandMatch::MissingPayload;
            }
            continue;
        };

        for (name, provider) in provider_names() {
            let normalized = rest.to_ascii_lowercase();
            let suffix = format!(" on {name}");
            if normalized.ends_with(&suffix) {
                let query_end = rest.len() - suffix.len();
                return trim_command_payload(&rest[..query_end])
                    .map(|query| {
                        CommandMatch::Matched(SearchMatch {
                            provider,
                            query: query.to_string(),
                        })
                    })
                    .unwrap_or(CommandMatch::MissingPayload);
            }

            if command == "search" {
                let prefix = format!("{name} for ");
                if normalized.starts_with(&prefix) {
                    return trim_command_payload(&rest[prefix.len()..])
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
