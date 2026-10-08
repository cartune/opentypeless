import { useTranslation } from 'react-i18next'
import { MessageCircle, WandSparkles } from 'lucide-react'
import { useAppStore } from '../../stores/appStore'
import { CapsuleWorkIndicator } from './CapsuleWorkIndicator'

export function CapsuleAskThinking() {
  const { t } = useTranslation()
  const askSelection = useAppStore((s) => s.askSelection)
  const commandMode = Boolean(askSelection?.commandMode)

  return (
    <div className="relative z-10 flex h-9 items-center gap-2 px-3">
      {commandMode ? (
        <WandSparkles size={13} className="shrink-0 text-current opacity-90" />
      ) : (
        <MessageCircle size={13} className="shrink-0 text-current opacity-90" />
      )}
      <span className="whitespace-nowrap text-[11px] font-medium text-current">
        {commandMode ? t('capsule.commandMode') : t('ask.title')}
      </span>
      <CapsuleWorkIndicator tone="thinking" />
      <p className="min-w-0 flex-1 truncate text-[11px] leading-snug text-current opacity-90">
        {commandMode ? t('capsule.commandThinking') : t('ask.thinking')}
      </p>
    </div>
  )
}
