use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Sample;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

/// A 20 ms chunk whose RMS exceeds this (about -45 dBFS) counts as voiced.
const VOICED_RMS_THRESHOLD: f32 = 0.0056;
/// Meter floor in dBFS; levels at or below this map to 0.
const METER_FLOOR_DBFS: f32 = -60.0;

struct CaptureStartupNotifier {
    sender: Option<
        oneshot::Sender<std::result::Result<crate::recording_deadline::CaptureReadyAt, String>>,
    >,
}

struct CaptureStartupWaiter {
    receiver:
        oneshot::Receiver<std::result::Result<crate::recording_deadline::CaptureReadyAt, String>>,
}

fn capture_startup_channel() -> (CaptureStartupNotifier, CaptureStartupWaiter) {
    let (sender, receiver) = oneshot::channel();
    (
        CaptureStartupNotifier {
            sender: Some(sender),
        },
        CaptureStartupWaiter { receiver },
    )
}

impl CaptureStartupNotifier {
    fn ready(&mut self, ready_at: crate::recording_deadline::CaptureReadyAt) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(Ok(ready_at));
        }
    }

    fn failed(&mut self, message: String) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(Err(message));
        }
    }
}

impl CaptureStartupWaiter {
    async fn wait(self) -> std::result::Result<crate::recording_deadline::CaptureReadyAt, String> {
        self.receiver.await.unwrap_or_else(|_| {
            Err("Audio capture thread ended before reporting readiness".to_string())
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CaptureState {
    Idle,
    Starting,
    Recording,
}

fn initial_capture_state() -> CaptureState {
    CaptureState::Starting
}

#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub chunk_duration_ms: u32,
    /// Run RNNoise on the input before resampling to the target rate.
    pub noise_suppression: bool,
    /// Lower the system output volume to this percentage of itself while the
    /// microphone is open. `None` leaves the output alone.
    pub output_ducking: Option<u8>,
    /// When to open the microphone in voice-processing (call) mode.
    pub mic_sharing: MicSharingMode,
}

/// How to open the microphone when another process (a call) already uses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MicSharingMode {
    /// Voice-processing mode only while some other process holds the mic.
    #[default]
    Auto,
    /// Voice-processing mode for every recording.
    Always,
    /// Plain HAL stream, whatever else is running.
    Never,
}

impl MicSharingMode {
    pub fn from_config(value: &str) -> Self {
        match value {
            "always" => Self::Always,
            "never" => Self::Never,
            _ => Self::Auto,
        }
    }
}

/// Which capture backend to use for this recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureBackend {
    /// cpal / plain HAL input unit.
    Hal,
    /// Apple voice-processing I/O, shared with an active call.
    VoiceProcessing,
}

impl CaptureBackend {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hal => "hal",
            Self::VoiceProcessing => "vpio",
        }
    }
}

/// Pure decision used by the capture thread.
pub fn select_capture_backend(mode: MicSharingMode, mic_in_use_elsewhere: bool) -> CaptureBackend {
    match mode {
        MicSharingMode::Never => CaptureBackend::Hal,
        MicSharingMode::Always => CaptureBackend::VoiceProcessing,
        MicSharingMode::Auto if mic_in_use_elsewhere => CaptureBackend::VoiceProcessing,
        MicSharingMode::Auto => CaptureBackend::Hal,
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            channels: 1,
            chunk_duration_ms: 20,
            noise_suppression: false,
            output_ducking: None,
            mic_sharing: MicSharingMode::default(),
        }
    }
}

impl AudioConfig {
    /// Default capture config with the user's noise-suppression and output
    /// ducking preferences applied.
    pub fn for_app_config(config: &crate::storage::AppConfig) -> Self {
        Self {
            noise_suppression: config.noise_suppression_enabled,
            output_ducking: config
                .audio_ducking_enabled
                .then_some(config.audio_ducking_level),
            mic_sharing: MicSharingMode::from_config(&config.mic_sharing_mode),
            ..Self::default()
        }
    }
}

/// Maximum audio buffer size in samples before we stop accumulating.
/// ~24 MB of i16 samples ≈ 12.5 min at 16kHz mono, matching the STT provider limits.
const MAX_BUFFER_SAMPLES: usize = 12 * 1024 * 1024;
const AUDIO_CHANNEL_BUFFER_DURATION_MS: u32 = 60_000;

fn audio_channel_capacity(config: &AudioConfig) -> usize {
    let chunk_duration_ms = config.chunk_duration_ms.max(1);
    AUDIO_CHANNEL_BUFFER_DURATION_MS.div_ceil(chunk_duration_ms) as usize
}

/// Handle to control audio capture running on a dedicated thread.
/// This is Send + Sync safe because it only holds channels and atomic state.
/// What the capture thread learned about the input, for the diagnostics line
/// logged when a recording ends.
#[derive(Debug, Clone, Default)]
pub struct CaptureDiagnostics {
    /// "hal" or "vpio".
    pub mode: String,
    pub device: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub noise_suppression: bool,
    pub ducking: bool,
    pub total_chunks: u32,
    pub voiced_chunks: u32,
    /// Loudest processed sample over the whole recording, in dBFS.
    pub peak_dbfs: f32,
}

impl std::fmt::Display for CaptureDiagnostics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "mode={} device=\"{}\" {}Hz/{}ch ns={} duck={} chunks={} voiced={} peak={:.1}dBFS",
            self.mode,
            self.device,
            self.sample_rate,
            self.channels,
            self.noise_suppression,
            self.ducking,
            self.total_chunks,
            self.voiced_chunks,
            self.peak_dbfs
        )
    }
}

pub struct AudioCaptureHandle {
    stop_tx: Option<std::sync::mpsc::Sender<()>>,
    startup_waiter: Option<CaptureStartupWaiter>,
    volume: Arc<Mutex<f32>>,
    meter: Arc<Mutex<super::dsp::AudioMeter>>,
    state: Arc<Mutex<CaptureState>>,
    voiced_chunks: Arc<AtomicU32>,
    total_chunks: Arc<AtomicU32>,
    /// Peak linear amplitude so far, stored as f32 bits.
    peak: Arc<AtomicU32>,
    device_info: Arc<Mutex<Option<(String, u32, u16)>>>,
    backend: Arc<Mutex<CaptureBackend>>,
    noise_suppression: bool,
    diagnostics_logged: bool,
    /// Restores the system output volume when capture ends (any path).
    duck: Option<super::ducking::OutputDuckGuard>,
}

impl AudioCaptureHandle {
    /// Start audio capture on a dedicated thread. Returns a handle and a receiver for audio chunks.
    pub fn start(config: AudioConfig) -> Result<(Self, mpsc::Receiver<Vec<u8>>)> {
        let duck = config
            .output_ducking
            .map(super::ducking::OutputDuckGuard::duck);
        let (audio_tx, audio_rx) = mpsc::channel::<Vec<u8>>(audio_channel_capacity(&config));
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let volume = Arc::new(Mutex::new(0.0f32));
        let state = Arc::new(Mutex::new(initial_capture_state()));
        let meter = Arc::new(Mutex::new(super::dsp::AudioMeter::default()));
        let voiced_chunks = Arc::new(AtomicU32::new(0));
        let total_chunks = Arc::new(AtomicU32::new(0));
        let peak = Arc::new(AtomicU32::new(0f32.to_bits()));
        let device_info = Arc::new(Mutex::new(None));
        let backend = Arc::new(Mutex::new(CaptureBackend::Hal));
        let noise_suppression = config.noise_suppression;
        let (mut startup_notifier, startup_waiter) = capture_startup_channel();

        let state_clone = state.clone();
        let failed_state = state.clone();
        let shared = CaptureShared {
            volume: volume.clone(),
            meter: meter.clone(),
            voiced_chunks: voiced_chunks.clone(),
            total_chunks: total_chunks.clone(),
            peak: peak.clone(),
            device_info: device_info.clone(),
            backend: backend.clone(),
        };

        // Audio capture must run on a dedicated OS thread because cpal::Stream is !Send
        std::thread::spawn(move || {
            if let Err(e) = run_capture(
                config,
                audio_tx,
                stop_rx,
                state_clone,
                shared,
                &mut startup_notifier,
            ) {
                *failed_state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = CaptureState::Idle;
                startup_notifier.failed(e.to_string());
                tracing::error!("Audio capture thread error: {}", e);
            }
        });

        Ok((
            Self {
                stop_tx: Some(stop_tx),
                startup_waiter: Some(startup_waiter),
                volume,
                meter,
                state,
                voiced_chunks,
                total_chunks,
                peak,
                device_info,
                backend,
                noise_suppression,
                diagnostics_logged: false,
                duck,
            },
            audio_rx,
        ))
    }

    /// Wait until the platform backend has opened the input stream and
    /// `play()` has succeeded. The CPAL boundary is shared by CoreAudio,
    /// WASAPI, ALSA and PipeWire, so callers do not need platform delays.
    pub async fn wait_until_ready(&mut self) -> Result<crate::recording_deadline::CaptureReadyAt> {
        let waiter = self
            .startup_waiter
            .take()
            .ok_or_else(|| anyhow::anyhow!("Audio capture readiness was already consumed"))?;
        waiter.wait().await.map_err(anyhow::Error::msg)
    }

    /// Snapshot of what was captured so far.
    pub fn diagnostics(&self) -> CaptureDiagnostics {
        let (device, sample_rate, channels) = self
            .device_info
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .unwrap_or_else(|| ("(not opened)".to_string(), 0, 0));
        let peak = f32::from_bits(self.peak.load(Ordering::Relaxed));
        CaptureDiagnostics {
            mode: self
                .backend
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .label()
                .to_string(),
            device,
            sample_rate,
            channels,
            noise_suppression: self.noise_suppression,
            ducking: self.duck.as_ref().is_some_and(|d| d.is_active()),
            total_chunks: self.total_chunks.load(Ordering::Relaxed),
            voiced_chunks: self.voiced_chunks.load(Ordering::Relaxed),
            peak_dbfs: if peak > 0.0 {
                20.0 * peak.log10()
            } else {
                f32::NEG_INFINITY
            },
        }
    }

    pub fn stop(&mut self) {
        if !self.diagnostics_logged {
            self.diagnostics_logged = true;
            tracing::info!("Recording diagnostics: {}", self.diagnostics());
        }
        // Signal the capture thread to stop
        self.stop_tx = None;
        // Give the output volume back the moment the key is released.
        self.duck = None;
        *self.volume.lock().unwrap_or_else(|e| e.into_inner()) = 0.0;
        *self.meter.lock().unwrap_or_else(|e| e.into_inner()) = super::dsp::AudioMeter::default();
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = CaptureState::Idle;
    }

    pub fn get_volume(&self) -> f32 {
        *self.volume.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Latest level + band snapshot for the capsule waveform.
    pub fn get_meter(&self) -> super::dsp::AudioMeter {
        *self.meter.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Shared counter of 20 ms chunks that contained audible signal so far.
    pub fn voiced_counter(&self) -> Arc<AtomicU32> {
        self.voiced_chunks.clone()
    }

    pub fn state(&self) -> CaptureState {
        *self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Mix multi-channel audio down to mono by averaging channels.
fn to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    let ch = channels as usize;
    samples
        .chunks(ch)
        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
        .collect()
}

fn samples_to_f32<T>(samples: &[T]) -> Vec<f32>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    samples.iter().copied().map(f32::from_sample).collect()
}

/// State shared between the capture thread and the handle: live meter plus
/// the counters behind the diagnostics line.
struct CaptureShared {
    volume: Arc<Mutex<f32>>,
    meter: Arc<Mutex<super::dsp::AudioMeter>>,
    voiced_chunks: Arc<AtomicU32>,
    total_chunks: Arc<AtomicU32>,
    peak: Arc<AtomicU32>,
    device_info: Arc<Mutex<Option<(String, u32, u16)>>>,
    backend: Arc<Mutex<CaptureBackend>>,
}

struct InputProcessingContext {
    device_channels: u16,
    target_channels: u16,
    samples_per_chunk: usize,
    sender: mpsc::Sender<Vec<u8>>,
    volume: Arc<Mutex<f32>>,
    meter: Arc<Mutex<super::dsp::AudioMeter>>,
    bands: super::dsp::BandAnalyzer,
    buffer: Arc<Mutex<Vec<i16>>>,
    voiced_chunks: Arc<AtomicU32>,
    total_chunks: Arc<AtomicU32>,
    peak: Arc<AtomicU32>,
    /// Resampling (+ optional denoising) state; lives on the capture thread.
    front_end: super::dsp::AudioFrontEnd,
    /// Pre-allocated output scratch for one callback.
    processed: Vec<f32>,
}

/// Map a linear RMS (0..1) to a 0..1 meter level on a dB scale between
/// `METER_FLOOR_DBFS` and 0 dBFS, so quiet speech is still visible.
fn meter_level(rms: f32) -> f32 {
    if rms.is_nan() || rms <= 0.0 {
        return 0.0;
    }
    let dbfs = 20.0 * rms.log10();
    ((dbfs - METER_FLOOR_DBFS) / -METER_FLOOR_DBFS).clamp(0.0, 1.0)
}

fn rms_i16(chunk: &[i16]) -> f32 {
    if chunk.is_empty() {
        return 0.0;
    }
    let sum: f64 = chunk
        .iter()
        .map(|&s| {
            let v = s as f64 / 32768.0;
            v * v
        })
        .sum();
    (sum / chunk.len() as f64).sqrt() as f32
}

fn chunk_is_voiced(chunk: &[i16]) -> bool {
    rms_i16(chunk) > VOICED_RMS_THRESHOLD
}

fn normalized_rms(data: &[f32]) -> f32 {
    if data.is_empty() {
        return 0.0;
    }

    let rms = (data.iter().map(|sample| sample * sample).sum::<f32>() / data.len() as f32).sqrt();
    if rms.is_finite() {
        rms.min(1.0)
    } else {
        0.0
    }
}

fn process_input_samples(data: &[f32], context: &mut InputProcessingContext) {
    if data.is_empty() {
        return;
    }

    // Convert to mono if needed
    let mono = if context.device_channels > context.target_channels {
        to_mono(data, context.device_channels)
    } else {
        data.to_vec()
    };

    // Anti-aliased resample (and denoise when enabled) to the target rate.
    context.processed.clear();
    context.front_end.process(&mono, &mut context.processed);
    if context.processed.is_empty() {
        return;
    }

    // Meter and voiced gate both look at the processed signal, so with noise
    // suppression on, steady fan noise no longer registers as "audio".
    let level = meter_level(normalized_rms(&context.processed));
    if let Ok(mut volume) = context.volume.lock() {
        *volume = level;
    }
    let bands = context.bands.analyze(&context.processed);
    if let Ok(mut meter) = context.meter.lock() {
        *meter = super::dsp::AudioMeter { level, bands };
    }
    let chunk_peak = context
        .processed
        .iter()
        .fold(0f32, |acc, sample| acc.max(sample.abs()));
    if chunk_peak > f32::from_bits(context.peak.load(Ordering::Relaxed)) {
        context.peak.store(chunk_peak.to_bits(), Ordering::Relaxed);
    }

    // Convert f32 to i16 PCM and buffer
    let mut buffer = context
        .buffer
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    for &sample in &context.processed {
        if buffer.len() >= MAX_BUFFER_SAMPLES {
            break;
        }
        let sample = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
        buffer.push(sample);
    }

    // Send complete chunks
    while buffer.len() >= context.samples_per_chunk {
        let chunk: Vec<i16> = buffer.drain(..context.samples_per_chunk).collect();
        context.total_chunks.fetch_add(1, Ordering::Relaxed);
        if chunk_is_voiced(&chunk) {
            context.voiced_chunks.fetch_add(1, Ordering::Relaxed);
        }
        let bytes: Vec<u8> = chunk
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();
        let _ = context.sender.try_send(bytes);
    }
}

fn build_input_stream_for_sample<T>(
    device: &cpal::Device,
    stream_config: &cpal::StreamConfig,
    mut context: InputProcessingContext,
) -> std::result::Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    device.build_input_stream(
        stream_config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let samples = samples_to_f32(data);
            process_input_samples(&samples, &mut context);
        },
        |error| {
            tracing::error!("Audio capture error: {}", error);
        },
        None,
    )
}

fn run_capture(
    config: AudioConfig,
    sender: mpsc::Sender<Vec<u8>>,
    stop_rx: std::sync::mpsc::Receiver<()>,
    state: Arc<Mutex<CaptureState>>,
    shared: CaptureShared,
    startup_notifier: &mut CaptureStartupNotifier,
) -> Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow::anyhow!("No input device available"))?;

    let device_description = device
        .description()
        .map(|description| description.name().to_string())
        .unwrap_or_else(|_| "Default microphone".to_string());
    tracing::info!("Using input device: {}", device_description);

    #[cfg(target_os = "macos")]
    {
        let mic_busy = super::coreaudio::mic_in_use_elsewhere();
        let backend = select_capture_backend(config.mic_sharing, mic_busy);
        tracing::info!(
            "Capture backend: {:?} (mic_sharing={:?}, mic_in_use_elsewhere={mic_busy})",
            backend,
            config.mic_sharing
        );
        if backend == CaptureBackend::VoiceProcessing {
            *shared.backend.lock().unwrap_or_else(|e| e.into_inner()) = backend;
            return run_voice_processing_capture(
                config,
                device_description,
                sender,
                stop_rx,
                state,
                shared,
                startup_notifier,
            );
        }
    }

    // Use the device's default config instead of forcing 16kHz mono
    let default_config = device.default_input_config()?;
    let device_sample_rate = default_config.sample_rate();
    let device_channels = default_config.channels();
    let device_sample_format = default_config.sample_format();

    tracing::info!(
        "Device default config: {}Hz, {} channels, format: {:?}",
        device_sample_rate,
        device_channels,
        device_sample_format
    );

    *shared.device_info.lock().unwrap_or_else(|e| e.into_inner()) = Some((
        device_description.clone(),
        device_sample_rate,
        device_channels,
    ));

    let stream_config = cpal::StreamConfig {
        channels: device_channels,
        sample_rate: device_sample_rate,
        buffer_size: cpal::BufferSize::Default,
    };

    let target_rate = config.sample_rate;
    let target_channels = config.channels;
    let samples_per_chunk = (target_rate * config.chunk_duration_ms / 1000) as usize;
    let buffer: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::with_capacity(samples_per_chunk)));

    let processing_context = InputProcessingContext {
        device_channels,
        target_channels,
        samples_per_chunk,
        sender,
        volume: shared.volume,
        meter: shared.meter,
        bands: super::dsp::BandAnalyzer::new(target_rate),
        buffer,
        voiced_chunks: shared.voiced_chunks,
        total_chunks: shared.total_chunks,
        peak: shared.peak,
        front_end: super::dsp::AudioFrontEnd::new(
            device_sample_rate,
            target_rate,
            config.noise_suppression,
        ),
        processed: Vec::with_capacity(8192),
    };
    if config.noise_suppression {
        tracing::info!("Noise suppression (RNNoise) enabled for this capture");
    }

    let stream = match device_sample_format {
        cpal::SampleFormat::F32 => {
            build_input_stream_for_sample::<f32>(&device, &stream_config, processing_context)
        }
        cpal::SampleFormat::F64 => {
            build_input_stream_for_sample::<f64>(&device, &stream_config, processing_context)
        }
        cpal::SampleFormat::I8 => {
            build_input_stream_for_sample::<i8>(&device, &stream_config, processing_context)
        }
        cpal::SampleFormat::I16 => {
            build_input_stream_for_sample::<i16>(&device, &stream_config, processing_context)
        }
        cpal::SampleFormat::I24 => {
            build_input_stream_for_sample::<cpal::I24>(&device, &stream_config, processing_context)
        }
        cpal::SampleFormat::I32 => {
            build_input_stream_for_sample::<i32>(&device, &stream_config, processing_context)
        }
        cpal::SampleFormat::I64 => {
            build_input_stream_for_sample::<i64>(&device, &stream_config, processing_context)
        }
        cpal::SampleFormat::U8 => {
            build_input_stream_for_sample::<u8>(&device, &stream_config, processing_context)
        }
        cpal::SampleFormat::U16 => {
            build_input_stream_for_sample::<u16>(&device, &stream_config, processing_context)
        }
        cpal::SampleFormat::U24 => {
            build_input_stream_for_sample::<cpal::U24>(&device, &stream_config, processing_context)
        }
        cpal::SampleFormat::U32 => {
            build_input_stream_for_sample::<u32>(&device, &stream_config, processing_context)
        }
        cpal::SampleFormat::U64 => {
            build_input_stream_for_sample::<u64>(&device, &stream_config, processing_context)
        }
        cpal::SampleFormat::DsdU8 | cpal::SampleFormat::DsdU16 | cpal::SampleFormat::DsdU32 => {
            return Err(anyhow::anyhow!(
                "Unsupported DSD input sample format: {device_sample_format}"
            ));
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Unsupported input sample format: {device_sample_format}"
            ));
        }
    }?;

    stream.play()?;
    let capture_ready_at = crate::recording_deadline::CaptureReadyAt::now();
    *state.lock().unwrap_or_else(|e| e.into_inner()) = CaptureState::Recording;
    startup_notifier.ready(capture_ready_at);
    tracing::info!(
        "Audio capture started (device: {}Hz {}ch -> target: {}Hz {}ch)",
        device_sample_rate,
        device_channels,
        target_rate,
        target_channels
    );

    // Block until stop signal (sender dropped)
    let _ = stop_rx.recv();

    // Stream is dropped here, stopping capture
    drop(stream);
    *state.lock().unwrap_or_else(|e| e.into_inner()) = CaptureState::Idle;
    tracing::info!("Audio capture stopped");
    Ok(())
}

/// Capture through Apple's voice-processing I/O so a microphone already held
/// by a call still delivers audio. Mirrors the cpal path: same processing
/// context, same chunking, same stop protocol.
#[cfg(target_os = "macos")]
fn run_voice_processing_capture(
    config: AudioConfig,
    device_description: String,
    sender: mpsc::Sender<Vec<u8>>,
    stop_rx: std::sync::mpsc::Receiver<()>,
    state: Arc<Mutex<CaptureState>>,
    shared: CaptureShared,
    startup_notifier: &mut CaptureStartupNotifier,
) -> Result<()> {
    let target_rate = config.sample_rate;
    let target_channels = config.channels;
    let samples_per_chunk = (target_rate * config.chunk_duration_ms / 1000) as usize;
    let buffer: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::with_capacity(samples_per_chunk)));

    // The engine reports its rate only once started, but the front end needs
    // it up front. Voice processing runs at 48 kHz on every Mac we have seen;
    // verify after start and bail out loudly if it differs.
    let device_rate = super::vpio::VPIO_SAMPLE_RATE;
    let mut context = InputProcessingContext {
        device_channels: 1,
        target_channels,
        samples_per_chunk,
        sender,
        volume: shared.volume,
        meter: shared.meter,
        bands: super::dsp::BandAnalyzer::new(target_rate),
        buffer,
        voiced_chunks: shared.voiced_chunks,
        total_chunks: shared.total_chunks,
        peak: shared.peak,
        front_end: super::dsp::AudioFrontEnd::new(
            device_rate,
            target_rate,
            config.noise_suppression,
        ),
        processed: Vec::with_capacity(8192),
    };
    let capture = super::vpio::VoiceProcessingCapture::start(move |mono| {
        process_input_samples(mono, &mut context);
    })?;
    if capture.sample_rate != device_rate {
        return Err(anyhow::anyhow!(
            "voice-processing input runs at {} Hz, expected {}",
            capture.sample_rate,
            device_rate
        ));
    }
    *shared.device_info.lock().unwrap_or_else(|e| e.into_inner()) = Some((
        format!("{device_description} (voice processing)"),
        capture.sample_rate,
        1,
    ));

    let capture_ready_at = crate::recording_deadline::CaptureReadyAt::now();
    *state.lock().unwrap_or_else(|e| e.into_inner()) = CaptureState::Recording;
    startup_notifier.ready(capture_ready_at);
    tracing::info!(
        "Audio capture started via voice processing ({}Hz 1ch -> target: {}Hz {}ch)",
        capture.sample_rate,
        target_rate,
        target_channels
    );

    let _ = stop_rx.recv();
    drop(capture);
    *state.lock().unwrap_or_else(|e| e.into_inner()) = CaptureState::Idle;
    tracing::info!("Audio capture stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn backend_follows_mic_sharing_mode_and_whether_a_call_holds_the_mic() {
        assert_eq!(
            select_capture_backend(MicSharingMode::Auto, false),
            CaptureBackend::Hal
        );
        assert_eq!(
            select_capture_backend(MicSharingMode::Auto, true),
            CaptureBackend::VoiceProcessing
        );
        assert_eq!(
            select_capture_backend(MicSharingMode::Never, true),
            CaptureBackend::Hal
        );
        assert_eq!(
            select_capture_backend(MicSharingMode::Always, false),
            CaptureBackend::VoiceProcessing
        );
        assert_eq!(
            MicSharingMode::from_config("always"),
            MicSharingMode::Always
        );
        assert_eq!(MicSharingMode::from_config("bogus"), MicSharingMode::Auto);
        assert_eq!(CaptureBackend::VoiceProcessing.label(), "vpio");
    }

    #[test]
    fn capture_does_not_report_recording_before_the_backend_is_ready() {
        assert_eq!(initial_capture_state(), CaptureState::Starting);
    }

    #[test]
    fn audio_queue_preserves_a_minute_while_the_provider_connects() {
        assert_eq!(audio_channel_capacity(&AudioConfig::default()), 3_000);
    }

    #[test]
    fn converts_f32_input_samples_without_changing_values() {
        assert_eq!(
            samples_to_f32(&[-1.0_f32, 0.0, 0.5, 1.0]),
            vec![-1.0, 0.0, 0.5, 1.0]
        );
    }

    #[test]
    fn converts_i16_input_samples_to_normalized_f32() {
        assert_eq!(
            samples_to_f32(&[i16::MIN, 0, i16::MAX]),
            vec![-1.0, 0.0, i16::MAX as f32 / 32768.0]
        );
    }

    #[test]
    fn converts_u16_input_samples_around_unsigned_equilibrium() {
        assert_eq!(
            samples_to_f32(&[u16::MIN, 32768, u16::MAX]),
            vec![-1.0, 0.0, (u16::MAX as f32 - 32768.0) / 32768.0]
        );
    }

    #[test]
    fn empty_input_reports_zero_volume() {
        assert_eq!(normalized_rms(&[]), 0.0);
    }

    #[test]
    fn non_finite_input_reports_zero_volume() {
        assert_eq!(normalized_rms(&[f32::NAN]), 0.0);
        assert_eq!(normalized_rms(&[f32::INFINITY]), 0.0);
    }

    #[tokio::test]
    async fn capture_startup_waits_for_the_backend_ready_signal() {
        let (_notifier, waiter) = capture_startup_channel();

        assert!(
            tokio::time::timeout(Duration::from_millis(20), waiter.wait())
                .await
                .is_err(),
            "capture startup completed before the backend reported readiness"
        );
    }

    #[tokio::test]
    async fn capture_startup_completes_after_the_backend_is_ready() {
        let (mut notifier, waiter) = capture_startup_channel();
        let ready_at = crate::recording_deadline::CaptureReadyAt::now();
        notifier.ready(ready_at);

        let observed = waiter.wait().await.unwrap();
        assert_eq!(observed.unix_millis, ready_at.unix_millis);
        assert_eq!(observed.monotonic, ready_at.monotonic);
    }

    #[tokio::test]
    async fn capture_startup_propagates_backend_failure() {
        let (mut notifier, waiter) = capture_startup_channel();
        notifier.failed("input device unavailable".to_string());

        assert_eq!(
            waiter.wait().await,
            Err("input device unavailable".to_string())
        );
    }
}

#[cfg(test)]
mod voiced_tests {
    use super::*;

    fn tone(amplitude: f32, len: usize) -> Vec<i16> {
        (0..len)
            .map(|i| {
                let t = i as f32 / 16_000.0;
                (amplitude * (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 32767.0) as i16
            })
            .collect()
    }

    #[test]
    fn silence_is_not_voiced() {
        assert!(!chunk_is_voiced(&vec![0i16; 320]));
        // Low-level hiss around -70 dBFS stays below the gate.
        assert!(!chunk_is_voiced(&tone(0.0003, 320)));
    }

    #[test]
    fn speech_level_tone_is_voiced() {
        // Quiet speech around -35 dBFS.
        assert!(chunk_is_voiced(&tone(0.018, 320)));
        assert!(chunk_is_voiced(&tone(0.3, 320)));
    }

    #[test]
    fn meter_level_is_log_scaled() {
        assert_eq!(meter_level(0.0), 0.0);
        assert_eq!(meter_level(1.0), 1.0);
        // -20 dBFS sits two thirds up a -60 dB meter.
        assert!((meter_level(0.1) - 2.0 / 3.0).abs() < 0.01);
        // -60 dBFS is the floor.
        assert!(meter_level(0.001) < 0.01);
        assert!(meter_level(0.0001) == 0.0);
    }
}

#[cfg(test)]
mod real_mic_tests {
    use super::*;

    /// Manual hardware check: opens the real default microphone for ~2.5 s
    /// while `say` plays a sentence through the speakers, then prints the
    /// diagnostics line. Run with
    /// `cargo test capture_real_mic -- --ignored --nocapture`, once alone and
    /// once while another process holds the mic in voice-processing mode.
    #[test]
    #[ignore]
    fn capture_real_mic_peak() {
        // MIC_DUCK=75 forces ducking, MIC_SHARING=always forces the vpio path.
        let config = AudioConfig {
            output_ducking: std::env::var("MIC_DUCK").ok().and_then(|v| v.parse().ok()),
            mic_sharing: MicSharingMode::from_config(
                &std::env::var("MIC_SHARING").unwrap_or_else(|_| "auto".to_string()),
            ),
            ..AudioConfig::default()
        };
        let (mut handle, mut rx) = AudioCaptureHandle::start(config).unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            handle.wait_until_ready().await.unwrap();
            let speaker = std::process::Command::new("say")
                .args(["testing one two three four five six seven"])
                .spawn()
                .ok();
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(2500);
            let mut bytes = 0usize;
            while tokio::time::Instant::now() < deadline {
                match tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await {
                    Ok(Some(chunk)) => bytes += chunk.len(),
                    Ok(None) => break,
                    Err(_) => {}
                }
            }
            if let Some(mut child) = speaker {
                let _ = child.kill();
            }
            println!("bytes={bytes} {}", handle.diagnostics());
        });
        handle.stop();
    }
}

#[cfg(test)]
mod mic_probe_tests {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::{Arc, Mutex};

    /// Manual probe: lists the default input's configs and records ~1.5 s
    /// with the requested channel count / rate (env MIC_CH, MIC_RATE),
    /// printing the per-channel peak. `cargo test mic_probe -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn mic_probe() {
        let host = cpal::default_host();
        let device = host.default_input_device().expect("input device");
        let default = device.default_input_config().expect("default config");
        println!("default: {:?}", default);
        if let Ok(configs) = device.supported_input_configs() {
            for c in configs {
                println!("  supported: {:?}", c);
            }
        }
        let channels: u16 = std::env::var("MIC_CH")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default.channels());
        let rate: u32 = std::env::var("MIC_RATE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default.sample_rate());
        let config = cpal::StreamConfig {
            channels,
            sample_rate: rate,
            buffer_size: cpal::BufferSize::Default,
        };
        let peaks = Arc::new(Mutex::new(vec![0f32; channels as usize]));
        let frames = Arc::new(Mutex::new(0usize));
        let p2 = peaks.clone();
        let f2 = frames.clone();
        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _| {
                    let mut peaks = p2.lock().unwrap();
                    for frame in data.chunks(channels as usize) {
                        for (i, s) in frame.iter().enumerate() {
                            if s.abs() > peaks[i] {
                                peaks[i] = s.abs();
                            }
                        }
                    }
                    *f2.lock().unwrap() += data.len() / channels as usize;
                },
                |e| eprintln!("stream error: {e}"),
                None,
            )
            .expect("build stream");
        stream.play().expect("play");
        std::thread::sleep(std::time::Duration::from_millis(1500));
        drop(stream);
        let peaks = peaks.lock().unwrap();
        let db: Vec<String> = peaks
            .iter()
            .map(|p| format!("{:.1}", if *p > 0.0 { 20.0 * p.log10() } else { -120.0 }))
            .collect();
        println!(
            "probe ch={channels} rate={rate} frames={} peak_dbfs_per_channel={:?}",
            frames.lock().unwrap(),
            db
        );
    }
}
