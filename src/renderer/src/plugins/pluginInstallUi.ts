import type { PluginDownloadPhase } from '@shared-types'

export function formatDownloadBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '—'
  const mb = bytes / (1024 * 1024)
  if (mb >= 1024) return `${(mb / 1024).toFixed(2)} GB`
  if (mb >= 1) return `${mb.toFixed(1)} MB`
  return `${Math.max(1, Math.round(bytes / 1024))} KB`
}

/** Etiqueta única: descargando o instalando (extracción / VC++). */
export function pluginInstallStatusLabel(
  phase: PluginDownloadPhase | undefined,
  t: (key: string) => string,
): string {
  if (phase === 'msvc-redist') {
    return t('Installing Visual C++ Redistributable…')
  }
  if (phase === 'extracting') {
    return t('Installing…')
  }
  if (phase === 'llama-server' || phase === 'model') {
    return t('Downloading…')
  }
  return t('Preparing download…')
}
