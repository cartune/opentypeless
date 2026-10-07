import { createElement } from 'react'
import { act, cleanup, render, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  CAPSULE_GLASS_RADIUS,
  CAPSULE_WINDOW_PADDING,
  getCapsuleBottomCenterPosition,
  getCapsuleFocusable,
  getCapsuleLeftAnchoredX,
  getCapsuleRecoveryPosition,
  getCapsuleVisibility,
  getCapsuleWindowPadding,
  isCapsuleVisibleOnAnyMonitor,
  shouldApplyCapsuleGlass,
  useCapsuleResize,
} from '../useCapsuleResize'
import { useAppStore } from '../../stores/appStore'

const tauriMocks = vi.hoisted(() => ({
  setCapsuleGlass: vi.fn(),
}))

vi.mock('../../lib/tauri', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../lib/tauri')>()),
  setCapsuleGlass: tauriMocks.setCapsuleGlass,
}))

const windowApiMocks = vi.hoisted(() => ({
  getCurrentWindow: vi.fn(),
  availableMonitors: vi.fn(),
  currentMonitor: vi.fn(),
  primaryMonitor: vi.fn(),
  setFocusable: vi.fn(),
  setSize: vi.fn(),
  setPosition: vi.fn(),
  outerPosition: vi.fn(),
  outerSize: vi.fn(),
  scaleFactor: vi.fn(),
  show: vi.fn(),
  hide: vi.fn(),
}))

vi.mock('@tauri-apps/api/window', () => {
  class LogicalSize {
    constructor(
      public width: number,
      public height: number,
    ) {}
  }

  class PhysicalPosition {
    constructor(
      public x: number,
      public y: number,
    ) {}
  }

  return {
    getCurrentWindow: windowApiMocks.getCurrentWindow,
    availableMonitors: windowApiMocks.availableMonitors,
    currentMonitor: windowApiMocks.currentMonitor,
    primaryMonitor: windowApiMocks.primaryMonitor,
    LogicalSize,
    PhysicalPosition,
  }
})

const logicalCapsuleSize = { width: 224, height: 60 }

describe('getCapsuleVisibility', () => {
  it('hides idle capsule when auto-hide is enabled', () => {
    expect(
      getCapsuleVisibility({
        capsuleAutoHide: true,
        contextMenuOpen: false,
        capsuleExpanded: false,
        hasError: false,
        pipelineState: 'idle',
      }),
    ).toBe(false)
  })

  it('shows idle capsule when an error appears', () => {
    expect(
      getCapsuleVisibility({
        capsuleAutoHide: true,
        contextMenuOpen: false,
        capsuleExpanded: false,
        hasError: true,
        pipelineState: 'idle',
      }),
    ).toBe(true)
  })

  it('shows active capsule while recording', () => {
    expect(
      getCapsuleVisibility({
        capsuleAutoHide: true,
        contextMenuOpen: false,
        capsuleExpanded: false,
        hasError: false,
        pipelineState: 'recording',
      }),
    ).toBe(true)
  })

  it('keeps capsule visible while preparing even when auto-hide is enabled', () => {
    expect(
      getCapsuleVisibility({
        capsuleAutoHide: true,
        contextMenuOpen: false,
        capsuleExpanded: false,
        hasError: false,
        pipelineState: 'preparing',
      }),
    ).toBe(true)
  })

  it('keeps capsule visible while Ask is recording', () => {
    expect(
      getCapsuleVisibility({
        capsuleAutoHide: true,
        contextMenuOpen: false,
        capsuleExpanded: false,
        hasError: false,
        pipelineState: 'ask_recording',
      }),
    ).toBe(true)
  })

  it('shows idle capsule while the context menu is open', () => {
    expect(
      getCapsuleVisibility({
        capsuleAutoHide: true,
        contextMenuOpen: true,
        capsuleExpanded: false,
        hasError: false,
        pipelineState: 'idle',
      }),
    ).toBe(true)
  })

  it('keeps the capsule overlay from stealing keyboard output focus', () => {
    expect(getCapsuleFocusable()).toBe(false)
  })
})

describe('capsule monitor geometry', () => {
  it.each([
    {
      name: 'primary monitor',
      workArea: { position: { x: 0, y: 0 }, size: { width: 1920, height: 1040 } },
      expected: { x: 848, y: 900 },
    },
    {
      name: 'monitor to the right',
      workArea: { position: { x: 3440, y: 0 }, size: { width: 1920, height: 1040 } },
      expected: { x: 4288, y: 900 },
    },
    {
      name: 'monitor to the left',
      workArea: { position: { x: -1920, y: 0 }, size: { width: 1920, height: 1040 } },
      expected: { x: -1072, y: 900 },
    },
    {
      name: 'monitor above',
      workArea: { position: { x: 0, y: -1080 }, size: { width: 1920, height: 1040 } },
      expected: { x: 848, y: -180 },
    },
  ])('uses the global work-area origin for the $name', ({ workArea, expected }) => {
    expect(
      getCapsuleBottomCenterPosition({ scaleFactor: 1, workArea }, logicalCapsuleSize),
    ).toEqual(expected)
  })

  it('uses physical pixels for mixed-DPI monitors', () => {
    expect(
      getCapsuleBottomCenterPosition(
        {
          scaleFactor: 1.5,
          workArea: { position: { x: 1920, y: 100 }, size: { width: 2560, height: 1400 } },
        },
        logicalCapsuleSize,
      ),
    ).toEqual({ x: 3032, y: 1290 })
  })

  it('supports a 200% scale factor without converting the global origin', () => {
    expect(
      getCapsuleBottomCenterPosition(
        {
          scaleFactor: 2,
          workArea: { position: { x: -3840, y: -200 }, size: { width: 3840, height: 2080 } },
        },
        logicalCapsuleSize,
      ),
    ).toEqual({ x: -2144, y: 1600 })
  })

  it('uses the work area rather than placing the capsule behind the taskbar', () => {
    expect(
      getCapsuleBottomCenterPosition(
        {
          scaleFactor: 1,
          workArea: { position: { x: 0, y: 40 }, size: { width: 1920, height: 1000 } },
        },
        logicalCapsuleSize,
      ),
    ).toEqual({ x: 848, y: 900 })
  })

  it('preserves a capsule that is even partially visible on any monitor', () => {
    const monitorBounds = [
      { position: { x: 0, y: 0 }, size: { width: 1920, height: 1040 } },
      { position: { x: 1920, y: 0 }, size: { width: 1920, height: 1040 } },
    ]

    expect(
      isCapsuleVisibleOnAnyMonitor({ x: 3830, y: 900, width: 224, height: 60 }, monitorBounds),
    ).toBe(true)
  })

  it('reports a capsule as off-screen when it touches no monitor', () => {
    const monitorBounds = [
      { position: { x: -1920, y: 0 }, size: { width: 1920, height: 1040 } },
      { position: { x: 0, y: 0 }, size: { width: 1920, height: 1040 } },
    ]

    expect(
      isCapsuleVisibleOnAnyMonitor({ x: 2500, y: 900, width: 224, height: 60 }, monitorBounds),
    ).toBe(false)
  })

  it('does not recover a user-positioned capsule that remains partially visible', () => {
    const monitors = [
      {
        position: { x: 0, y: 0 },
        size: { width: 1920, height: 1080 },
        scaleFactor: 1,
        workArea: { position: { x: 0, y: 0 }, size: { width: 1920, height: 1040 } },
      },
      {
        position: { x: 1920, y: 0 },
        size: { width: 2560, height: 1440 },
        scaleFactor: 1.5,
        workArea: { position: { x: 1920, y: 0 }, size: { width: 2560, height: 1400 } },
      },
    ]

    expect(
      getCapsuleRecoveryPosition(
        { x: 4400, y: 1200, width: 336, height: 90 },
        monitors,
        monitors[0],
        logicalCapsuleSize,
      ),
    ).toBeNull()
  })

  it('does not recover a capsule visible only in a taskbar or Dock reserved area', () => {
    const monitors = [
      {
        position: { x: 0, y: 0 },
        size: { width: 1920, height: 1080 },
        scaleFactor: 1,
        workArea: { position: { x: 0, y: 0 }, size: { width: 1920, height: 1040 } },
      },
    ]

    expect(
      getCapsuleRecoveryPosition(
        { x: 848, y: 1050, width: 224, height: 60 },
        monitors,
        monitors[0],
        logicalCapsuleSize,
      ),
    ).toBeNull()
  })

  it('recovers a fully off-screen capsule to the primary work area', () => {
    const monitors = [
      {
        position: { x: 0, y: 0 },
        size: { width: 1920, height: 1080 },
        scaleFactor: 1,
        workArea: { position: { x: 0, y: 0 }, size: { width: 1920, height: 1040 } },
      },
      {
        position: { x: 1920, y: 0 },
        size: { width: 2560, height: 1440 },
        scaleFactor: 1.5,
        workArea: { position: { x: 1920, y: 0 }, size: { width: 2560, height: 1400 } },
      },
    ]

    expect(
      getCapsuleRecoveryPosition(
        { x: 5200, y: 1200, width: 336, height: 90 },
        monitors,
        monitors[0],
        logicalCapsuleSize,
      ),
    ).toEqual({ x: 848, y: 900 })
  })
})

function HookHarness() {
  useCapsuleResize()
  return null
}

describe('capsule glass geometry', () => {
  it('applies glass only while the window is pill-sized', () => {
    const base = {
      glassEnabled: true,
      contextMenuOpen: false,
      translationTargetMenuOpen: false,
      capsuleExpanded: false,
    }
    expect(shouldApplyCapsuleGlass(base)).toBe(true)
    expect(shouldApplyCapsuleGlass({ ...base, contextMenuOpen: true })).toBe(false)
    expect(shouldApplyCapsuleGlass({ ...base, translationTargetMenuOpen: true })).toBe(false)
    expect(shouldApplyCapsuleGlass({ ...base, capsuleExpanded: true })).toBe(false)
    expect(shouldApplyCapsuleGlass({ ...base, glassEnabled: false })).toBe(false)
  })

  it('drops the transparent window padding when glass is on', () => {
    expect(getCapsuleWindowPadding(true)).toBe(0)
    expect(getCapsuleWindowPadding(false)).toBe(CAPSULE_WINDOW_PADDING)
    expect(CAPSULE_GLASS_RADIUS).toBe(18)
  })

  it('keeps the pill left edge fixed when the padding changes', () => {
    expect(getCapsuleLeftAnchoredX(100, 0, 24, 1)).toBe(88)
    expect(getCapsuleLeftAnchoredX(88, 24, 0, 1)).toBe(100)
    expect(getCapsuleLeftAnchoredX(88, 24, 0, 2)).toBe(112)
    expect(getCapsuleLeftAnchoredX(50, 24, 24, 1)).toBe(50)
  })
})

describe('useCapsuleResize async updates', () => {
  const monitor = {
    position: { x: 0, y: 0 },
    size: { width: 1920, height: 1080 },
    scaleFactor: 1,
    workArea: { position: { x: 0, y: 0 }, size: { width: 1920, height: 1040 } },
  }

  beforeEach(() => {
    useAppStore.setState((state) => ({
      pipelineState: 'idle',
      capsuleExpanded: false,
      pipelineError: null,
      contextMenuOpen: false,
      contextMenuReady: false,
      translationTargetMenuOpen: false,
      config: { ...state.config, capsule_auto_hide: true, capsule_glass_enabled: false },
    }))

    tauriMocks.setCapsuleGlass.mockReset().mockResolvedValue('liquid_glass')
    windowApiMocks.getCurrentWindow.mockReset().mockReturnValue({
      setFocusable: windowApiMocks.setFocusable,
      setSize: windowApiMocks.setSize,
      setPosition: windowApiMocks.setPosition,
      outerPosition: windowApiMocks.outerPosition,
      outerSize: windowApiMocks.outerSize,
      scaleFactor: windowApiMocks.scaleFactor,
      show: windowApiMocks.show,
      hide: windowApiMocks.hide,
    })
    windowApiMocks.availableMonitors.mockReset().mockResolvedValue([monitor])
    windowApiMocks.currentMonitor.mockReset().mockResolvedValue(monitor)
    windowApiMocks.primaryMonitor.mockReset().mockResolvedValue(monitor)
    windowApiMocks.setFocusable.mockReset().mockResolvedValue(undefined)
    windowApiMocks.setSize.mockReset().mockResolvedValue(undefined)
    windowApiMocks.setPosition.mockReset().mockResolvedValue(undefined)
    windowApiMocks.outerPosition.mockReset().mockResolvedValue({ x: 0, y: 0 })
    windowApiMocks.outerSize.mockReset().mockResolvedValue({ width: 60, height: 60 })
    windowApiMocks.scaleFactor.mockReset().mockResolvedValue(1)
    windowApiMocks.show.mockReset().mockResolvedValue(undefined)
    windowApiMocks.hide.mockReset().mockResolvedValue(undefined)
  })

  afterEach(() => {
    cleanup()
  })

  it('prevents a superseded async resize from overwriting the latest window state', async () => {
    render(createElement(HookHarness))

    await waitFor(() => {
      expect(windowApiMocks.hide).toHaveBeenCalledTimes(1)
    })

    windowApiMocks.currentMonitor.mockReset()
    windowApiMocks.setSize.mockClear()
    windowApiMocks.setPosition.mockClear()
    windowApiMocks.show.mockClear()
    windowApiMocks.hide.mockClear()

    let resolveFirstMonitor: (value: typeof monitor) => void = () => {}
    const delayedFirstMonitor = new Promise<typeof monitor>((resolve) => {
      resolveFirstMonitor = resolve
    })
    windowApiMocks.currentMonitor
      .mockReset()
      .mockImplementationOnce(() => delayedFirstMonitor)
      .mockResolvedValue(monitor)

    act(() => {
      useAppStore.setState({ contextMenuOpen: true })
    })

    await waitFor(() => {
      expect(windowApiMocks.currentMonitor).toHaveBeenCalledTimes(1)
    })

    act(() => {
      useAppStore.setState({ contextMenuOpen: false })
    })

    await waitFor(() => {
      expect(windowApiMocks.setPosition).toHaveBeenCalledTimes(1)
      expect(windowApiMocks.hide).toHaveBeenCalledTimes(1)
    })

    await act(async () => {
      resolveFirstMonitor(monitor)
      await delayedFirstMonitor
    })

    expect(windowApiMocks.setSize.mock.calls.map(([size]) => [size.width, size.height])).toEqual([
      [60, 60],
    ])
    expect(windowApiMocks.setPosition).toHaveBeenCalledTimes(1)
    expect(windowApiMocks.show).not.toHaveBeenCalled()
    expect(windowApiMocks.hide).toHaveBeenCalledTimes(1)
    expect(useAppStore.getState().contextMenuReady).toBe(false)

    windowApiMocks.outerPosition.mockResolvedValue({ x: 100, y: 100 })
    windowApiMocks.outerSize.mockResolvedValue(null)
    windowApiMocks.setPosition.mockClear()

    act(() => {
      useAppStore.setState({ pipelineState: 'recording' })
    })

    await waitFor(() => {
      expect(windowApiMocks.setPosition).toHaveBeenCalledWith(expect.objectContaining({ y: 100 }))
    })
    expect(tauriMocks.setCapsuleGlass).not.toHaveBeenCalled()
  })

  it('applies glass only while pill-sized and clears it before the window grows', async () => {
    useAppStore.setState((state) => ({
      config: { ...state.config, capsule_glass_enabled: true },
    }))
    render(createElement(HookHarness))

    await waitFor(() => {
      expect(windowApiMocks.hide).toHaveBeenCalledTimes(1)
    })
    // First mount: the window is exactly the idle pill and the glass is on.
    expect(windowApiMocks.setSize.mock.calls.map(([size]) => [size.width, size.height])).toEqual([
      [36, 36],
    ])
    expect(tauriMocks.setCapsuleGlass).toHaveBeenCalledWith(true, CAPSULE_GLASS_RADIUS)

    tauriMocks.setCapsuleGlass.mockClear()
    windowApiMocks.setSize.mockClear()
    windowApiMocks.setPosition.mockClear()
    windowApiMocks.outerPosition.mockResolvedValue({ x: 100, y: 100 })
    windowApiMocks.outerSize.mockResolvedValue({ width: 36, height: 36 })

    act(() => {
      useAppStore.setState({ contextMenuOpen: true })
    })

    await waitFor(() => {
      expect(windowApiMocks.setPosition).toHaveBeenCalledTimes(1)
    })
    // Glass is cleared before the window grows for the menu, and the window
    // shifts left by half the padding so the pill's left edge does not move.
    expect(tauriMocks.setCapsuleGlass).toHaveBeenCalledWith(false, CAPSULE_GLASS_RADIUS)
    expect(tauriMocks.setCapsuleGlass.mock.invocationCallOrder[0]).toBeLessThan(
      windowApiMocks.setSize.mock.invocationCallOrder[0],
    )
    expect(windowApiMocks.setSize.mock.calls[0][0]).toMatchObject({ width: 244, height: 244 })
    expect(windowApiMocks.setPosition).toHaveBeenCalledWith(expect.objectContaining({ x: 88 }))

    tauriMocks.setCapsuleGlass.mockClear()
    windowApiMocks.setSize.mockClear()
    windowApiMocks.setPosition.mockClear()
    windowApiMocks.outerPosition.mockResolvedValue({ x: 88, y: 100 })
    windowApiMocks.outerSize.mockResolvedValue({ width: 244, height: 244 })

    act(() => {
      useAppStore.setState({ contextMenuOpen: false, contextMenuReady: false })
    })

    await waitFor(() => {
      expect(tauriMocks.setCapsuleGlass).toHaveBeenCalledWith(true, CAPSULE_GLASS_RADIUS)
    })
    // Back to the pill: resize first, then re-apply the glass.
    expect(windowApiMocks.setSize.mock.invocationCallOrder[0]).toBeLessThan(
      tauriMocks.setCapsuleGlass.mock.invocationCallOrder[0],
    )
    expect(windowApiMocks.setSize.mock.calls[0][0]).toMatchObject({ width: 36, height: 36 })
    expect(windowApiMocks.setPosition).toHaveBeenCalledWith(expect.objectContaining({ x: 100 }))
  })
})
