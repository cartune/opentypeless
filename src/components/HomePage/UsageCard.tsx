import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { BarChart3 } from 'lucide-react'
import { useAppStore } from '../../stores/appStore'
import { getUsageSummary, type UsageSummary } from '../../lib/tauri'
import {
  estimateCost,
  formatMinutes,
  formatTokens,
  formatUsd,
  monthStartIso,
  resolvePricing,
} from '../../lib/usage'

/** Home-page card: this month's BYOK minutes, tokens and estimated cost from local history. */
export function UsageCard() {
  const { t } = useTranslation()
  const historyLength = useAppStore((s) => s.history.length)
  const usagePricing = useAppStore((s) => s.config.usage_pricing)
  const [summary, setSummary] = useState<UsageSummary | null>(null)

  useEffect(() => {
    let cancelled = false
    getUsageSummary(monthStartIso())
      .then((result) => {
        if (!cancelled) setSummary(result)
      })
      .catch((error) => {
        console.error('Failed to load usage summary:', error)
      })
    return () => {
      cancelled = true
    }
  }, [historyLength])

  const pricing = resolvePricing(usagePricing)
  const cost = estimateCost(summary, pricing)
  const totals = summary?.totals
  const tokens = (totals?.promptTokens ?? 0) + (totals?.completionTokens ?? 0)

  return (
    <div className="rounded-[18px] p-5 jelly-card" data-testid="usage-card">
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <BarChart3 size={16} className="text-text-secondary" />
          <h3 className="text-[13px] font-medium">{t('home.usageTitle')}</h3>
        </div>
        <button
          type="button"
          onClick={() => {
            window.location.hash = '#/settings?pane=usage'
          }}
          className="text-[12px] text-accent font-medium bg-transparent border-none cursor-pointer hover:underline"
        >
          {t('home.usageManage')}
        </button>
      </div>
      <div className="grid grid-cols-4 gap-3">
        <Stat
          label={t('home.usageMinutes')}
          value={`${formatMinutes(totals?.audioSeconds ?? 0)} min`}
        />
        <Stat label={t('home.usageTokens')} value={formatTokens(tokens)} />
        <Stat label={t('home.usageCost')} value={formatUsd(cost.usd)} highlight />
        <Stat label={t('home.usageRuns')} value={String(totals?.runs ?? 0)} />
      </div>
      <p className="mt-3 text-[11px] text-text-tertiary leading-relaxed">{t('home.usageHint')}</p>
    </div>
  )
}

function Stat({ label, value, highlight }: { label: string; value: string; highlight?: boolean }) {
  return (
    <div className="rounded-[10px] bg-bg-secondary/60 px-3 py-2.5 min-w-0">
      <p className="text-[11px] text-text-tertiary uppercase tracking-wider truncate">{label}</p>
      <p
        className={`text-[16px] font-semibold tabular-nums truncate ${highlight ? 'text-accent' : 'text-text-primary'}`}
      >
        {value}
      </p>
    </div>
  )
}
