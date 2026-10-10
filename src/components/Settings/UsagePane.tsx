import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useAppStore, type UsagePrice } from '../../stores/appStore'
import { getUsageSummary, type UsageByModel, type UsageSummary } from '../../lib/tauri'
import {
  DEFAULT_USAGE_PRICING,
  estimateCost,
  estimateRowCost,
  findPrice,
  formatMinutes,
  formatTokens,
  formatUsd,
  periodStartIso,
  resolvePricing,
  type UsagePeriod,
} from '../../lib/usage'
import { SegmentedControl } from './shared/SegmentedControl'

type PriceField = 'usd_per_minute' | 'usd_per_mtok_in' | 'usd_per_mtok_out'

export function UsagePane() {
  const { t } = useTranslation()
  const config = useAppStore((s) => s.config)
  const updateConfig = useAppStore((s) => s.updateConfig)
  const historyLength = useAppStore((s) => s.history.length)
  const [period, setPeriod] = useState<UsagePeriod>('month')
  const [summary, setSummary] = useState<UsageSummary | null>(null)
  const [newModel, setNewModel] = useState('')

  useEffect(() => {
    let cancelled = false
    getUsageSummary(periodStartIso(period))
      .then((result) => {
        if (!cancelled) setSummary(result)
      })
      .catch((error) => console.error('Failed to load usage summary:', error))
    return () => {
      cancelled = true
    }
  }, [period, historyLength])

  const pricing = useMemo(() => resolvePricing(config.usage_pricing), [config.usage_pricing])
  const cost = estimateCost(summary, pricing)
  const rows = useMemo(() => summary?.byModel ?? [], [summary])

  // Price table rows: defaults + overrides + any model seen in usage without a price.
  const priceRows = useMemo(() => {
    const seen = new Set(pricing.map((row) => row.model.toLowerCase()))
    const extra: UsagePrice[] = []
    for (const row of rows) {
      const model = row.model.trim()
      if (model && !seen.has(model.toLowerCase())) {
        seen.add(model.toLowerCase())
        extra.push({
          model,
          usd_per_minute: null,
          usd_per_mtok_in: null,
          usd_per_mtok_out: null,
        })
      }
    }
    return [...pricing, ...extra]
  }, [pricing, rows])

  const savePrice = (model: string, field: PriceField, raw: string) => {
    const parsed = raw.trim() === '' ? null : Number(raw)
    if (parsed !== null && !Number.isFinite(parsed)) return
    const current = findPrice(priceRows, model) ?? {
      model,
      usd_per_minute: null,
      usd_per_mtok_in: null,
      usd_per_mtok_out: null,
    }
    const next: UsagePrice = { ...current, model, [field]: parsed }
    const others = (config.usage_pricing ?? []).filter(
      (row) => row.model.toLowerCase() !== model.toLowerCase(),
    )
    updateConfig({ usage_pricing: [...others, next] })
  }

  const addModel = () => {
    const model = newModel.trim()
    if (!model || findPrice(priceRows, model)) return
    updateConfig({
      usage_pricing: [
        ...(config.usage_pricing ?? []),
        { model, usd_per_minute: null, usd_per_mtok_in: null, usd_per_mtok_out: null },
      ],
    })
    setNewModel('')
  }

  return (
    <div className="space-y-5 text-[13px]">
      <p className="text-text-secondary leading-relaxed">{t('settings.usageDescription')}</p>

      <SegmentedControl
        options={[
          { value: 'month', label: t('settings.usagePeriodMonth') },
          { value: '7d', label: t('settings.usagePeriod7') },
          { value: '30d', label: t('settings.usagePeriod30') },
        ]}
        value={period}
        onChange={(value) => setPeriod(value as UsagePeriod)}
      />

      <div className="rounded-[14px] border border-border overflow-hidden">
        {rows.length === 0 ? (
          <p className="p-4 text-text-tertiary">{t('settings.usageNoData')}</p>
        ) : (
          <table className="w-full text-[12px]" data-testid="usage-table">
            <thead className="bg-bg-secondary/60 text-text-tertiary">
              <tr>
                <th className="text-left px-3 py-2 font-medium">{t('settings.usageModel')}</th>
                <th className="text-right px-2 py-2 font-medium">{t('settings.usageRuns')}</th>
                <th className="text-right px-2 py-2 font-medium">{t('settings.usageMinutes')}</th>
                <th className="text-right px-2 py-2 font-medium">{t('settings.usageTokensIn')}</th>
                <th className="text-right px-2 py-2 font-medium">{t('settings.usageTokensOut')}</th>
                <th className="text-right px-3 py-2 font-medium">{t('settings.usageCost')}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <UsageRow
                  key={`${row.kind}-${row.provider}-${row.model}`}
                  row={row}
                  pricing={pricing}
                />
              ))}
              <tr className="border-t border-border font-medium">
                <td className="px-3 py-2">{t('settings.usageTotal')}</td>
                <td className="text-right px-2 py-2 tabular-nums">{summary?.totals.runs ?? 0}</td>
                <td className="text-right px-2 py-2 tabular-nums">
                  {formatMinutes(summary?.totals.audioSeconds ?? 0)}
                </td>
                <td className="text-right px-2 py-2 tabular-nums">
                  {formatTokens(summary?.totals.promptTokens ?? 0)}
                </td>
                <td className="text-right px-2 py-2 tabular-nums">
                  {formatTokens(summary?.totals.completionTokens ?? 0)}
                </td>
                <td className="text-right px-3 py-2 tabular-nums text-accent">
                  {formatUsd(cost.usd)}
                </td>
              </tr>
            </tbody>
          </table>
        )}
      </div>
      {cost.unpricedModels.length > 0 && (
        <p className="text-[12px] text-warning">
          {t('settings.usageUnpriced', { models: cost.unpricedModels.join(', ') })}
        </p>
      )}

      <div>
        <div className="flex items-center justify-between mb-2">
          <h3 className="font-medium text-text-primary">{t('settings.usagePricingTitle')}</h3>
          <button
            type="button"
            onClick={() => updateConfig({ usage_pricing: [] })}
            className="text-[12px] text-accent bg-transparent border-none cursor-pointer hover:underline"
          >
            {t('settings.usageResetPricing')}
          </button>
        </div>
        <p className="text-[12px] text-text-tertiary leading-relaxed mb-3">
          {t('settings.usagePricingHint')}
        </p>
        <div className="rounded-[14px] border border-border overflow-hidden">
          <table className="w-full text-[12px]" data-testid="pricing-table">
            <thead className="bg-bg-secondary/60 text-text-tertiary">
              <tr>
                <th className="text-left px-3 py-2 font-medium">{t('settings.usageModel')}</th>
                <th className="text-right px-2 py-2 font-medium">{t('settings.usagePerMinute')}</th>
                <th className="text-right px-2 py-2 font-medium">{t('settings.usagePerMtokIn')}</th>
                <th className="text-right px-2 py-2 font-medium">
                  {t('settings.usagePerMtokOut')}
                </th>
              </tr>
            </thead>
            <tbody>
              {priceRows.map((row) => (
                <tr key={row.model} className="border-t border-border/60">
                  <td className="px-3 py-1.5 font-mono text-[11px]">{row.model}</td>
                  {(['usd_per_minute', 'usd_per_mtok_in', 'usd_per_mtok_out'] as const).map(
                    (field) => (
                      <td key={field} className="px-2 py-1.5 text-right">
                        <input
                          type="number"
                          inputMode="decimal"
                          step="any"
                          min="0"
                          aria-label={`${row.model} ${field}`}
                          defaultValue={row[field] ?? ''}
                          onBlur={(event) => savePrice(row.model, field, event.target.value)}
                          className="w-[84px] rounded-[6px] border border-border bg-bg-secondary px-2 py-1 text-right text-[12px] tabular-nums text-text-primary outline-none focus:border-border-focus"
                        />
                      </td>
                    ),
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div className="mt-2 flex items-center gap-2">
          <input
            type="text"
            value={newModel}
            onChange={(event) => setNewModel(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') addModel()
            }}
            placeholder={t('settings.usageModelPlaceholder')}
            className="flex-1 rounded-[8px] border border-border bg-bg-secondary px-3 py-1.5 text-[12px] text-text-primary outline-none focus:border-border-focus"
          />
          <button
            type="button"
            onClick={addModel}
            className="rounded-[8px] border border-border bg-bg-secondary px-3 py-1.5 text-[12px] text-text-primary cursor-pointer hover:border-border-focus"
          >
            {t('settings.usageAddModel')}
          </button>
        </div>
        <p className="mt-2 text-[11px] text-text-tertiary">
          {DEFAULT_USAGE_PRICING.length} built-in rows · {config.usage_pricing?.length ?? 0} edited
        </p>
      </div>
    </div>
  )
}

function UsageRow({ row, pricing }: { row: UsageByModel; pricing: UsagePrice[] }) {
  const { t } = useTranslation()
  const cost = estimateRowCost(row, pricing)
  return (
    <tr className="border-t border-border/60">
      <td className="px-3 py-2">
        <span className="font-mono text-[11px]">{row.model || row.provider}</span>
        <span className="ml-1.5 rounded-full bg-bg-secondary px-1.5 py-0.5 text-[10px] uppercase text-text-tertiary">
          {row.kind === 'stt_shadow' ? t('usage.shadowKind') : row.kind}
        </span>
      </td>
      <td className="text-right px-2 py-2 tabular-nums">{row.runs}</td>
      <td className="text-right px-2 py-2 tabular-nums">
        {row.kind === 'stt' || row.kind === 'stt_shadow' ? formatMinutes(row.audioSeconds) : '—'}
      </td>
      <td className="text-right px-2 py-2 tabular-nums">
        {row.kind === 'llm' ? formatTokens(row.promptTokens) : '—'}
      </td>
      <td className="text-right px-2 py-2 tabular-nums">
        {row.kind === 'llm' ? formatTokens(row.completionTokens) : '—'}
      </td>
      <td className="text-right px-3 py-2 tabular-nums">{cost === null ? '—' : formatUsd(cost)}</td>
    </tr>
  )
}
