//! Audio front end for the capture thread: anti-aliased resampling and
//! optional RNNoise suppression.
//!
//! The old path downsampled with linear interpolation, which aliases
//! everything above the target Nyquist (8 kHz) back into the speech band.
//! This module replaces it with a windowed-sinc resampler and, when enabled,
//! runs `nnnoiseless` (a pure-Rust RNNoise) at 48 kHz before decimating to
//! the 16 kHz the STT providers expect.
//!
//! All stages are stateful so audio can arrive in arbitrary callback sizes
//! (441, 480, 512 … samples) without dropping or duplicating samples.

use nnnoiseless::DenoiseState;

/// Sample rate RNNoise is trained for.
pub const DENOISE_RATE: u32 = 48_000;
/// RNNoise frame length in samples (10 ms at 48 kHz).
pub const DENOISE_FRAME: usize = DenoiseState::FRAME_SIZE;

/// Streaming windowed-sinc resampler (Hann window). Acts as an anti-aliasing
/// low-pass when decimating and as an interpolation filter when upsampling.
pub struct SincResampler {
    /// Input samples per output sample.
    ratio: f64,
    /// Low-pass cutoff in cycles per *input* sample (≤ 0.5).
    cutoff: f64,
    /// Kernel half width in input samples.
    half_taps: usize,
    /// Unconsumed input; the first `half_taps` entries are history.
    pending: Vec<f32>,
    /// Position of the next output sample within `pending`.
    pos: f64,
    passthrough: bool,
}

impl SincResampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Self {
        let from = f64::from(from_rate.max(1));
        let to = f64::from(to_rate.max(1));
        let ratio = from / to;
        // Cut just below the narrower Nyquist so the transition band does not alias.
        let cutoff = 0.5 * (1.0_f64).min(to / from) * 0.92;
        // Wider kernel when decimating: the low-pass needs more taps.
        let half_taps = (16.0 * ratio.max(1.0)).ceil() as usize;
        Self {
            ratio,
            cutoff,
            half_taps,
            pending: Vec::with_capacity(8192),
            pos: 0.0,
            passthrough: from_rate == to_rate,
        }
    }

    /// Append `input` and push every output sample that is now computable.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.passthrough {
            out.extend_from_slice(input);
            return;
        }
        self.pending.extend_from_slice(input);
        let half = self.half_taps as isize;
        // We need samples up to floor(pos) + half inclusive.
        while (self.pos.floor() as isize + half) < self.pending.len() as isize {
            let center = self.pos;
            let i0 = center.floor() as isize;
            let mut acc = 0.0f64;
            let mut weight_sum = 0.0f64;
            for k in (i0 - half + 1)..=(i0 + half) {
                if k < 0 {
                    continue;
                }
                let x = center - k as f64;
                let w = windowed_sinc(x, self.cutoff, self.half_taps as f64);
                acc += f64::from(self.pending[k as usize]) * w;
                weight_sum += w;
            }
            out.push(if weight_sum.abs() > 1e-9 {
                (acc / weight_sum) as f32
            } else {
                0.0
            });
            self.pos += self.ratio;
        }
        // Drop input that can no longer influence future outputs.
        let keep_from = (self.pos.floor() as isize - half).max(0) as usize;
        if keep_from > 0 {
            self.pending.drain(..keep_from);
            self.pos -= keep_from as f64;
        }
    }
}

fn windowed_sinc(x: f64, cutoff: f64, half_taps: f64) -> f64 {
    if x.abs() >= half_taps {
        return 0.0;
    }
    let window = 0.5 * (1.0 + (std::f64::consts::PI * x / half_taps).cos());
    let u = 2.0 * cutoff * x;
    let sinc = if u.abs() < 1e-9 {
        1.0
    } else {
        (std::f64::consts::PI * u).sin() / (std::f64::consts::PI * u)
    };
    2.0 * cutoff * sinc * window
}

/// Streaming RNNoise wrapper working on 48 kHz mono samples in the -1..1 range.
pub struct NoiseSuppressor {
    state: Box<DenoiseState<'static>>,
    pending: Vec<f32>,
    in_frame: [f32; DENOISE_FRAME],
    out_frame: [f32; DENOISE_FRAME],
    frames_done: u64,
}

impl NoiseSuppressor {
    pub fn new() -> Self {
        Self {
            state: DenoiseState::new(),
            pending: Vec::with_capacity(DENOISE_FRAME * 8),
            in_frame: [0.0; DENOISE_FRAME],
            out_frame: [0.0; DENOISE_FRAME],
            frames_done: 0,
        }
    }

    /// Append `input` (48 kHz, -1..1) and push denoised samples for every
    /// complete 10 ms frame. The sample count out equals the sample count in,
    /// modulo the sub-frame remainder that is carried to the next call.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        self.pending
            .extend(input.iter().map(|s| (s * 32768.0).clamp(-32768.0, 32767.0)));
        let mut consumed = 0;
        while self.pending.len() - consumed >= DENOISE_FRAME {
            self.in_frame
                .copy_from_slice(&self.pending[consumed..consumed + DENOISE_FRAME]);
            let _vad = self
                .state
                .process_frame(&mut self.out_frame, &self.in_frame);
            consumed += DENOISE_FRAME;
            if self.frames_done == 0 {
                // The very first frame carries fade-in artefacts; emit silence
                // instead so the sample count stays continuous.
                out.extend(std::iter::repeat_n(0.0, DENOISE_FRAME));
            } else {
                out.extend(self.out_frame.iter().map(|s| s / 32768.0));
            }
            self.frames_done += 1;
        }
        if consumed > 0 {
            self.pending.drain(..consumed);
        }
    }
}

impl Default for NoiseSuppressor {
    fn default() -> Self {
        Self::new()
    }
}

/// Device-rate mono → target-rate mono, with optional noise suppression.
pub struct AudioFrontEnd {
    to_denoise_rate: Option<SincResampler>,
    denoiser: Option<NoiseSuppressor>,
    to_target: SincResampler,
    stage_a: Vec<f32>,
    stage_b: Vec<f32>,
}

impl AudioFrontEnd {
    pub fn new(device_rate: u32, target_rate: u32, noise_suppression: bool) -> Self {
        if noise_suppression {
            Self {
                to_denoise_rate: Some(SincResampler::new(device_rate, DENOISE_RATE)),
                denoiser: Some(NoiseSuppressor::new()),
                to_target: SincResampler::new(DENOISE_RATE, target_rate),
                stage_a: Vec::with_capacity(8192),
                stage_b: Vec::with_capacity(8192),
            }
        } else {
            Self {
                to_denoise_rate: None,
                denoiser: None,
                to_target: SincResampler::new(device_rate, target_rate),
                stage_a: Vec::new(),
                stage_b: Vec::new(),
            }
        }
    }

    pub fn noise_suppression_enabled(&self) -> bool {
        self.denoiser.is_some()
    }

    /// Process one callback worth of mono samples, appending target-rate samples to `out`.
    pub fn process(&mut self, mono: &[f32], out: &mut Vec<f32>) {
        match (&mut self.to_denoise_rate, &mut self.denoiser) {
            (Some(up), Some(denoiser)) => {
                self.stage_a.clear();
                up.process(mono, &mut self.stage_a);
                self.stage_b.clear();
                denoiser.process(&self.stage_a, &mut self.stage_b);
                self.to_target.process(&self.stage_b, out);
            }
            _ => self.to_target.process(mono, out),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, hz: f32, amplitude: f32, seconds: f32) -> Vec<f32> {
        let n = (rate as f32 * seconds) as usize;
        (0..n)
            .map(|i| amplitude * (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin())
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    fn db(ratio: f32) -> f32 {
        20.0 * ratio.max(1e-12).log10()
    }

    fn run_resampler(from: u32, to: u32, input: &[f32], chunk: usize) -> Vec<f32> {
        let mut resampler = SincResampler::new(from, to);
        let mut out = Vec::new();
        for block in input.chunks(chunk) {
            resampler.process(block, &mut out);
        }
        out
    }

    #[test]
    fn decimation_rejects_tones_above_target_nyquist() {
        let input = tone(48_000, 10_000.0, 0.5, 1.0);
        let out = run_resampler(48_000, 16_000, &input, 480);
        let settled = &out[200..];
        let attenuation = db(rms(settled) / rms(&input));
        assert!(
            attenuation < -30.0,
            "10 kHz leaked through at {attenuation:.1} dB"
        );
    }

    #[test]
    fn decimation_keeps_speech_band_flat() {
        for hz in [300.0, 1_000.0, 3_000.0, 5_000.0] {
            let input = tone(48_000, hz, 0.5, 1.0);
            let out = run_resampler(48_000, 16_000, &input, 512);
            let settled = &out[200..];
            let gain = db(rms(settled) / rms(&input));
            assert!(gain.abs() < 1.0, "{hz} Hz gain {gain:.2} dB");
        }
    }

    #[test]
    fn upsampling_to_48k_keeps_level_and_count() {
        let input = tone(44_100, 1_000.0, 0.5, 1.0);
        let out = run_resampler(44_100, 48_000, &input, 441);
        let gain = db(rms(&out[200..]) / rms(&input));
        assert!(gain.abs() < 1.0, "gain {gain:.2} dB");
        // 1 s of input minus the kernel tail that is still pending.
        assert!((out.len() as i64 - 48_000).abs() < 64, "got {}", out.len());
    }

    #[test]
    fn output_count_is_independent_of_callback_size() {
        let input = tone(44_100, 440.0, 0.3, 2.0);
        let a = run_resampler(44_100, 16_000, &input, 441);
        let b = run_resampler(44_100, 16_000, &input, 512);
        let c = run_resampler(44_100, 16_000, &input, 7);
        assert_eq!(a.len(), b.len());
        assert_eq!(a.len(), c.len());
        assert!((a.len() as i64 - 32_000).abs() < 64, "got {}", a.len());
        // Same samples regardless of how the input was chunked.
        for (x, y) in a.iter().zip(c.iter()) {
            assert!((x - y).abs() < 1e-5);
        }
    }

    #[test]
    fn passthrough_when_rates_match() {
        let input = tone(16_000, 440.0, 0.3, 0.1);
        let out = run_resampler(16_000, 16_000, &input, 160);
        assert_eq!(out, input);
    }

    fn white_noise(len: usize, amplitude: f32) -> Vec<f32> {
        let mut seed = 0x1234_5678u32;
        (0..len)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let unit = (seed >> 8) as f32 / (1u32 << 24) as f32; // 0..1
                amplitude * (unit * 2.0 - 1.0)
            })
            .collect()
    }

    /// Harmonic-rich, pitch-gliding, syllable-modulated signal that RNNoise
    /// treats as speech-like.
    fn speech_like(rate: u32, seconds: f32) -> Vec<f32> {
        let n = (rate as f32 * seconds) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / rate as f32;
                let f0 = 140.0 + 30.0 * (2.0 * std::f32::consts::PI * 0.7 * t).sin();
                let envelope = 0.55 + 0.45 * (2.0 * std::f32::consts::PI * 4.0 * t).sin();
                let mut sample = 0.0;
                for harmonic in 1..=20 {
                    let hz = f0 * harmonic as f32;
                    if hz > 3_800.0 {
                        break;
                    }
                    sample += (2.0 * std::f32::consts::PI * hz * t).sin() / harmonic as f32;
                }
                0.12 * envelope * sample
            })
            .collect()
    }

    #[test]
    fn noise_suppressor_attenuates_white_noise_by_at_least_6_db() {
        let input = white_noise(48_000 * 3, 0.05);
        let mut suppressor = NoiseSuppressor::new();
        let mut out = Vec::new();
        for block in input.chunks(480) {
            suppressor.process(block, &mut out);
        }
        assert_eq!(out.len(), input.len());
        let settled_in = &input[48_000..];
        let settled_out = &out[48_000..];
        let change = db(rms(settled_out) / rms(settled_in));
        assert!(change <= -6.0, "noise only changed by {change:.1} dB");
    }

    #[test]
    fn noise_suppressor_keeps_speech_like_signal_within_6_db() {
        let input = speech_like(48_000, 3.0);
        let mut suppressor = NoiseSuppressor::new();
        let mut out = Vec::new();
        for block in input.chunks(512) {
            suppressor.process(block, &mut out);
        }
        let settled_in = &input[48_000..out.len()];
        let settled_out = &out[48_000..];
        let change = db(rms(settled_out) / rms(settled_in));
        assert!(change > -6.0, "speech-like signal lost {change:.1} dB");
    }

    #[test]
    fn noise_suppressor_carries_sub_frame_remainder() {
        let input = white_noise(1_000, 0.1);
        let mut suppressor = NoiseSuppressor::new();
        let mut out = Vec::new();
        for block in input.chunks(7) {
            suppressor.process(block, &mut out);
        }
        assert_eq!(out.len(), 960);
        assert_eq!(suppressor.pending.len(), 40);
    }

    #[test]
    fn front_end_with_noise_suppression_resamples_44k_to_16k() {
        let input = speech_like(44_100, 2.0);
        let mut front_end = AudioFrontEnd::new(44_100, 16_000, true);
        assert!(front_end.noise_suppression_enabled());
        let mut out = Vec::new();
        for block in input.chunks(441) {
            front_end.process(block, &mut out);
        }
        assert!((out.len() as i64 - 32_000).abs() < 400, "got {}", out.len());
        assert!(rms(&out[16_000..]) > 0.01);
    }

    #[test]
    fn front_end_without_noise_suppression_is_plain_resampling() {
        let input = tone(48_000, 1_000.0, 0.5, 1.0);
        let mut front_end = AudioFrontEnd::new(48_000, 16_000, false);
        assert!(!front_end.noise_suppression_enabled());
        let mut out = Vec::new();
        front_end.process(&input, &mut out);
        let gain = db(rms(&out[200..]) / rms(&input));
        assert!(gain.abs() < 1.0);
    }
}

/// Number of frequency bands reported to the capsule waveform.
pub const METER_BANDS: usize = 5;
/// Band centres in Hz: fundamentals, low formants, mid formants, consonants, sibilance.
pub const METER_BAND_HZ: [f32; METER_BANDS] = [150.0, 400.0, 1000.0, 2500.0, 5000.0];
/// Meter floor in dBFS; levels at or below this map to 0.
const BAND_FLOOR_DBFS: f32 = -60.0;

/// Overall level plus a tiny spectrum, both 0..1 on a dB scale.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct AudioMeter {
    pub level: f32,
    pub bands: [f32; METER_BANDS],
}

impl Default for AudioMeter {
    fn default() -> Self {
        Self {
            level: 0.0,
            bands: [0.0; METER_BANDS],
        }
    }
}

/// Goertzel band energies for one chunk. No allocation; the coefficients are
/// computed once per sample rate so this is safe inside the audio callback.
#[derive(Debug, Clone)]
pub struct BandAnalyzer {
    coeffs: [f32; METER_BANDS],
}

impl BandAnalyzer {
    pub fn new(sample_rate: u32) -> Self {
        let fs = sample_rate.max(1) as f32;
        let mut coeffs = [0.0f32; METER_BANDS];
        for (coeff, hz) in coeffs.iter_mut().zip(METER_BAND_HZ) {
            // Bands above Nyquist cannot exist at this rate; leave them silent.
            *coeff = if hz < fs / 2.0 {
                2.0 * (2.0 * std::f32::consts::PI * hz / fs).cos()
            } else {
                f32::NAN
            };
        }
        Self { coeffs }
    }

    /// Band levels 0..1 (dB scaled like the main meter) for `samples` in -1..1.
    pub fn analyze(&self, samples: &[f32]) -> [f32; METER_BANDS] {
        let mut out = [0.0f32; METER_BANDS];
        if samples.is_empty() {
            return out;
        }
        let n = samples.len() as f32;
        for (level, &coeff) in out.iter_mut().zip(&self.coeffs) {
            if coeff.is_nan() {
                continue;
            }
            let (mut s1, mut s2) = (0.0f32, 0.0f32);
            for &x in samples {
                let s0 = x + coeff * s1 - s2;
                s2 = s1;
                s1 = s0;
            }
            let power = (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0);
            // Amplitude of a full-scale sine at this bin would be ~1.0.
            let amplitude = 2.0 * power.sqrt() / n;
            *level = db_level(amplitude);
        }
        out
    }
}

/// Map a linear amplitude (0..1) to a 0..1 meter level on a dB scale.
pub fn db_level(amplitude: f32) -> f32 {
    if amplitude <= 0.0 {
        return 0.0;
    }
    let dbfs = 20.0 * amplitude.log10();
    ((dbfs - BAND_FLOOR_DBFS) / -BAND_FLOOR_DBFS).clamp(0.0, 1.0)
}

#[cfg(test)]
mod band_tests {
    use super::*;

    fn tone(hz: f32, rate: u32, n: usize, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin())
            .collect()
    }

    #[test]
    fn a_1khz_tone_lands_in_the_1khz_band() {
        let analyzer = BandAnalyzer::new(16_000);
        let bands = analyzer.analyze(&tone(1000.0, 16_000, 320, 0.5));
        let (best, _) = bands
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap();
        assert_eq!(best, 2, "bands={bands:?}");
        // The 1 kHz bin reads near -6 dBFS; neighbours sit well below it.
        assert!(bands[2] > 0.85, "bands={bands:?}");
        assert!(
            bands[1] < bands[2] - 0.15 && bands[3] < bands[2] - 0.15,
            "bands={bands:?}"
        );
    }

    #[test]
    fn silence_and_empty_input_are_all_zero() {
        let analyzer = BandAnalyzer::new(16_000);
        assert_eq!(analyzer.analyze(&[]), [0.0; METER_BANDS]);
        assert_eq!(analyzer.analyze(&[0.0; 320]), [0.0; METER_BANDS]);
    }

    #[test]
    fn bands_above_nyquist_stay_silent() {
        let analyzer = BandAnalyzer::new(8_000);
        let bands = analyzer.analyze(&tone(1000.0, 8_000, 160, 0.5));
        assert_eq!(bands[4], 0.0, "5 kHz does not exist at 8 kHz");
        assert!(bands[2] > 0.8);
    }

    #[test]
    fn db_level_maps_full_scale_to_one_and_floor_to_zero() {
        assert!((db_level(1.0) - 1.0).abs() < 1e-6);
        assert_eq!(db_level(0.0), 0.0);
        assert!((db_level(0.001) - 0.0).abs() < 1e-6);
        assert!((db_level(0.1) - 0.666_67).abs() < 1e-3);
    }
}
