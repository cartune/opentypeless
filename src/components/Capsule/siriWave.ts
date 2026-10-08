export const SIRI_WAVE_WIDTH = 104
export const SIRI_WAVE_HEIGHT = 30

interface Layer {
  /** Colour stops across the width. */
  colors: [string, string, string]
  /** Neon glow colour behind the stroke. */
  glow: string
  /** Spatial frequency in whole cycles across the canvas. */
  cycles: number
  /** Phase drift per frame (radians). */
  speed: number
  /** Peak height as a fraction of half the canvas height. */
  gain: number
  lineWidth: number
}

/** Fluorescent palette: cyan, magenta, lime, violet, each with its own glow. */
export const LAYERS: Layer[] = [
  {
    colors: ['#00f5ff', '#4df2ff', '#7cf5ff'],
    glow: '#00e5ff',
    cycles: 1.4,
    speed: 0.11,
    gain: 1.0,
    lineWidth: 1.1,
  },
  {
    colors: ['#ff2bd6', '#ff5ce6', '#c06bff'],
    glow: '#ff2bd6',
    cycles: 1.9,
    speed: -0.14,
    gain: 0.85,
    lineWidth: 0.9,
  },
  {
    colors: ['#b8ff00', '#d9ff4d', '#ffe600'],
    glow: '#b8ff00',
    cycles: 2.6,
    speed: 0.19,
    gain: 0.7,
    lineWidth: 0.8,
  },
  {
    colors: ['#9d6bff', '#d7c8ff', '#9d6bff'],
    glow: '#9d6bff',
    cycles: 1.1,
    speed: -0.08,
    gain: 0.6,
    lineWidth: 0.8,
  },
]

const STEPS = 48
/** Glow radius in CSS px; the canvas is tiny so this is cheap per frame. */
const GLOW_BLUR = 5

/** Vertical excursion of one layer at horizontal position `t` (0..1). */
export function waveOffset(layer: Layer, t: number, amp: number, phase: number, half: number) {
  // Raised-cosine envelope: energy lives in the middle, tails stay thin.
  const envelope = Math.pow(Math.sin(Math.PI * t), 1.6)
  return Math.sin(t * layer.cycles * Math.PI * 2 + phase) * envelope * half * layer.gain * amp
}

function tracePath(
  ctx: CanvasRenderingContext2D,
  layer: Layer,
  width: number,
  mid: number,
  half: number,
  amp: number,
  phase: number,
  sign: 1 | -1,
) {
  for (let s = 0; s <= STEPS; s++) {
    const t = s / STEPS
    const x = t * width
    const y = mid + sign * waveOffset(layer, t, amp, phase, half)
    if (s === 0) ctx.moveTo(x, y)
    else ctx.lineTo(x, y)
  }
}

/**
 * Siri-style layered waveform driven by the backend band meter. Each layer is
 * mirrored around the centre line, filled with a translucent gradient and
 * stroked with a neon glow plus a bright core so it reads over a Clear glass
 * capsule on any desktop.
 */
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
  // Normal blending: additive 'lighter' turns four overlapping neon layers
  // into a white blob on a light desktop.
  ctx.globalCompositeOperation = 'source-over'
  ctx.lineCap = 'round'
  ctx.lineJoin = 'round'
  LAYERS.forEach((layer, i) => {
    const amp = Math.max(0, Math.min(1, amplitudes[i] ?? 0))
    const phase = phases[i] ?? 0
    const gradient = ctx.createLinearGradient(0, 0, width, 0)
    gradient.addColorStop(0, layer.colors[0])
    gradient.addColorStop(0.5, layer.colors[1])
    gradient.addColorStop(1, layer.colors[2])

    // Translucent body between the mirrored curves.
    ctx.shadowBlur = 0
    ctx.beginPath()
    tracePath(ctx, layer, width, mid, half, amp, phase, -1)
    for (let s = STEPS; s >= 0; s--) {
      const t = s / STEPS
      ctx.lineTo(t * width, mid + waveOffset(layer, t, amp, phase, half))
    }
    ctx.closePath()
    ctx.fillStyle = gradient
    ctx.globalAlpha = 0.05 + 0.12 * amp
    ctx.fill()

    // Neon stroke with glow on both curves.
    ctx.shadowColor = layer.glow
    ctx.shadowBlur = GLOW_BLUR
    ctx.strokeStyle = gradient
    ctx.lineWidth = layer.lineWidth
    ctx.globalAlpha = 0.55 + 0.45 * Math.min(1, amp * 1.4)
    ctx.beginPath()
    tracePath(ctx, layer, width, mid, half, amp, phase, -1)
    ctx.stroke()
    if (amp > 0.02) {
      ctx.beginPath()
      tracePath(ctx, layer, width, mid, half, amp, phase, 1)
      ctx.stroke()
    }
  })

  // One thin bright core on the main layer keeps the shape readable over a
  // light desktop without bleaching the colours.
  const main = LAYERS[0]
  const mainAmp = Math.max(0, Math.min(1, amplitudes[0] ?? 0))
  ctx.shadowBlur = 0
  ctx.strokeStyle = '#ffffff'
  ctx.lineWidth = 0.5
  ctx.globalAlpha = 0.15 + 0.3 * mainAmp
  ctx.beginPath()
  tracePath(ctx, main, width, mid, half, mainAmp, phases[0] ?? 0, -1)
  ctx.stroke()
  ctx.shadowBlur = 0
  ctx.globalAlpha = 1
  ctx.globalCompositeOperation = 'source-over'
}
