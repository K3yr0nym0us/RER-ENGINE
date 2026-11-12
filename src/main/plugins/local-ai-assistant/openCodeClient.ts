import type {
  AssistantModelOption,
} from '../../../shared-types/plugins'
import type { OpenCodeEndpoint } from './openCodeRuntime'
import { basicAuthHeader } from './openCodeRuntime'

interface OpenCodeErrorBody {
  error?: { message?: string } | string
}

async function request<T>(
  endpoint: OpenCodeEndpoint,
  pathname: string,
  init: { method?: string; body?: unknown } = {},
): Promise<{ ok: boolean; status: number; data: T | null; error?: string }> {
  let response: Response
  try {
    response = await fetch(endpoint.url + pathname, {
      method: init.method ?? 'GET',
      headers: {
        Authorization: basicAuthHeader(endpoint.password),
        'Content-Type': 'application/json',
      },
      body: init.body != null ? JSON.stringify(init.body) : undefined,
      signal: AbortSignal.timeout(30_000),
    })
  } catch (err) {
    return { ok: false, status: 0, data: null, error: err instanceof Error ? err.message : String(err) }
  }

  const text = await response.text()
  if (!response.ok) {
    let message = `HTTP ${response.status}`
    try {
      const parsed = JSON.parse(text) as OpenCodeErrorBody
      const inner = parsed.error
      if (typeof inner === 'string') message = inner
      else if (inner?.message) message = inner.message
    } catch {
      // keep default message
    }
    return { ok: false, status: response.status, data: null, error: message }
  }

  try {
    return { ok: true, status: response.status, data: JSON.parse(text) as T }
  } catch {
    return { ok: false, status: response.status, data: null, error: 'Invalid JSON response' }
  }
}

interface ApiModelInfo {
  id: string
  providerID: string
  name?: string
  type?: string
  enabled?: boolean
  capabilities?: { tools?: boolean }
  cost?: Array<{ input?: number; output?: number }>
  limit?: { context?: number }
}

interface ApiSessionInfo {
  id: string
  agent?: string
  model?: { providerID?: string; id?: string }
}

interface ApiMessagePart {
  type: string
  text?: string
  name?: string
  state?: { status?: string; input?: unknown; error?: unknown }
}

export interface ApiSessionMessage {
  id: string
  type: string
  finish?: string
  content?: ApiMessagePart[]
}

export async function listModels(
  endpoint: OpenCodeEndpoint,
): Promise<{ ok: boolean; models?: AssistantModelOption[]; error?: string }> {
  const result = await request<{ data?: ApiModelInfo[] }>(endpoint, '/api/model')
  if (!result.ok || !result.data?.data) {
    return { ok: false, error: result.error ?? 'Model catalog unavailable' }
  }

  const options: AssistantModelOption[] = []
  for (const model of result.data.data) {
    if (model.enabled === false) continue
    if (model.type !== 'chat') continue
    if (!model.capabilities?.tools) continue
    const isOpencodeFamily = model.providerID === 'opencode' || model.providerID === 'opencode-go'
    if (!isOpencodeFamily) continue
    const inputCost = model.cost?.[0]?.input ?? Number.POSITIVE_INFINITY
    options.push({
      ref: `${model.providerID}/${model.id}`,
      providerID: model.providerID,
      modelID: model.id,
      name: model.name ?? model.id,
      contextTokens: model.limit?.context ?? 0,
      free: inputCost === 0,
    })
  }

  const preferredFree = [
    'deepseek-v4.1-flash-free',
    'glm-5.3-flash-free',
    'mimo-v2.6-flash-free',
    'big-pickle',
    'beam-free',
    'flint-40b-free',
    'longcat-2.5-free',
  ]
  options.sort((a, b) => {
    if (a.free !== b.free) return a.free ? -1 : 1
    const ai = a.free ? preferredFree.indexOf(a.modelID) : -1
    const bi = b.free ? preferredFree.indexOf(b.modelID) : -1
    if (ai !== bi) {
      if (ai === -1) return 1
      if (bi === -1) return -1
      return ai - bi
    }
    return a.name.localeCompare(b.name)
  })

  return { ok: true, models: options }
}

export async function isModelAvailable(
  endpoint: OpenCodeEndpoint,
  ref: string,
): Promise<boolean> {
  const result = await listModels(endpoint)
  return result.ok && (result.models ?? []).some((model) => model.ref === ref)
}

export async function createSession(
  endpoint: OpenCodeEndpoint,
  input: {
    directory: string
    agent: string
    model?: { providerID: string; id: string }
  },
): Promise<{ ok: boolean; session?: ApiSessionInfo; error?: string }> {
  const body: Record<string, unknown> = {
    location: { directory: input.directory },
    agent: input.agent,
  }
  if (input.model) body.model = { providerID: input.model.providerID, id: input.model.id }
  const result = await request<{ data?: ApiSessionInfo }>(endpoint, '/api/session', {
    method: 'POST',
    body,
  })
  if (!result.ok || !result.data?.data) {
    return { ok: false, error: result.error ?? 'Failed to create assistant session' }
  }
  return { ok: true, session: result.data.data }
}

export async function promptSession(
  endpoint: OpenCodeEndpoint,
  sessionID: string,
  text: string,
): Promise<{ ok: boolean; error?: string }> {
  const result = await request(endpoint, `/api/session/${sessionID}/prompt`, {
    method: 'POST',
    body: { text },
  })
  if (!result.ok) return { ok: false, error: result.error ?? 'Failed to send prompt' }
  return { ok: true }
}

export async function listSessionMessages(
  endpoint: OpenCodeEndpoint,
  sessionID: string,
): Promise<{ ok: boolean; messages?: ApiSessionMessage[]; error?: string }> {
  const result = await request<{ data?: ApiSessionMessage[] }>(
    endpoint,
    `/api/session/${sessionID}/message`,
  )
  if (!result.ok || !result.data?.data) {
    return { ok: false, error: result.error ?? 'Failed to read session messages' }
  }
  return { ok: true, messages: result.data.data }
}

export async function interruptSession(
  endpoint: OpenCodeEndpoint,
  sessionID: string,
): Promise<void> {
  await request(endpoint, `/api/session/${sessionID}/interrupt`, { method: 'POST', body: {} })
}

interface ApiCredential {
  id: string
  integrationID: string
}

export async function isOpenCodeGoConnected(
  endpoint: OpenCodeEndpoint,
): Promise<boolean> {
  const result = await request<{ data?: ApiCredential[] }>(endpoint, '/api/credential')
  if (!result.ok || !result.data?.data) return false
  return result.data.data.some((credential) => credential.integrationID === 'opencode-go')
}

export async function connectOpenCodeGo(
  endpoint: OpenCodeEndpoint,
  apiKey: string,
): Promise<{ ok: boolean; error?: string }> {
  const connect = await request(endpoint, '/api/integration/opencode-go/connect/key', {
    method: 'POST',
    body: { key: apiKey },
  })
  if (!connect.ok) return { ok: false, error: connect.error ?? 'Failed to connect OpenCode Go' }

  // Confirma que el provider de Go ya expone modelos.
  for (let attempt = 0; attempt < 5; attempt += 1) {
    const models = await listModels(endpoint)
    if (models.ok && (models.models ?? []).some((model) => model.providerID === 'opencode-go')) {
      return { ok: true }
    }
    await new Promise((resolve) => setTimeout(resolve, 1_000))
  }
  return {
    ok: false,
    error: 'Token accepted but no OpenCode Go models appeared. Verify your subscription.',
  }
}

export async function disconnectOpenCodeGo(
  endpoint: OpenCodeEndpoint,
): Promise<{ ok: boolean; error?: string }> {
  const credentials = await request<{ data?: ApiCredential[] }>(endpoint, '/api/credential')
  if (!credentials.ok || !credentials.data?.data) {
    return { ok: false, error: credentials.error ?? 'Failed to read credentials' }
  }
  const targets = credentials.data.data.filter((c) => c.integrationID === 'opencode-go')
  for (const credential of targets) {
    const removed = await request(endpoint, `/api/credential/${credential.id}`, { method: 'DELETE' })
    if (!removed.ok) return { ok: false, error: removed.error ?? 'Failed to remove token' }
  }
  return { ok: true }
}

export async function switchSessionModel(
  endpoint: OpenCodeEndpoint,
  sessionID: string,
  model: { providerID: string; id: string },
): Promise<{ ok: boolean; error?: string }> {
  const result = await request(endpoint, `/api/session/${sessionID}/model`, {
    method: 'POST',
    body: { model: { providerID: model.providerID, id: model.id } },
  })
  if (!result.ok) return { ok: false, error: result.error ?? 'Failed to switch model' }
  return { ok: true }
}

interface ApiIntegration {
  id: string
  connections?: unknown[]
}

interface ApiOauthAttempt {
  attemptID?: string
  url?: string
}

/** Inicia el device OAuth de la cuenta OpenCode (fallback cuando no hay modelos free). */
export async function startOpenCodeAccountLogin(
  endpoint: OpenCodeEndpoint,
): Promise<{ ok: boolean; url?: string; error?: string }> {
  const start = await request<{ data?: ApiOauthAttempt }>(
    endpoint,
    '/api/integration/opencode/connect/oauth',
    { method: 'POST', body: { methodID: 'device' } },
  )
  if (!start.ok || !start.data?.data?.url) {
    return { ok: false, error: start.error ?? 'Failed to start OpenCode login' }
  }
  return { ok: true, url: start.data.data.url }
}

/** True cuando la cuenta OpenCode ya tiene una conexión activa. */
export async function isOpenCodeAccountConnected(
  endpoint: OpenCodeEndpoint,
): Promise<boolean> {
  const result = await request<{ data?: ApiIntegration[] }>(endpoint, '/api/integration')
  if (!result.ok || !result.data?.data) return false
  const opencode = result.data.data.find((integration) => integration.id === 'opencode')
  return Boolean(opencode && Array.isArray(opencode.connections) && opencode.connections.length > 0)
}
