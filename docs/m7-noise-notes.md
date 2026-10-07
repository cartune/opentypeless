# M7 — Noise suppression and anti-aliased resampling

Date: 2026-10-08. Branch `m7-noise` (on top of `m5-usage`).

## What changed

- **`src-tauri/src/audio/dsp.rs`** (new):
  - `SincResampler`: streaming windowed-sinc (Hann) resampler. Replaces the linear-interpolation
    `downsample()` which aliased everything above 8 kHz into the speech band. Stateful, so cpal
    callback sizes (441 / 480 / 512 …) do not matter; output is bit-identical regardless of chunking.
    Tests: 10 kHz at 48 kHz → 16 kHz attenuated > 30 dB; 300 Hz–5 kHz within ±1 dB; 44.1 kHz works.
  - `NoiseSuppressor`: `nnnoiseless` (pure-Rust RNNoise, `default-features = false`) at 48 kHz in
    480-sample frames with a carried remainder; first frame replaced by silence (fade-in artefact).
    Tests: white noise ≥ 6 dB down; harmonic pitch-gliding speech-like signal loses < 6 dB.
  - `AudioFrontEnd`: device rate → (48 kHz → RNNoise →) target rate. With suppression off it is just
    the new resampler.
- **`capture.rs`**: the front end lives in the cpal callback closure; the volume meter and the
  voiced-chunk gate (M1 silence gate) now look at the *processed* signal, so steady fan noise no
  longer counts as "audio" when suppression is on. `AudioConfig::for_app_config` carries the flag;
  both the dictation pipeline and the Ask flow use it.
- **Config**: `noise_suppression_enabled` (default **off**; Settings → 語音辨識 → 背景降噪 toggle;
  included in settings backup). Off by default because it is unverified on real microphones;
  the anti-aliased resampler is always on.

## Human checks

1. Noise off (default): dictation still works; nothing audible should change except slightly
   cleaner transcripts of sibilants (no aliasing).
2. Turn 背景降噪 on next to a fan / air-con: the capsule meter should sit near zero while silent,
   speech should still trigger the bars; transcripts should drop fewer words. If speech sounds
   "underwater" or words are dropped, turn it off and report.
3. Compare STT latency chip with suppression on/off — should be unchanged (RNNoise is ~1% CPU).
