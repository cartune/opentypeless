import { useEffect, useRef } from 'react'
import { useAppStore, type PipelineState } from '../stores/appStore'
import { animateCapsuleFrame, setCapsuleGlass, type CapsuleGlassTint } from '../lib/tauri'

interface CapsuleSize {
  width: number
  height: number
}

interface PhysicalPoint {
  x: number
  y: number
}

interface PhysicalDimensions {
  width: number
  height: number
}

export interface CapsuleWorkArea {
  position: PhysicalPoint
  size: PhysicalDimensions
}

export interface CapsuleMonitorBounds {
  position: PhysicalPoint
  size: PhysicalDimensions
}

export interface CapsulePlacementMonitor {
  scaleFactor: number
  workArea: CapsuleWorkArea
}

export interface CapsuleMonitorGeometry extends CapsuleMonitorBounds, CapsulePlacementMonitor {}

export interface CapsuleWindowRect extends PhysicalPoint, PhysicalDimensions {}

const CAPSULE_BOTTOM_MARGIN = 80
/** Transparent margin around the pill so CSS shadows and menus have room. */
export const CAPSULE_WINDOW_PADDING = 24
/** All pill states are 36px tall, so the native glass corner radius is fixed. */
export const CAPSULE_GLASS_RADIUS = 18

export interface CapsuleGlassInput {
  glassEnabled: boolean
  contextMenuOpen: boolean
  translationTargetMenuOpen?: boolean
  capsuleExpanded: boolean
}

/**
 * The native glass backdrop covers the whole window, so it is only applied
 * while the window is exactly pill-sized. Menus and the expanded preview grow
 * the window and must turn it off first.
 */
export function shouldApplyCapsuleGlass({
  glassEnabled,
  contextMenuOpen,
  translationTargetMenuOpen = false,
  capsuleExpanded,
}: CapsuleGlassInput): boolean {
  return glassEnabled && !contextMenuOpen && !translationTargetMenuOpen && !capsuleExpanded
}

/** With glass the window equals the pill; otherwise keep the 12px margin each side. */
export function getCapsuleWindowPadding(glass: boolean): number {
  return glass ? 0 : CAPSULE_WINDOW_PADDING
}

/**
 * Keep the pill's left edge fixed on screen when the window padding changes:
 * the pill sits `padding / 2` logical px inside the window.
 */
export function getCapsuleLeftAnchoredX(
  prevX: number,
  prevPadding: number,
  nextPadding: number,
  scale: number,
): number {
  return Math.round(prevX + ((prevPadding - nextPadding) / 2) * scale)
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), Math.max(min, max))
}

export function getCapsuleBottomCenterPosition(
  monitor: CapsulePlacementMonitor,
  logicalSize: CapsuleSize,
): PhysicalPoint {
  const physicalWidth = Math.round(logicalSize.width * monitor.scaleFactor)
  const physicalHeight = Math.round(logicalSize.height * monitor.scaleFactor)
  const margin = Math.round(CAPSULE_BOTTOM_MARGIN * monitor.scaleFactor)
  const { position, size } = monitor.workArea
  const centeredX = position.x + Math.round((size.width - physicalWidth) / 2)
  const bottomY = position.y + size.height - physicalHeight - margin

  return {
    x: clamp(centeredX, position.x, position.x + size.width - physicalWidth),
    y: clamp(bottomY, position.y, position.y + size.height - physicalHeight),
  }
}

export function isCapsuleVisibleOnAnyMonitor(
  capsule: CapsuleWindowRect,
  monitors: CapsuleMonitorBounds[],
): boolean {
  return monitors.some(({ position, size }) => {
    const right = position.x + size.width
    const bottom = position.y + size.height
    const capsuleRight = capsule.x + capsule.width
    const capsuleBottom = capsule.y + capsule.height

    return (
      capsule.x < right &&
      capsuleRight > position.x &&
      capsule.y < bottom &&
      capsuleBottom > position.y
    )
  })
}

export function getCapsuleRecoveryPosition(
  capsule: CapsuleWindowRect,
  monitors: CapsuleMonitorBounds[],
  recoveryMonitor: CapsulePlacementMonitor,
  logicalSize: CapsuleSize,
): PhysicalPoint | null {
  if (monitors.length === 0 || isCapsuleVisibleOnAnyMonitor(capsule, monitors)) {
    return null
  }

  return getCapsuleBottomCenterPosition(recoveryMonitor, logicalSize)
}

export interface CapsuleVisibilityInput {
  capsuleAutoHide: boolean
  contextMenuOpen: boolean
  translationTargetMenuOpen?: boolean
  capsuleExpanded: boolean
  hasError: boolean
  /** A brief non-error notice (e.g. "cancelled") keeps the capsule visible. */
  hasNotice?: boolean
  pipelineState: PipelineState
}

export function getCapsuleVisibility({
  capsuleAutoHide,
  contextMenuOpen,
  translationTargetMenuOpen = false,
  capsuleExpanded,
  hasError,
  hasNotice = false,
  pipelineState,
}: CapsuleVisibilityInput): boolean {
  return (
    !capsuleAutoHide ||
    contextMenuOpen ||
    translationTargetMenuOpen ||
    capsuleExpanded ||
    hasError ||
    hasNotice ||
    pipelineState !== 'idle'
  )
}

export function getCapsuleFocusable(): boolean {
  return false
}

export const CAPSULE_NOTICE_SIZE: CapsuleSize = { width: 120, height: 36 }
/** Pill size (before padding) the window shrinks to when it scales away. */
export const CAPSULE_COLLAPSED_SIZE: CapsuleSize = { width: 6, height: 6 }

export interface CapsuleLayoutInput {
  contextMenuOpen: boolean
  translationTargetMenuOpen?: boolean
  capsuleExpanded: boolean
}

/** True while the window is just the pill (no menu or expanded view). */
export function isPillLayout({
  contextMenuOpen,
  translationTargetMenuOpen = false,
  capsuleExpanded,
}: CapsuleLayoutInput): boolean {
  return !contextMenuOpen && !translationTargetMenuOpen && !capsuleExpanded
}

export type CapsuleFrameKind = 'show' | 'grow' | 'shrink' | 'cancel' | 'collapse' | 'pop'

export interface CapsuleFrameTransition {
  durationMs: number
  overshoot: boolean
}

/**
 * Timing for each native pill transition. Appearing and growing spring a
 * little past the target; the cancel shrink springs too so it reads as a
 * bounce rather than a snap; collapsing to the centre is a plain ease-out.
 */
export function getCapsuleFrameTransition(
  kind: CapsuleFrameKind,
  reducedMotion = false,
): CapsuleFrameTransition {
  if (reducedMotion) return { durationMs: 0, overshoot: false }
  switch (kind) {
    case 'show':
      return { durationMs: 320, overshoot: true }
    case 'grow':
      return { durationMs: 280, overshoot: true }
    case 'cancel':
      return { durationMs: 260, overshoot: true }
    case 'pop':
      return { durationMs: 280, overshoot: true }
    case 'collapse':
      return { durationMs: 200, overshoot: false }
    case 'shrink':
    default:
      return { durationMs: 220, overshoot: false }
  }
}

/** Which native transition a pill→pill change needs. */
export function getCapsulePillFrameKind(input: {
  collapsing: boolean
  shouldShow: boolean
  wasShown: boolean
  wasCollapsed: boolean
  isNotice: boolean
  previousWidth: number
  nextWidth: number
}): CapsuleFrameKind | 'hide' | 'none' {
  if (input.collapsing) return 'collapse'
  if (!input.shouldShow) return input.wasShown && !input.wasCollapsed ? 'hide' : 'none'
  if (!input.wasShown) return 'show'
  if (input.wasCollapsed) return 'pop'
  if (input.isNotice) return 'cancel'
  return input.nextWidth > input.previousWidth ? 'grow' : 'shrink'
}

/** Glass tint for the current UI theme: dark UI gets smoked glass, light gets frosted. */
export function getCapsuleGlassTint(darkTheme: boolean): CapsuleGlassTint {
  return darkTheme ? 'dark' : 'light'
}

function isDarkTheme(): boolean {
  return typeof document !== 'undefined' && document.documentElement.classList.contains('dark')
}

function prefersReducedMotion(): boolean {
  try {
    return typeof window !== 'undefined' && typeof window.matchMedia === 'function'
      ? window.matchMedia('(prefers-reduced-motion: reduce)').matches
      : false
  } catch {
    return false
  }
}

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms))

function getSizeForState(
  state: PipelineState,
  expanded: boolean,
  hasError: boolean,
  contextMenuOpen: boolean,
  translationTargetMenuOpen = false,
  hasNotice = false,
): CapsuleSize {
  if (translationTargetMenuOpen) return { width: 360, height: 180 }
  if (contextMenuOpen) return { width: 220, height: 220 }
  if (hasError) return { width: 200, height: 36 }
  if (expanded) return { width: 220, height: 90 }
  if (hasNotice && state === 'idle') return CAPSULE_NOTICE_SIZE
  switch (state) {
    case 'idle':
      return { width: 36, height: 36 }
    case 'preparing':
      return { width: 180, height: 36 }
    case 'recording':
      // Dot + waveform + timer + close button need the extra room.
      return { width: 236, height: 36 }
    case 'transcribing':
    case 'polishing':
      return { width: 200, height: 36 }
    case 'outputting':
      return { width: 144, height: 36 }
    case 'ask_recording':
    case 'ask_thinking':
      return { width: 168, height: 36 }
    default:
      return { width: 36, height: 36 }
  }
}

export function useCapsuleResize() {
  const pipelineState = useAppStore((s) => s.pipelineState)
  const capsuleExpanded = useAppStore((s) => s.capsuleExpanded)
  const pipelineError = useAppStore((s) => s.pipelineError)
  const pipelineNotice = useAppStore((s) => s.pipelineNotice)
  const capsuleCollapsing = useAppStore((s) => s.capsuleCollapsing)
  const contextMenuOpen = useAppStore((s) => s.contextMenuOpen)
  const translationTargetMenuOpen = useAppStore((s) => s.translationTargetMenuOpen)
  const setContextMenuReady = useAppStore((s) => s.setContextMenuReady)
  const capsuleAutoHide = useAppStore((s) => s.config.capsule_auto_hide)
  const glassEnabled = useAppStore((s) => s.config.capsule_glass_enabled)
  const glassStyle = useAppStore((s) => s.config.capsule_glass_style)
  const theme = useAppStore((s) => s.config.theme)
  const initialized = useRef(false)
  const prevWindowSize = useRef<{ width: number; height: number } | null>(null)
  const prevPadding = useRef(CAPSULE_WINDOW_PADDING)
  const glassApplied = useRef(false)
  const glassAppliedKey = useRef('')
  const effectGeneration = useRef(0)
  const shown = useRef(false)
  const collapsed = useRef(false)
  const prevPill = useRef(true)

  const hasError = pipelineError !== null
  const hasNotice = pipelineNotice !== null && !hasError

  useEffect(() => {
    const generation = ++effectGeneration.current
    let cancelled = false
    const isCurrent = () => !cancelled && effectGeneration.current === generation
    const size = getSizeForState(
      pipelineState,
      capsuleExpanded,
      hasError,
      contextMenuOpen,
      translationTargetMenuOpen,
      hasNotice,
    )
    const glass = shouldApplyCapsuleGlass({
      glassEnabled,
      contextMenuOpen,
      translationTargetMenuOpen,
      capsuleExpanded,
    })
    const padding = getCapsuleWindowPadding(glass)
    const windowWidth = size.width + padding
    const windowHeight = size.height + padding
    const pill = isPillLayout({ contextMenuOpen, translationTargetMenuOpen, capsuleExpanded })
    const collapsing = capsuleCollapsing && pill
    const shouldShow = getCapsuleVisibility({
      capsuleAutoHide,
      contextMenuOpen,
      translationTargetMenuOpen,
      capsuleExpanded,
      hasError,
      hasNotice,
      pipelineState,
    })

    import('@tauri-apps/api/window')
      .then(
        async ({
          getCurrentWindow,
          LogicalSize,
          PhysicalPosition,
          availableMonitors,
          currentMonitor,
          primaryMonitor,
        }) => {
          if (!isCurrent()) return

          const win = getCurrentWindow()
          if (!isCurrent()) return
          await win.setFocusable(getCapsuleFocusable()).catch(() => {})
          if (!isCurrent()) return

          // Clear the native backdrop *before* the window grows for a menu so
          // no glass rectangle flashes around it.
          if (glassApplied.current && !glass) {
            await setCapsuleGlass(false, CAPSULE_GLASS_RADIUS).catch(() => {})
            glassApplied.current = false
            if (!isCurrent()) return
          }
          const applyGlassIfNeeded = async () => {
            const tint = getCapsuleGlassTint(isDarkTheme())
            const key = `${glassStyle}:${tint}`
            if (glass && (!glassApplied.current || glassAppliedKey.current !== key)) {
              await setCapsuleGlass(true, CAPSULE_GLASS_RADIUS, glassStyle, tint).catch(() => {})
              glassApplied.current = true
              glassAppliedKey.current = key
            }
          }

          if (!initialized.current) {
            // First mount: position at bottom-center of screen, then show
            if (!isCurrent()) return
            await win.setSize(new LogicalSize(windowWidth, windowHeight)).catch(() => {})
            if (!isCurrent()) return

            try {
              const monitors = await availableMonitors()
              if (!isCurrent()) return

              let monitor = await currentMonitor().catch(() => null)
              if (!isCurrent()) return

              if (!monitor) {
                monitor = await primaryMonitor().catch(() => null)
                if (!isCurrent()) return
              }
              monitor ??= monitors[0]

              if (monitor) {
                const position = getCapsuleBottomCenterPosition(monitor, {
                  width: windowWidth,
                  height: windowHeight,
                })
                if (!isCurrent()) return
                await win.setPosition(new PhysicalPosition(position.x, position.y)).catch(() => {})
                if (!isCurrent()) return
              }
            } catch {
              /* ignore – monitor info unavailable */
            }

            if (!isCurrent()) return
            await applyGlassIfNeeded()
            if (!isCurrent()) return
            if (shouldShow) {
              await win.show().catch(() => {})
            } else {
              await win.hide().catch(() => {})
            }
            if (!isCurrent()) return

            initialized.current = true
            if (!isCurrent()) return
            prevWindowSize.current = { width: windowWidth, height: windowHeight }
            prevPadding.current = padding
            prevPill.current = pill
            shown.current = shouldShow
            return
          }

          // Pill → pill: animate the window natively around its centre, so the
          // capsule grows out of the middle, springs when it shrinks, and
          // scales away to a point instead of being clipped by the resize.
          if (pill && prevPill.current && padding === prevPadding.current) {
            const reduced = prefersReducedMotion()
            const kind = getCapsulePillFrameKind({
              collapsing,
              shouldShow,
              wasShown: shown.current,
              wasCollapsed: collapsed.current,
              isNotice: hasNotice && pipelineState === 'idle',
              previousWidth: prevWindowSize.current?.width ?? windowWidth,
              nextWidth: windowWidth,
            })
            const collapsedWindow = {
              width: CAPSULE_COLLAPSED_SIZE.width + padding,
              height: CAPSULE_COLLAPSED_SIZE.height + padding,
            }
            if (kind === 'collapse' || kind === 'hide') {
              const transition = getCapsuleFrameTransition('collapse', reduced)
              await animateCapsuleFrame({ ...collapsedWindow, ...transition, alpha: 0 }).catch(
                () => {},
              )
              if (!isCurrent()) return
              collapsed.current = true
              if (kind === 'hide') {
                await sleep(transition.durationMs)
                if (!isCurrent()) return
              }
            } else if (kind !== 'none') {
              if (kind === 'show') {
                // Start from a point at the centre, show, then spring open.
                await animateCapsuleFrame({ ...collapsedWindow, durationMs: 0, alpha: 0 }).catch(
                  () => {},
                )
                if (!isCurrent()) return
                await win.show().catch(() => {})
                if (!isCurrent()) return
                shown.current = true
              }
              await animateCapsuleFrame({
                width: windowWidth,
                height: windowHeight,
                ...getCapsuleFrameTransition(kind, reduced),
                alpha: 1,
              }).catch(() => {})
              if (!isCurrent()) return
              collapsed.current = false
            }
            prevWindowSize.current = { width: windowWidth, height: windowHeight }
            prevPadding.current = padding
            prevPill.current = pill
            await applyGlassIfNeeded()
            if (!isCurrent()) return
            if (shouldShow) {
              await win.show().catch(() => {})
            } else {
              await win.hide().catch(() => {})
            }
            if (!isCurrent()) return
            shown.current = shouldShow
            return
          }

          // Subsequent resizes: left edge + vertical center stay fixed.
          // Since content is always padded 12px each side, the capsule at x=12
          // is identical to a centered capsule — so the mic icon never moves.
          const prev = prevWindowSize.current
          if (prev) {
            const pos = await win.outerPosition().catch(() => null)
            if (!isCurrent()) return

            if (pos) {
              const monitor = await currentMonitor().catch(() => null)
              if (!isCurrent()) return

              let scale = monitor?.scaleFactor
              if (scale === undefined) {
                scale = await win.scaleFactor().catch(() => 1)
                if (!isCurrent()) return
              }

              const oldSize = await win.outerSize().catch(() => null)
              if (!isCurrent()) return

              const oldHeight = oldSize?.height ?? Math.round(prev.height * scale)
              const physicalWidth = Math.round(windowWidth * scale)
              const physicalHeight = Math.round(windowHeight * scale)
              let newX = getCapsuleLeftAnchoredX(pos.x, prevPadding.current, padding, scale)
              let newY = Math.round(pos.y + oldHeight / 2 - physicalHeight / 2)

              if (!isCurrent()) return
              await win.setSize(new LogicalSize(windowWidth, windowHeight)).catch(() => {})
              if (!isCurrent()) return

              try {
                const monitors = await availableMonitors()
                if (!isCurrent()) return

                const proposedRect = {
                  x: newX,
                  y: newY,
                  width: physicalWidth,
                  height: physicalHeight,
                }
                if (monitors.length > 0) {
                  const recoveryMonitor =
                    (await primaryMonitor().catch(() => null)) ?? monitor ?? monitors[0]
                  if (!isCurrent()) return

                  const recovered = getCapsuleRecoveryPosition(
                    proposedRect,
                    monitors,
                    recoveryMonitor,
                    {
                      width: windowWidth,
                      height: windowHeight,
                    },
                  )
                  if (recovered) {
                    newX = recovered.x
                    newY = recovered.y
                  }
                }
              } catch {
                /* keep the previous position when monitor info is unavailable */
              }

              if (!isCurrent()) return
              await win.setPosition(new PhysicalPosition(newX, newY)).catch(() => {})
              if (!isCurrent()) return
            } else {
              if (!isCurrent()) return
              await win.setSize(new LogicalSize(windowWidth, windowHeight)).catch(() => {})
              if (!isCurrent()) return
            }
          } else {
            if (!isCurrent()) return
            await win.setSize(new LogicalSize(windowWidth, windowHeight)).catch(() => {})
            if (!isCurrent()) return
          }

          if (!isCurrent()) return
          prevWindowSize.current = { width: windowWidth, height: windowHeight }
          prevPadding.current = padding
          prevPill.current = pill
          if (collapsed.current) {
            // A menu opened from the collapsed point: restore full alpha.
            await animateCapsuleFrame({
              width: windowWidth,
              height: windowHeight,
              anchor: 'left',
              durationMs: 0,
              alpha: 1,
            }).catch(() => {})
            if (!isCurrent()) return
            collapsed.current = false
          }
          await applyGlassIfNeeded()
          if (!isCurrent()) return

          // Signal that the window has finished resizing for context menu
          if (contextMenuOpen) {
            if (!isCurrent()) return
            setContextMenuReady(true)
          }

          if (!isCurrent()) return
          if (shouldShow) {
            await win.show().catch(() => {})
          } else {
            await win.hide().catch(() => {})
          }
          if (!isCurrent()) return
          shown.current = shouldShow
        },
      )
      .catch(() => {})

    return () => {
      cancelled = true
    }
  }, [
    pipelineState,
    capsuleExpanded,
    hasError,
    hasNotice,
    capsuleCollapsing,
    contextMenuOpen,
    translationTargetMenuOpen,
    capsuleAutoHide,
    glassEnabled,
    glassStyle,
    theme,
    setContextMenuReady,
  ])

  return getSizeForState(
    pipelineState,
    capsuleExpanded,
    hasError,
    contextMenuOpen,
    translationTargetMenuOpen,
    hasNotice,
  )
}
