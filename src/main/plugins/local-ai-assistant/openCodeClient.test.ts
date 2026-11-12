import { afterEach, describe, expect, it, vi } from 'vitest'

import { listModels } from './openCodeClient'

const endpoint = { url: 'http://127.0.0.1:47611', password: 'secret' }

interface FakeModel {
  id: string
  providerID: string
  name?: string
  type?: string
  enabled?: boolean
  capabilities?: { tools?: boolean }
  cost?: Array<{ input?: number; output?: number }>
  limit?: { context?: number }
}

function fakePayload(models: FakeModel[]): Response {
  return new Response(JSON.stringify({ data: models }), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  })
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('listModels', () => {
  it('filtra a chat+tools de opencode/opencode-go y ordena free primero', async () => {
    const fetchMock = vi.fn(async (input: unknown) => {
      const url = String(input)
      if (!url.endsWith('/api/model')) {
        return new Response('{}', { status: 404 })
      }
      return fakePayload([
        {
          id: 'claude-opus-5',
          providerID: 'anthropic',
          type: 'chat',
          enabled: true,
          capabilities: { tools: true },
          cost: [{ input: 5, output: 20 }],
          limit: { context: 200000 },
        },
        {
          id: 'big-pickle',
          providerID: 'opencode',
          type: 'chat',
          enabled: true,
          capabilities: { tools: true },
          cost: [{ input: 0, output: 0 }],
          limit: { context: 200000 },
        },
        {
          id: 'gpt-6-sol',
          providerID: 'opencode-go',
          type: 'chat',
          enabled: true,
          capabilities: { tools: true },
          cost: [{ input: 2, output: 10 }],
          limit: { context: 272000 },
        },
        {
          id: 'deepseek-v4.1-flash-free',
          providerID: 'opencode',
          type: 'chat',
          enabled: true,
          capabilities: { tools: true },
          cost: [{ input: 0, output: 0 }],
          limit: { context: 200000 },
        },
        {
          id: 'toolless',
          providerID: 'opencode',
          type: 'chat',
          enabled: true,
          capabilities: { tools: false },
          cost: [{ input: 0, output: 0 }],
        },
        {
          id: 'disabled-model',
          providerID: 'opencode',
          type: 'chat',
          enabled: false,
          capabilities: { tools: true },
          cost: [{ input: 0, output: 0 }],
        },
      ])
    })
    vi.stubGlobal('fetch', fetchMock)

    const result = await listModels(endpoint)
    expect(result.ok).toBe(true)
    const refs = (result.models ?? []).map((model) => model.ref)

    expect(refs).toEqual([
      'opencode/deepseek-v4.1-flash-free',
      'opencode/big-pickle',
      'opencode-go/gpt-6-sol',
    ])
    expect((result.models ?? [])[0].free).toBe(true)
    expect((result.models ?? [])[2].free).toBe(false)
  })

  it('devuelve error cuando la API falla', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('{"error":{"message":"boom"}}', { status: 500 })),
    )
    const result = await listModels(endpoint)
    expect(result.ok).toBe(false)
    expect(result.error).toBe('boom')
  })
})
