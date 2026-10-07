import { useTranslation } from 'react-i18next'

import type { HistoryEntry } from '../../stores/appStore'
import { summarizeRunTiming } from '../../lib/timing'

type Props = Pick<HistoryEntry, 'stt_ms' | 'llm_ms' | 'stt_model' | 'llm_model'>

/** Compact "STT 0.8s · LLM 1.1s · whisper-1 / gpt-4.1-mini" line under a history row. */
export function RunTimingMeta(props: Props) {
  const { t } = useTranslation()
  const summary = summarizeRunTiming(props)
  if (!summary) return null

  const models = [summary.sttModel, summary.llmModel].filter(Boolean).join(' / ')

  return (
    <div
      className="mt-0.5 flex min-w-0 items-center gap-1.5 text-[11px] text-text-tertiary"
      data-testid="run-timing"
    >
      {summary.stt && (
        <span className="shrink-0">{t('history.timingStt', { value: summary.stt })}</span>
      )}
      {summary.stt && summary.llm && (
        <span aria-hidden="true" className="shrink-0">
          ·
        </span>
      )}
      {summary.llm && (
        <span className="shrink-0">{t('history.timingLlm', { value: summary.llm })}</span>
      )}
      {models && (
        <>
          <span aria-hidden="true" className="shrink-0">
            ·
          </span>
          <span className="min-w-0 truncate">{models}</span>
        </>
      )}
    </div>
  )
}
