//! Audible cues for the capsule: one sound when recording starts, another
//! when it stops, a low tap on cancel. Styles are pairs of bundled assets
//! (see `assets/sounds/`, provenance in docs/m17-sound-cues-notes.md) plus
//! one synthesised chime. Playback is `NSSound` on macOS, scheduled from the
//! Rust state transitions, so no webview autoplay policy is involved.

use std::f32::consts::PI;

/// Which moment to mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    Start,
    Stop,
    Cancel,
}

/// The sound set, chosen in Settings → General.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CueStyle {
    /// Dashla's 紅燈起步 ding, as is, for both start and stop.
    #[default]
    Dashla,
    /// Mixkit "Software interface start" / "Software interface back".
    Interface,
    /// Mixkit "Positive notification" / "Software interface remove".
    Positive,
    /// Mixkit "Correct answer tone" / "Confirmation tone".
    Confirm,
    /// Kenney Interface Sounds "maximize_006" / "minimize_006" (CC0).
    Arcade,
    /// Synthesised sine chime, do → mi and back.
    Chime,
}

impl CueStyle {
    pub const ALL: [CueStyle; 6] = [
        CueStyle::Dashla,
        CueStyle::Interface,
        CueStyle::Positive,
        CueStyle::Confirm,
        CueStyle::Arcade,
        CueStyle::Chime,
    ];

    pub fn parse(value: &str) -> CueStyle {
        match value.trim().to_ascii_lowercase().as_str() {
            "interface" => CueStyle::Interface,
            "positive" => CueStyle::Positive,
            "confirm" => CueStyle::Confirm,
            "arcade" => CueStyle::Arcade,
            "chime" => CueStyle::Chime,
            _ => CueStyle::Dashla,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            CueStyle::Dashla => "dashla",
            CueStyle::Interface => "interface",
            CueStyle::Positive => "positive",
            CueStyle::Confirm => "confirm",
            CueStyle::Arcade => "arcade",
            CueStyle::Chime => "chime",
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            CueStyle::Dashla => 0,
            CueStyle::Interface => 1,
            CueStyle::Positive => 2,
            CueStyle::Confirm => 3,
            CueStyle::Arcade => 4,
            CueStyle::Chime => 5,
        }
    }

    pub fn from_u8(value: u8) -> CueStyle {
        CueStyle::ALL
            .into_iter()
            .find(|style| style.as_u8() == value)
            .unwrap_or_default()
    }

    /// The bundled WAV for `cue`, when the style is asset based.
    fn asset(self, cue: Cue) -> Option<&'static [u8]> {
        let bytes: &'static [u8] = match (self, cue) {
            (CueStyle::Dashla, Cue::Start | Cue::Stop) => {
                include_bytes!("../assets/sounds/dashla-ding.wav")
            }
            (CueStyle::Interface, Cue::Start) => {
                include_bytes!("../assets/sounds/mixkit-interface-start.wav")
            }
            (CueStyle::Interface, Cue::Stop) => {
                include_bytes!("../assets/sounds/mixkit-interface-back.wav")
            }
            (CueStyle::Positive, Cue::Start) => {
                include_bytes!("../assets/sounds/mixkit-positive.wav")
            }
            (CueStyle::Positive, Cue::Stop) => {
                include_bytes!("../assets/sounds/mixkit-remove.wav")
            }
            (CueStyle::Confirm, Cue::Start) => {
                include_bytes!("../assets/sounds/mixkit-correct.wav")
            }
            (CueStyle::Confirm, Cue::Stop) => {
                include_bytes!("../assets/sounds/mixkit-confirm.wav")
            }
            (CueStyle::Arcade, Cue::Start) => {
                include_bytes!("../assets/sounds/kenney-maximize.wav")
            }
            (CueStyle::Arcade, Cue::Stop) => {
                include_bytes!("../assets/sounds/kenney-minimize.wav")
            }
            _ => return None,
        };
        Some(bytes)
    }
}

const SAMPLE_RATE: u32 = 44_100;
/// NSSound gain; the system output volume applies on top.
const VOLUME: f32 = 1.0;
/// Peak of a synthesised note (full scale is 1.0).
const PEAK: f32 = 0.6;
/// Longest cue, in milliseconds.
pub const MAX_CUE_MS: u32 = 1_200;
/// The synthesised chime: do and mi, in Hz.
const CHIME_DO: f32 = 659.3;
const CHIME_MI: f32 = 830.6;
/// Cancel: one low tap, in Hz.
const CANCEL_HZ: f32 = 392.0;

/// Pull the 16-bit mono samples out of a RIFF/WAVE file (the assets are
/// written in exactly that shape; anything else yields silence, not a panic).
fn decode_wav_mono_16(bytes: &[u8]) -> Vec<i16> {
    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;
        let body = offset + 8;
        if id == b"data" {
            let end = (body + size).min(bytes.len());
            return bytes[body..end]
                .chunks_exact(2)
                .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
                .collect();
        }
        offset = body + size + (size & 1);
    }
    Vec::new()
}

/// One sine note with a short ramp at both ends and a touch of second
/// harmonic so it does not read as a test tone.
fn synth_note(hz: f32, ms: u32) -> Vec<f32> {
    let count = (SAMPLE_RATE as f32 * ms as f32 / 1000.0) as usize;
    let ramp = (SAMPLE_RATE as f32 * 0.008) as usize;
    (0..count)
        .map(|index| {
            let t = index as f32 / SAMPLE_RATE as f32;
            let attack = if index < ramp {
                index as f32 / ramp as f32
            } else {
                1.0
            };
            let release = if index + ramp >= count {
                (count - index) as f32 / ramp as f32
            } else {
                1.0
            };
            let phase = 2.0 * PI * hz * t;
            (phase.sin() * 0.85 + (2.0 * phase).sin() * 0.15) * attack * release * PEAK
        })
        .collect()
}

fn to_i16(samples: Vec<f32>) -> Vec<i16> {
    samples
        .into_iter()
        .map(|sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
        .collect()
}

/// Mono 16-bit samples of the cue in `style`.
pub fn samples(style: CueStyle, cue: Cue) -> Vec<i16> {
    if let Some(bytes) = style.asset(cue) {
        return decode_wav_mono_16(bytes);
    }
    match cue {
        Cue::Cancel => to_i16(synth_note(CANCEL_HZ, 90)),
        Cue::Start => {
            let mut out = synth_note(CHIME_DO, 90);
            out.extend(synth_note(CHIME_MI, 120));
            to_i16(out)
        }
        Cue::Stop => {
            let mut out = synth_note(CHIME_MI, 90);
            out.extend(synth_note(CHIME_DO, 120));
            to_i16(out)
        }
    }
}

/// Duration of the cue in milliseconds.
pub fn cue_duration_ms(style: CueStyle, cue: Cue) -> u32 {
    (samples(style, cue).len() as u64 * 1000 / SAMPLE_RATE as u64) as u32
}

/// The cue as a RIFF/WAVE file in memory (PCM, mono, 16-bit).
pub fn wav_bytes(style: CueStyle, cue: Cue) -> Vec<u8> {
    if let Some(bytes) = style.asset(cue) {
        return bytes.to_vec();
    }
    let samples = samples(style, cue);
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    bytes.extend_from_slice(&2u16.to_le_bytes()); // block align
    bytes.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

/// Play `cue` in `style` when cues are switched on. Never blocks the caller;
/// playback failures are logged and otherwise ignored.
pub fn play(app: &tauri::AppHandle, enabled: bool, style: CueStyle, cue: Cue) {
    if !enabled {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        let bytes = wav_bytes(style, cue);
        if let Err(error) = app.run_on_main_thread(move || macos::play_on_main_thread(&bytes)) {
            tracing::warn!("Sound cue {cue:?} could not be scheduled: {error}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, style, cue);
    }
}

/// Settings preview: the start cue, then the stop cue once it has finished.
#[tauri::command]
pub async fn preview_sound_cue(app: tauri::AppHandle, style: String) -> Result<(), String> {
    let style = CueStyle::parse(&style);
    play(&app, true, style, Cue::Start);
    let wait = cue_duration_ms(style, Cue::Start) + 150;
    tokio::time::sleep(std::time::Duration::from_millis(wait as u64)).await;
    play(&app, true, style, Cue::Stop);
    Ok(())
}

#[cfg(target_os = "macos")]
mod macos {
    use std::cell::RefCell;

    use objc2_06::rc::Retained;
    use objc2_06::AnyThread;
    use objc2_app_kit::NSSound;
    use objc2_foundation::NSData;

    thread_local! {
        // The last sound is retained until the next one so it finishes
        // playing; NSSound stops when its last reference goes away.
        static CURRENT: RefCell<Option<Retained<NSSound>>> = const { RefCell::new(None) };
    }

    pub(super) fn play_on_main_thread(bytes: &[u8]) {
        let data = NSData::with_bytes(bytes);
        let Some(sound) = NSSound::initWithData(NSSound::alloc(), &data) else {
            tracing::warn!("Sound cue: NSSound rejected the WAV");
            return;
        };
        sound.setVolume(super::VOLUME);
        if !sound.play() {
            tracing::warn!("Sound cue: NSSound did not start");
        }
        CURRENT.with(|current| {
            *current.borrow_mut() = Some(sound);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_style_has_audible_cues_within_the_length_limit() {
        for style in CueStyle::ALL {
            for cue in [Cue::Start, Cue::Stop, Cue::Cancel] {
                let samples = samples(style, cue);
                assert!(!samples.is_empty(), "{style:?} {cue:?}");
                assert!(
                    cue_duration_ms(style, cue) <= MAX_CUE_MS,
                    "{style:?} {cue:?}"
                );
                let peak = samples
                    .iter()
                    .map(|sample| sample.unsigned_abs())
                    .max()
                    .unwrap_or(0);
                assert!(peak > 3_000, "{style:?} {cue:?} is too quiet ({peak})");
                assert!(peak <= i16::MAX as u16, "{style:?} {cue:?} clips");
            }
        }
    }

    #[test]
    fn asset_styles_have_distinct_start_and_stop_sounds_except_dashla() {
        for style in CueStyle::ALL {
            let same = samples(style, Cue::Start) == samples(style, Cue::Stop);
            assert_eq!(same, style == CueStyle::Dashla, "{style:?}");
        }
    }

    #[test]
    fn styles_round_trip_through_their_names_and_codes() {
        for style in CueStyle::ALL {
            assert_eq!(CueStyle::parse(style.as_str()), style);
            assert_eq!(CueStyle::from_u8(style.as_u8()), style);
        }
        assert_eq!(CueStyle::parse("nonsense"), CueStyle::Dashla);
    }

    #[test]
    fn wav_bytes_decode_back_to_the_samples() {
        for style in CueStyle::ALL {
            let bytes = wav_bytes(style, Cue::Start);
            assert_eq!(&bytes[0..4], b"RIFF");
            assert_eq!(&bytes[8..12], b"WAVE");
            assert_eq!(
                decode_wav_mono_16(&bytes),
                samples(style, Cue::Start),
                "{style:?}"
            );
        }
    }
}
