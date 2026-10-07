import { describe, expect, it } from 'vitest'
import { formatLatency, summarizeRunTiming } from '../timing'

describe('formatLatency', () => {
  it('formats milliseconds as seconds with one decimal', () => {
    expect(formatLatency(875)).toBe('0.9s')
    expect(formatLatency(1234)).toBe('1.2s')
    expect(formatLatency(0)).toBe('0.0s')
  })

  it('returns null for missing or invalid values', () => {
    expect(formatLatency(null)).toBeNull()
    expect(formatLatency(undefined)).toBeNull()
    expect(formatLatency(-5)).toBeNull()
    expect(formatLatency(Number.NaN)).toBeNull()
  })
})

describe('summarizeRunTiming', () => {
  it('returns null when no latency is recorded', () => {
    expect(summarizeRunTiming({})).toBeNull()
    expect(summarizeRunTiming({ stt_ms: null, llm_ms: null })).toBeNull()
  })

  it('keeps model labels and formats both latencies', () => {
    expect(
      summarizeRunTiming({
        stt_ms: 800,
        llm_ms: 1100,
        stt_model: 'whisper-1',
        llm_model: ' gpt-4.1-mini ',
      }),
    ).toEqual({ stt: '0.8s', llm: '1.1s', sttModel: 'whisper-1', llmModel: 'gpt-4.1-mini' })
  })

  it('omits the LLM part when polish did not run', () => {
    expect(summarizeRunTiming({ stt_ms: 600, llm_ms: null, stt_model: 'whisper-1' })).toEqual({
      stt: '0.6s',
      llm: null,
      sttModel: 'whisper-1',
      llmModel: null,
    })
  })
})
