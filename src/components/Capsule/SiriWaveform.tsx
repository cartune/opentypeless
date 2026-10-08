import { useEffect, useRef } from 'react'
import { useAppStore } from '../../stores/appStore'
import { WAVE_LAYER_COUNT, nextWaveAmplitudes } from './waveformLevels'
import { LAYERS, SIRI_WAVE_HEIGHT, SIRI_WAVE_WIDTH, drawSiriWave } from './siriWave'

export function SiriWaveform() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null)
  const ampsRef = useRef<number[]>(Array(WAVE_LAYER_COUNT).fill(0))
  const phasesRef = useRef<number[]>([0, 1.3, 2.1, 0.7])
  const rafRef = useRef(0)

  useEffect(() => {
    const canvas = canvasRef.current
    const ctx = canvas?.getContext('2d')
    if (!canvas || !ctx) return
    const dpr = Math.max(1, window.devicePixelRatio || 1)
    canvas.width = SIRI_WAVE_WIDTH * dpr
    canvas.height = SIRI_WAVE_HEIGHT * dpr
    ctx.scale(dpr, dpr)

    const animate = () => {
      const meter = useAppStore.getState().audioMeter
      ampsRef.current = nextWaveAmplitudes(ampsRef.current, meter)
      // Phase speed follows energy, so the wave hurries when you speak.
      const energy = 0.6 + ampsRef.current[3] * 1.4
      phasesRef.current = phasesRef.current.map((p, i) => p + LAYERS[i].speed * energy)
      drawSiriWave(ctx, SIRI_WAVE_WIDTH, SIRI_WAVE_HEIGHT, ampsRef.current, phasesRef.current)
      rafRef.current = requestAnimationFrame(animate)
    }
    rafRef.current = requestAnimationFrame(animate)
    return () => cancelAnimationFrame(rafRef.current)
  }, [])

  return (
    <canvas
      ref={canvasRef}
      data-testid="waveform"
      data-variant="siri"
      style={{ width: SIRI_WAVE_WIDTH, height: SIRI_WAVE_HEIGHT, display: 'block' }}
    />
  )
}
