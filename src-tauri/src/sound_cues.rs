//! Short audible cues for the capsule: a rising blip when recording starts,
//! a falling one when it stops, a low tap on cancel. The tones are
//! synthesised here (no bundled audio, nothing to license) and played
//! through `NSSound` on macOS so no webview autoplay policy is involved.

use std::f32::consts::PI;
use std::sync::OnceLock;

/// Which moment to mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    Start,
    Stop,
    Cancel,
}

const SAMPLE_RATE: u32 = 44_100;
/// Playback gain; NSSound scales by the system output volume on top.
const VOLUME: f32 = 0.55;
/// Peak amplitude of the synthesised tone (full scale is 1.0).
const PEAK: f32 = 0.42;
/// Attack / release of each note, in seconds.
const RAMP_SECS: f32 = 0.008;
/// Longest cue, in milliseconds: the microphone must not count it as speech.
pub const MAX_CUE_MS: u32 = 120;

/// One note of a cue: frequency in Hz and length in milliseconds.
#[derive(Clone, Copy)]
struct Note {
    hz: f32,
    ms: u32,
}

fn notes(cue: Cue) -> &'static [Note] {
    match cue {
        // Rising fifth, like a device waking up.
        Cue::Start => &[Note { hz: 659.3, ms: 50 }, Note { hz: 987.8, ms: 65 }],
        // The same two notes down: done.
        Cue::Stop => &[Note { hz: 987.8, ms: 50 }, Note { hz: 659.3, ms: 65 }],
        // A single low tap.
        Cue::Cancel => &[Note { hz: 392.0, ms: 90 }],
    }
}

/// Duration of a cue in milliseconds.
pub fn cue_duration_ms(cue: Cue) -> u32 {
    notes(cue).iter().map(|note| note.ms).sum()
}

/// Mono 16-bit samples of the cue, each note shaped by a short ramp so there
/// is no click, and a gentle second harmonic so it does not sound like a
/// test tone.
pub fn samples(cue: Cue) -> Vec<i16> {
    let mut out = Vec::new();
    for note in notes(cue) {
        let count = (SAMPLE_RATE as f32 * note.ms as f32 / 1000.0) as usize;
        let ramp = (SAMPLE_RATE as f32 * RAMP_SECS) as usize;
        for index in 0..count {
            let t = index as f32 / SAMPLE_RATE as f32;
            let envelope = if index < ramp {
                index as f32 / ramp as f32
            } else if index + ramp >= count {
                (count - index) as f32 / ramp as f32
            } else {
                1.0
            };
            let phase = 2.0 * PI * note.hz * t;
            let wave = phase.sin() * 0.85 + (2.0 * phase).sin() * 0.15;
            out.push((wave * envelope * PEAK * i16::MAX as f32) as i16);
        }
    }
    out
}

/// The cue as a RIFF/WAVE file in memory (PCM, mono, 16-bit).
pub fn wav_bytes(cue: Cue) -> Vec<u8> {
    let samples = samples(cue);
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

fn cached_wav(cue: Cue) -> &'static [u8] {
    static START: OnceLock<Vec<u8>> = OnceLock::new();
    static STOP: OnceLock<Vec<u8>> = OnceLock::new();
    static CANCEL: OnceLock<Vec<u8>> = OnceLock::new();
    match cue {
        Cue::Start => START.get_or_init(|| wav_bytes(Cue::Start)),
        Cue::Stop => STOP.get_or_init(|| wav_bytes(Cue::Stop)),
        Cue::Cancel => CANCEL.get_or_init(|| wav_bytes(Cue::Cancel)),
    }
}

/// Play `cue` when the user has cues switched on. Never blocks the caller;
/// playback failures are logged and otherwise ignored.
pub fn play(app: &tauri::AppHandle, enabled: bool, cue: Cue) {
    if !enabled {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        let bytes = cached_wav(cue);
        if let Err(error) = app.run_on_main_thread(move || macos::play_on_main_thread(bytes)) {
            tracing::warn!("Sound cue {cue:?} could not be scheduled: {error}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, cue);
    }
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

    pub(super) fn play_on_main_thread(bytes: &'static [u8]) {
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
    fn cues_are_short_and_quiet_enough_for_the_microphone() {
        for cue in [Cue::Start, Cue::Stop, Cue::Cancel] {
            assert!(cue_duration_ms(cue) <= MAX_CUE_MS, "{cue:?}");
            let peak = samples(cue)
                .iter()
                .map(|sample| sample.unsigned_abs())
                .max()
                .unwrap_or(0);
            assert!(peak > 0, "{cue:?} is silent");
            assert!(peak as f32 <= PEAK * i16::MAX as f32 + 1.0, "{cue:?} clips");
        }
    }

    #[test]
    fn start_rises_and_stop_falls() {
        assert!(notes(Cue::Start)[0].hz < notes(Cue::Start)[1].hz);
        assert!(notes(Cue::Stop)[0].hz > notes(Cue::Stop)[1].hz);
    }

    #[test]
    fn wav_header_describes_the_samples() {
        let bytes = wav_bytes(Cue::Start);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        let data_len = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]) as usize;
        assert_eq!(data_len, bytes.len() - 44);
        assert_eq!(data_len, samples(Cue::Start).len() * 2);
        let rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
        assert_eq!(rate, SAMPLE_RATE);
    }
}
