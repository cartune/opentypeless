import type { HistoryEntry } from '../../stores/appStore'

export type HistoryEntryKind = 'dictation' | 'ask' | 'command'

const ASK_INTENTS = new Set(['open_question', 'ask_selection'])
const COMMAND_INTENTS = new Set([
  'draft_insert',
  'rewrite_selection',
  'translate_selection',
  'search',
])

/**
 * How a history row should read. Rows written before `intent_kind` existed
 * are recognised by their `popup` output status.
 */
export function historyEntryKind(
  entry: Pick<HistoryEntry, 'intent_kind' | 'output_status'>,
): HistoryEntryKind {
  const intent = entry.intent_kind ?? null
  if (intent && ASK_INTENTS.has(intent)) return 'ask'
  if (intent && COMMAND_INTENTS.has(intent)) return 'command'
  if (!intent && entry.output_status === 'popup') return 'ask'
  return 'dictation'
}
