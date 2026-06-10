import { useEffect } from 'react'

/** Convierte IPC `plugins:ui-action` del main en CustomEvent para el sidebar. */
export function usePluginUiActionBridge(): void {
  useEffect(() => {
    const off = window.electronAPI.onPluginsUiAction((action) => {
      window.dispatchEvent(new CustomEvent('plugins:ui-action', { detail: action }))
    })
    return off
  }, [])
}
