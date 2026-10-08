import { describe, expect, it } from 'vitest'
import {
  WAVEFORM_BAR_COUNT,
  WAVEFORM_MIN_HEIGHT,
  WAVEFORM_MAX_HEIGHT,
  AGC_FLOOR,
  EMPTY_LEVEL_HISTORY,
  INITIAL_WAVE_STATE,
  LEVEL_BAR_COUNT,
  LEVEL_BAR_INTERVAL_MS,
  nextLevelHistory,
  isSilentLevel,
  nextWaveAmplitudes,
  nextWaveState,
  nextWaveformHeights,
  speechDrive,
} from '../waveformLevels'
import { LAYERS, SIRI_WAVE_HEIGHT, waveOffset } from '../siriWave'

describe('nextWaveformHeights', () => {
  it('collapses to the minimum height on silence', () => {
    let heights = Array(WAVEFORM_BAR_COUNT).fill(WAVEFORM_MAX_HEIGHT)
    for (let i = 0; i < 40; i += 1) heights = nextWaveformHeights(heights, 0)
    heights.forEach((h) => expect(h).toBeCloseTo(WAVEFORM_MIN_HEIGHT, 1))
  })

  it('rises towards the maximum at full level and never exceeds it', () => {
    let heights = Array(WAVEFORM_BAR_COUNT).fill(WAVEFORM_MIN_HEIGHT)
    for (let i = 0; i < 40; i += 1) heights = nextWaveformHeights(heights, 1)
    const centre = heights[Math.floor(WAVEFORM_BAR_COUNT / 2)]
    expect(centre).toBeCloseTo(WAVEFORM_MAX_HEIGHT, 1)
    heights.forEach((h) => expect(h).toBeLessThanOrEqual(WAVEFORM_MAX_HEIGHT))
  })

  it('does not move on its own between identical silent frames', () => {
    const a = nextWaveformHeights(Array(WAVEFORM_BAR_COUNT).fill(WAVEFORM_MIN_HEIGHT), 0)
    const b = nextWaveformHeights(a, 0)
    expect(b).toEqual(a)
  })

  it('ignores invalid levels', () => {
    const heights = nextWaveformHeights([], Number.NaN)
    heights.forEach((h) => expect(h).toBe(WAVEFORM_MIN_HEIGHT))
  })
})

describe('isSilentLevel', () => {
  it('treats low meter levels as silence', () => {
    expect(isSilentLevel(0)).toBe(true)
    expect(isSilentLevel(0.1)).toBe(true)
    expect(isSilentLevel(0.5)).toBe(false)
  })
})

describe('nextWaveAmplitudes', () => {
  const silence = { level: 0, bands: [0, 0, 0, 0, 0] }

  it('flattens every layer on silence', () => {
    const next = nextWaveAmplitudes([0.8, 0.8, 0.8, 0.8], silence)
    next.forEach((a) => expect(a).toBeLessThan(0.8))
    let settled = next
    for (let i = 0; i < 40; i++) settled = nextWaveAmplitudes(settled, silence)
    settled.forEach((a) => expect(a).toBeLessThan(0.01))
  })

  it('moves the low layer for low bands and the high layer for high bands', () => {
    const low = nextWaveAmplitudes([0, 0, 0, 0], { level: 0.8, bands: [0.9, 0.6, 0, 0, 0] })
    expect(low[0]).toBeGreaterThan(low[2])
    const high = nextWaveAmplitudes([0, 0, 0, 0], { level: 0.8, bands: [0, 0, 0, 0.9, 0.7] })
    expect(high[2]).toBeGreaterThan(high[0])
  })

  it('attacks faster than it releases and never exceeds 1', () => {
    const loud = { level: 1, bands: [1, 1, 1, 1, 1] }
    const up = nextWaveAmplitudes([0, 0, 0, 0], loud)
    const down = nextWaveAmplitudes([1, 1, 1, 1], silence)
    expect(up[3]).toBeGreaterThan(1 - down[3])
    up.forEach((a) => expect(a).toBeLessThanOrEqual(1))
    expect(nextWaveAmplitudes([0, 0, 0, 0], { level: NaN, bands: [NaN] })).toEqual([0, 0, 0, 0])
  })
})

describe('speech window and automatic gain', () => {
  // A normal voice at a laptop mic: peaks around -20 dBFS, RMS around -36 dBFS
  // (meter 0.4), with single Goertzel bins well below the broadband level.
  const normalVoice = { level: 0.4, bands: [0.3, 0.35, 0.3, 0.2, 0.15] }
  const quietVoice = { level: 0.3, bands: [0.2, 0.25, 0.2, 0.1, 0.1] }
  const silence = { level: 0, bands: [0, 0, 0, 0, 0] }

  it('maps conversational levels into the upper half of the drive range', () => {
    expect(speechDrive(0)).toBe(0)
    expect(speechDrive(0.2)).toBe(0)
    expect(speechDrive(0.4)).toBeGreaterThan(0.5)
    expect(speechDrive(0.65)).toBeCloseTo(1, 5)
    expect(speechDrive(1)).toBe(1)
    expect(speechDrive(NaN)).toBe(0)
  })

  it('fills the wave for a normal speaking voice without shouting', () => {
    let state = INITIAL_WAVE_STATE
    for (let i = 0; i < 10; i++) state = nextWaveState(state, normalVoice)
    const [low, mid, , crest] = state.amplitudes
    expect(crest).toBeGreaterThanOrEqual(0.6)
    expect(low).toBeGreaterThanOrEqual(0.4)
    expect(mid).toBeGreaterThanOrEqual(0.4)
    // The main layer must move a clearly visible number of pixels at its crest.
    const half = SIRI_WAVE_HEIGHT / 2 - 1
    let excursion = 0
    for (let phase = 0; phase < Math.PI * 2; phase += 0.05) {
      excursion = Math.max(excursion, Math.abs(waveOffset(LAYERS[0], 0.5, low, phase, half)))
    }
    expect(excursion).toBeGreaterThanOrEqual(9)
  })

  it('lifts a quiet talker with automatic gain but never amplifies silence', () => {
    let state = INITIAL_WAVE_STATE
    for (let i = 0; i < 10; i++) state = nextWaveState(state, quietVoice)
    expect(state.amplitudes[3]).toBeGreaterThanOrEqual(0.5)
    expect(state.peak).toBeLessThan(AGC_FLOOR)
    for (let i = 0; i < 60; i++) state = nextWaveState(state, silence)
    state.amplitudes.forEach((a) => expect(a).toBeLessThan(0.01))
  })

  it('keeps the band meter as shape only: the crest never exceeds the drive', () => {
    const loudBands = { level: 0.4, bands: [1, 1, 1, 1, 1] }
    const flat = nextWaveState(INITIAL_WAVE_STATE, loudBands)
    const normal = nextWaveState(INITIAL_WAVE_STATE, normalVoice)
    expect(flat.amplitudes[3]).toBeCloseTo(normal.amplitudes[3], 5)
    flat.amplitudes.forEach((a) => expect(a).toBeLessThanOrEqual(flat.amplitudes[3] + 1e-6))
  })

  it('decays the running peak so a loud burst does not mute later soft speech', () => {
    let state = nextWaveState(INITIAL_WAVE_STATE, { level: 1, bands: [1, 1, 1, 1, 1] })
    expect(state.peak).toBe(1)
    for (let i = 0; i < 600; i++) state = nextWaveState(state, quietVoice)
    expect(state.peak).toBeLessThan(0.5)
    expect(state.amplitudes[3]).toBeGreaterThanOrEqual(0.5)
  })
})

describe('nextLevelHistory', () => {
  const loud = { level: 0.6, bands: [0, 0, 0, 0, 0] }
  const quiet = { level: 0.3, bands: [0, 0, 0, 0, 0] }
  const silence = { level: 0, bands: [0, 0, 0, 0, 0] }

  it('appends the loudest moment since the last bar and scrolls left', () => {
    let h = nextLevelHistory(EMPTY_LEVEL_HISTORY, loud, 1000)
    expect(h.bars).toHaveLength(LEVEL_BAR_COUNT)
    expect(h.bars[LEVEL_BAR_COUNT - 1]).toBeGreaterThan(0.9)
    // Within the interval nothing is appended, but the peak is remembered.
    h = nextLevelHistory(h, quiet, 1000 + LEVEL_BAR_INTERVAL_MS / 2)
    expect(h.bars[LEVEL_BAR_COUNT - 1]).toBeGreaterThan(0.9)
    expect(h.pending).toBeGreaterThan(0)
    h = nextLevelHistory(h, silence, 1000 + LEVEL_BAR_INTERVAL_MS)
    expect(h.bars).toHaveLength(LEVEL_BAR_COUNT)
    expect(h.bars[LEVEL_BAR_COUNT - 2]).toBeGreaterThan(0.9)
    expect(h.bars[LEVEL_BAR_COUNT - 1]).toBeGreaterThan(0.3)
    expect(h.pending).toBe(0)
  })

  it('shows a flat line for silence', () => {
    let h = EMPTY_LEVEL_HISTORY
    for (let i = 0; i < LEVEL_BAR_COUNT + 2; i++) {
      h = nextLevelHistory(h, silence, 1000 + i * LEVEL_BAR_INTERVAL_MS)
    }
    h.bars.forEach((b) => expect(b).toBe(0))
  })
})
