//! Learn from the user's edits.
//!
//! When dictated text has just been inserted, a watcher keeps re-reading the
//! target field for a while (macOS Accessibility, see `ax`). If the user
//! replaces a word inside the dictated span, the replacement becomes a
//! dictionary word and a correction rule, and the UI is told what was learned.
//! Everything that decides *what* counts as an edit lives in `diff` and is
//! pure; this module only schedules reads and writes the results.

pub mod diff;
pub mod phonetic;

#[cfg(target_os = "macos")]
pub mod ax;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;

/// Let the app finish its own autocorrect before taking the baseline.
pub const INITIAL_DELAY: Duration = Duration::from_millis(500);
/// Spacing between reads of the field.
pub const POLL_INTERVAL: Duration = Duration::from_millis(1500);
/// How long the text has to stay unchanged after an edit before we learn from it.
pub const SETTLE_AFTER_EDIT: Duration = Duration::from_secs(4);
/// Give up watching after this long.
pub const MAX_WATCH: Duration = Duration::from_secs(120);

pub const LEARNED_EVENT: &str = "learning:learned";
pub const SOURCE_MANUAL: &str = "manual";
pub const SOURCE_LEARNED: &str = "learned";
/// Seen once; becomes a learned rule when the same fix is made again.
pub const SOURCE_PENDING: &str = "pending";

/// What to watch after one dictation was inserted.
#[derive(Debug, Clone)]
pub struct WatchRequest {
    pub pid: u32,
    pub inserted: String,
    pub app_label: String,
}

/// One learned replacement and what was done with it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearnedItem {
    pub from: String,
    pub to: String,
    pub word_added: bool,
    pub rule_added: bool,
    /// An earlier learned rule said the opposite; it was switched off instead.
    pub rule_disabled: bool,
    /// First sighting: noted, applied only after the same fix happens again.
    pub rule_pending: bool,
    /// Nothing to add: the word and the rule were both already there.
    pub already_known: bool,
}

/// Payload of `learning:learned`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearnedPayload {
    pub items: Vec<LearnedItem>,
    pub app_label: String,
    /// Whether the main window is showing; decides pill vs in-window toast.
    pub main_visible: bool,
}

/// Owns the watcher generation: a new recording invalidates the old watcher
/// so it evaluates what it has and stops before the next insertion lands.
#[derive(Default)]
pub struct EditLearning {
    generation: Arc<AtomicU64>,
}

impl EditLearning {
    /// Make any running watcher finish now.
    pub fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Start watching the field that just received `request.inserted`.
    #[cfg(target_os = "macos")]
    pub fn arm(&self, app: tauri::AppHandle, request: WatchRequest) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let shared = Arc::clone(&self.generation);
        let spawned = std::thread::Builder::new()
            .name("edit-learning".into())
            .spawn(move || {
                if let Some(edits) = watch(&request, &shared, generation) {
                    apply(&app, &request, edits);
                }
            });
        if let Err(error) = spawned {
            tracing::warn!("Edit learning watcher could not start: {error}");
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn arm(&self, _app: tauri::AppHandle, request: WatchRequest) {
        tracing::debug!(
            "Edit learning is macOS-only; ignoring {} chars for {}",
            request.inserted.chars().count(),
            request.app_label
        );
    }
}

/// Decide whether a learned pair should also become a dictionary word.
pub fn should_add_word(to: &str) -> bool {
    to.chars().any(|c| c.is_alphabetic())
}

/// Poll the field until the user's edit settles, then return the edits.
#[cfg(target_os = "macos")]
fn watch(
    request: &WatchRequest,
    shared: &AtomicU64,
    generation: u64,
) -> Option<Vec<diff::LearnedEdit>> {
    use std::time::Instant;

    std::thread::sleep(INITIAL_DELAY);
    let field = ax::FocusedField::capture(request.pid)?;
    let first = field.read()?;
    let chars: Vec<char> = first.value.chars().collect();
    let caret = first
        .caret_utf16
        .and_then(|offset| diff::utf16_offset_to_char_index(&chars, offset));
    let Some(anchor) = diff::anchor_from_caret(&chars, caret, &request.inserted) else {
        tracing::info!(
            "Edit learning: inserted text not found in {} field ({:?}); not watching",
            request.app_label,
            first.role
        );
        return None;
    };
    tracing::info!(
        "Edit learning: watching {} chars in {} ({:?})",
        anchor.span.len(),
        request.app_label,
        first.role
    );

    let started = Instant::now();
    let mut region = anchor.span.clone();
    let mut last_change: Option<Instant> = None;
    loop {
        std::thread::sleep(POLL_INTERVAL);
        let stale = shared.load(Ordering::SeqCst) != generation;
        match field.read() {
            Some(read) => {
                let chars: Vec<char> = read.value.chars().collect();
                match diff::locate_span(&anchor, &chars) {
                    Some((start, end)) => {
                        if chars[start..end] != region[..] {
                            region = chars[start..end].to_vec();
                            last_change = Some(Instant::now());
                        }
                    }
                    None => break,
                }
            }
            None => break,
        }
        let settled = last_change.is_some_and(|at| at.elapsed() >= SETTLE_AFTER_EDIT);
        if stale || settled || started.elapsed() >= MAX_WATCH {
            break;
        }
    }
    last_change?;
    let edits = diff::learn_edits(&anchor.span, &region);
    tracing::info!(
        "Edit learning: {} learnable edit(s) in {}",
        edits.len(),
        request.app_label
    );
    (!edits.is_empty()).then_some(edits)
}

/// Write the edits to the dictionary and tell the windows.
#[cfg(target_os = "macos")]
fn apply(app: &tauri::AppHandle, request: &WatchRequest, edits: Vec<diff::LearnedEdit>) {
    use tauri::{Emitter, Manager};

    let store = app.state::<crate::storage::DictionaryStore>();
    let words = tauri::async_runtime::block_on(store.list()).unwrap_or_default();
    let rules = tauri::async_runtime::block_on(store.correction_rules()).unwrap_or_default();
    let mut items = Vec::new();
    for edit in edits {
        let mut item = LearnedItem {
            from: edit.from.clone(),
            to: edit.to.clone(),
            word_added: false,
            rule_added: false,
            rule_disabled: false,
            rule_pending: false,
            already_known: false,
        };
        let forward = crate::storage::normalized_correction_identity(&edit.from, &edit.to);
        let backward = crate::storage::normalized_correction_identity(&edit.to, &edit.from);
        let identity = |rule: &crate::storage::CorrectionRule| {
            crate::storage::normalized_correction_identity(&rule.pattern, &rule.replacement)
        };
        // One edit is a note; the same edit twice is a rule. Nothing is
        // applied, and no word is added, until the second sighting.
        if let Some(reverse) = rules.iter().find(|rule| identity(rule) == backward) {
            if reverse.source == SOURCE_PENDING {
                // Changed their mind before it ever applied: forget the note.
                item.rule_disabled =
                    tauri::async_runtime::block_on(store.remove_correction(reverse.id)).is_ok();
            } else if reverse.enabled && reverse.source == SOURCE_LEARNED {
                item.rule_disabled =
                    tauri::async_runtime::block_on(store.set_correction_enabled(reverse.id, false))
                        .is_ok();
            }
        } else if let Some(existing) = rules.iter().find(|rule| identity(rule) == forward) {
            if existing.source == SOURCE_PENDING {
                item.rule_added = tauri::async_runtime::block_on(
                    store.confirm_correction(existing.id, SOURCE_LEARNED),
                )
                .is_ok();
            }
        } else {
            item.rule_pending = tauri::async_runtime::block_on(store.add_pending_correction(
                &edit.from,
                &edit.to,
                SOURCE_PENDING,
            ))
            .is_ok();
        }
        if item.rule_added {
            // Same normalisation as the store's duplicate checks (NFKC, trim, case).
            let word_known = words.iter().any(|entry| {
                crate::storage::normalized_dictionary_identity(&entry.word)
                    == crate::storage::normalized_dictionary_identity(&edit.to)
            });
            if should_add_word(&edit.to) && !word_known {
                item.word_added = tauri::async_runtime::block_on(store.add_with_source(
                    &edit.to,
                    None,
                    SOURCE_LEARNED,
                ))
                .is_ok();
            }
        }
        item.already_known =
            !(item.word_added || item.rule_added || item.rule_disabled || item.rule_pending);
        tracing::info!(
            "Edit learning: {:?} -> {:?} (word={}, rule={}, pending={}, disabled={}, known={})",
            item.from,
            item.to,
            item.word_added,
            item.rule_added,
            item.rule_pending,
            item.rule_disabled,
            item.already_known
        );
        // Known pairs are reported too, so an edit never looks like it went unnoticed.
        items.push(item);
    }
    if items.is_empty() {
        return;
    }
    let main_visible = app
        .get_webview_window("main")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false);
    let _ = app.emit(
        LEARNED_EVENT,
        LearnedPayload {
            items,
            app_label: request.app_label.clone(),
            main_visible,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalidate_bumps_the_generation() {
        let learning = EditLearning::default();
        let before = learning.generation.load(Ordering::SeqCst);
        learning.invalidate();
        assert_eq!(learning.generation.load(Ordering::SeqCst), before + 1);
    }

    #[test]
    fn words_need_letters() {
        assert!(should_add_word("Cartune"));
        assert!(should_add_word("卡通"));
        assert!(!should_add_word("2024"));
    }

    /// Manual check of the raw AX reader: open a TextEdit document, then
    /// `TYPELAZY_AX_PID=$(pgrep -x TextEdit) cargo test ax_reads -- --ignored --nocapture`.
    #[test]
    #[ignore]
    #[cfg(target_os = "macos")]
    fn ax_reads_the_focused_field_of_a_running_app() {
        let pid: u32 = std::env::var("TYPELAZY_AX_PID")
            .expect("TYPELAZY_AX_PID")
            .parse()
            .expect("pid");
        println!(
            "accessibility trusted: {}",
            crate::pipeline::is_accessibility_trusted()
        );
        let field = match ax::FocusedField::try_capture(pid) {
            Ok(field) => field,
            Err(error) => panic!("no focused element for pid {pid}: AXError {error}"),
        };
        let read = match field.try_read() {
            Ok(read) => read,
            Err(error) => panic!(
                "field value unreadable: AXError {error} (role {:?})",
                field.role()
            ),
        };
        println!(
            "role={:?} caret={:?} chars={}",
            read.role,
            read.caret_utf16,
            read.value.chars().count()
        );
        let chars: Vec<char> = read.value.chars().collect();
        if let Some(caret) = read.caret_utf16 {
            assert!(diff::utf16_offset_to_char_index(&chars, caret).is_some());
        }
    }

    #[test]
    fn learned_payload_uses_camel_case_wire_names() {
        let payload = LearnedPayload {
            items: vec![LearnedItem {
                from: "a".into(),
                to: "b".into(),
                word_added: true,
                rule_added: false,
                rule_disabled: false,
                rule_pending: false,
                already_known: false,
            }],
            app_label: "Notes".into(),
            main_visible: true,
        };
        let json = serde_json::to_value(payload).unwrap();
        assert_eq!(json["appLabel"], "Notes");
        assert_eq!(json["mainVisible"], true);
        assert_eq!(json["items"][0]["wordAdded"], true);
        assert_eq!(json["items"][0]["alreadyKnown"], false);
    }
}
