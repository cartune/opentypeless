//! Microphone capture through Apple's voice-processing I/O (AVAudioEngine
//! with `setVoiceProcessingEnabled`).
//!
//! Why: once a FaceTime / phone call opens the built-in microphone in
//! voice-processing mode, macOS hands every *plain* HAL client (what cpal
//! opens) a near-silent stream, so dictation during a call reports "no
//! audio". A second voice-processing client receives the full processed
//! signal. Measured on 2026-10-08, see docs/m9-polish-notes.md.
//!
//! The engine is created and dropped on the capture thread and delivers mono
//! f32 frames at the device rate to the supplied callback.

use anyhow::{anyhow, Result};
use block2_06::RcBlock;
use objc2_06::rc::Retained;
use objc2_avf_audio::{AVAudioEngine, AVAudioInputNode, AVAudioPCMBuffer, AVAudioTime};
use std::ptr::NonNull;
use std::sync::Mutex;

/// Rate Apple's voice processing delivers on current Macs; verified after start.
pub const VPIO_SAMPLE_RATE: u32 = 48_000;

pub struct VoiceProcessingCapture {
    engine: Retained<AVAudioEngine>,
    input: Retained<AVAudioInputNode>,
    /// Sample rate of the frames handed to the callback.
    pub sample_rate: u32,
    /// Channels the tap reports (all carry the same processed signal).
    pub channels: u32,
}

impl VoiceProcessingCapture {
    /// Open the default microphone in voice-processing mode and stream mono
    /// f32 frames (channel 0 of the processed signal) to `on_audio`.
    pub fn start<F>(on_audio: F) -> Result<Self>
    where
        F: FnMut(&[f32]) + Send + 'static,
    {
        // SAFETY: AVAudioEngine is a plain Objective-C object; every call
        // below is on the thread that owns it until `drop`.
        let engine = unsafe { AVAudioEngine::new() };
        let input = unsafe { engine.inputNode() };
        unsafe { input.setVoiceProcessingEnabled_error(true) }
            .map_err(|error| anyhow!("setVoiceProcessingEnabled: {error}"))?;
        let format = unsafe { input.outputFormatForBus(0) };
        let sample_rate = unsafe { format.sampleRate() } as u32;
        let channels = unsafe { format.channelCount() };
        if sample_rate == 0 || channels == 0 {
            return Err(anyhow!(
                "voice-processing input has no usable format ({sample_rate} Hz, {channels} ch)"
            ));
        }

        // The tap block is `Fn`; serialize our `FnMut` sink behind a mutex.
        let sink = Mutex::new(on_audio);
        let block = RcBlock::new(
            move |buffer: NonNull<AVAudioPCMBuffer>, _when: NonNull<AVAudioTime>| {
                let mut on_audio = match sink.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                // SAFETY: AVFoundation hands us a live buffer for the duration
                // of the block; floatChannelData is valid for float formats.
                let buffer = unsafe { buffer.as_ref() };
                let frames = unsafe { buffer.frameLength() } as usize;
                let data = unsafe { buffer.floatChannelData() };
                if frames == 0 || data.is_null() {
                    return;
                }
                let stride = unsafe { buffer.stride() }.max(1);
                let channel0 = unsafe { *data }.as_ptr().cast_const();
                if stride == 1 {
                    let samples = unsafe { std::slice::from_raw_parts(channel0, frames) };
                    on_audio(samples);
                } else {
                    // Interleaved: pick channel 0 of each frame.
                    let all = unsafe { std::slice::from_raw_parts(channel0, frames * stride) };
                    let mono: Vec<f32> = all.iter().step_by(stride).copied().collect();
                    on_audio(&mono);
                }
            },
        );
        let tap_block = (&*block as *const block2_06::DynBlock<_>).cast_mut();
        unsafe { input.installTapOnBus_bufferSize_format_block(0, 1024, Some(&format), tap_block) };
        unsafe { engine.prepare() };
        unsafe { engine.startAndReturnError() }
            .map_err(|error| anyhow!("engine start: {error}"))?;
        tracing::info!(
            "Voice-processing capture started ({sample_rate} Hz, {channels} ch, vp={})",
            unsafe { input.isVoiceProcessingEnabled() }
        );
        Ok(Self {
            engine,
            input,
            sample_rate,
            channels,
        })
    }
}

impl Drop for VoiceProcessingCapture {
    fn drop(&mut self) {
        unsafe {
            self.input.removeTapOnBus(0);
            self.engine.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Manual: `cargo test vpio_capture_real -- --ignored --nocapture`, idle
    /// and while a call-like holder runs; prints frames and peak dBFS.
    #[test]
    #[ignore]
    fn vpio_capture_real() {
        let stats = Arc::new(Mutex::new((0usize, 0f32)));
        let s2 = stats.clone();
        let capture = VoiceProcessingCapture::start(move |frames| {
            let mut s = s2.lock().unwrap();
            s.0 += frames.len();
            for &x in frames {
                if x.abs() > s.1 {
                    s.1 = x.abs();
                }
            }
        })
        .unwrap();
        let speaker = std::process::Command::new("say")
            .args(["testing one two three four five six seven"])
            .spawn()
            .ok();
        std::thread::sleep(std::time::Duration::from_millis(2500));
        let (rate, channels) = (capture.sample_rate, capture.channels);
        drop(capture);
        if let Some(mut child) = speaker {
            let _ = child.kill();
        }
        let (frames, peak) = *stats.lock().unwrap();
        println!(
            "vpio rate={rate} ch={channels} frames={frames} peak={:.1} dBFS",
            20.0 * peak.max(1e-6).log10()
        );
    }
}
