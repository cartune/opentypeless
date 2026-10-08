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

function clamp01(value: number): number {
  return Math.max(0, Math.min(1, Number.isFinite(value) ? value : 0))
}

/**
 * Map the band meter onto wave-layer amplitudes (0..1). Each layer follows a
 * different part of the spectrum so vowels, sibilants and pitch changes move
 * different waves, and everything is scaled by the overall level so silence
 * flattens every layer. Fast attack, slower release, like the bar meter.
 */
export function nextWaveAmplitudes(previous: number[], meter: AudioMeter): number[] {
  const level = clamp01(meter.level)
  const band = (i: number) => clamp01(meter.bands[i] ?? 0)
  // Low layer: fundamentals; mid: formants; high: consonants/sibilance; crest: overall.
  const targets = [
    Math.max(band(0), band(1)) * 0.9,
    band(2),
    Math.max(band(3), band(4)) * 0.85,
    level,
  ].map((t) => t * (0.35 + 0.65 * level))
  return targets.map((target, i) => {
    const prev = previous[i] ?? 0
    const blend = target > prev ? 0.55 : 0.18
    return prev + (target - prev) * blend
  })
}
