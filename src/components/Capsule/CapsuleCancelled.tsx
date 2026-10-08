import { useEffect } from 'react'
import { motion } from 'framer-motion'
import { useTranslation } from 'react-i18next'
import { useAppStore } from '../../stores/appStore'

/** How long the quiet "cancelled" pill stays before collapsing to the dot. */
export const CAPSULE_CANCELLED_MS = 700
/** How long the window takes to scale away to its centre afterwards. */
export const CAPSULE_COLLAPSE_MS = 200

export function CapsuleCancelled() {
  const { t } = useTranslation()
  const setCapsuleCollapsing = useAppStore((s) => s.setCapsuleCollapsing)
  const resetRecording = useAppStore((s) => s.resetRecording)

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
    }, CAPSULE_CANCELLED_MS)
    return () => {
      clearTimeout(collapse)
      if (clear) clearTimeout(clear)
      // A new run that interrupts the notice must not leave the window collapsed.
      if (useAppStore.getState().capsuleCollapsing) setCapsuleCollapsing(false)
    }
  }, [resetRecording, setCapsuleCollapsing])

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
