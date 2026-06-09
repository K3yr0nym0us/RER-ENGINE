import { useCallback, useEffect, useState } from 'react'

import type {
  PluginCatalogEntry,
  PluginDownloadProgress,
  PluginId,
  PluginInstallResult,
  PluginsState,
  PluginUiAction,
} from '@shared-types'

export interface PluginLlmStatusSnapshot {
  status: string
  error: string | null
  enabled: boolean
  installed: boolean
}

export function usePlugins() {
  const [catalog, setCatalog] = useState<PluginCatalogEntry[]>([])
  const [state, setState] = useState<PluginsState>({ installed: {}, enabled: [] })
  const [llmStatus, setLlmStatus] = useState<PluginLlmStatusSnapshot>({
    status: 'idle',
    error: null,
    enabled: false,
    installed: false,
  })
  const [downloadProgress, setDownloadProgress] = useState<PluginDownloadProgress | null>(null)

  const refresh = useCallback(async () => {
    const [cat, st, llm] = await Promise.all([
      window.electronAPI.pluginsGetCatalog(),
      window.electronAPI.pluginsGetState(),
      window.electronAPI.pluginsGetLlmStatus(),
    ])
    setCatalog(cat)
    setState(st)
    setLlmStatus(llm)
  }, [])

  useEffect(() => {
    void refresh()
    const offProgress = window.electronAPI.onPluginsDownloadProgress((p) => {
      setDownloadProgress(p)
    })
    const offUiAction = window.electronAPI.onPluginsUiAction((action) => {
      window.dispatchEvent(new CustomEvent('plugins:ui-action', { detail: action }))
    })
    const offStateChanged = window.electronAPI.onPluginsStateChanged(() => {
      void refresh()
    })
    const offModalClosed = window.electronAPI.onModalElectronClosed((data) => {
      if (data.componentKey === 'PluginsModalBody') void refresh()
    })
    const onRefreshRequest = () => {
      void refresh()
    }
    window.addEventListener('plugins:refresh-request', onRefreshRequest)
    return () => {
      offProgress()
      offUiAction()
      offStateChanged()
      offModalClosed()
      window.removeEventListener('plugins:refresh-request', onRefreshRequest)
    }
  }, [refresh])

  const setEnabled = useCallback(
    async (pluginId: PluginId, enabled: boolean) => {
      const next = await window.electronAPI.pluginsSetEnabled(pluginId, enabled)
      setState(next)
      const llm = await window.electronAPI.pluginsGetLlmStatus()
      setLlmStatus(llm)
      if (enabled) {
        await window.electronAPI.pluginsStartLlm()
        const llmAfter = await window.electronAPI.pluginsGetLlmStatus()
        setLlmStatus(llmAfter)
      } else {
        await window.electronAPI.pluginsStopLlm()
      }
    },
    [],
  )

  const install = useCallback(
    async (pluginId: PluginId): Promise<PluginInstallResult> => {
      setDownloadProgress(null)
      const result = await window.electronAPI.pluginsInstall(pluginId)
      setDownloadProgress(null)
      await refresh()
      return result
    },
    [refresh],
  )

  const uninstall = useCallback(
    async (pluginId: PluginId): Promise<PluginInstallResult> => {
      const result = await window.electronAPI.pluginsUninstall(pluginId)
      await refresh()
      return result
    },
    [refresh],
  )

  const isInstalled = useCallback(
    (pluginId: PluginId) => Boolean(state.installed[pluginId]),
    [state],
  )

  const isEnabled = useCallback(
    (pluginId: PluginId) => state.enabled.includes(pluginId) && isInstalled(pluginId),
    [state, isInstalled],
  )

  return {
    catalog,
    state,
    llmStatus,
    downloadProgress,
    refresh,
    setEnabled,
    install,
    uninstall,
    isInstalled,
    isEnabled,
  }
}

export type { PluginUiAction }
