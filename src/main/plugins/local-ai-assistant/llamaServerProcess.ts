import { spawn, type ChildProcess } from 'child_process'
import fs from 'fs'
import http from 'http'
import path from 'path'

import type { PluginLlmStatus } from '../../../shared-types/plugins'
import { getPluginCatalogEntry } from '../pluginCatalog'
import { ensureLlamaRuntime, formatWindowsExitCode } from './llamaRuntime'
import { ensureMsvc2015to2022X64 } from './msvcRedistributable'
import { resolvePluginUiLocale } from './pluginUiLocale'

const DEFAULT_PORT = 8765
const STARTUP_TIMEOUT_MS = 120_000

let llamaProcess: ChildProcess | null = null
let currentStatus: PluginLlmStatus = 'idle'
let currentError: string | null = null

function listenForReady(chunk: Buffer): boolean {
  const text = chunk.toString().toLowerCase()
  return (
    text.includes('http server listening')
    || text.includes('server listening')
    || text.includes('listening on')
    || text.includes(`127.0.0.1:${DEFAULT_PORT}`)
    || text.includes(`localhost:${DEFAULT_PORT}`)
  )
}

export function probeLlamaServerHttp(timeoutMs = 2_500): Promise<boolean> {
  const paths = ['/health', '/v1/models', '/']
  return new Promise((resolve) => {
    let index = 0
    const tryNext = () => {
      if (index >= paths.length) {
        resolve(false)
        return
      }
      const path = paths[index++]
      const req = http.get(
        { hostname: '127.0.0.1', port: DEFAULT_PORT, path, timeout: timeoutMs },
        (res) => {
          res.resume()
          resolve(res.statusCode != null && res.statusCode >= 200 && res.statusCode < 500)
        },
      )
      req.on('timeout', () => {
        req.destroy()
        tryNext()
      })
      req.on('error', () => tryNext())
    }
    tryNext()
  })
}

export function getLlamaServerStatus(): { status: PluginLlmStatus; error: string | null } {
  return { status: currentStatus, error: currentError }
}

export async function refreshLlamaServerReachability(): Promise<PluginLlmStatus> {
  const up = await probeLlamaServerHttp()
  if (up) {
    currentStatus = 'running'
    currentError = null
    return currentStatus
  }
  if (llamaProcess != null && currentStatus === 'starting') {
    return currentStatus
  }
  if (currentStatus === 'running' && llamaProcess == null) {
    currentStatus = 'error'
    currentError = 'AI server stopped unexpectedly'
  }
  return currentStatus
}

export async function startLlamaServer(
  executablePath: string,
  modelPath: string,
): Promise<{ ok: boolean; error?: string }> {
  if (process.platform === 'win32') {
    currentStatus = 'starting'
    currentError = null
    const msvc = await ensureMsvc2015to2022X64(resolvePluginUiLocale())
    if (!msvc.ok) {
      currentStatus = 'error'
      currentError = msvc.error
      return { ok: false, error: currentError }
    }
  }

  if (!fs.existsSync(executablePath)) {
    return { ok: false, error: 'llama-server executable not found' }
  }
  if (!fs.existsSync(modelPath)) {
    return { ok: false, error: 'Model file not found' }
  }

  const alreadyUp = await probeLlamaServerHttp()
  if (alreadyUp) {
    currentStatus = 'running'
    currentError = null
    return { ok: true }
  }

  const catalogEntry = getPluginCatalogEntry('local-ai-assistant')
  if (catalogEntry) {
    const runtime = await ensureLlamaRuntime(executablePath, catalogEntry.llamaServer)
    if (!runtime.ok) {
      currentStatus = 'error'
      currentError = runtime.error ?? 'Failed to prepare llama.cpp runtime'
      return { ok: false, error: currentError }
    }
  }

  await stopLlamaServer()

  currentStatus = 'starting'
  currentError = null

  const binDir = path.dirname(executablePath)

  return new Promise((resolve) => {
    try {
      llamaProcess = spawn(
        executablePath,
        [
          '-m',
          modelPath,
          '--port',
          String(DEFAULT_PORT),
          '-c',
          '4096',
          '--jinja',
          '-ngl',
          '0',
        ],
        { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true, cwd: binDir },
      )

      let settled = false
      const settle = async (ok: boolean, error?: string) => {
        if (settled) return
        settled = true
        clearTimeout(timeout)
        if (!ok) {
          currentStatus = 'error'
          currentError = error ?? 'Failed to start llama-server'
          resolve({ ok: false, error: currentError })
          return
        }
        const up = await probeLlamaServerHttp(5_000)
        if (up) {
          currentStatus = 'running'
          currentError = null
          resolve({ ok: true })
        } else {
          currentStatus = 'error'
          currentError = 'llama-server started but HTTP API is not reachable'
          resolve({ ok: false, error: currentError })
        }
      }

      const timeout = setTimeout(() => {
        void settle(true)
      }, STARTUP_TIMEOUT_MS)

      llamaProcess.on('error', (err) => {
        void settle(false, err.message)
      })

      llamaProcess.on('exit', (code) => {
        llamaProcess = null
        if (code != null && code !== 0) {
          const detail = formatWindowsExitCode(code)
          if (!settled) void settle(false, `llama-server exited: ${detail}`)
          else if (currentStatus === 'running') {
            currentStatus = 'error'
            currentError = `llama-server exited: ${detail}`
          }
        } else if (currentStatus === 'running') {
          currentStatus = 'idle'
        }
      })

      const onOutput = (chunk: Buffer) => {
        if (listenForReady(chunk)) {
          void settle(true)
        }
      }

      llamaProcess.stdout?.on('data', onOutput)
      llamaProcess.stderr?.on('data', onOutput)
    } catch (err) {
      currentStatus = 'error'
      currentError = err instanceof Error ? err.message : String(err)
      resolve({ ok: false, error: currentError })
    }
  })
}

export async function stopLlamaServer(): Promise<void> {
  if (!llamaProcess) {
    currentStatus = 'idle'
    currentError = null
    return
  }

  const proc = llamaProcess
  llamaProcess = null
  currentStatus = 'idle'
  currentError = null

  return new Promise((resolve) => {
    proc.once('exit', () => resolve())
    proc.kill()
    setTimeout(() => {
      if (!proc.killed) proc.kill('SIGKILL')
      resolve()
    }, 3_000)
  })
}

export function getLlamaServerPort(): number {
  return DEFAULT_PORT
}

export async function isLlamaServerRunning(): Promise<boolean> {
  if (currentStatus === 'running') return true
  if (llamaProcess != null) {
    const up = await probeLlamaServerHttp()
    if (up) {
      currentStatus = 'running'
      currentError = null
      return true
    }
  }
  return false
}
