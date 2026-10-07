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
