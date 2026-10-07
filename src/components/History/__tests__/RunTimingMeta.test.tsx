import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { RunTimingMeta } from '../RunTimingMeta'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: Record<string, string>) =>
      ({
        'history.timingStt': `STT ${values?.value ?? ''}`,
        'history.timingLlm': `AI ${values?.value ?? ''}`,
      })[key] ?? key,
  }),
}))

afterEach(() => {
  cleanup()
})

describe('RunTimingMeta', () => {
  it('renders nothing for rows without recorded latency', () => {
    const { container } = render(
      <RunTimingMeta stt_ms={null} llm_ms={null} stt_model={null} llm_model={null} />,
    )
    expect(container).toBeEmptyDOMElement()
  })

  it('shows STT and AI latency with model labels', () => {
    render(
      <RunTimingMeta stt_ms={812} llm_ms={1104} stt_model="whisper-1" llm_model="gpt-4.1-mini" />,
    )
    expect(screen.getByText('STT 0.8s')).toBeInTheDocument()
    expect(screen.getByText('AI 1.1s')).toBeInTheDocument()
    expect(screen.getByText('whisper-1 / gpt-4.1-mini')).toBeInTheDocument()
  })

  it('omits the AI part when polish did not run', () => {
    render(<RunTimingMeta stt_ms={600} llm_ms={null} stt_model="whisper-1" llm_model={null} />)
    expect(screen.getByText('STT 0.6s')).toBeInTheDocument()
    expect(screen.queryByText(/^AI /)).not.toBeInTheDocument()
    expect(screen.getByText('whisper-1')).toBeInTheDocument()
  })
})
