pub mod executor;
pub mod grammar;
mod guards;
mod normalize;
pub mod search;
pub mod types;

pub use types::*;

use grammar::{CommandMatch, SearchMatch};
use guards::{guard_reason, has_any_supported_command_signal, has_command_signal};
use normalize::NormalizedUtterance;

pub struct VoiceIntentRouter;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceProviderWorkPlan {
    pub provider_call_limit: u8,
    pub provider_input: Option<String>,
    pub allow_streaming: bool,
    pub restore_target_before_insert: bool,
}

pub fn plan_voice_provider_work(
    mode: VoiceMode,
    utterance: &str,
    intent: &VoiceIntent,
) -> VoiceProviderWorkPlan {
    let provider_input = match intent.kind {
        VoiceIntentKind::Search => None,
        VoiceIntentKind::DraftInsert => intent.payload.clone(),
        _ => Some(utterance.to_string()),
    };

    VoiceProviderWorkPlan {
        provider_call_limit: u8::from(provider_input.is_some()),
        provider_input,
        allow_streaming: mode == VoiceMode::Dictate
            && intent.kind == VoiceIntentKind::DictateInsert,
        restore_target_before_insert: mode == VoiceMode::Ask
            && intent.kind == VoiceIntentKind::DraftInsert,
    }
}

enum LocaleResolution {
    Supported(CommandLocale),
    Unsupported,
    Ambiguous,
    NoCommand,
}

impl VoiceIntentRouter {
    pub fn route(request: VoiceRouteRequest<'_>) -> VoiceIntent {
        if request.mode == VoiceMode::Translate {
            return route_translate_mode(&request);
        }
        if request.utterance.trim().is_empty() {
            return fallback_intent(
                request.mode,
                request.has_selected_text,
                None,
                Some(RouteFallbackReason::MissingPayload),
            );
        }

        let locale = match resolve_locale(request.speech_language, request.utterance) {
            LocaleResolution::Supported(locale) => locale,
            LocaleResolution::Unsupported => {
                return fallback_intent(
                    request.mode,
                    request.has_selected_text,
                    None,
                    Some(RouteFallbackReason::UnsupportedLocale),
                )
            }
            LocaleResolution::Ambiguous => {
                return fallback_intent(
                    request.mode,
                    request.has_selected_text,
                    None,
                    Some(RouteFallbackReason::Ambiguous),
                )
            }
            LocaleResolution::NoCommand => {
                return fallback_intent(request.mode, request.has_selected_text, None, None)
            }
        };
        let view = NormalizedUtterance::new(request.utterance);

        if has_command_signal(locale, request.utterance) {
            if let Some(reason) = guard_reason(locale, request.utterance) {
                return fallback_intent(
                    request.mode,
                    request.has_selected_text,
                    Some(locale),
                    Some(reason.into()),
                );
            }
        }

        match request.mode {
            VoiceMode::Dictate => route_dictate(request, locale, &view),
            VoiceMode::Ask => route_ask(request, locale, &view),
            VoiceMode::Translate => unreachable!("translate mode returned before grammar routing"),
        }
    }
}

fn route_dictate(
    request: VoiceRouteRequest<'_>,
    locale: CommandLocale,
    view: &NormalizedUtterance<'_>,
) -> VoiceIntent {
    if !request.has_selected_text {
        return match grammar::match_draft(locale, view) {
            CommandMatch::Matched(payload) if request.flags.draft_insert => intent(
                VoiceIntentKind::DraftInsert,
                VoiceOutputPlacement::InsertAtCursor,
                grammar::exact_confidence(view),
                None,
                Some(payload),
                Some(locale),
                None,
            ),
            CommandMatch::Matched(_) => fallback_intent(
                request.mode,
                false,
                Some(locale),
                Some(RouteFallbackReason::FeatureDisabled),
            ),
            CommandMatch::MissingPayload => fallback_intent(
                request.mode,
                false,
                Some(locale),
                Some(RouteFallbackReason::MissingPayload),
            ),
            CommandMatch::NoMatch => fallback_intent(
                request.mode,
                false,
                Some(locale),
                discussed_command_reason(locale, request.utterance),
            ),
        };
    }

    if grammar::matches_translation(locale, view) {
        if !request.flags.translate_selection {
            return fallback_intent(
                request.mode,
                true,
                Some(locale),
                Some(RouteFallbackReason::FeatureDisabled),
            );
        }
        return intent(
            VoiceIntentKind::TranslateSelection,
            VoiceOutputPlacement::ReplaceSelection,
            grammar::exact_confidence(view),
            None,
            None,
            Some(locale),
            None,
        );
    }
    if grammar::matches_rewrite(locale, view) {
        if !request.flags.rewrite_selection {
            return fallback_intent(
                request.mode,
                true,
                Some(locale),
                Some(RouteFallbackReason::FeatureDisabled),
            );
        }
        return intent(
            VoiceIntentKind::RewriteSelection,
            VoiceOutputPlacement::ReplaceSelection,
            grammar::exact_confidence(view),
            None,
            None,
            Some(locale),
            None,
        );
    }
    if grammar::matches_informational(locale, view) {
        return fallback_intent(request.mode, true, Some(locale), None);
    }

    fallback_intent(
        request.mode,
        true,
        Some(locale),
        discussed_command_reason(locale, request.utterance),
    )
}

fn route_ask(
    request: VoiceRouteRequest<'_>,
    locale: CommandLocale,
    view: &NormalizedUtterance<'_>,
) -> VoiceIntent {
    if request.has_selected_text {
        return route_ask_with_selection(request, locale, view);
    }

    match grammar::match_search(locale, view) {
        CommandMatch::Matched(SearchMatch { provider, query }) if request.flags.search => {
            return intent(
                VoiceIntentKind::Search,
                VoiceOutputPlacement::OpenUrl,
                grammar::exact_confidence(view),
                Some(provider),
                Some(query),
                Some(locale),
                None,
            )
        }
        CommandMatch::Matched(_) => {
            return fallback_intent(
                VoiceMode::Ask,
                false,
                Some(locale),
                Some(RouteFallbackReason::FeatureDisabled),
            )
        }
        CommandMatch::MissingPayload => {
            return fallback_intent(
                VoiceMode::Ask,
                false,
                Some(locale),
                Some(RouteFallbackReason::MissingPayload),
            )
        }
        CommandMatch::NoMatch => {}
    }

    match grammar::match_draft(locale, view) {
        CommandMatch::Matched(payload) if request.flags.draft_insert => intent(
            VoiceIntentKind::DraftInsert,
            VoiceOutputPlacement::InsertAtCursor,
            grammar::exact_confidence(view),
            None,
            Some(payload),
            Some(locale),
            None,
        ),
        CommandMatch::Matched(_) => fallback_intent(
            VoiceMode::Ask,
            false,
            Some(locale),
            Some(RouteFallbackReason::FeatureDisabled),
        ),
        CommandMatch::MissingPayload => fallback_intent(
            VoiceMode::Ask,
            false,
            Some(locale),
            Some(RouteFallbackReason::MissingPayload),
        ),
        CommandMatch::NoMatch => fallback_intent(
            VoiceMode::Ask,
            false,
            Some(locale),
            discussed_command_reason(locale, request.utterance),
        ),
    }
}

/// Ask shortcut with text selected. Only a positive imperative that the
/// grammar recognises becomes a destructive in-place rewrite or translation;
/// questions, comments, and unrecognised phrasing stay on the popup route.
fn route_ask_with_selection(
    request: VoiceRouteRequest<'_>,
    locale: CommandLocale,
    view: &NormalizedUtterance<'_>,
) -> VoiceIntent {
    let nondestructive = || {
        fallback_intent(
            VoiceMode::Ask,
            true,
            Some(locale),
            discussed_command_reason(locale, request.utterance),
        )
    };
    if grammar::is_question_shaped(view) || grammar::matches_informational(locale, view) {
        return nondestructive();
    }

    let translation = grammar::matches_translation(locale, view);
    let rewrite = !translation && grammar::matches_rewrite(locale, view);
    if !translation && !rewrite {
        // "跟他說…" / "tell them…" with a message selected: draft the reply at
        // the cursor, using the selection as context. Nothing is replaced.
        if let CommandMatch::Matched(payload) = grammar::match_draft(locale, view) {
            if request.flags.draft_insert {
                return intent(
                    VoiceIntentKind::DraftInsert,
                    VoiceOutputPlacement::InsertAtCursor,
                    grammar::exact_confidence(view),
                    None,
                    Some(payload),
                    Some(locale),
                    None,
                );
            }
            return fallback_intent(
                VoiceMode::Ask,
                true,
                Some(locale),
                Some(RouteFallbackReason::FeatureDisabled),
            );
        }
        return nondestructive();
    }
    if !request.flags.command_mode
        || (translation && !request.flags.translate_selection)
        || (rewrite && !request.flags.rewrite_selection)
    {
        return fallback_intent(
            VoiceMode::Ask,
            true,
            Some(locale),
            Some(RouteFallbackReason::FeatureDisabled),
        );
    }
    let kind = if translation {
        VoiceIntentKind::TranslateSelection
    } else {
        VoiceIntentKind::RewriteSelection
    };
    intent(
        kind,
        VoiceOutputPlacement::ReplaceSelection,
        grammar::exact_confidence(view),
        None,
        None,
        Some(locale),
        None,
    )
}

fn route_translate_mode(request: &VoiceRouteRequest<'_>) -> VoiceIntent {
    if request.has_selected_text {
        if !request.flags.translate_selection {
            return fallback_intent(
                VoiceMode::Translate,
                true,
                None,
                Some(RouteFallbackReason::FeatureDisabled),
            );
        }
        return intent(
            VoiceIntentKind::TranslateSelection,
            VoiceOutputPlacement::ReplaceSelection,
            1.0,
            None,
            None,
            None,
            None,
        );
    }
    intent(
        VoiceIntentKind::TranslateInsert,
        VoiceOutputPlacement::InsertAtCursor,
        1.0,
        None,
        None,
        None,
        None,
    )
}

fn fallback_intent(
    mode: VoiceMode,
    has_selected_text: bool,
    locale: Option<CommandLocale>,
    reason: Option<RouteFallbackReason>,
) -> VoiceIntent {
    let (kind, placement) = match (mode, has_selected_text) {
        (VoiceMode::Dictate, false) => (
            VoiceIntentKind::DictateInsert,
            VoiceOutputPlacement::InsertAtCursor,
        ),
        (VoiceMode::Ask, false) => (
            VoiceIntentKind::OpenQuestion,
            VoiceOutputPlacement::PopupAnswer,
        ),
        (_, true) => (
            VoiceIntentKind::AskSelection,
            VoiceOutputPlacement::PopupAnswer,
        ),
        (VoiceMode::Translate, false) => (
            VoiceIntentKind::TranslateInsert,
            VoiceOutputPlacement::InsertAtCursor,
        ),
    };
    intent(
        kind,
        placement,
        if reason.is_some() { 0.0 } else { 1.0 },
        None,
        None,
        locale,
        reason,
    )
}

#[allow(clippy::too_many_arguments)]
fn intent(
    kind: VoiceIntentKind,
    placement: VoiceOutputPlacement,
    confidence: f32,
    provider: Option<SearchProvider>,
    payload: Option<String>,
    locale: Option<CommandLocale>,
    reason: Option<RouteFallbackReason>,
) -> VoiceIntent {
    VoiceIntent::from_parts(
        kind, placement, confidence, provider, payload, locale, reason,
    )
    .expect("router must construct only valid voice intents")
}

fn resolve_locale(mode: SpeechLanguageMode<'_>, utterance: &str) -> LocaleResolution {
    match mode {
        SpeechLanguageMode::Explicit(value) => match value.trim().to_ascii_lowercase().as_str() {
            "en" | "en-us" | "en-gb" | "english" => LocaleResolution::Supported(CommandLocale::En),
            "zh" | "zh-cn" | "zh-hans" | "zh_hans" => {
                LocaleResolution::Supported(CommandLocale::ZhHans)
            }
            "zh-tw" | "zh-hk" | "zh-hant" | "zh_hant" => {
                LocaleResolution::Supported(CommandLocale::ZhHant)
            }
            "multi" | "auto" | "automatic" => resolve_automatic_locale(utterance),
            _ => LocaleResolution::Unsupported,
        },
        SpeechLanguageMode::Automatic => resolve_automatic_locale(utterance),
    }
}

fn resolve_automatic_locale(utterance: &str) -> LocaleResolution {
    if !has_any_supported_command_signal(utterance) {
        return LocaleResolution::NoCommand;
    }

    let has_ascii_command = has_command_signal(CommandLocale::En, utterance);
    let has_cjk = utterance.chars().any(is_cjk);
    if has_ascii_command && has_cjk {
        return LocaleResolution::Ambiguous;
    }
    if has_ascii_command {
        return LocaleResolution::Supported(CommandLocale::En);
    }

    let simplified = utterance
        .chars()
        .filter(|character| "写帮复说发这译选条简个润扩汇总释么为哪".contains(*character))
        .count();
    let traditional = utterance
        .chars()
        .filter(|character| "寫幫覆說發這譯選條簡個潤擴彙總釋麼為哪則郵".contains(*character))
        .count();
    match (simplified, traditional) {
        (0, 0) => LocaleResolution::Ambiguous,
        (0, _) => LocaleResolution::Supported(CommandLocale::ZhHant),
        (_, 0) => LocaleResolution::Supported(CommandLocale::ZhHans),
        _ => LocaleResolution::Ambiguous,
    }
}

fn is_cjk(character: char) -> bool {
    matches!(character as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF)
}

fn discussed_command_reason(locale: CommandLocale, utterance: &str) -> Option<RouteFallbackReason> {
    let normalized = utterance.trim().to_lowercase();
    let discussed = match locale {
        CommandLocale::En => [
            "i need to ",
            "we should ",
            "can we ",
            "i might ",
            "the next step is to ",
            "we discussed how to ",
            "i plan to ",
            "maybe ",
            "the team will ",
            "i want to ",
            "i should ",
        ]
        .iter()
        .any(|prefix| normalized.starts_with(prefix)),
        CommandLocale::ZhHans => [
            "我明天需要",
            "我们稍后",
            "团队会",
            "下周",
            "也许",
            "先讨论",
            "之后",
            "我想",
            "有人说",
            "计划",
        ]
        .iter()
        .any(|prefix| normalized.starts_with(prefix)),
        CommandLocale::ZhHant => [
            "我明天需要",
            "我們稍後",
            "團隊會",
            "下週",
            "也許",
            "先討論",
            "之後",
            "我想",
            "有人說",
            "計畫",
        ]
        .iter()
        .any(|prefix| normalized.starts_with(prefix)),
    };
    (discussed && has_command_signal(locale, utterance)).then_some(RouteFallbackReason::Ambiguous)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde::Deserialize;

    use super::*;

    fn request<'a>(
        mode: VoiceMode,
        utterance: &'a str,
        has_selected_text: bool,
        speech_language: SpeechLanguageMode<'a>,
    ) -> VoiceRouteRequest<'a> {
        VoiceRouteRequest {
            mode,
            utterance,
            has_selected_text,
            speech_language,
            flags: VoiceRoutingFlags::default(),
        }
    }

    #[test]
    fn voice_intent_grammar_requires_start_boundary_and_preserves_original_payload() {
        let routed = VoiceIntentRouter::route(request(
            VoiceMode::Dictate,
            "  DrAfT:\u{3000}Project X — Tomorrow!  ",
            false,
            SpeechLanguageMode::Explicit("en"),
        ));
        assert_eq!(routed.kind, VoiceIntentKind::DraftInsert);
        assert_eq!(routed.payload.as_deref(), Some("Project X — Tomorrow!"));

        let discussed = VoiceIntentRouter::route(request(
            VoiceMode::Dictate,
            "I need to draft Project X tomorrow",
            false,
            SpeechLanguageMode::Explicit("en"),
        ));
        assert_eq!(discussed.kind, VoiceIntentKind::DictateInsert);
        assert_eq!(
            discussed.fallback_reason,
            Some(RouteFallbackReason::Ambiguous)
        );
    }

    #[test]
    fn voice_intent_grammar_blocks_negated_quoted_and_identifier_commands() {
        for (utterance, reason) in [
            ("please don't draft a reply", RouteFallbackReason::Negated),
            (
                "she said \"draft a reply\"",
                RouteFallbackReason::QuotedOrReported,
            ),
            (
                "compose.yaml is the config",
                RouteFallbackReason::CodeOrIdentifier,
            ),
        ] {
            let routed = VoiceIntentRouter::route(request(
                VoiceMode::Dictate,
                utterance,
                false,
                SpeechLanguageMode::Explicit("en"),
            ));
            assert_eq!(routed.kind, VoiceIntentKind::DictateInsert, "{utterance}");
            assert_eq!(routed.fallback_reason, Some(reason), "{utterance}");
        }
    }

    #[test]
    fn voice_intent_grammar_keeps_ask_with_selection_nondestructive_when_command_mode_off() {
        for utterance in [
            "rewrite this",
            "translate this to French",
            "make this warmer",
        ] {
            let routed = VoiceIntentRouter::route(VoiceRouteRequest {
                mode: VoiceMode::Ask,
                utterance,
                has_selected_text: true,
                speech_language: SpeechLanguageMode::Explicit("en"),
                flags: VoiceRoutingFlags {
                    command_mode: false,
                    ..VoiceRoutingFlags::default()
                },
            });
            assert_eq!(routed.kind, VoiceIntentKind::AskSelection);
            assert_eq!(routed.placement, VoiceOutputPlacement::PopupAnswer);
            assert_eq!(
                routed.fallback_reason,
                Some(RouteFallbackReason::FeatureDisabled)
            );
        }
    }

    #[test]
    fn voice_intent_grammar_ask_command_mode_rewrites_selection_on_positive_imperative() {
        for (utterance, language, kind) in [
            (
                "make this more formal",
                "en",
                VoiceIntentKind::RewriteSelection,
            ),
            ("make it shorter", "en", VoiceIntentKind::RewriteSelection),
            ("fix the grammar", "en", VoiceIntentKind::RewriteSelection),
            (
                "translate this to French",
                "en",
                VoiceIntentKind::TranslateSelection,
            ),
            ("改成正式語氣", "zh-TW", VoiceIntentKind::RewriteSelection),
            (
                "幫我改得簡潔一點",
                "zh-TW",
                VoiceIntentKind::RewriteSelection,
            ),
            ("潤飾一下", "zh-TW", VoiceIntentKind::RewriteSelection),
            ("翻成英文", "zh-TW", VoiceIntentKind::TranslateSelection),
            ("改成正式语气", "zh-CN", VoiceIntentKind::RewriteSelection),
            ("翻成英文", "zh-CN", VoiceIntentKind::TranslateSelection),
        ] {
            let routed = VoiceIntentRouter::route(request(
                VoiceMode::Ask,
                utterance,
                true,
                SpeechLanguageMode::Explicit(language),
            ));
            assert_eq!(routed.kind, kind, "{utterance}");
            assert_eq!(
                routed.placement,
                VoiceOutputPlacement::ReplaceSelection,
                "{utterance}"
            );
            assert_eq!(routed.fallback_reason, None, "{utterance}");
        }
    }

    #[test]
    fn voice_intent_grammar_ask_command_mode_keeps_questions_and_comments_on_popup() {
        for (utterance, language) in [
            ("what does this mean?", "en"),
            ("is this correct", "en"),
            ("this looks good", "en"),
            ("make this shorter?", "en"),
            ("summarize this", "en"),
            ("這段寫得不錯", "zh-TW"),
            ("這段是什麼意思", "zh-TW"),
            ("改成功了嗎", "zh-TW"),
            ("總結這段", "zh-TW"),
            ("不要改成正式語氣", "zh-TW"),
            ("他說「改成正式語氣」", "zh-TW"),
            ("这段写得不错", "zh-CN"),
            ("不要改成正式语气", "zh-CN"),
        ] {
            let routed = VoiceIntentRouter::route(request(
                VoiceMode::Ask,
                utterance,
                true,
                SpeechLanguageMode::Explicit(language),
            ));
            assert_eq!(routed.kind, VoiceIntentKind::AskSelection, "{utterance}");
            assert_eq!(
                routed.placement,
                VoiceOutputPlacement::PopupAnswer,
                "{utterance}"
            );
        }
    }

    #[test]
    fn voice_intent_grammar_ask_with_selection_drafts_replies_at_the_cursor() {
        for (utterance, language) in [
            ("跟他說這個方法還蠻讚的，然後翻譯成英文", "zh-TW"),
            ("告訴她我明天會到", "zh-TW"),
            ("幫我回他說沒問題", "zh-TW"),
            ("跟他说这个方法还蛮赞的", "zh-CN"),
            ("tell them the method works great", "en"),
            ("reply that I will be there tomorrow", "en"),
            ("let her know the build is ready", "en"),
        ] {
            let routed = VoiceIntentRouter::route(request(
                VoiceMode::Ask,
                utterance,
                true,
                SpeechLanguageMode::Explicit(language),
            ));
            assert_eq!(routed.kind, VoiceIntentKind::DraftInsert, "{utterance}");
            assert_eq!(
                routed.placement,
                VoiceOutputPlacement::InsertAtCursor,
                "{utterance}"
            );
            assert!(routed.payload.is_some(), "{utterance}");
        }
        // Without a selection the same reply still drafts.
        let routed = VoiceIntentRouter::route(request(
            VoiceMode::Ask,
            "跟他說這個方法還蠻讚的",
            false,
            SpeechLanguageMode::Explicit("zh-TW"),
        ));
        assert_eq!(routed.kind, VoiceIntentKind::DraftInsert);
    }

    #[test]
    fn voice_intent_grammar_automatic_mode_uses_unambiguous_script_only() {
        let english = VoiceIntentRouter::route(request(
            VoiceMode::Dictate,
            "draft a release note",
            false,
            SpeechLanguageMode::Automatic,
        ));
        assert_eq!(english.grammar_locale, Some(CommandLocale::En));

        let simplified = VoiceIntentRouter::route(request(
            VoiceMode::Dictate,
            "帮我写一封跟进邮件",
            false,
            SpeechLanguageMode::Automatic,
        ));
        assert_eq!(simplified.grammar_locale, Some(CommandLocale::ZhHans));

        let mixed = VoiceIntentRouter::route(request(
            VoiceMode::Dictate,
            "draft 帮我写一封邮件",
            false,
            SpeechLanguageMode::Automatic,
        ));
        assert_eq!(mixed.kind, VoiceIntentKind::DictateInsert);
        assert_eq!(mixed.grammar_locale, None);
        assert_eq!(mixed.fallback_reason, Some(RouteFallbackReason::Ambiguous));
    }

    #[test]
    fn voice_route_e2e_plans_one_provider_call_max_and_exact_output_behavior() {
        struct Case<'a> {
            mode: VoiceMode,
            utterance: &'a str,
            has_selection: bool,
            kind: VoiceIntentKind,
            provider_calls: u8,
            provider_input: Option<&'a str>,
            allow_streaming: bool,
            restore_target: bool,
        }

        let cases = [
            Case {
                mode: VoiceMode::Dictate,
                utterance: "ordinary dictated text",
                has_selection: false,
                kind: VoiceIntentKind::DictateInsert,
                provider_calls: 1,
                provider_input: Some("ordinary dictated text"),
                allow_streaming: true,
                restore_target: false,
            },
            Case {
                mode: VoiceMode::Dictate,
                utterance: "draft a concise launch email",
                has_selection: false,
                kind: VoiceIntentKind::DraftInsert,
                provider_calls: 1,
                provider_input: Some("a concise launch email"),
                allow_streaming: false,
                restore_target: false,
            },
            Case {
                mode: VoiceMode::Dictate,
                utterance: "rewrite this more warmly",
                has_selection: true,
                kind: VoiceIntentKind::RewriteSelection,
                provider_calls: 1,
                provider_input: Some("rewrite this more warmly"),
                allow_streaming: false,
                restore_target: false,
            },
            Case {
                mode: VoiceMode::Ask,
                utterance: "what does this mean?",
                has_selection: true,
                kind: VoiceIntentKind::AskSelection,
                provider_calls: 1,
                provider_input: Some("what does this mean?"),
                allow_streaming: false,
                restore_target: false,
            },
            Case {
                mode: VoiceMode::Ask,
                utterance: "draft a concise launch email",
                has_selection: false,
                kind: VoiceIntentKind::DraftInsert,
                provider_calls: 1,
                provider_input: Some("a concise launch email"),
                allow_streaming: false,
                restore_target: true,
            },
            Case {
                mode: VoiceMode::Ask,
                utterance: "search Rust Tauri hotkeys on Google",
                has_selection: false,
                kind: VoiceIntentKind::Search,
                provider_calls: 0,
                provider_input: None,
                allow_streaming: false,
                restore_target: false,
            },
            Case {
                mode: VoiceMode::Translate,
                utterance: "See you tomorrow",
                has_selection: false,
                kind: VoiceIntentKind::TranslateInsert,
                provider_calls: 1,
                provider_input: Some("See you tomorrow"),
                allow_streaming: false,
                restore_target: false,
            },
            Case {
                mode: VoiceMode::Translate,
                utterance: "translate this",
                has_selection: true,
                kind: VoiceIntentKind::TranslateSelection,
                provider_calls: 1,
                provider_input: Some("translate this"),
                allow_streaming: false,
                restore_target: false,
            },
        ];

        for case in cases {
            let intent = VoiceIntentRouter::route(request(
                case.mode,
                case.utterance,
                case.has_selection,
                SpeechLanguageMode::Explicit("en"),
            ));
            assert_eq!(intent.kind, case.kind, "{}", case.utterance);

            let plan = plan_voice_provider_work(case.mode, case.utterance, &intent);
            assert_eq!(
                plan.provider_call_limit, case.provider_calls,
                "{}",
                case.utterance
            );
            assert_eq!(
                plan.provider_input.as_deref(),
                case.provider_input,
                "{}",
                case.utterance
            );
            assert_eq!(
                plan.allow_streaming, case.allow_streaming,
                "{}",
                case.utterance
            );
            assert_eq!(
                plan.restore_target_before_insert, case.restore_target,
                "{}",
                case.utterance
            );
        }
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CorpusCase {
        id: String,
        mode: VoiceMode,
        locale: String,
        utterance: String,
        has_selection: bool,
        flags: VoiceRoutingFlags,
        expected_kind: VoiceIntentKind,
        expected_placement: VoiceOutputPlacement,
        expected_provider: Option<SearchProvider>,
        expected_payload: Option<String>,
        expected_fallback_reason: Option<RouteFallbackReason>,
        destructive_blocker: bool,
    }

    fn run_corpus(name: &str, source: &str) {
        let cases: Vec<CorpusCase> = serde_json::from_str(source).unwrap();
        let mut counts = BTreeMap::<String, usize>::new();
        let mut blockers = 0;
        let mut destructive_false_positives = 0;

        for case in &cases {
            let speech_language = if case.locale == "automatic" {
                SpeechLanguageMode::Automatic
            } else {
                SpeechLanguageMode::Explicit(&case.locale)
            };
            let result = VoiceIntentRouter::route(VoiceRouteRequest {
                mode: case.mode,
                utterance: &case.utterance,
                has_selected_text: case.has_selection,
                speech_language,
                flags: case.flags,
            });

            assert_eq!(result.kind, case.expected_kind, "case {}", case.id);
            assert_eq!(
                result.placement, case.expected_placement,
                "case {}",
                case.id
            );
            assert_eq!(
                result.search_provider, case.expected_provider,
                "case {}",
                case.id
            );
            assert_eq!(result.payload, case.expected_payload, "case {}", case.id);
            assert_eq!(
                result.fallback_reason, case.expected_fallback_reason,
                "case {}",
                case.id
            );

            *counts.entry(format!("{:?}", result.kind)).or_default() += 1;
            if case.destructive_blocker {
                blockers += 1;
                if matches!(
                    result.kind,
                    VoiceIntentKind::DraftInsert
                        | VoiceIntentKind::RewriteSelection
                        | VoiceIntentKind::TranslateSelection
                        | VoiceIntentKind::Search
                ) {
                    destructive_false_positives += 1;
                }
            }
        }

        println!(
            "{name}: cases={}, blockers={}, destructive_false_positives={}, kinds={counts:?}",
            cases.len(),
            blockers,
            destructive_false_positives
        );
        assert_eq!(destructive_false_positives, 0);
    }

    #[test]
    fn voice_intent_corpus_en() {
        run_corpus(
            "en",
            include_str!("../../tests/fixtures/voice_intent_en.json"),
        );
    }

    #[test]
    fn voice_intent_corpus_zh_hans() {
        run_corpus(
            "zh_hans",
            include_str!("../../tests/fixtures/voice_intent_zh_hans.json"),
        );
    }

    #[test]
    fn voice_intent_corpus_zh_hant() {
        run_corpus(
            "zh_hant",
            include_str!("../../tests/fixtures/voice_intent_zh_hant.json"),
        );
    }
}
