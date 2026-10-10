# M21 — Faster pipeline: warm-up, predicted outputs, cache telemetry, shadow STT

Date: 2026-10-10. Branch `m21-faster-pipeline` (on top of m20).

## Baseline (this machine, 399 runs up to 2026-10-10)

| stage | mean |
|---|---|
| STT finalize (whisper-1, ~18 s audio) | 2.3 s |
| LLM polish (gpt-4o-mini, ~2,070 prompt / 60 output tokens) | 1.6 s |
| Output + save | 0.7 s (first run after launch 3.1 s, others 0.5 s median) |

Upload is already Ogg/Opus (~6 KB/s); the network is not the bottleneck.

## What changed

- **Output+Save spike fixed** (`llm/post_process.rs`, `lib.rs`): the zhhz (OpenCC) converters
  were `thread_local!`, so every tokio worker built the multi-megabyte tables on its first
  dictation — the first run after launch averaged 3.1 s of output time, and later runs on a fresh
  worker spiked too. One shared converter behind a mutex, built at startup on a
  `script-warm-up` thread.
- **Per-stage logging**: `[Pipeline Timing] output: post-process Xms, execute Yms (status),
  cached N, prediction accepted A / rejected R` and `[Pipeline Timing] save: Zms`, so any
  remaining slow run can be attributed.
- **Predicted outputs** (`llm/openai.rs`, `llm/protocol.rs`): on OpenAI's own endpoint with a
  gpt-4o / gpt-4.1 family model, the transcript is sent as `prediction` and
  `max_completion_tokens` is dropped (the API refuses the pair). A 400 mentioning `prediction`
  retries once without it. Config `llm_predicted_outputs` (default on, LLM pane → 進階 →
  預測輸出加速). `LlmUsage` now carries `cached_tokens`, `accepted_prediction_tokens`,
  `rejected_prediction_tokens`; compare the LLM-ms average after a day against 1,569 ms.
- **Prompt cache**: order was already static-first; the new `cached` figure in the log shows
  whether OpenAI's automatic prefix cache hits (it should after the first run in an app).
- **Shadow STT** (`stt/mod.rs`, `stt/whisper_compat.rs`, `pipeline.rs`, `storage/mod.rs`):
  dictation sends the same Opus bytes, prompt and language to the other member of the
  {whisper-1, gpt-4o-transcribe} pair in a background task (`shadow_model_for`). The receiver is
  parked in `SttConfig.shadow_sink`; after the history row is saved the pipeline attaches the
  result with `HistoryStore::set_shadow` (new columns `shadow_model`, `shadow_text`,
  `shadow_ms`) and emits `history:updated`. Config `stt_shadow_enabled` (default on, STT pane).
  Cost cap: `SHADOW_STT_MAX_AUDIO_SECONDS` = 8,000 minutes (≈ US$48) of shadow audio, checked
  against `shadow_audio_seconds()` at every start. Ask flow is not shadowed.
- **History UI**: rows whose shadow transcript differs from the primary show both texts with
  model labels and the shadow latency; the page header counts "N / M runs differ". Neither
  transcript is ground truth — read the pairs.
- **Usage**: `get_usage_summary` adds `stt_shadow` rows (per model, runs, minutes) priced like
  STT, so the experiment shows up in Settings → Usage.
- `HistoryStore::add` / `add_with_policy` return the inserted row id.

## How to evaluate (after a few days)

1. History page: read the differing pairs; decide which model misreads Chinese names less.
2. Log: average `LLM polish` ms and `prediction accepted/rejected`; if rejected dominates and
   LLM ms did not drop, switch 預測輸出加速 off.
3. If gpt-4o-transcribe wins, set Settings → STT → OpenAI model to `gpt-4o-transcribe`; the
   shadow automatically becomes whisper-1.

## Not done

- Paste-instead-of-type threshold: the typing path is not where the time went (see above);
  revisit only if the new per-stage log shows `execute` dominating on long texts.
- Streaming (Realtime) STT: next step once the model comparison is in.
