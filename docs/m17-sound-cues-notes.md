# M17 — Sound cues when recording starts and stops

Date: 2026-10-10. Branch `m17-sound-cues` (on top of m16).

## What changed

- `src-tauri/src/sound_cues.rs`: three short tones synthesised in memory as 44.1 kHz mono PCM
  WAV (nothing bundled, nothing to license): **start** is a rising fifth (659 → 988 Hz, 115 ms),
  **stop** the same two notes falling, **cancel** a single low tap (392 Hz, 90 ms). Each note has
  an 8 ms ramp so there is no click, a touch of second harmonic so it does not read as a test tone,
  and a 0.42 peak with NSSound volume 0.55 on top of the system output volume.
- Playback is `NSSound` on the main thread (`objc2-app-kit` feature `NSSound`); the last sound is
  retained in a thread-local until the next cue so it finishes. No webview autoplay policy is
  involved. Other platforms compile to a no-op.
- Triggers: dictation `Recording` (start), the `Recording → Transcribing` transition in
  `stop()` (stop), `abort()` of an active run (cancel); Ask flow `AskRecording` (start),
  `stop_ask_dictation` (stop), `abort_ask_flow` with a live session (cancel).
- Config `capsule_sound_enabled` (default true): Rust struct + default, TS `AppConfig` +
  `defaultConfig`, backup allowlist, General pane toggle 「錄音開始與結束提示音」 (i18n ×11).
  The pipeline mirrors the flag in an atomic so stop/abort need no config load.

## Human checks

1. Option → rising blip; release → falling blip; Esc during recording → low tap.
2. Option+Space (Ask) → the same cues.
3. Settings → General → turn the toggle off → silence on the next run.
4. The start cue lands as capture begins; if the microphone picks it up, the no-speech guard
   still needs 10 voiced chunks, so a 115 ms tone alone never counts as speech.

## Known limitations

- Output ducking lowers the system output while recording, so the stop cue is slightly quieter
  than the start cue on headphones-free setups.
