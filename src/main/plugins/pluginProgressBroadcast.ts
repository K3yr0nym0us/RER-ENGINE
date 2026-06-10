import { webContents } from 'electron'

import type { PluginDownloadProgress } from '../../shared-types/plugins'

let lastBroadcastMs = 0

export function resetProgressBroadcastThrottle(): void {
  lastBroadcastMs = 0
}

/** Envía progreso a todas las ventanas renderer (main + modal plugins). */
export function broadcastPluginsProgress(progress: PluginDownloadProgress, force = false): void {
  const now = Date.now()
  if (!force && progress.overallPercent < 100 && now - lastBroadcastMs < 120) {
    return
  }
  lastBroadcastMs = now

  for (const wc of webContents.getAllWebContents()) {
    if (wc.isDestroyed()) continue
    const url = wc.getURL()
    if (url.includes('devtools://')) continue
    try {
      wc.send('plugins:download-progress', progress)
    } catch {
      // ignore destroyed mid-send
    }
  }
}
