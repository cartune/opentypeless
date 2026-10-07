import React from 'react'
import { act, cleanup, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { CapsuleRecording } from '../CapsuleRecording'
import { useAppStore } from '../../../stores/appStore'

vi.mock('framer-motion', () => ({
  motion: new Proxy(
    {},
    {
      get:
        (_target, tag: string) =>
        ({ children, ...props }: React.HTMLAttributes<HTMLElement>) =>
          React.createElement(tag, props, children),
    },
  ),
  useReducedMotion: () => true,
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

vi.mock('../../../lib/tauri', () => ({
  abortRecording: vi.fn().mockResolvedValue(undefined),
  switchTranslationTarget: vi.fn().mockResolvedValue(undefined),
}))

beforeEach(() => {
  vi.useFakeTimers()
  useAppStore.getState().setAudioVolume(0)
})

afterEach(() => {
  cleanup()
  vi.useRealTimers()
})

describe('CapsuleRecording no-audio hint', () => {
  it('shows the hint after three silent seconds and hides it once audio arrives', () => {
    render(<CapsuleRecording />)
    expect(screen.queryByTestId('no-audio-hint')).toBeNull()

    act(() => {
      vi.advanceTimersByTime(3300)
    })
    expect(screen.getByTestId('no-audio-hint')).toHaveTextContent('capsule.noAudioHint')

    act(() => {
      useAppStore.getState().setAudioVolume(0.6)
    })
    act(() => {
      vi.advanceTimersByTime(300)
    })
    expect(screen.queryByTestId('no-audio-hint')).toBeNull()
  })

  it('keeps the waveform while audio keeps arriving', () => {
    render(<CapsuleRecording />)
    for (let i = 0; i < 5; i += 1) {
      act(() => {
        useAppStore.getState().setAudioVolume(0.5 + (i % 2) * 0.1)
        vi.advanceTimersByTime(1000)
      })
    }
    expect(screen.queryByTestId('no-audio-hint')).toBeNull()
    expect(screen.getByTestId('waveform')).toBeInTheDocument()
  })
})
