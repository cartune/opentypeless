import { describe, expect, it } from 'vitest'
import {
  WAVEFORM_BAR_COUNT,
  WAVEFORM_MIN_HEIGHT,
  WAVEFORM_MAX_HEIGHT,
  isSilentLevel,
  nextWaveAmplitudes,
  nextWaveformHeights,
} from '../waveformLevels'

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
