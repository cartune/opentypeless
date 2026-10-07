# M4 — Command mode for the Ask shortcut (Fn+Space)

Date: 2026-10-08. Branch `m4-command-mode` (on top of `m2-speed`).

## What changed

- **Routing** (`src-tauri/src/voice_intent/mod.rs`): Ask mode with selected text now goes through
  `route_ask_with_selection`. Only a positive imperative the grammar recognises becomes destructive:
  translation prefixes → `TranslateSelection`, rewrite prefixes → `RewriteSelection`, both with
  `ReplaceSelection` placement. Question-shaped utterances (trailing `?`/`？`/嗎/吗/呢/麼/么),
  informational prefixes (總結/解釋/what/why…), negations, quoted/reported speech, identifiers and
  anything unrecognised stay on the nondestructive `AskSelection` popup.
- **Flag** `VoiceRoutingFlags.command_mode` (default `true`, serde default so old configs migrate).
  Off → recognised commands fall back to the popup with `FeatureDisabled`.
- **Grammars** (`grammar/{zh_hant,zh_hans,en}.rs`): Typeless-style imperative prefixes
  (改成/改寫/潤飾/精簡/縮短/重寫/修正錯字/讓它…/正式一點/翻成/翻譯成…;
  rewrite/rephrase/make this/make it/fix the/shorten/simplify/proofread/translate to…). Draft prefixes
  gained 草擬/幫我擬/回一封/寫一段/幫我產生…. Ambiguous comment-like prefixes (寫得/改得/變成) were
  deliberately excluded.
- **Execution** (`commands/ask.rs`, `pipeline.rs`): `run_ask_draft` became `run_ask_voice_command`
  and also accepts `RewriteSelection` / `TranslateSelection` with the captured selection. It runs
  through `polish_text`, so the rewrite inherits the dictionary, `[CHINESE_SCRIPT]`, s2twp +
  correction-rule post-processing, the 4096-token LLM limit (not Ask's 80), and the executor's
  selection-unchanged check before replacing. New result output `replacedSelection` (no popup).
- **Capsule**: backend emits `ask:selection_captured {chars, commandMode}`; `CapsuleAskRecording` /
  `CapsuleAskThinking` show a wand icon + "指令 / Command" badge and "處理指令中…".
- **Settings**: General → Advanced → "指令模式" toggle (turning it on also enables
  `selected_text_enabled`, which the capture depends on). New installs default
  `selected_text_enabled = true`.
- **Hotkeys**: `HotkeyRole::EditSelection` now behaves like Ask (selection is always captured).
- **Fixtures**: generator adds ask+selection positives, negatives, blocked (negated/quoted/identifier),
  command-mode-disabled and zh draft cases. en 324 / zh-Hans 309 / zh-Hant 309 cases; the
  "zero destructive misroutes" assertion still holds.

## Human checks (Notes.app)

1. Select a paragraph → Fn+Space → say 「改成正式語氣」 → selection replaced in place, no popup.
2. Select text → Fn+Space → 「翻成英文」 → replaced with English.
3. Select text → Fn+Space → 「這段在講什麼」 → answer popup, text untouched.
4. Select text → Fn+Space → 「不要改成正式語氣」 → popup, text untouched.
5. No selection → Fn+Space → 「幫我寫一封請假信」 → draft inserted at cursor.
6. No selection → Fn+Space → 「台北今天幾度」 → popup answer.
7. Capsule shows the wand "指令" badge only in cases 1–4.
8. Settings → General → Advanced → turn off 指令模式 → case 1 opens a popup instead.
