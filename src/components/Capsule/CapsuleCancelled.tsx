import { useEffect } from 'react'
import { motion } from 'framer-motion'
import { Sparkles } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useAppStore } from '../../stores/appStore'

import { CAPSULE_COLLAPSE_MS, getCapsuleNoticeHoldMs } from './noticeTiming'

export { CAPSULE_CANCELLED_MS, CAPSULE_COLLAPSE_MS, CAPSULE_LEARNED_MS } from './noticeTiming'

/** Quiet notices: "cancelled" after Esc, or what was just learned from an edit. */
export function CapsuleCancelled() {
  const { t } = useTranslation()
  const notice = useAppStore((s) => s.pipelineNotice)
  const setCapsuleCollapsing = useAppStore((s) => s.setCapsuleCollapsing)
  const resetRecording = useAppStore((s) => s.resetRecording)
  const kind = notice?.kind ?? 'cancelled'
  const holdMs = getCapsuleNoticeHoldMs(notice)

  useEffect(() => {
    // Hold the notice, scale the whole pill away to its centre, then go idle.
    let clear: ReturnType<typeof setTimeout> | null = null
    const collapse = setTimeout(() => {
      setCapsuleCollapsing(true)
      clear = setTimeout(() => {
        useAppStore.setState({ capsuleCollapsing: false, pipelineNotice: null })
        // Only reset recording state if the pipeline is still idle; a new run
        // started inside the notice window must not be clobbered.
        if (useAppStore.getState().pipelineState === 'idle') {
          resetRecording()
        }
      }, CAPSULE_COLLAPSE_MS)
    }, holdMs)
    return () => {
      clearTimeout(collapse)
      if (clear) clearTimeout(clear)
      // A new run that interrupts the notice must not leave the window collapsed.
      if (useAppStore.getState().capsuleCollapsing) setCapsuleCollapsing(false)
    }
  }, [holdMs, resetRecording, setCapsuleCollapsing])

  return (
    <motion.div
      className="relative z-10 flex h-9 items-center justify-center gap-2 px-3"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      transition={{ duration: 0.15, ease: 'easeOut' }}
      data-capsule-notice={kind}
    >
      {kind === 'learned' ? (
        <Sparkles size={12} className="flex-shrink-0 opacity-80" />
      ) : (
        <span className="h-2 w-2 flex-shrink-0 rounded-full bg-current opacity-40" />
      )}
      <p className="truncate text-[11px] font-medium">
        {notice?.kind === 'learned' ? notice.text : t('capsule.errors.cancelled')}
      </p>
    </motion.div>
  )
}
