import { useEffect, useRef, useState } from 'react'
import { useAppStore } from '../../stores/appStore'
import { NO_AUDIO_HINT_AFTER_SECONDS, isSilentLevel } from './waveformLevels'

/**
 * True once the microphone has delivered nothing audible for
 * NO_AUDIO_HINT_AFTER_SECONDS since recording started (or since the last
 * audible frame). Lets the capsule say "no audio" instead of looking alive.
 */
export function useNoAudioHint(): boolean {
  const audioVolume = useAppStore((s) => s.audioVolume)
  const lastAudibleAtRef = useRef(Date.now())
  const [showHint, setShowHint] = useState(false)

  useEffect(() => {
    if (!isSilentLevel(audioVolume)) {
      lastAudibleAtRef.current = Date.now()
      setShowHint(false)
    }
  }, [audioVolume])

  useEffect(() => {
    const interval = setInterval(() => {
      const silentFor = (Date.now() - lastAudibleAtRef.current) / 1000
      setShowHint(silentFor >= NO_AUDIO_HINT_AFTER_SECONDS)
    }, 250)
    return () => clearInterval(interval)
  }, [])

  return showHint
}
