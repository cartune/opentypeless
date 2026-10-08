export const WAVEFORM_BAR_COUNT = 7
export const WAVEFORM_MIN_HEIGHT = 3
export const WAVEFORM_MAX_HEIGHT = 16
/** Meter level (0..1, dB scaled by the backend) under which input counts as silent. */
export const SILENT_LEVEL_THRESHOLD = 0.2
/** Seconds of continuous silence after which the capsule warns about missing audio. */
export const NO_AUDIO_HINT_AFTER_SECONDS = 3

/**
 * Compute bar heights for the recording waveform from the current meter level.
 * Bars decay towards the new level instead of jumping, and silence collapses
 * every bar to the minimum so a dead microphone is visible at a glance.
 */
export function nextWaveformHeights(previous: number[], level: number): number[] {
  const clamped = Math.max(0, Math.min(1, Number.isFinite(level) ? level : 0))
  const span = WAVEFORM_MAX_HEIGHT - WAVEFORM_MIN_HEIGHT
  return Array.from({ length: WAVEFORM_BAR_COUNT }, (_, i) => {
    // Centre bars follow the level fully; outer bars are scaled down for shape.
    const distance = Math.abs(i - (WAVEFORM_BAR_COUNT - 1) / 2)
    const shape = 1 - distance * 0.18
    const target = WAVEFORM_MIN_HEIGHT + span * clamped * shape
    const prev = previous[i] ?? WAVEFORM_MIN_HEIGHT
    // Fast attack, slower release.
    const blend = target > prev ? 0.6 : 0.25
    return prev + (target - prev) * blend
  })
}

export function isSilentLevel(level: number): boolean {
  return !(level > SILENT_LEVEL_THRESHOLD)
}

/** Number of frequency bands the backend reports in `audio:meter`. */
export const METER_BAND_COUNT = 5
/** Number of translucent wave layers drawn by the Siri-style waveform. */
export const WAVE_LAYER_COUNT = 4

export interface AudioMeter {
  /** 0..1 dB-scaled overall level, same scale as `audioVolume`. */
  level: number
  /** 0..1 dB-scaled band levels, low to high (≈150, 400, 1000, 2500, 5000 Hz). */
  bands: number[]
}

export const EMPTY_METER: AudioMeter = { level: 0, bands: Array(METER_BAND_COUNT).fill(0) }

/**
 * The backend meter spans -60..0 dBFS, but conversational speech at a laptop
 * mic sits around -45..-20 dBFS RMS (meter 0.25..0.65). These bounds map that
 * window onto 0..1 so a normal voice fills the wave instead of needing a shout.
 */
export const SPEECH_WINDOW_LOW = 0.2
export const SPEECH_WINDOW_HIGH = 0.65
/** Curve applied inside the window; < 1 lifts quiet syllables. */
const SPEECH_CURVE = 0.7
/**
 * Automatic gain: amplitudes are normalised by the loudest recent syllable, so
 * a soft talker still fills the wave. The floor keeps silence and room noise
 * from being amplified into fake speech.
 */
export const AGC_FLOOR = 0.55
/** Per-frame decay of the running peak (≈2 s to fall by half at 60 fps). */
const AGC_DECAY = 0.994

export interface WaveState {
  amplitudes: number[]
  /** Running peak of the speech drive used for automatic gain. */
  peak: number
}

export const INITIAL_WAVE_STATE: WaveState = {
  amplitudes: Array(WAVE_LAYER_COUNT).fill(0),
  peak: 0,
}

function clamp01(value: number): number {
  return Math.max(0, Math.min(1, Number.isFinite(value) ? value : 0))
}

/** Map the dB meter level onto the speech window (0..1). */
export function speechDrive(level: number): number {
  const x = clamp01((clamp01(level) - SPEECH_WINDOW_LOW) / (SPEECH_WINDOW_HIGH - SPEECH_WINDOW_LOW))
  return Math.pow(x, SPEECH_CURVE)
}

/**
 * Advance the wave state by one frame. The overall level (through the speech
 * window and automatic gain) sets how big every layer is; the band meter only
 * decides the *shape*, so vowels, sibilants and pitch changes move different
 * waves whether you whisper or shout. Fast attack, slower release.
 */
export function nextWaveState(previous: WaveState, meter: AudioMeter): WaveState {
  const speech = speechDrive(meter.level)
  const peak = Math.max(speech, previous.peak * AGC_DECAY)
  const drive = clamp01(speech / Math.max(peak, AGC_FLOOR))
  const bands = Array.from({ length: METER_BAND_COUNT }, (_, i) => clamp01(meter.bands[i] ?? 0))
  const loudest = Math.max(0.05, ...bands)
  const shape = (i: number) => bands[i] / loudest
  // Low layer: fundamentals; mid: formants; high: consonants/sibilance; crest: overall.
  const targets = [
    (0.4 + 0.6 * Math.max(shape(0), shape(1))) * drive,
    (0.4 + 0.6 * shape(2)) * drive,
    (0.4 + 0.6 * Math.max(shape(3), shape(4))) * drive * 0.9,
    drive,
  ]
  const amplitudes = targets.map((target, i) => {
    const prev = previous.amplitudes[i] ?? 0
    const blend = target > prev ? 0.55 : 0.18
    return prev + (target - prev) * blend
  })
  return { amplitudes, peak }
}

/** Stateless convenience wrapper (no automatic gain memory). */
export function nextWaveAmplitudes(previous: number[], meter: AudioMeter): number[] {
  return nextWaveState({ amplitudes: previous, peak: 0 }, meter).amplitudes
}

/** Number of bars in the live level-bars waveform. */
export const LEVEL_BAR_COUNT = 26
/** How often a new bar is appended (ms); older bars scroll left. */
export const LEVEL_BAR_INTERVAL_MS = 60

export interface LevelHistory {
  /** Newest last; each entry is a 0..1 speech drive. */
  bars: number[]
  /** Loudest drive seen since the last bar was appended. */
  pending: number
  /** Timestamp (ms) of the last appended bar. */
  lastAt: number
}

export const EMPTY_LEVEL_HISTORY: LevelHistory = {
  bars: Array(LEVEL_BAR_COUNT).fill(0),
  pending: 0,
  lastAt: 0,
}

/**
 * Feed one frame of the meter into the scrolling level history. Between
 * appends the loudest drive is kept, so a short consonant is not lost; every
 * `LEVEL_BAR_INTERVAL_MS` the pending value becomes the newest bar and the
 * oldest falls off the left.
 */
export function nextLevelHistory(
  previous: LevelHistory,
  meter: AudioMeter,
  now: number,
  intervalMs = LEVEL_BAR_INTERVAL_MS,
): LevelHistory {
  const pending = Math.max(previous.pending, speechDrive(meter.level))
  if (previous.lastAt !== 0 && now - previous.lastAt < intervalMs) {
    return { ...previous, pending }
  }
  const bars = previous.bars.slice(-(LEVEL_BAR_COUNT - 1))
  bars.push(pending)
  while (bars.length < LEVEL_BAR_COUNT) bars.unshift(0)
  return { bars, pending: 0, lastAt: now }
}
