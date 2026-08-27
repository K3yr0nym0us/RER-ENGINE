import { describe, expect, it, vi } from 'vitest'
import {
  MSVC_X64_REDIST_URL,
  collectVcRedistPathsToDelete,
  ensureMsvc2015to2022X64,
  evaluateMsvc2015to2022X64,
  isMsvc2015to2022X64Installed,
  msvcMissingUserMessage,
  parseRegInstalledForTests,
  requireMsvc2015to2022X64,
  resolveMsvcMessageLocale,
} from './msvcRedistributable'

describe('msvcRedistributable', () => {
  it('parses REG QUERY Installed DWORD', () => {
    expect(
      parseRegInstalledForTests(
        'HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\VisualStudio\\14.0\\VC\\Runtimes\\x64\n'
          + '    Installed    REG_DWORD    0x1\n',
      ),
    ).toBe(true)
    expect(
      parseRegInstalledForTests(
        '    Installed    REG_DWORD    0x0\n',
      ),
    ).toBe(false)
    expect(parseRegInstalledForTests('no values')).toBe(false)
  })

  it('evaluates registry or DLL signals', () => {
    expect(evaluateMsvc2015to2022X64({ registryInstalled: true, systemDllPresent: false })).toBe(true)
    expect(evaluateMsvc2015to2022X64({ registryInstalled: false, systemDllPresent: true })).toBe(true)
    expect(evaluateMsvc2015to2022X64({ registryInstalled: false, systemDllPresent: false })).toBe(false)
  })

  it('treats non-Windows as installed (plugin is Windows-only)', () => {
    expect(
      isMsvc2015to2022X64Installed({
        platform: 'linux',
        queryRegistryInstalled: () => false,
        systemDllsExist: () => false,
      }),
    ).toBe(true)
  })

  it('detects missing MSVC on Windows when registry and DLL fail', () => {
    expect(
      isMsvc2015to2022X64Installed({
        platform: 'win32',
        queryRegistryInstalled: () => false,
        systemDllsExist: () => false,
      }),
    ).toBe(false)
  })

  it('accepts registry hit without DLL', () => {
    expect(
      isMsvc2015to2022X64Installed({
        platform: 'win32',
        queryRegistryInstalled: (key) => key.includes('VC\\Runtimes\\x64'),
        systemDllsExist: () => false,
      }),
    ).toBe(true)
  })

  it('accepts DLL fallback when registry misses', () => {
    expect(
      isMsvc2015to2022X64Installed({
        platform: 'win32',
        queryRegistryInstalled: () => false,
        systemDllsExist: () => true,
      }),
    ).toBe(true)
  })

  it('builds localized missing messages with download URL', () => {
    expect(msvcMissingUserMessage('en')).toContain(MSVC_X64_REDIST_URL)
    expect(msvcMissingUserMessage('en')).toMatch(/Visual C\+\+/i)
    expect(msvcMissingUserMessage('es')).toContain(MSVC_X64_REDIST_URL)
    expect(msvcMissingUserMessage('es')).toMatch(/Falta/i)
  })

  it('resolves locale from language tags', () => {
    expect(resolveMsvcMessageLocale('es-CL')).toBe('es')
    expect(resolveMsvcMessageLocale('en-US')).toBe('en')
    expect(resolveMsvcMessageLocale(null)).toBe('en')
  })

  it('requireMsvc returns error payload when missing', () => {
    const result = requireMsvc2015to2022X64('es', {
      platform: 'win32',
      queryRegistryInstalled: () => false,
      systemDllsExist: () => false,
    })
    expect(result.ok).toBe(false)
    if (!result.ok) {
      expect(result.error).toContain(MSVC_X64_REDIST_URL)
      expect(result.error).toMatch(/Falta/i)
    }
  })

  it('ensureMsvc skips installer when already present', async () => {
    const runInstaller = vi.fn(async () => ({ exitCode: 0 }))
    const deleteInstallerFiles = vi.fn()
    const result = await ensureMsvc2015to2022X64('en', {
      platform: 'win32',
      queryRegistryInstalled: () => true,
      systemDllsExist: () => false,
      resolveBundledInstallerPath: () => 'C:\\Program Files\\RER-ENGINE\\resources\\vc_redist\\vc_redist.x64.exe',
      resolveWorkingInstallerPath: () => 'C:\\Users\\user\\AppData\\Roaming\\rer-engine\\plugins\\vc_redist\\vc_redist.x64.exe',
      prepareWorkingInstaller: (p) => p,
      runInstaller,
      collectInstallerPathsToDelete: (bundled, working) => [bundled, working],
      deleteInstallerFiles,
    })
    expect(result.ok).toBe(true)
    expect(runInstaller).not.toHaveBeenCalled()
    expect(deleteInstallerFiles).toHaveBeenCalled()
  })

  it('ensureMsvc runs bundled installer then succeeds after re-probe', async () => {
    let installed = false
    const onInstalling = vi.fn()
    const runInstaller = vi.fn(async () => {
      installed = true
      return { exitCode: 0 }
    })
    const deleteInstallerFiles = vi.fn()
    const working = 'C:\\Users\\user\\AppData\\Roaming\\rer-engine\\plugins\\vc_redist\\vc_redist.x64.exe'
    const bundled = 'C:\\Program Files\\RER-ENGINE\\resources\\vc_redist\\vc_redist.x64.exe'
    const result = await ensureMsvc2015to2022X64(
      'en',
      {
        platform: 'win32',
        queryRegistryInstalled: () => installed,
        systemDllsExist: () => false,
        resolveBundledInstallerPath: () => bundled,
        resolveWorkingInstallerPath: () => working,
        prepareWorkingInstaller: () => working,
        runInstaller,
        collectInstallerPathsToDelete: (b, w) => [b, w],
        deleteInstallerFiles,
      },
      onInstalling,
    )
    expect(onInstalling).toHaveBeenCalledOnce()
    expect(runInstaller).toHaveBeenCalledWith(working)
    expect(result.ok).toBe(true)
    expect(deleteInstallerFiles).toHaveBeenCalledWith([bundled, working])
  })

  it('ensureMsvc fails when bundled installer is missing', async () => {
    const result = await ensureMsvc2015to2022X64('en', {
      platform: 'win32',
      queryRegistryInstalled: () => false,
      systemDllsExist: () => false,
      resolveBundledInstallerPath: () => null,
      resolveWorkingInstallerPath: () => 'C:\\Users\\user\\AppData\\Roaming\\rer-engine\\plugins\\vc_redist\\vc_redist.x64.exe',
      prepareWorkingInstaller: (p) => p,
      runInstaller: async () => ({ exitCode: 0 }),
      collectInstallerPathsToDelete: () => [],
      deleteInstallerFiles: () => {},
    })
    expect(result.ok).toBe(false)
    if (!result.ok) {
      expect(result.error).toContain(MSVC_X64_REDIST_URL)
    }
  })

  it('ensureMsvc fails when installer finishes but probe still misses', async () => {
    const deleteInstallerFiles = vi.fn()
    const result = await ensureMsvc2015to2022X64('es', {
      platform: 'win32',
      queryRegistryInstalled: () => false,
      systemDllsExist: () => false,
      resolveBundledInstallerPath: () => 'C:\\Program Files\\RER-ENGINE\\resources\\vc_redist\\vc_redist.x64.exe',
      resolveWorkingInstallerPath: () => 'C:\\Users\\user\\AppData\\Roaming\\rer-engine\\plugins\\vc_redist\\vc_redist.x64.exe',
      prepareWorkingInstaller: (p) => p,
      runInstaller: async () => ({ exitCode: 1602 }),
      collectInstallerPathsToDelete: (bundled, working) => [bundled, working],
      deleteInstallerFiles,
    })
    expect(result.ok).toBe(false)
    expect(deleteInstallerFiles).not.toHaveBeenCalled()
    if (!result.ok) {
      expect(result.error).toMatch(/no se completó|did not complete/i)
    }
  })

  it('collectVcRedistPathsToDelete keeps working copy and skips non-packaged bundled', () => {
    const paths = collectVcRedistPathsToDelete(
      'C:\\repo\\src\\resources\\vc_redist.x64.exe',
      'C:\\Users\\user\\AppData\\Roaming\\rer-engine\\plugins\\vc_redist\\vc_redist.x64.exe',
    )
    expect(paths.some((p) => p.includes('Roaming'))).toBe(true)
  })
})
