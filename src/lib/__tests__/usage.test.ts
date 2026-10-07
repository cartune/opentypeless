import { describe, expect, it } from 'vitest'
import {
  DEFAULT_USAGE_PRICING,
  daysAgoIso,
  estimateCost,
  estimateRowCost,
  findPrice,
  formatMinutes,
  formatTokens,
  formatUsd,
  monthStartIso,
  periodStartIso,
  resolvePricing,
} from '../usage'
import type { UsageSummary } from '../tauri'

const summary: UsageSummary = {
  since: '2026-10-01',
  totals: { runs: 3, audioSeconds: 600, promptTokens: 2_000_000, completionTokens: 500_000 },
  byModel: [
    {
      kind: 'stt',
      provider: 'openai-whisper',
      model: 'gpt-4o-mini-transcribe',
      runs: 3,
      audioSeconds: 600,
      promptTokens: 0,
      completionTokens: 0,
    },
    {
      kind: 'llm',
      provider: 'openai',
      model: 'gpt-4.1-mini',
      runs: 2,
      audioSeconds: 0,
      promptTokens: 2_000_000,
      completionTokens: 500_000,
    },
    {
      kind: 'llm',
      provider: 'custom',
      model: 'my-local-model',
      runs: 1,
      audioSeconds: 0,
      promptTokens: 100,
      completionTokens: 10,
    },
  ],
  byDay: [],
}

describe('usage pricing', () => {
  it('merges overrides over defaults by model name, ignoring case and provider prefix', () => {
    const pricing = resolvePricing([
      {
        model: 'openai/GPT-4.1-mini',
        usd_per_minute: null,
        usd_per_mtok_in: 1,
        usd_per_mtok_out: 2,
      },
      { model: 'extra-model', usd_per_minute: 0.01, usd_per_mtok_in: null, usd_per_mtok_out: null },
    ])
    expect(pricing).toHaveLength(DEFAULT_USAGE_PRICING.length + 1)
    expect(findPrice(pricing, 'gpt-4.1-mini')?.usd_per_mtok_in).toBe(1)
    expect(findPrice(pricing, 'EXTRA-MODEL')?.usd_per_minute).toBe(0.01)
    expect(findPrice(pricing, 'unknown')).toBeNull()
    expect(resolvePricing(undefined)).toHaveLength(DEFAULT_USAGE_PRICING.length)
  })

  it('prices audio rows per minute and text rows per million tokens', () => {
    const pricing = resolvePricing([])
    expect(estimateRowCost(summary.byModel[0], pricing)).toBeCloseTo(10 * 0.003, 10)
    expect(estimateRowCost(summary.byModel[1], pricing)).toBeCloseTo(2 * 0.4 + 0.5 * 1.6, 10)
    expect(estimateRowCost(summary.byModel[2], pricing)).toBeNull()
  })

  it('sums priced rows and reports unpriced models', () => {
    const estimate = estimateCost(summary, resolvePricing([]))
    expect(estimate.usd).toBeCloseTo(0.03 + 1.6, 10)
    expect(estimate.unpricedModels).toEqual(['my-local-model'])
    expect(estimateCost(null, resolvePricing([]))).toEqual({ usd: 0, unpricedModels: [] })
  })

  it('does not treat a zero-usage unpriced row as missing', () => {
    const estimate = estimateCost(
      {
        ...summary,
        byModel: [{ ...summary.byModel[2], promptTokens: 0, completionTokens: 0 }],
      },
      resolvePricing([]),
    )
    expect(estimate.unpricedModels).toEqual([])
  })
})

describe('usage formatting and periods', () => {
  it('formats minutes, tokens and dollars compactly', () => {
    expect(formatMinutes(90)).toBe('1.5')
    expect(formatMinutes(-5)).toBe('0.0')
    expect(formatMinutes(6000 * 60)).toBe('6000')
    expect(formatTokens(950)).toBe('950')
    expect(formatTokens(1234)).toBe('1.2k')
    expect(formatTokens(2_500_000)).toBe('2.5M')
    expect(formatUsd(0)).toBe('$0.00')
    expect(formatUsd(0.00123)).toBe('$0.0012')
    expect(formatUsd(0.123)).toBe('$0.123')
    expect(formatUsd(12.345)).toBe('$12.35')
  })

  it('computes ISO period starts in local time', () => {
    const now = new Date(2026, 9, 8, 15, 30)
    expect(monthStartIso(now)).toBe('2026-10-01')
    expect(daysAgoIso(7, now)).toBe('2026-10-01')
    expect(daysAgoIso(30, now)).toBe('2026-09-08')
    expect(periodStartIso('month', now)).toBe('2026-10-01')
    expect(periodStartIso('7d', now)).toBe('2026-10-01')
    expect(periodStartIso('30d', now)).toBe('2026-09-08')
  })
})
