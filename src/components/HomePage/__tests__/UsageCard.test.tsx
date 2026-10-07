import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { UsageCard } from '../UsageCard'
import { useAppStore } from '../../../stores/appStore'
import { getUsageSummary } from '../../../lib/tauri'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}))

vi.mock('../../../lib/tauri', () => ({
  getUsageSummary: vi.fn(),
}))

const mockedGetUsageSummary = vi.mocked(getUsageSummary)

beforeEach(() => {
  mockedGetUsageSummary.mockReset()
  useAppStore.setState((state) => ({
    config: { ...state.config, usage_pricing: [] },
  }))
})

afterEach(() => {
  cleanup()
})

describe('UsageCard', () => {
  it('renders zeros before the summary arrives and asks for this month', async () => {
    mockedGetUsageSummary.mockImplementation(() => new Promise(() => {}))
    render(<UsageCard />)
    expect(screen.getByTestId('usage-card')).toBeTruthy()
    expect(screen.getByText('$0.00')).toBeTruthy()
    await waitFor(() => expect(mockedGetUsageSummary).toHaveBeenCalledTimes(1))
    expect(mockedGetUsageSummary.mock.calls[0][0]).toMatch(/^\d{4}-\d{2}-01$/)
  })

  it('shows minutes, tokens and the estimated cost from the summary', async () => {
    mockedGetUsageSummary.mockResolvedValue({
      since: '2026-10-01',
      totals: { runs: 4, audioSeconds: 300, promptTokens: 1_000_000, completionTokens: 250_000 },
      byModel: [
        {
          kind: 'stt',
          provider: 'openai-whisper',
          model: 'whisper-1',
          runs: 4,
          audioSeconds: 300,
          promptTokens: 0,
          completionTokens: 0,
        },
        {
          kind: 'llm',
          provider: 'openai',
          model: 'gpt-4.1-mini',
          runs: 4,
          audioSeconds: 0,
          promptTokens: 1_000_000,
          completionTokens: 250_000,
        },
      ],
      byDay: [],
    })
    render(<UsageCard />)
    await waitFor(() => expect(screen.getByText('5.0 min')).toBeTruthy())
    expect(screen.getByText('1.3M')).toBeTruthy()
    // 5 min × $0.006 + 1M × $0.40 + 0.25M × $1.60 = 0.03 + 0.4 + 0.4
    expect(screen.getByText('$0.830')).toBeTruthy()
    expect(screen.getByText('4')).toBeTruthy()
  })

  it('uses the user price overrides for the estimate', async () => {
    useAppStore.setState((state) => ({
      config: {
        ...state.config,
        usage_pricing: [
          {
            model: 'whisper-1',
            usd_per_minute: 0.1,
            usd_per_mtok_in: null,
            usd_per_mtok_out: null,
          },
        ],
      },
    }))
    mockedGetUsageSummary.mockResolvedValue({
      since: '2026-10-01',
      totals: { runs: 1, audioSeconds: 60, promptTokens: 0, completionTokens: 0 },
      byModel: [
        {
          kind: 'stt',
          provider: 'openai-whisper',
          model: 'whisper-1',
          runs: 1,
          audioSeconds: 60,
          promptTokens: 0,
          completionTokens: 0,
        },
      ],
      byDay: [],
    })
    render(<UsageCard />)
    await waitFor(() => expect(screen.getByText('$0.100')).toBeTruthy())
  })
})
