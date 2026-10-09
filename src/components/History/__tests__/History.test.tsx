import React from 'react'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useAppStore, type HistoryEntry } from '../../../stores/appStore'
import { addCorrectionRule, clearHistory, getCorrectionRules } from '../../../lib/tauri'
import { copyTextToClipboard } from '../../../lib/clipboard'
import { History } from '../index'

vi.mock('framer-motion', () => ({
  AnimatePresence: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  motion: new Proxy(
    {},
    {
      get:
        (_target, tag: string) =>
        ({ children, ...props }: React.HTMLAttributes<HTMLElement>) =>
          React.createElement(tag, props, children),
    },
  ),
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))

vi.mock('../../../lib/clipboard', () => ({
  copyTextToClipboard: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('../../../lib/tauri', () => ({
  addCorrectionRule: vi.fn().mockResolvedValue(undefined),
  clearHistory: vi.fn().mockResolvedValue(undefined),
  getCorrectionRules: vi.fn().mockResolvedValue([]),
}))

const entry: HistoryEntry = {
  id: 1,
  created_at: new Date().toISOString(),
  context_profile_id: 'chat.slack',
  context_label: 'Slack',
  context_icon_key: 'slack',
  context_family: 'work_chat',
  browser_access_status: 'not_applicable',
  provider_kind: 'local',
  raw_text: 'open type less',
  polished_text: 'OpenTypeless',
  language: 'en',
  duration_ms: 1000,
  active_scene_id: null,
  active_scene_source: null,
  active_scene_name: null,
  active_scene_prompt_chars: null,
  active_scene_prompt_truncated: false,
  output_status: null,
  output_error: null,
}

describe('History correction creation', () => {
  beforeEach(() => {
    useAppStore.setState({ history: [entry], correctionRules: [] })
    vi.clearAllMocks()
    vi.mocked(addCorrectionRule).mockResolvedValue(undefined)
    vi.mocked(getCorrectionRules).mockResolvedValue([
      {
        id: 2,
        pattern: 'open type less',
        replacement: 'OpenTypeless',
        enabled: true,
        source: 'manual',
      },
    ])
  })

  afterEach(() => {
    cleanup()
    useAppStore.setState(useAppStore.getInitialState())
  })

  it('does not infer a correction until the user explicitly opens and saves it', async () => {
    render(<History />)
    expect(addCorrectionRule).not.toHaveBeenCalled()

    const menuButton = screen.getByRole('button', { name: 'history.moreActions' })
    fireEvent.click(menuButton)
    fireEvent.click(screen.getByRole('menuitem', { name: 'history.createCorrection' }))

    const dialog = screen.getByRole('dialog', { name: 'history.createCorrection' })
    expect(dialog).toBeInTheDocument()
    expect(screen.getByLabelText('dictionary.wrongPhrase')).toHaveValue('open type less')
    expect(screen.getByLabelText('dictionary.correctPhrase')).toHaveValue('OpenTypeless')
    expect(addCorrectionRule).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: 'history.saveCorrection' }))
    await waitFor(() => {
      expect(addCorrectionRule).toHaveBeenCalledWith('open type less', 'OpenTypeless')
      expect(getCorrectionRules).toHaveBeenCalled()
      expect(useAppStore.getState().correctionRules).toHaveLength(1)
    })
  })

  it('leaves fields empty instead of truncating an overlong history source', () => {
    useAppStore.setState({
      history: [{ ...entry, raw_text: 'x'.repeat(121), polished_text: 'y'.repeat(121) }],
    })
    render(<History />)

    fireEvent.click(screen.getByRole('button', { name: 'history.moreActions' }))
    fireEvent.click(screen.getByRole('menuitem', { name: 'history.createCorrection' }))

    expect(screen.getByLabelText('dictionary.wrongPhrase')).toHaveValue('')
    expect(screen.getByLabelText('dictionary.correctPhrase')).toHaveValue('')
    expect(screen.getByLabelText('dictionary.wrongPhrase')).toHaveFocus()
  })

  it('closes the row menu on Escape and restores its trigger focus', async () => {
    render(<History />)
    const menuButton = screen.getByRole('button', { name: 'history.moreActions' })

    fireEvent.click(menuButton)
    expect(screen.getByRole('menu')).toBeInTheDocument()
    fireEvent.keyDown(window, { key: 'Escape' })

    expect(screen.queryByRole('menu')).toBeNull()
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'history.moreActions' })).toHaveFocus(),
    )
  })

  it('clears history through an in-app confirmation instead of window.confirm', async () => {
    const confirmSpy = vi.spyOn(window, 'confirm')
    render(<History />)

    fireEvent.click(screen.getByRole('button', { name: 'history.clearAll' }))
    expect(confirmSpy).not.toHaveBeenCalled()
    expect(screen.getByText('history.clearConfirm')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'history.confirmClear' }))

    await waitFor(() => {
      expect(clearHistory).toHaveBeenCalled()
      expect(useAppStore.getState().history).toEqual([])
    })
    confirmSpy.mockRestore()
  })
})

describe('History ask entries', () => {
  afterEach(cleanup)

  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(copyTextToClipboard).mockResolvedValue(undefined)
  })

  it('shows selection, command and output as separate blocks for a command run', () => {
    useAppStore.setState({
      history: [
        {
          ...entry,
          id: 7,
          intent_kind: 'draft_insert',
          selected_text: 'Saw this post from a user on Reddit',
          raw_text: '跟他說這個方法還蠻讚的，翻譯成英文',
          polished_text: 'This method is great.',
        },
      ],
      correctionRules: [],
    })

    render(<History />)

    const body = document.querySelector('[data-history-entry-kind="command"]')
    expect(body).not.toBeNull()
    expect(screen.getByText('history.modeCommand')).toBeInTheDocument()
    expect(screen.getByText('Saw this post from a user on Reddit')).toBeInTheDocument()
    expect(screen.getByText('跟他說這個方法還蠻讚的，翻譯成英文')).toBeInTheDocument()
    expect(screen.getByText('This method is great.')).toBeInTheDocument()
  })

  it('treats legacy popup rows as ask entries and copies the output natively', async () => {
    useAppStore.setState({
      history: [
        {
          ...entry,
          id: 8,
          output_status: 'popup',
          raw_text: '台北今天幾度',
          polished_text: '大約 28 度。',
        },
      ],
      correctionRules: [],
    })

    render(<History />)

    expect(document.querySelector('[data-history-entry-kind="ask"]')).not.toBeNull()
    expect(screen.getByText('history.modeAsk')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: /Copy text/ }))
    expect(copyTextToClipboard).toHaveBeenCalledWith('大約 28 度。')
    await waitFor(() => expect(screen.getByText('history.copied')).toBeInTheDocument())
  })
})
