//! Audible cues for the capsule: a two-note rise when recording starts, the
//! same two notes falling when it stops, a single low note on cancel. Four
//! styles; the default reuses the Dashla "traffic light, go" ding (an asset of
//! this company's own app) for both notes. Everything else is synthesised, so
//! nothing needs a licence. Playback is `NSSound` on macOS, scheduled from the
//! Rust state transitions, so no webview autoplay policy is involved.

use std::f32::consts::PI;
use std::sync::OnceLock;

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
    /// Dashla's 紅燈起步 ding, played do → mi and back.
    #[default]
    Dashla,
    /// Clean sine chime.
    Chime,
    /// Soft struck tone with a quick decay.
    Marimba,
    /// Bright bell-like tone.
    Glass,
}

impl CueStyle {
    pub const ALL: [CueStyle; 4] = [
        CueStyle::Dashla,
        CueStyle::Chime,
        CueStyle::Marimba,
        CueStyle::Glass,
    ];

    pub fn parse(value: &str) -> CueStyle {
        match value.trim().to_ascii_lowercase().as_str() {
            "chime" => CueStyle::Chime,
            "marimba" => CueStyle::Marimba,
            "glass" => CueStyle::Glass,
            _ => CueStyle::Dashla,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            CueStyle::Dashla => "dashla",
            CueStyle::Chime => "chime",
            CueStyle::Marimba => "marimba",
            CueStyle::Glass => "glass",
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            CueStyle::Dashla => 0,
            CueStyle::Chime => 1,
            CueStyle::Marimba => 2,
            CueStyle::Glass => 3,
        }
    }

    pub fn from_u8(value: u8) -> CueStyle {
        CueStyle::ALL
            .into_iter()
            .find(|style| style.as_u8() == value)
            .unwrap_or_default()
    }
}

const SAMPLE_RATE: u32 = 44_100;
/// NSSound gain; the system output volume applies on top.
const VOLUME: f32 = 1.0;
/// Peak of a synthesised note (full scale is 1.0).
const PEAK: f32 = 0.6;
/// Longest cue, in milliseconds.
pub const MAX_CUE_MS: u32 = 1_000;
/// Major third: the "mi" above "do".
const MAJOR_THIRD: f32 = 1.2599;
/// Gap between the two notes of a cue.
const NOTE_GAP_MS: u32 = 130;
/// Pitches of the two-note figure per style (do, mi), in Hz.
const CHIME_DO: f32 = 659.3;
const MARIMBA_DO: f32 = 523.3;
const GLASS_DO: f32 = 1046.5;

/// Two notes as pitch multipliers of the style's base note.
fn figure(cue: Cue) -> &'static [f32] {
    match cue {
        Cue::Start => &[1.0, MAJOR_THIRD],
        Cue::Stop => &[MAJOR_THIRD, 1.0],
        // A lone note a fourth below: unmistakably "no".
        Cue::Cancel => &[0.749],
    }
}

/// The Dashla ding: 700 ms of the traffic-light sound, faded and normalised
/// (`assets/sounds/dashla-ding.wav`, from dashla-app `assets/sounds/traffic-light.wav`).
fn dashla_ding() -> &'static [i16] {
    static DING: OnceLock<Vec<i16>> = OnceLock::new();
    DING.get_or_init(|| decode_wav_mono_16(include_bytes!("../assets/sounds/dashla-ding.wav")))
}

/// Pull the 16-bit mono samples out of a RIFF/WAVE file (the asset is written
/// in exactly that shape; anything else yields silence rather than a panic).
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

/// Play `source` faster or slower (linear interpolation), which shifts its
/// pitch by `ratio` and shortens it accordingly.
fn resample(source: &[i16], ratio: f32) -> Vec<f32> {
    if source.is_empty() || ratio <= 0.0 {
        return Vec::new();
    }
    let out_len = (source.len() as f32 / ratio) as usize;
    (0..out_len)
        .map(|index| {
            let position = index as f32 * ratio;
            let left = position.floor() as usize;
            let frac = position - left as f32;
            let a = source[left.min(source.len() - 1)] as f32;
            let b = source[(left + 1).min(source.len() - 1)] as f32;
            (a + (b - a) * frac) / i16::MAX as f32
        })
        .collect()
}

/// One synthesised note of `ms` at `hz`, shaped by `style`.
fn synth_note(style: CueStyle, hz: f32, ms: u32) -> Vec<f32> {
    let count = (SAMPLE_RATE as f32 * ms as f32 / 1000.0) as usize;
    let ramp = (SAMPLE_RATE as f32 * 0.006) as usize;
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
            let (wave, decay) = match style {
                CueStyle::Chime => (phase.sin() * 0.85 + (2.0 * phase).sin() * 0.15, 1.0),
                CueStyle::Marimba => (
                    phase.sin() * 0.8 + (4.0 * phase).sin() * 0.2 * (-t / 0.03).exp(),
                    (-t / 0.11).exp(),
                ),
                CueStyle::Glass => (
                    phase.sin() * 0.6
                        + (2.76 * phase).sin() * 0.25 * (-t / 0.06).exp()
                        + (5.4 * phase).sin() * 0.15 * (-t / 0.03).exp(),
                    (-t / 0.14).exp(),
                ),
                CueStyle::Dashla => (0.0, 0.0),
            };
            wave * attack * release * decay * PEAK
        })
        .collect()
}

/// Mix `note` into `out` starting at `offset`, growing `out` as needed.
fn mix_at(out: &mut Vec<f32>, offset: usize, note: &[f32]) {
    if out.len() < offset + note.len() {
        out.resize(offset + note.len(), 0.0);
    }
    for (index, sample) in note.iter().enumerate() {
        out[offset + index] += sample;
    }
}

/// Mono 16-bit samples of the cue in `style`.
pub fn samples(style: CueStyle, cue: Cue) -> Vec<i16> {
    let gap = (SAMPLE_RATE * NOTE_GAP_MS / 1000) as usize;
    let mut mixed: Vec<f32> = Vec::new();
    for (index, ratio) in figure(cue).iter().enumerate() {
        let note = match style {
            CueStyle::Dashla => {
                let mut note = resample(dashla_ding(), *ratio);
                // Two overlapping dings would clip; keep headroom.
                for sample in &mut note {
                    *sample *= 0.8;
                }
                note
            }
            CueStyle::Chime => synth_note(style, CHIME_DO * ratio, 110),
            CueStyle::Marimba => synth_note(style, MARIMBA_DO * ratio, 260),
            CueStyle::Glass => synth_note(style, GLASS_DO * ratio, 300),
        };
        mix_at(&mut mixed, index * gap, &note);
    }
    let limit = (SAMPLE_RATE * MAX_CUE_MS / 1000) as usize;
    mixed.truncate(limit);
    mixed
        .into_iter()
        .map(|sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
        .collect()
}

/// Duration of the cue in milliseconds.
pub fn cue_duration_ms(style: CueStyle, cue: Cue) -> u32 {
    (samples(style, cue).len() as u64 * 1000 / SAMPLE_RATE as u64) as u32
}

/// The cue as a RIFF/WAVE file in memory (PCM, mono, 16-bit).
pub fn wav_bytes(style: CueStyle, cue: Cue) -> Vec<u8> {
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
            tracing::warn!("Sound cue: NSSound rejected the synthesised WAV");
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
    fn start_rises_and_stop_falls() {
        assert!(figure(Cue::Start)[0] < figure(Cue::Start)[1]);
        assert!(figure(Cue::Stop)[0] > figure(Cue::Stop)[1]);
        assert_eq!(figure(Cue::Cancel).len(), 1);
    }

    #[test]
    fn dashla_asset_decodes() {
        let ding = dashla_ding();
        assert!(ding.len() > SAMPLE_RATE as usize / 2, "asset too short");
        assert!(ding.iter().any(|sample| sample.unsigned_abs() > 10_000));
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
    fn wav_header_describes_the_samples() {
        let bytes = wav_bytes(CueStyle::Chime, Cue::Start);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        let data_len = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]) as usize;
        assert_eq!(data_len, bytes.len() - 44);
        assert_eq!(data_len, samples(CueStyle::Chime, Cue::Start).len() * 2);
        assert_eq!(
            decode_wav_mono_16(&bytes),
            samples(CueStyle::Chime, Cue::Start)
        );
    }
}
