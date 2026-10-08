import { useEffect, useRef } from 'react'
import { useAppStore } from '../../stores/appStore'
import { EMPTY_LEVEL_HISTORY, LEVEL_BAR_COUNT, nextLevelHistory } from './waveformLevels'
import { SIRI_WAVE_HEIGHT, SIRI_WAVE_WIDTH } from './siriWave'

const BAR_WIDTH = 2
const MIN_BAR = 3

/**
 * Live loudness as scrolling bars (like a voice memo): each bar is the
 * loudest moment of the last 60 ms, newest on the right. Drawn in the
 * capsule's text colour so it reads on both glass tints.
 */
export function LevelBarsWaveform() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null)
  const historyRef = useRef(EMPTY_LEVEL_HISTORY)
  const rafRef = useRef(0)

  useEffect(() => {
    const canvas = canvasRef.current
    const ctx = canvas?.getContext('2d')
    if (!canvas || !ctx) return
    const dpr = Math.max(1, window.devicePixelRatio || 1)
    canvas.width = SIRI_WAVE_WIDTH * dpr
    canvas.height = SIRI_WAVE_HEIGHT * dpr
    ctx.scale(dpr, dpr)
    const gap = (SIRI_WAVE_WIDTH - LEVEL_BAR_COUNT * BAR_WIDTH) / (LEVEL_BAR_COUNT - 1)

    const animate = (now: number) => {
      const meter = useAppStore.getState().audioMeter
      historyRef.current = nextLevelHistory(historyRef.current, meter, now)
      const color = getComputedStyle(canvas).color || '#ffffff'
      ctx.clearRect(0, 0, SIRI_WAVE_WIDTH, SIRI_WAVE_HEIGHT)
      ctx.fillStyle = color
      const mid = SIRI_WAVE_HEIGHT / 2
      historyRef.current.bars.forEach((value, i) => {
        const h = MIN_BAR + (SIRI_WAVE_HEIGHT - MIN_BAR) * value
        const x = i * (BAR_WIDTH + gap)
        ctx.globalAlpha = 0.35 + 0.65 * value
        ctx.beginPath()
        ctx.roundRect(x, mid - h / 2, BAR_WIDTH, h, BAR_WIDTH / 2)
        ctx.fill()
      })
      ctx.globalAlpha = 1
      rafRef.current = requestAnimationFrame(animate)
    }
    rafRef.current = requestAnimationFrame(animate)
    return () => cancelAnimationFrame(rafRef.current)
  }, [])

  return (
    <canvas
      ref={canvasRef}
      data-testid="waveform"
      data-variant="bars"
      style={{ width: SIRI_WAVE_WIDTH, height: SIRI_WAVE_HEIGHT, display: 'block' }}
    />
  )
}
