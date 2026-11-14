import { describe, expect, it, vi } from 'vitest'

// El estado de plugins lee userData de Electron; en tests simulamos el estado.
const mockState = {
  installed: {} as Record<string, { version: string; installedAt: string; libraryPath?: string }>,
  enabled: [] as string[],
}
vi.mock('../plugins/pluginState', async (importOriginal) => {
  const original = await importOriginal<typeof import('../plugins/pluginState')>()
  return {
    ...original,
    isPluginEnabled: (id: string) => mockState.enabled.includes(id) && isInstalledMock(id),
    isPluginInstalled: (id: string) => isInstalledMock(id),
  }
})

function isInstalledMock(id: string): boolean {
  return id === 'world-gen' ? Boolean(mockState.installed['world-gen']) : false
}

import { handleEngineCommand, type EngineIpcRegistryDeps } from './engineIpcRegistry'
import { PLUGIN_CATALOG } from '../plugins/pluginCatalog'
import { ENGINE_COMMAND_SET_3D } from '../../shared-types/engineCommandCatalog'

function makeDeps(): { deps: EngineIpcRegistryDeps; sent: unknown[] } {
  const sent: unknown[] = []
  return {
    sent,
    deps: {
      getProjectType: () => '3D',
      sendToEngine: (cmd) => sent.push(cmd),
      setLocale: () => {},
      watchPngAsset: () => {},
    },
  }
}

describe('gating de comandos world_gen_*', () => {
  const cmd = { cmd: 'world_gen_start' } as unknown as Parameters<typeof handleEngineCommand>[0]

  it('rechaza world_gen_* sin el plugin instalado y activo', () => {
    const { deps, sent } = makeDeps()
    handleEngineCommand(cmd, deps)
    expect(sent).toHaveLength(0)
  })

  it('rechaza world_gen_* con el plugin instalado pero deshabilitado', () => {
    mockState.installed['world-gen'] = { version: '1.0.0', installedAt: '', libraryPath: 'x.dll' }
    const { deps, sent } = makeDeps()
    handleEngineCommand(cmd, deps)
    expect(sent).toHaveLength(0)
  })

  it('reenvía world_gen_* con el plugin instalado y activo', () => {
    mockState.installed['world-gen'] = { version: '1.0.0', installedAt: '', libraryPath: 'x.dll' }
    mockState.enabled.push('world-gen')
    const { deps, sent } = makeDeps()
    handleEngineCommand(cmd, deps)
    expect(sent).toHaveLength(1)
  })
})

describe('catálogo del plugin world-gen', () => {
  it('declara el artefacto de descarga con binario y URL', () => {
    const entry = PLUGIN_CATALOG.find((p) => p.id === 'world-gen')
    expect(entry).toBeDefined()
    expect(entry?.kind).toBe('download')
    expect(entry?.artifact?.archiveInnerPath).toBe('rer_worldgen.dll')
    expect(entry?.artifact?.executableName).toBe('rer_worldgen.dll')
    expect(entry?.artifact?.downloadUrl).toMatch(/^https:\/\/github\.com\/.+\.zip$/)
  })

  it('los seis comandos world_gen_* pertenecen al catálogo 3D', () => {
    for (const name of [
      'world_gen_set_library_path',
      'world_gen_set_config',
      'world_gen_start',
      'world_gen_stop',
      'world_gen_reset',
      'world_gen_scatter',
    ]) {
      expect(ENGINE_COMMAND_SET_3D.has(name)).toBe(true)
    }
  })
})
