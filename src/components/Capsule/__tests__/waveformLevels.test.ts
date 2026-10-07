import { describe, expect, it } from 'vitest'
import {
  WAVEFORM_BAR_COUNT,
  WAVEFORM_MIN_HEIGHT,
  WAVEFORM_MAX_HEIGHT,
  isSilentLevel,
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
