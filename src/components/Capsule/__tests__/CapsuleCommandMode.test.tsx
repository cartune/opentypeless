import React from 'react'
import { render, screen, cleanup } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { CapsuleAskRecording } from '../CapsuleAskRecording'
import { CapsuleAskThinking } from '../CapsuleAskThinking'
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
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}))

vi.mock('../../../lib/tauri', () => ({
  abortAskDictation: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('../DurationTimer', () => ({
  DurationTimer: () => null,
}))

beforeEach(() => {
  useAppStore.getState().setAskSelection(null)
})

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})

describe('Capsule Ask command-mode badge', () => {
  it('shows the plain Ask label when no selection was captured', () => {
    render(<CapsuleAskRecording />)
    expect(screen.getByText('ask.title')).toBeTruthy()
    expect(screen.queryByText('capsule.commandMode')).toBeNull()
  })

  it('shows the command badge while recording when selection was captured in command mode', () => {
    useAppStore.getState().setAskSelection({ chars: 42, commandMode: true })
    render(<CapsuleAskRecording />)
    expect(screen.getByText('capsule.commandMode')).toBeTruthy()
    expect(screen.queryByText('ask.title')).toBeNull()
  })

  it('keeps the Ask label when selection was captured but command mode is off', () => {
    useAppStore.getState().setAskSelection({ chars: 42, commandMode: false })
    render(<CapsuleAskRecording />)
    expect(screen.getByText('ask.title')).toBeTruthy()
  })

  it('shows the command working text while thinking', () => {
    useAppStore.getState().setAskSelection({ chars: 12, commandMode: true })
    render(<CapsuleAskThinking />)
    expect(screen.getByText('capsule.commandThinking')).toBeTruthy()
    expect(screen.queryByText('ask.thinking')).toBeNull()
  })
})
