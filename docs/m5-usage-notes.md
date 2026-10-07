# M5 — BYOK usage visibility

Date: 2026-10-08. Branch `m5-usage` (on top of `m4-command-mode`).

## What changed

- **Token usage is now real** (`llm/protocol.rs`, `llm/openai.rs`): direct `api.openai.com` streaming
  requests send `stream_options.include_usage = true` (gated so OpenAI-compatible proxies are not
  sent an unknown field); the final usage chunk is parsed and merged. Anthropic `message_start` /
  `message_delta` usage and non-streaming `usage` bodies are parsed too. Previously
  `PolishResponse.usage` was always `None`.
- **Ask runs count** (`commands/ask.rs`, `pipeline.rs`): popup answers (BYOK path reads `usage`
  from the JSON body) and Fn+Space commands (draft / rewrite / translate) now write a history row
  with model, latency and tokens, so the usage numbers cover everything the keys are used for.
  Popup answers carry `output_status = "popup"`.
- **Aggregation** (`storage::HistoryStore::usage_summary(since)`, command `get_usage_summary`):
  totals, by STT/LLM model, and by day for rows with `created_at >= since`.
- **Price table** (`AppConfig.usage_pricing: Vec<UsagePrice>`, default empty): user overrides for
  the frontend's built-in `DEFAULT_USAGE_PRICING` (whisper-1 $0.006/min, gpt-4o-mini-transcribe
  $0.003/min, gpt-4.1-mini $0.40/$1.60 per MTok, …). The built-ins are labelled as estimates.
- **UI**: HomePage `UsageCard` (this month: minutes, tokens, estimated USD, runs; shown when there
  is no managed-cloud subscription); Settings → **Usage** pane (period selector, per-model table,
  editable price table with reset and "add model"); History rows show `· 1.3k tok`.

## Human checks

1. Dictate once → History row shows `STT … · AI … · gpt-4o-mini-transcribe / gpt-4.1-mini · N tok`.
2. Home page shows the "本月 BYOK 用量" card with non-zero minutes and a dollar estimate.
3. Settings → 用量: the table lists both models; editing a price cell and tabbing out changes the
   estimate; "還原預設" clears edits; the period switch changes the numbers.
4. Fn+Space popup question → a new History row with tokens appears.
