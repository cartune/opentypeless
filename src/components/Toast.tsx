import { useState, useEffect, useCallback } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { CheckCircle2, XCircle, Info, Sparkles } from 'lucide-react'
import { spring } from '../lib/animations'
import { registerToastHandler, type ToastType } from './toast-service'

interface ToastMessage {
  id: number
  text: string
  type: ToastType
}

const icons: Record<ToastType, typeof Info> = {
  success: CheckCircle2,
  error: XCircle,
  info: Info,
  learned: Sparkles,
}

/** How long each kind stays; learned toasts carry words worth reading. */
const durations: Record<ToastType, number> = {
  success: 3000,
  error: 3000,
  info: 3000,
  learned: 6000,
}

const colors: Record<ToastType, string> = {
  success: 'text-success',
  error: 'text-error',
  info: 'text-accent',
  learned: 'text-accent',
}

const surfaces: Record<ToastType, string> = {
  success: 'bg-bg-secondary border-border shadow-lg',
  error: 'bg-bg-secondary border-border shadow-lg',
  info: 'bg-bg-secondary border-border shadow-lg',
  // Frosted glass pill, like the capsule.
  learned:
    'glass-toast bg-white/60 dark:bg-white/10 border-black/10 dark:border-white/15 shadow-xl backdrop-blur-xl',
}

export function ToastContainer() {
  const [toasts, setToasts] = useState<ToastMessage[]>([])

  const remove = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id))
  }, [])

  useEffect(() => {
    return registerToastHandler((text: string, type: ToastType = 'info') => {
      const id = Date.now()
      setToasts((prev) => [...prev, { id, text, type }])
      setTimeout(() => remove(id), durations[type])
    })
  }, [remove])

  return (
    <div className="fixed top-4 right-4 z-[9999] flex flex-col gap-2 pointer-events-none">
      <AnimatePresence>
        {toasts.map((t) => {
          const Icon = icons[t.type]
          return (
            <motion.div
              key={t.id}
              initial={{ opacity: 0, x: 40, scale: 0.95 }}
              animate={{ opacity: 1, x: 0, scale: 1 }}
              exit={{ opacity: 0, x: 40, scale: 0.95 }}
              transition={spring.jellyGentle}
              className={`pointer-events-auto flex items-center gap-2 px-3 py-2.5 border rounded-full text-[13px] text-text-primary max-w-[360px] ${surfaces[t.type]}`}
              data-toast-type={t.type}
              role="alert"
            >
              <Icon size={14} className={`flex-shrink-0 ${colors[t.type]}`} />
              <span>{t.text}</span>
            </motion.div>
          )
        })}
      </AnimatePresence>
    </div>
  )
}
