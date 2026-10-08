import { useTranslation } from 'react-i18next'
import { motion } from 'framer-motion'
import { X } from 'lucide-react'
import { abortRecording } from '../../lib/tauri'
import { CapsuleWorkIndicator } from './CapsuleWorkIndicator'

export function CapsulePolishing() {
  const { t } = useTranslation()

  const handleCancel = async (e: React.MouseEvent) => {
    e.stopPropagation()
    try {
      await abortRecording()
    } catch (err) {
      console.error('Failed to abort polishing:', err)
    }
  }

  return (
    <motion.div className="relative z-10 flex items-center gap-2 h-9 px-3">
      <CapsuleWorkIndicator tone="thinking" />
      <p className="text-[11px] text-current leading-snug truncate flex-1 min-w-0">
        {t('capsule.thinking')}
      </p>
      <button
        onClick={handleCancel}
        aria-label={t('capsule.cancelPolishing')}
        className="flex-shrink-0 p-1 rounded-full text-current opacity-70 hover:opacity-100 hover:bg-black/10 dark:hover:bg-white/15 transition-colors bg-transparent border-none cursor-pointer"
      >
        <X size={12} />
      </button>
    </motion.div>
  )
}
