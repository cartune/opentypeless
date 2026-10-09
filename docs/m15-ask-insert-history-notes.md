# M15 — Ask output goes to the field; history shows selection / command / output

Date: 2026-10-09. Branch `m15-ask-insert-history` (on top of m14).

## What was wrong (from the 13:49 WhatsApp run, history row 269)

- The selection was never captured: `start_reserved_ask_dictation` only captured when
  `selected_text_enabled` was on. Command mode (`voice_routing_flags.command_mode`) arrived as a
  serde default on this install, so the GeneralPane coupling that flips `selected_text_enabled`
  never ran. No "captured selected text" line in the log for that run.
- "跟他說這個方法還蠻讚的，然後翻譯成英文…" matched none of the start-anchored draft / translate
  / rewrite prefixes, so the router fell to `OpenQuestion` → popup. The Ask prompt answered as a
  Q&A ("…翻譯成英文：…"), and history stored utterance + answer with `output_status = popup`.
- The History and Ask popup copy buttons used `navigator.clipboard.writeText`, which WKWebView
  rejects outside a focused trusted gesture → "複製到剪貼簿失敗".

## What changed

- **Capture** (`commands/ask.rs`): the Ask shortcut captures the selection when
  `selected_text_enabled || voice_routing_flags.command_mode`.
- **Grammar** (`voice_intent/grammar/{zh_hant,zh_hans,en}.rs`): a separate `match_reply` /
  `REPLY_PREFIXES` list — 跟他說 / 跟她說 / 跟他們說 / 跟對方說 / 告訴他 / 回他說 / 幫我回 /
  幫我跟他說 …; tell him / tell her / tell them / reply that / respond with / let them know / say
  that …. Only the Ask routes consult it: dictation that opens with 跟他說 / tell him stays
  `DictateInsert` (`match_draft` is unchanged). Reply prefixes count as command signals so the
  automatic (`multi`) STT language still resolves a locale; a script-neutral reply such as
  告訴她… resolves to Traditional instead of Ambiguous.
- **Guard** (`voice_intent/guards.rs`): a *leading* reply verb (跟他說…) is stripped before the
  quoted-or-reported check, so "跟他說 X" is an instruction while "他說「改成正式語氣」" is still
  reported speech.
- **Routing** (`voice_intent/mod.rs`): with a selection, a draft prefix that is not a translation
  or rewrite becomes `DraftInsert` / `InsertAtCursor` with the selection as context. Questions,
  comments and unrecognised phrasing still go to the popup (the M4 rule is unchanged).
- **Draft keeps the selection** (`commands/ask.rs`, `llm/prompt.rs`): `DraftInsert` now receives
  the selected text as untrusted "REPLY CONTEXT"; the prompt says to write the whole draft in a
  requested language (翻譯成英文 / in English) and output only the finished text.
- **Insert-or-popup** (`voice_intent/executor.rs`, `pipeline.rs`, `edit_learning/ax.rs`): new
  backend probe `insert_target_ready`. For drafts on macOS it reads the target app's focused
  element via Accessibility (`focused_selection`: role + `AXSelectedTextRange` length). Only a
  non-empty selection inside an editable role (AXTextField / AXTextArea / AXComboBox /
  AXSearchField) blocks typing: the draft is copied and shown in the Ask popup with fallback
  reason `insert_target_unavailable` ("未直接輸入（欄位中有選取的文字），結果已複製"). A selection
  in a read-only view (a chat bubble) or no AX data → insert as before. Dictation and selection
  rewrites are not gated.
- **History** (`storage/mod.rs`, `commands/backup.rs`, `pipeline.rs`): columns `intent_kind` and
  `selected_text` (ALTER on open, INSERT/SELECT/restore, backup optional fields). Ask commands and
  popup answers fill them; dictation rows leave them NULL.
- **History UI** (`components/History/index.tsx`, `entryKind.ts`): ask / command rows render a chip
  (問答 / 指令) and three blocks — 框選內容 (clamped, quoted), 指令, 輸出. Legacy rows with
  `output_status = popup` are treated as ask rows. Search covers the selection. Copy copies 輸出.
- **Copy** (`lib/clipboard.ts`): History and the Ask popup copy through
  `@tauri-apps/plugin-clipboard-manager` (permission already granted for main/ask) with the web
  clipboard as fallback.

## Human checks

1. WhatsApp: select a received message → Option+Space → 「跟他說這個方法還蠻讚的，翻譯成英文」 →
   the English reply is typed into the composer; no popup. History shows chip 指令 + 框選內容 /
   指令 / 輸出.
2. Same, but with text selected *inside the composer* → popup with the draft and the
   "未直接輸入…" line; nothing in the composer is overwritten.
3. Select text → Option+Space → 「這段在講什麼」 → popup answer (unchanged); history chip 問答.
4. History page copy button → 已複製 (no "複製到剪貼簿失敗").
5. Notes: 「改成正式語氣」 with a selection still replaces in place (M4 unchanged).

## Known limitations

- Reply detection is prefix-based; "這個方法很讚，跟他說一下" (verb at the end) still goes to the
  popup. Unrecognised non-question phrasing deliberately stays nondestructive.
- The AX selection probe only sees apps that expose `AXSelectedTextRange` on the focused element;
  terminals and some Electron apps return nothing and the draft is typed as before.
