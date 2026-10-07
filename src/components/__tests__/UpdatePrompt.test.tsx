import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { UpdatePrompt } from '../UpdatePrompt'
import { shouldCheckForUpdates } from '../../lib/update-check'

const { mockCheck, mockRelaunch } = vi.hoisted(() => ({
  mockCheck: vi.fn(),
  mockRelaunch: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-updater', () => ({
  check: mockCheck,
}))

vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: mockRelaunch,
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (
      key: string,
      defaultTextOrValues?: string | Record<string, string>,
      valuesArg?: Record<string, string>,
    ) => {
      const values = typeof defaultTextOrValues === 'object' ? defaultTextOrValues : valuesArg
      return (
        (
          {
            'updates.availableTitle': 'Update available',
            'updates.availableBody': `Version ${values?.version ?? ''} is ready to install.`,
            'updates.install': 'Install update',
            'updates.installing': 'Installing...',
            'updates.dismiss': 'Dismiss',
            'updates.error': 'Update failed. Please download the latest version from the website.',
          } as Record<string, string>
        )[key] ?? key
      )
    },
  }),
}))

beforeEach(() => {
  // vitest runs with DEV=true; the prompt skips update checks in dev mode.
  vi.stubEnv('DEV', false)
})

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
  vi.unstubAllEnvs()
})

describe('shouldCheckForUpdates', () => {
  it('skips in dev mode', () => {
    expect(shouldCheckForUpdates({ DEV: true })).toBe(false)
  })

  it('skips when VITE_DISABLE_UPDATE_CHECK=1', () => {
    expect(shouldCheckForUpdates({ DEV: false, VITE_DISABLE_UPDATE_CHECK: '1' })).toBe(false)
  })

  it('checks in production builds by default', () => {
    expect(shouldCheckForUpdates({ DEV: false })).toBe(true)
  })
})

describe('UpdatePrompt', () => {
  it('stays hidden when no update is available', async () => {
    mockCheck.mockResolvedValueOnce(null)

    render(<UpdatePrompt />)

    await waitFor(() => expect(mockCheck).toHaveBeenCalled())
    expect(screen.queryByText('Update available')).not.toBeInTheDocument()
  })

  it('shows an available update and relaunches after installing it', async () => {
    const downloadAndInstall = vi.fn().mockResolvedValue(undefined)
    mockCheck.mockResolvedValueOnce({
      version: '0.1.42',
      downloadAndInstall,
    })

    render(<UpdatePrompt />)

    expect(await screen.findByText('Update available')).toBeInTheDocument()
    expect(screen.getByText('Version 0.1.42 is ready to install.')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Install update' }))

    await waitFor(() => {
      expect(downloadAndInstall).toHaveBeenCalled()
      expect(mockRelaunch).toHaveBeenCalled()
    })
  })

  it('shows a download fallback when install fails', async () => {
    mockCheck.mockResolvedValueOnce({
      version: '0.1.42',
      downloadAndInstall: vi.fn().mockRejectedValue(new Error('network')),
    })

    render(<UpdatePrompt />)

    fireEvent.click(await screen.findByRole('button', { name: 'Install update' }))

    expect(
      await screen.findByText(
        'Update failed. Please download the latest version from the website.',
      ),
    ).toBeInTheDocument()
  })
})
