/** Format a millisecond latency as a compact seconds string, e.g. 850 -> "0.9s". */
export function formatLatency(ms: number | null | undefined): string | null {
  if (ms === null || ms === undefined || !Number.isFinite(ms) || ms < 0) return null
  return `${(ms / 1000).toFixed(1)}s`
}

export interface RunTimingSummary {
  stt: string | null
  llm: string | null
  sttModel: string | null
  llmModel: string | null
}

/**
 * Build the pieces shown in the history row / capsule timing chip.
 * Returns null when no latency is known, so older rows render nothing extra.
 */
export function summarizeRunTiming(input: {
  stt_ms?: number | null
  llm_ms?: number | null
  stt_model?: string | null
  llm_model?: string | null
}): RunTimingSummary | null {
  const stt = formatLatency(input.stt_ms)
  const llm = formatLatency(input.llm_ms)
  if (!stt && !llm) return null
  return {
    stt,
    llm,
    sttModel: input.stt_model?.trim() || null,
    llmModel: input.llm_model?.trim() || null,
  }
}
