import { spawn, type ChildProcess } from 'child_process'
import fs from 'fs'
import http from 'http'
import net from 'net'
import path from 'path'

import type { PluginLlmStatus } from '../../../shared-types/plugins'
import {
  getOpenCodeBinDir,
  getOpenCodeExecutablePath,
  getOpenCodeServeLogPath,
  getServiceJsonPath,
  getWorkspaceDir,
  getXdgDir,
} from './openCodePaths'

export interface OpenCodeEndpoint {
  url: string
  password: string
}

const STARTUP_TIMEOUT_MS = 90_000

let serverProcess: ChildProcess | null = null
let currentStatus: PluginLlmStatus = 'idle'
let currentError: string | null = null
let endpoint: OpenCodeEndpoint | null = null

function appendServeLog(line: string): void {
  try {
    const file = getOpenCodeServeLogPath()
    fs.mkdirSync(path.dirname(file), { recursive: true })
    fs.appendFileSync(file, line, 'utf8')
  } catch {
    // best-effort
  }
}

export function basicAuthHeader(password: string): string {
  return 'Basic ' + Buffer.from(`opencode:${password}`).toString('base64')
}

function probeInfo(endpointValue: OpenCodeEndpoint, timeoutMs = 3_000): Promise<boolean> {
  return new Promise((resolve) => {
    try {
      const url = new URL('/api/info', endpointValue.url)
      const req = http.get(
        {
          hostname: url.hostname,
          port: url.port,
          path: url.pathname,
          headers: { Authorization: basicAuthHeader(endpointValue.password) },
          timeout: timeoutMs,
        },
        (res) => {
          res.resume()
          resolve(res.statusCode != null && res.statusCode >= 200 && res.statusCode < 500)
        },
      )
      req.on('timeout', () => {
        req.destroy()
        resolve(false)
      })
      req.on('error', () => resolve(false))
    } catch {
      resolve(false)
    }
  })
}

function getFreePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = net.createServer()
    server.listen(0, '127.0.0.1', () => {
      const address = server.address()
      if (address && typeof address === 'object') {
        server.close(() => resolve(address.port))
        return
      }
      server.close()
      reject(new Error('No free port available'))
    })
    server.on('error', reject)
  })
}

interface ServiceRegistration {
  url?: string
  password?: string
}

function readServiceRegistration(): OpenCodeEndpoint | null {
  try {
    const raw = JSON.parse(fs.readFileSync(getServiceJsonPath(), 'utf8')) as ServiceRegistration
    if (raw.url && raw.password) return { url: raw.url, password: raw.password }
    return null
  } catch {
    return null
  }
}

async function waitForEndpoint(timeoutMs: number): Promise<OpenCodeEndpoint | null> {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    const registration = readServiceRegistration()
    if (registration && (await probeInfo(registration, 2_000))) return registration
    await new Promise((resolve) => setTimeout(resolve, 500))
  }
  return null
}

export function getOpenCodeServerStatus(): { status: PluginLlmStatus; error: string | null } {
  return { status: currentStatus, error: currentError }
}

export function getOpenCodeEndpoint(): OpenCodeEndpoint | null {
  return endpoint
}

export async function refreshOpenCodeReachability(): Promise<PluginLlmStatus> {
  if (!endpoint) return currentStatus
  const up = await probeInfo(endpoint)
  if (up) {
    currentStatus = 'running'
    currentError = null
    return currentStatus
  }
  if (serverProcess != null && currentStatus === 'starting') return currentStatus
  if (currentStatus === 'running' && serverProcess == null) {
    currentStatus = 'error'
    currentError = 'OpenCode server stopped unexpectedly'
  }
  return currentStatus
}

export async function startOpenCodeServer(): Promise<{ ok: boolean; error?: string }> {
  if (currentStatus === 'running' && endpoint && (await probeInfo(endpoint))) {
    return { ok: true }
  }

  const executablePath = getOpenCodeExecutablePath()
  if (!fs.existsSync(executablePath)) {
    return { ok: false, error: 'OpenCode runtime not installed' }
  }

  await stopOpenCodeServer()

  currentStatus = 'starting'
  currentError = null

  const port = await getFreePort()
  const childEnv: NodeJS.ProcessEnv = {
    ...process.env,
    XDG_CONFIG_HOME: getXdgDir('config'),
    XDG_DATA_HOME: getXdgDir('data'),
    XDG_CACHE_HOME: getXdgDir('cache'),
    XDG_STATE_HOME: getXdgDir('state'),
  }

  try {
    serverProcess = spawn(
      executablePath,
      ['serve', '--service', '--hostname', '127.0.0.1', '--port', String(port)],
      {
        cwd: getOpenCodeBinDir(),
        env: childEnv,
        stdio: ['ignore', 'pipe', 'pipe'],
        windowsHide: true,
      },
    )
  } catch (err) {
    currentStatus = 'error'
    currentError = err instanceof Error ? err.message : String(err)
    return { ok: false, error: currentError }
  }

  const child = serverProcess

  child.stdout?.on('data', (chunk: Buffer) => appendServeLog(chunk.toString('utf8')))
  child.stderr?.on('data', (chunk: Buffer) => appendServeLog(chunk.toString('utf8')))

  child.on('exit', (code) => {
    if (serverProcess === child) {
      serverProcess = null
      if (currentStatus === 'running') {
        currentStatus = code != null && code !== 0 ? 'error' : 'idle'
        if (currentStatus === 'error') currentError = `OpenCode server exited (code ${code})`
      }
    }
  })

  child.on('error', (err) => {
    if (serverProcess === child) {
      serverProcess = null
      currentStatus = 'error'
      currentError = err.message
    }
  })

  // La escritura de service.json es la señal de arranque del modo --service.
  const registration = await waitForEndpoint(STARTUP_TIMEOUT_MS)
  if (!registration) {
    await stopOpenCodeServer()
    currentStatus = 'error'
    currentError = 'OpenCode server did not become reachable in time'
    return { ok: false, error: currentError }
  }

  endpoint = registration
  currentStatus = 'running'
  currentError = null
  return { ok: true }
}

export async function stopOpenCodeServer(): Promise<void> {
  const child = serverProcess
  serverProcess = null
  endpoint = null

  if (!child || child.pid == null) {
    currentStatus = 'idle'
    currentError = null
    return
  }

  await new Promise<void>((resolve) => {
    const timer = setTimeout(() => resolve(), 3_000)
    child.once('exit', () => {
      clearTimeout(timer)
      resolve()
    })
    if (process.platform === 'win32') {
      // Mata el árbol: opencode puede mantener procesos auxiliares.
      const systemRoot = process.env['SystemRoot'] ?? 'C:\\Windows'
      const taskkill = path.join(systemRoot, 'System32', 'taskkill.exe')
      spawn(taskkill, ['/PID', String(child.pid), '/T', '/F'], { windowsHide: true })
    } else {
      child.kill()
    }
  })

  currentStatus = 'idle'
  currentError = null
}

export async function isOpenCodeServerRunning(): Promise<boolean> {
  if (currentStatus === 'running' && endpoint) return true
  if (serverProcess != null) {
    const registration = readServiceRegistration()
    if (registration && (await probeInfo(registration))) {
      endpoint = registration
      currentStatus = 'running'
      currentError = null
      return true
    }
  }
  return false
}

export function getWorkspaceRoot(): string {
  return getWorkspaceDir()
}
