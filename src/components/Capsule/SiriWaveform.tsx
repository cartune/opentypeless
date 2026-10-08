import { useEffect, useRef } from 'react'
import { useAppStore } from '../../stores/appStore'
import { INITIAL_WAVE_STATE, WaveState, nextWaveState } from './waveformLevels'
import {
  SIRI_WAVE_HEIGHT,
  SIRI_WAVE_WIDTH,
  type SiriPalette,
  drawSiriWave,
  paletteLayers,
} from './siriWave'

export function SiriWaveform({ palette = 'siri' }: { palette?: SiriPalette }) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null)
  const stateRef = useRef<WaveState>(INITIAL_WAVE_STATE)
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
      stateRef.current = nextWaveState(stateRef.current, meter)
      const amplitudes = stateRef.current.amplitudes
      // Phase speed follows energy, so the wave hurries when you speak.
      const energy = 0.6 + amplitudes[3] * 1.4
      const layers = paletteLayers(palette)
      phasesRef.current = phasesRef.current.map((p, i) => p + layers[i].speed * energy)
      drawSiriWave(ctx, SIRI_WAVE_WIDTH, SIRI_WAVE_HEIGHT, amplitudes, phasesRef.current, layers)
      rafRef.current = requestAnimationFrame(animate)
    }
    rafRef.current = requestAnimationFrame(animate)
    return () => cancelAnimationFrame(rafRef.current)
  }, [palette])

  return (
    <canvas
      ref={canvasRef}
      data-testid="waveform"
      data-variant={palette}
      style={{ width: SIRI_WAVE_WIDTH, height: SIRI_WAVE_HEIGHT, display: 'block' }}
    />
  )
}
