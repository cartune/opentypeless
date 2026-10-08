export const SIRI_WAVE_WIDTH = 104
export const SIRI_WAVE_HEIGHT = 30

interface Layer {
  /** Colour stops across the width. */
  colors: [string, string, string]
  /** Spatial frequency in whole cycles across the canvas. */
  cycles: number
  /** Phase drift per frame (radians). */
  speed: number
  /** Peak height as a fraction of half the canvas height. */
  gain: number
  lineWidth: number
}

export const LAYERS: Layer[] = [
  { colors: ['#22d3ee', '#60a5fa', '#a78bfa'], cycles: 1.4, speed: 0.11, gain: 1.0, lineWidth: 2 },
  {
    colors: ['#f472b6', '#c084fc', '#38bdf8'],
    cycles: 1.9,
    speed: -0.14,
    gain: 0.85,
    lineWidth: 1.6,
  },
  {
    colors: ['#fb7185', '#fbbf24', '#f472b6'],
    cycles: 2.6,
    speed: 0.19,
    gain: 0.7,
    lineWidth: 1.3,
  },
  {
    colors: ['#ffffff', '#e0f2fe', '#ffffff'],
    cycles: 1.1,
    speed: -0.08,
    gain: 0.55,
    lineWidth: 1,
  },
]

/** Siri-style layered waveform driven by the backend band meter. */
export function drawSiriWave(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  amplitudes: number[],
  phases: number[],
) {
  ctx.clearRect(0, 0, width, height)
  const mid = height / 2
  const half = mid - 1
  ctx.globalCompositeOperation = 'lighter'
  ctx.lineCap = 'round'
  LAYERS.forEach((layer, i) => {
    const amp = amplitudes[i] ?? 0
    const gradient = ctx.createLinearGradient(0, 0, width, 0)
    gradient.addColorStop(0, layer.colors[0])
    gradient.addColorStop(0.5, layer.colors[1])
    gradient.addColorStop(1, layer.colors[2])
    ctx.strokeStyle = gradient
    ctx.lineWidth = layer.lineWidth
    ctx.globalAlpha = 0.35 + 0.65 * Math.min(1, amp * 1.5)
    ctx.beginPath()
    const steps = 48
    for (let s = 0; s <= steps; s++) {
      const x = (s / steps) * width
      const t = s / steps
      // Raised-cosine envelope: energy lives in the middle, tails stay thin.
      const envelope = Math.pow(Math.sin(Math.PI * t), 1.6)
      const y =
        mid +
        Math.sin(t * layer.cycles * Math.PI * 2 + (phases[i] ?? 0)) *
          envelope *
          half *
          layer.gain *
          amp
      if (s === 0) ctx.moveTo(x, y)
      else ctx.lineTo(x, y)
    }
    ctx.stroke()
  })
  ctx.globalAlpha = 1
  ctx.globalCompositeOperation = 'source-over'
}
