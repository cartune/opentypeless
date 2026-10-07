import type { UsagePrice } from '../stores/appStore'
import type { UsageByModel, UsageSummary } from './tauri'

/**
 * Built-in BYOK price table (USD). These are estimates seeded from public
 * OpenAI list prices and may be out of date; the user can override any row in
 * Settings → Usage. Audio models: per minute. Text models: per million tokens.
 */
export const DEFAULT_USAGE_PRICING: readonly UsagePrice[] = [
  { model: 'whisper-1', usd_per_minute: 0.006, usd_per_mtok_in: null, usd_per_mtok_out: null },
  {
    model: 'gpt-4o-mini-transcribe',
    usd_per_minute: 0.003,
    usd_per_mtok_in: null,
    usd_per_mtok_out: null,
  },
  {
    model: 'gpt-4o-transcribe',
    usd_per_minute: 0.006,
    usd_per_mtok_in: null,
    usd_per_mtok_out: null,
  },
  { model: 'gpt-4.1-mini', usd_per_minute: null, usd_per_mtok_in: 0.4, usd_per_mtok_out: 1.6 },
  { model: 'gpt-4.1-nano', usd_per_minute: null, usd_per_mtok_in: 0.1, usd_per_mtok_out: 0.4 },
  { model: 'gpt-4.1', usd_per_minute: null, usd_per_mtok_in: 2, usd_per_mtok_out: 8 },
  { model: 'gpt-4o-mini', usd_per_minute: null, usd_per_mtok_in: 0.15, usd_per_mtok_out: 0.6 },
  { model: 'gpt-4o', usd_per_minute: null, usd_per_mtok_in: 2.5, usd_per_mtok_out: 10 },
  { model: 'gpt-5-nano', usd_per_minute: null, usd_per_mtok_in: 0.05, usd_per_mtok_out: 0.4 },
  { model: 'gpt-5-mini', usd_per_minute: null, usd_per_mtok_in: 0.25, usd_per_mtok_out: 2 },
  { model: 'gpt-5', usd_per_minute: null, usd_per_mtok_in: 1.25, usd_per_mtok_out: 10 },
]

function pad(value: number): string {
  return String(value).padStart(2, '0')
}

function isoDate(date: Date): string {
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

/** First day of the current month as an ISO date prefix (local time). */
export function monthStartIso(now: Date = new Date()): string {
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-01`
}

/** ISO date prefix `days` days ago (local time). */
export function daysAgoIso(days: number, now: Date = new Date()): string {
  const date = new Date(now)
  date.setDate(date.getDate() - days)
  return isoDate(date)
}

export type UsagePeriod = 'month' | '7d' | '30d'

export function periodStartIso(period: UsagePeriod, now: Date = new Date()): string {
  switch (period) {
    case '7d':
      return daysAgoIso(7, now)
    case '30d':
      return daysAgoIso(30, now)
    default:
      return monthStartIso(now)
  }
}

function normalizeModel(model: string): string {
  return model
    .trim()
    .toLowerCase()
    .replace(/^openai\//, '')
}

/** Defaults merged with the user's overrides; an override replaces the whole row. */
export function resolvePricing(overrides: UsagePrice[] | undefined | null): UsagePrice[] {
  const rows = new Map<string, UsagePrice>()
  for (const row of DEFAULT_USAGE_PRICING) rows.set(normalizeModel(row.model), { ...row })
  for (const row of overrides ?? []) {
    if (!row || typeof row.model !== 'string' || !row.model.trim()) continue
    rows.set(normalizeModel(row.model), { ...row })
  }
  return [...rows.values()]
}

export function findPrice(pricing: readonly UsagePrice[], model: string): UsagePrice | null {
  const wanted = normalizeModel(model)
  if (!wanted) return null
  return pricing.find((row) => normalizeModel(row.model) === wanted) ?? null
}

/** Cost of one aggregated row, or null when no applicable price is known. */
export function estimateRowCost(row: UsageByModel, pricing: readonly UsagePrice[]): number | null {
  const price = findPrice(pricing, row.model)
  if (!price) return null
  if (row.kind === 'stt') {
    if (price.usd_per_minute === null) return null
    return (row.audioSeconds / 60) * price.usd_per_minute
  }
  if (price.usd_per_mtok_in === null && price.usd_per_mtok_out === null) return null
  const input = ((price.usd_per_mtok_in ?? 0) * row.promptTokens) / 1_000_000
  const output = ((price.usd_per_mtok_out ?? 0) * row.completionTokens) / 1_000_000
  return input + output
}

export interface CostEstimate {
  /** Sum over rows that have a price. */
  usd: number
  /** Models that contributed usage but have no price (shown as a warning). */
  unpricedModels: string[]
}

export function estimateCost(
  summary: UsageSummary | null | undefined,
  pricing: readonly UsagePrice[],
): CostEstimate {
  if (!summary) return { usd: 0, unpricedModels: [] }
  let usd = 0
  const unpriced = new Set<string>()
  for (const row of summary.byModel) {
    const cost = estimateRowCost(row, pricing)
    if (cost === null) {
      const hasUsage =
        row.kind === 'stt' ? row.audioSeconds > 0 : row.promptTokens + row.completionTokens > 0
      if (hasUsage) unpriced.add(row.model || row.provider)
    } else {
      usd += cost
    }
  }
  return { usd, unpricedModels: [...unpriced] }
}

/** "12.3" minutes from seconds, one decimal, never negative. */
export function formatMinutes(seconds: number): string {
  const minutes = Math.max(seconds, 0) / 60
  return minutes >= 100 ? minutes.toFixed(0) : minutes.toFixed(1)
}

/** 950 -> "950", 1234 -> "1.2k", 2_500_000 -> "2.5M". */
export function formatTokens(count: number): string {
  const value = Math.max(Math.round(count), 0)
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}k`
  return String(value)
}

/** "$0.0123" below one dollar, "$12.34" otherwise. */
export function formatUsd(usd: number): string {
  const value = Math.max(usd, 0)
  if (value === 0) return '$0.00'
  if (value < 0.01) return `$${value.toFixed(4)}`
  if (value < 1) return `$${value.toFixed(3)}`
  return `$${value.toFixed(2)}`
}
