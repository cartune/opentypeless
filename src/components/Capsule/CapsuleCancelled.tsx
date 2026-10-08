import { useEffect } from 'react'
import { motion } from 'framer-motion'
import { useTranslation } from 'react-i18next'
import { useAppStore } from '../../stores/appStore'

/** How long the quiet "cancelled" pill stays before collapsing to the dot. */
export const CAPSULE_CANCELLED_MS = 700

export function CapsuleCancelled() {
  const { t } = useTranslation()
  const setPipelineNotice = useAppStore((s) => s.setPipelineNotice)
  const resetRecording = useAppStore((s) => s.resetRecording)

  useEffect(() => {
    const timer = setTimeout(() => {
      setPipelineNotice(null)
      // Only reset recording state if the pipeline is still idle; a new run
      // started inside the notice window must not be clobbered.
      if (useAppStore.getState().pipelineState === 'idle') {
        resetRecording()
      }
    }, CAPSULE_CANCELLED_MS)
    return () => clearTimeout(timer)
  }, [resetRecording, setPipelineNotice])

  return (
    <motion.div
      className="relative z-10 flex h-9 items-center justify-center gap-2 px-3"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      transition={{ duration: 0.15, ease: 'easeOut' }}
      data-capsule-notice="cancelled"
    >
      <span className="h-2 w-2 flex-shrink-0 rounded-full bg-current opacity-40" />
      <p className="truncate text-[11px] font-medium">{t('capsule.errors.cancelled')}</p>
    </motion.div>
  )
}
