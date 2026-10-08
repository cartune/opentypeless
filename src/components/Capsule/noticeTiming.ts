import type { PipelineNotice } from '../../stores/appStore'

/** How long the quiet "cancelled" pill stays before collapsing to the dot. */
export const CAPSULE_CANCELLED_MS = 700
/** A learned word is worth reading, so that notice stays longer. */
export const CAPSULE_LEARNED_MS = 3200
/** How long the window takes to scale away to its centre afterwards. */
export const CAPSULE_COLLAPSE_MS = 200

export function getCapsuleNoticeHoldMs(notice: PipelineNotice | null): number {
  return notice?.kind === 'learned' ? CAPSULE_LEARNED_MS : CAPSULE_CANCELLED_MS
}
