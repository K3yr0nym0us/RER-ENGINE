import fs from 'fs'
import path from 'path'
import AdmZip from 'adm-zip'

import type { PluginLlamaServerManifest } from '../../../shared-types/plugins'
import { downloadFile } from '../fileDownload'

/** DLLs que deben estar junto a llama-server.exe (zip oficial de llama.cpp). */
const RUNTIME_MARKERS = ['llama.dll', 'ggml.dll', 'ggml-base.dll', 'ggml-cpu.dll'] as const

export function getLlamaBinDir(executablePath: string): string {
  return path.dirname(executablePath)
}

export function isLlamaRuntimeComplete(binDir: string): boolean {
  return RUNTIME_MARKERS.every((name) => fs.existsSync(path.join(binDir, name)))
}

export function extractLlamaRuntimeZip(zipPath: string, binDir: string): void {
  fs.mkdirSync(binDir, { recursive: true })
  const zip = new AdmZip(zipPath)
  zip.extractAllTo(binDir, true)
}

export async function downloadAndExtractLlamaRuntime(
  manifest: PluginLlamaServerManifest,
  binDir: string,
  onProgress?: (percent: number, bytesReceived: number, bytesTotal: number) => void,
  onExtracting?: () => void,
): Promise<void> {
  fs.mkdirSync(binDir, { recursive: true })
  const zipPath = path.join(binDir, 'llama-runtime.zip')

  await downloadFile(
    manifest.downloadUrl,
    zipPath,
    (p) => onProgress?.(p.percent, p.bytesReceived, p.bytesTotal || manifest.sizeBytes),
    1_000_000,
    manifest.sizeBytes,
  )

  onExtracting?.()
  extractLlamaRuntimeZip(zipPath, binDir)

  try {
    fs.unlinkSync(zipPath)
  } catch {
    // ignore
  }

  const exePath = path.join(binDir, manifest.executableName)
  if (!fs.existsSync(exePath)) {
    throw new Error('llama-server.exe was not extracted from runtime archive')
  }
  if (!isLlamaRuntimeComplete(binDir)) {
    throw new Error('llama.cpp runtime DLLs are missing after extraction')
  }
}

/** Repara instalaciones antiguas que solo extrajeron llama-server.exe. */
export async function ensureLlamaRuntime(
  executablePath: string,
  manifest: PluginLlamaServerManifest,
): Promise<{ ok: boolean; error?: string }> {
  const binDir = getLlamaBinDir(executablePath)
  if (isLlamaRuntimeComplete(binDir) && fs.existsSync(executablePath)) {
    return { ok: true }
  }

  try {
    await downloadAndExtractLlamaRuntime(manifest, binDir)
    return { ok: true }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    return { ok: false, error: message }
  }
}

import { msvcMissingUserMessage } from './msvcRedistributable'
import { resolvePluginUiLocale } from './pluginUiLocale'

export function formatWindowsExitCode(code: number | null): string {
  if (code == null) return 'unknown exit code'
  const unsigned = code >>> 0
  if (unsigned === 0xc0000135) {
    // Same root cause as the MSVC preflight (missing runtime DLL).
    return msvcMissingUserMessage(resolvePluginUiLocale())
  }
  if (unsigned === 0xc0000005) {
    return 'access violation (0xC0000005)'
  }
  return String(code)
}
