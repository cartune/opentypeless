import { useEffect, useRef } from 'react'
import { useReducedMotion } from 'framer-motion'
import { useAppStore } from '../../stores/appStore'
import { SiriWaveform } from './SiriWaveform'
import { LevelBarsWaveform } from './LevelBarsWaveform'
import {
  WAVEFORM_BAR_COUNT,
  WAVEFORM_MAX_HEIGHT,
  WAVEFORM_MIN_HEIGHT,
  nextWaveformHeights,
} from './waveformLevels'

/** True when a 2D canvas is available (not in jsdom) and motion is allowed. */
function canvasSupported(): boolean {
  if (typeof document === 'undefined') return false
  try {
    return Boolean(document.createElement('canvas').getContext('2d'))
  } catch {
    return false
  }
}

export function Waveform() {
  const reducedMotion = useReducedMotion()
  const style = useAppStore((s) => s.config.capsule_waveform_style)
  if (!reducedMotion && canvasSupported()) {
    if (style === 'bars') return <LevelBarsWaveform />
    return <SiriWaveform palette={style === 'mono' ? 'mono' : 'siri'} />
  }
  return <BarWaveform />
}

function BarWaveform() {
  const barsRef = useRef<(HTMLDivElement | null)[]>([])
  const heightsRef = useRef<number[]>(Array(WAVEFORM_BAR_COUNT).fill(WAVEFORM_MIN_HEIGHT))
  const rafRef = useRef<number>(0)
  const reduced = useReducedMotion()

  useEffect(() => {
    if (reduced) {
      // Static bars at mid-height when reduced motion is preferred
      barsRef.current.forEach((bar) => {
        if (!bar) return
        bar.style.height = `${(WAVEFORM_MIN_HEIGHT + WAVEFORM_MAX_HEIGHT) / 2}px`
        bar.style.opacity = '0.7'
      })
      return
    }

    const animate = () => {
      const level = useAppStore.getState().audioVolume
      heightsRef.current = nextWaveformHeights(heightsRef.current, level)
      barsRef.current.forEach((bar, i) => {
        if (!bar) return
        const height = heightsRef.current[i]
        const normalized =
          (height - WAVEFORM_MIN_HEIGHT) / (WAVEFORM_MAX_HEIGHT - WAVEFORM_MIN_HEIGHT)
        bar.style.height = `${height}px`
        bar.style.opacity = `${0.45 + 0.55 * normalized}`
      })
      rafRef.current = requestAnimationFrame(animate)
    }

    rafRef.current = requestAnimationFrame(animate)
    return () => cancelAnimationFrame(rafRef.current)
  }, [reduced])

  return (
    <div className="flex items-center justify-center gap-[3px] h-4" data-testid="waveform">
      {Array.from({ length: WAVEFORM_BAR_COUNT }).map((_, i) => (
        <div
          key={i}
          ref={(el) => {
            barsRef.current[i] = el
          }}
          className="w-[2px] rounded-full bg-current opacity-80"
          style={{
            height: `${WAVEFORM_MIN_HEIGHT}px`,
            opacity: 0.45,
            transition: 'height 60ms linear, opacity 60ms linear',
          }}
        />
      ))}
    </div>
  )
}
