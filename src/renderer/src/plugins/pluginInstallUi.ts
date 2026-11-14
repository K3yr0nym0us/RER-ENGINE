import type { PluginDownloadPhase } from '@shared-types'

export function formatDownloadBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '—'
  const mb = bytes / (1024 * 1024)
  if (mb >= 1024) return `${(mb / 1024).toFixed(2)} GB`
  if (mb >= 1) return `${mb.toFixed(1)} MB`
  return `${Math.max(1, Math.round(bytes / 1024))} KB`
}

/** Etiqueta por fase: descarga de OpenCode, extracción o preparación del sandbox. */
export function pluginInstallStatusLabel(
  phase: PluginDownloadPhase | undefined,
  t: (key: string) => string,
): string {
  if (phase === 'opencode') {
    return t('Downloading OpenCode…')
  }
  if (phase === 'extracting') {
    return t('Installing…')
  }
  if (phase === 'preparing') {
    return t('Preparing assistant workspace…')
  }
  if (phase === 'artifact') {
    return t('Downloading…')
  }
  return t('Preparing download…')
}
