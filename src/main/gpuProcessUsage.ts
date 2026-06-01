import { execFile } from 'child_process'
import { existsSync } from 'fs'
import { app } from 'electron'
import { promisify } from 'util'

import type { GpuMetricsPlatform } from '../shared-types/types'

const execFileAsync = promisify(execFile)

export function getGpuMetricsPlatform(): GpuMetricsPlatform {
  switch (process.platform) {
    case 'win32':
      return 'windows'
    case 'linux':
      return 'linux'
    case 'darwin':
      return 'darwin'
    default:
      return 'other'
  }
}

/** Lectura de % GPU de procesos Electron por PID (contadores Windows). */
export function isElectronGpuMetricsSupported(): boolean {
  return process.platform === 'win32'
}

/** Motor en Linux: ruta provisional vía `nvidia-smi` (ver CHECKLIST). */
export function isLinuxEngineGpuMetricsAvailable(): boolean {
  if (process.platform !== 'linux') return false
  return existsSync('/usr/bin/nvidia-smi') || existsSync('/bin/nvidia-smi')
}

/** Script PowerShell: 2 muestras (obligatorio para % utilización) + salida con punto decimal. */
export function buildWindowsProcessGpuScript(pids: number[]): string {
  const pidChecks = pids.map((pid) => `$_.InstanceName -match 'pid_${pid}_'`).join(' -or ')
  return [
    '$ErrorActionPreference = "SilentlyContinue"',
    "$c = Get-Counter '\\GPU Engine(*)\\Utilization Percentage' -SampleInterval 1 -MaxSamples 2",
    'if (-not $c) { Write-Output ""; exit 0 }',
    `$samples = $c[-1].CounterSamples | Where-Object { ${pidChecks} }`,
    'if (-not $samples) { Write-Output ""; exit 0 }',
    '$sum = ($samples | Measure-Object -Property CookedValue -Sum).Sum',
    'if ($null -eq $sum) { Write-Output ""; exit 0 }',
    'Write-Output ($sum.ToString("0.###", [System.Globalization.CultureInfo]::InvariantCulture))',
  ].join('; ')
}

export function parseGpuPercentStdout(stdout: string | Buffer): number | null {
  const raw = String(stdout).trim()
  if (!raw) return null
  const normalized = raw.replace(',', '.')
  const value = parseFloat(normalized)
  if (!Number.isFinite(value) || value < 0) return null
  return Math.min(100, value)
}

export async function queryWindowsProcessGpuPercent(pids: number[]): Promise<number | null> {
  if (!isElectronGpuMetricsSupported() || pids.length === 0) return null

  try {
    const { stdout } = await execFileAsync(
      'powershell',
      ['-NoProfile', '-Command', buildWindowsProcessGpuScript(pids)],
      { windowsHide: true, timeout: 10_000 },
    )
    return parseGpuPercentStdout(stdout)
  } catch {
    return null
  }
}

/**
 * % GPU de los procesos Electron de esta app (por PID), no utilización global del sistema.
 * Linux: pendiente de compatibilización (CHECKLIST).
 */
export async function queryElectronAppGpuPercent(): Promise<number | null> {
  if (!isElectronGpuMetricsSupported()) {
    return null
  }

  const pids = [...new Set(app.getAppMetrics().map((m) => m.pid).filter((pid) => pid > 0))]
  return queryWindowsProcessGpuPercent(pids)
}
