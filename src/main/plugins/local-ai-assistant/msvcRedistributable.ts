import { execFile, execFileSync } from 'child_process'
import fs from 'fs'
import path from 'path'
import { promisify } from 'util'

const execFileAsync = promisify(execFile)

/** Official x64 VC++ 2015–2022 Redistributable download (fallback if bundled installer missing). */
export const MSVC_X64_REDIST_URL = 'https://aka.ms/vs/17/release/vc_redist.x64.exe'

const BUNDLED_REDIST_NAME = 'vc_redist.x64.exe'

const REG_KEYS = [
  'HKLM\\SOFTWARE\\Microsoft\\VisualStudio\\14.0\\VC\\Runtimes\\x64',
  'HKLM\\SOFTWARE\\WOW6432Node\\Microsoft\\VisualStudio\\14.0\\VC\\Runtimes\\x64',
] as const

export type MsvcLocale = 'en' | 'es'

export type MsvcProbeSignals = {
  registryInstalled: boolean
  systemDllPresent: boolean
}

export type MsvcProbeDeps = {
  platform: NodeJS.Platform
  queryRegistryInstalled: (key: string) => boolean
  systemDllsExist: () => boolean
}

export type MsvcEnsureDeps = MsvcProbeDeps & {
  resolveBundledInstallerPath: () => string | null
  resolveWorkingInstallerPath: () => string
  prepareWorkingInstaller: (bundledPath: string) => string
  runInstaller: (installerPath: string) => Promise<{ exitCode: number | null }>
  /** Paths that are safe to delete after a successful MSVC probe (never the repo `src/resources` copy in dev). */
  collectInstallerPathsToDelete: (bundledPath: string, workingPath: string) => string[]
  deleteInstallerFiles: (paths: string[]) => void
}

function parseRegInstalled(stdout: string): boolean {
  // REG QUERY lines look like: "    Installed    REG_DWORD    0x1"
  const installedLine = stdout
    .split(/\r?\n/)
    .find((line) => /^\s*Installed\s+REG_DWORD\s+/i.test(line))
  if (!installedLine) return false
  const match = installedLine.match(/0x([0-9a-f]+)/i)
  if (!match) return false
  return Number.parseInt(match[1], 16) === 1
}

export function queryRegistryInstalledViaReg(key: string): boolean {
  try {
    const systemRoot = process.env.SystemRoot ?? 'C:\\Windows'
    const regExe = path.join(systemRoot, 'System32', 'reg.exe')
    const stdout = execFileSync(regExe, ['query', key, '/v', 'Installed'], {
      encoding: 'utf8',
      windowsHide: true,
      timeout: 5_000,
    })
    return parseRegInstalled(stdout)
  } catch {
    return false
  }
}

export function systemVcRuntimeDllsExist(systemRoot = process.env.SystemRoot ?? 'C:\\Windows'): boolean {
  const system32 = path.join(systemRoot, 'System32')
  // VC++ 2015+ ships vcruntime140.dll; vcruntime140_1.dll is optional on older builds.
  return fs.existsSync(path.join(system32, 'vcruntime140.dll'))
}

/** Pure evaluation used by tests and the live probe. */
export function evaluateMsvc2015to2022X64(signals: MsvcProbeSignals): boolean {
  return signals.registryInstalled || signals.systemDllPresent
}

function defaultProbeDeps(): MsvcProbeDeps {
  return {
    platform: process.platform,
    queryRegistryInstalled: queryRegistryInstalledViaReg,
    systemDllsExist: () => systemVcRuntimeDllsExist(),
  }
}

export function isMsvc2015to2022X64Installed(deps: MsvcProbeDeps = defaultProbeDeps()): boolean {
  if (deps.platform !== 'win32') {
    return true
  }
  const registryInstalled = REG_KEYS.some((key) => deps.queryRegistryInstalled(key))
  if (registryInstalled) {
    return true
  }
  return evaluateMsvc2015to2022X64({
    registryInstalled: false,
    systemDllPresent: deps.systemDllsExist(),
  })
}

/**
 * Bundled installer location.
 * Packaged (installed app): `{resources}/vc_redist/vc_redist.x64.exe`
 *   e.g. `…\RER-ENGINE\resources\vc_redist\vc_redist.x64.exe`
 * Dev only: repo `src/resources/vc_redist.x64.exe` (never used when `app.isPackaged`).
 */
export function resolveBundledVcRedistPath(): string | null {
  try {
    // eslint-disable-next-line @typescript-eslint/no-require-imports -- Electron optional in unit tests
    const { app } = require('electron') as typeof import('electron')
    if (app.isPackaged) {
      const packaged = path.join(process.resourcesPath, 'vc_redist', BUNDLED_REDIST_NAME)
      return fs.existsSync(packaged) ? packaged : null
    }
    const devCandidates = [
      path.join(app.getAppPath(), 'src', 'resources', BUNDLED_REDIST_NAME),
      path.join(process.cwd(), 'src', 'resources', BUNDLED_REDIST_NAME),
    ]
    for (const candidate of devCandidates) {
      if (fs.existsSync(candidate)) return candidate
    }
  } catch {
    // No Electron (unit tests): do not invent install paths.
  }
  return null
}

/**
 * Writable working copy for the elevated installer.
 * Installed app: `{userData}/plugins/vc_redist/vc_redist.x64.exe`
 * (`app.getPath('userData')`, typically `%APPDATA%\rer-engine\…`).
 */
export function resolveVcRedistWorkingPath(): string {
  try {
    // eslint-disable-next-line @typescript-eslint/no-require-imports -- Electron optional in unit tests
    const { app } = require('electron') as typeof import('electron')
    return path.join(app.getPath('userData'), 'plugins', 'vc_redist', BUNDLED_REDIST_NAME)
  } catch {
    throw new Error('resolveVcRedistWorkingPath requires Electron app.getPath(userData)')
  }
}

export function prepareVcRedistWorkingInstaller(bundledPath: string): string {
  const workingPath = resolveVcRedistWorkingPath()
  fs.mkdirSync(path.dirname(workingPath), { recursive: true })
  if (path.resolve(bundledPath) !== path.resolve(workingPath)) {
    fs.copyFileSync(bundledPath, workingPath)
  }
  return workingPath
}

/**
 * After a successful MSVC probe, delete:
 * - userData working copy (always, when present)
 * - packaged `resources/vc_redist/…` when `app.isPackaged` (installed layout only)
 * Never deletes the repo `src/resources` file used in development.
 */
export function collectVcRedistPathsToDelete(bundledPath: string, workingPath: string): string[] {
  const out: string[] = []
  if (workingPath) out.push(workingPath)
  try {
    // eslint-disable-next-line @typescript-eslint/no-require-imports -- Electron optional in unit tests
    const { app } = require('electron') as typeof import('electron')
    if (!app.isPackaged) {
      return [...new Set(out.map((p) => path.resolve(p)))]
    }
    const resourcesRoot = path.resolve(process.resourcesPath)
    const packagedBundled = path.join(resourcesRoot, 'vc_redist', BUNDLED_REDIST_NAME)
    if (bundledPath && path.resolve(bundledPath).startsWith(resourcesRoot)) {
      out.push(bundledPath)
    } else if (fs.existsSync(packagedBundled)) {
      out.push(packagedBundled)
    }
  } catch {
    // unit tests: only working copy
  }
  return [...new Set(out.map((p) => path.resolve(p)))]
}

export function deleteVcRedistInstallerFiles(paths: string[]): void {
  for (const filePath of paths) {
    try {
      if (fs.existsSync(filePath)) {
        fs.unlinkSync(filePath)
      }
    } catch {
      // Program Files may be read-only without elevation; ignore.
    }
    try {
      const dir = path.dirname(filePath)
      if (fs.existsSync(dir) && fs.readdirSync(dir).length === 0) {
        fs.rmdirSync(dir)
      }
    } catch {
      // ignore
    }
  }
}

/**
 * Runs the bundled VC++ redist with UAC elevation and waits until it exits.
 * `/passive` shows a progress UI; `/norestart` avoids forced reboot mid-plugin install.
 */
export async function runBundledVcRedistInstaller(installerPath: string): Promise<{ exitCode: number | null }> {
  const systemRoot = process.env.SystemRoot ?? 'C:\\Windows'
  const powershell = path.join(systemRoot, 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe')
  // Start-Process -Wait returns after elevation + installer finish (or cancel).
  const script =
    `$p = Start-Process -FilePath ${JSON.stringify(installerPath)} `
    + `-ArgumentList '/install','/passive','/norestart' -Verb RunAs -PassThru -Wait; `
    + 'if ($null -eq $p) { exit 1 }; exit $p.ExitCode'
  try {
    await execFileAsync(powershell, ['-NoProfile', '-NonInteractive', '-Command', script], {
      windowsHide: true,
      timeout: 30 * 60 * 1000,
    })
    return { exitCode: 0 }
  } catch (err) {
    const code =
      err && typeof err === 'object' && 'code' in err && typeof (err as { code: unknown }).code === 'number'
        ? (err as { code: number }).code
        : null
    // VC redist often returns 1638 (already installed) / 3010 (reboot required) — treat as soft success if probe passes later.
    return { exitCode: code }
  }
}

export function msvcMissingUserMessage(locale: MsvcLocale = 'en'): string {
  if (locale === 'es') {
    return (
      `Falta Microsoft Visual C++ 2015–2022 Redistributable (x64). `
      + `Instálalo desde ${MSVC_X64_REDIST_URL} y vuelve a intentar.`
    )
  }
  return (
    `Microsoft Visual C++ 2015–2022 Redistributable (x64) is missing. `
    + `Install it from ${MSVC_X64_REDIST_URL} and try again.`
  )
}

export function msvcInstallerMissingMessage(locale: MsvcLocale = 'en'): string {
  if (locale === 'es') {
    return (
      `No se encontró el instalador VC++ empaquetado. `
      + `Descárgalo desde ${MSVC_X64_REDIST_URL}, instálalo y vuelve a intentar.`
    )
  }
  return (
    `Bundled VC++ installer was not found. `
    + `Download it from ${MSVC_X64_REDIST_URL}, install it, and try again.`
  )
}

export function msvcInstallFailedMessage(locale: MsvcLocale = 'en'): string {
  if (locale === 'es') {
    return (
      `La instalación de Visual C++ Redistributable no se completó. `
      + `Instálalo manualmente desde ${MSVC_X64_REDIST_URL} y vuelve a intentar.`
    )
  }
  return (
    `Visual C++ Redistributable installation did not complete. `
    + `Install it manually from ${MSVC_X64_REDIST_URL} and try again.`
  )
}

/** Prefer Spanish when the OS/editor locale starts with `es`. */
export function resolveMsvcMessageLocale(raw?: string | null): MsvcLocale {
  const value = (raw ?? '').trim().toLowerCase()
  if (value.startsWith('es')) return 'es'
  return 'en'
}

export function requireMsvc2015to2022X64(
  locale: MsvcLocale = 'en',
  deps: MsvcProbeDeps = defaultProbeDeps(),
): { ok: true } | { ok: false; error: string } {
  if (isMsvc2015to2022X64Installed(deps)) {
    return { ok: true }
  }
  return { ok: false, error: msvcMissingUserMessage(locale) }
}

function defaultEnsureDeps(): MsvcEnsureDeps {
  return {
    ...defaultProbeDeps(),
    resolveBundledInstallerPath: resolveBundledVcRedistPath,
    resolveWorkingInstallerPath: resolveVcRedistWorkingPath,
    prepareWorkingInstaller: prepareVcRedistWorkingInstaller,
    runInstaller: runBundledVcRedistInstaller,
    collectInstallerPathsToDelete: collectVcRedistPathsToDelete,
    deleteInstallerFiles: deleteVcRedistInstallerFiles,
  }
}

function cleanupInstallersAfterSuccess(
  deps: MsvcEnsureDeps,
  bundledPath: string | null,
  workingPath: string | null,
): void {
  if (!bundledPath && !workingPath) {
    try {
      deps.deleteInstallerFiles([deps.resolveWorkingInstallerPath()])
    } catch {
      // ignore
    }
    return
  }
  const paths = deps.collectInstallerPathsToDelete(bundledPath ?? '', workingPath ?? '')
  deps.deleteInstallerFiles(paths)
}

/**
 * If MSVC x64 is missing, run the bundled redistributable (UAC + passive UI),
 * wait for exit, then re-probe. On success, delete the installer copies to free disk.
 */
export async function ensureMsvc2015to2022X64(
  locale: MsvcLocale = 'en',
  deps: MsvcEnsureDeps = defaultEnsureDeps(),
  onInstalling?: () => void,
): Promise<{ ok: true } | { ok: false; error: string }> {
  if (deps.platform !== 'win32') {
    return { ok: true }
  }

  if (isMsvc2015to2022X64Installed(deps)) {
    // Already present: drop leftover installer copies (userData + packaged resources).
    const bundled = deps.resolveBundledInstallerPath()
    let working: string | null
    try {
      working = deps.resolveWorkingInstallerPath()
    } catch {
      working = null
    }
    cleanupInstallersAfterSuccess(deps, bundled, working)
    return { ok: true }
  }

  const bundledPath = deps.resolveBundledInstallerPath()
  if (!bundledPath) {
    return { ok: false, error: msvcInstallerMissingMessage(locale) }
  }

  onInstalling?.()
  const workingPath = deps.prepareWorkingInstaller(bundledPath)
  await deps.runInstaller(workingPath)

  if (isMsvc2015to2022X64Installed(deps)) {
    cleanupInstallersAfterSuccess(deps, bundledPath, workingPath)
    return { ok: true }
  }
  return { ok: false, error: msvcInstallFailedMessage(locale) }
}

/** Exported for unit tests of REG QUERY parsing. */
export function parseRegInstalledForTests(stdout: string): boolean {
  return parseRegInstalled(stdout)
}
