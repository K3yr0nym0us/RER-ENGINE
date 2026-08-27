import { useState } from 'react'
import { ProgressBar } from 'react-bootstrap'
import { Hourglass } from 'react-bootstrap-icons'

import { useTraslate } from '@hooks'
import type { PluginCatalogEntry, PluginDownloadProgress, PluginId } from '@shared-types'
import rerAiIcon from '../../../resources/RER-AI.png'
import { formatDownloadBytes, pluginInstallStatusLabel } from './pluginInstallUi'
import { usePlugins } from './usePlugins'

function formatPluginSizeLabel(label: string): string {
  return label.replace(/\s*\([^)]*\)/g, '').trim()
}

function pluginTitle(entry: PluginCatalogEntry, t: (key: string) => string): string {
  if (entry.id === 'local-ai-assistant') {
    return t('RER-AI local assistant')
  }
  return entry.name
}

function PluginInstallProgress({
  progress,
  cancelling,
  onCancel,
}: {
  progress: PluginDownloadProgress | null
  cancelling: boolean
  onCancel: () => void
}) {
  const { t } = useTraslate()

  const overallPercent = progress?.overallPercent ?? 0
  const statusLabel = pluginInstallStatusLabel(progress?.phase, t)
  const bytesOverallReceived = progress?.bytesOverallReceived ?? 0
  const bytesOverallTotal = progress?.bytesOverallTotal ?? 0
  const isExtracting = progress?.phase === 'extracting' || progress?.phase === 'msvc-redist'

  const bytesLabel = `${formatDownloadBytes(bytesOverallReceived)} / ${formatDownloadBytes(bytesOverallTotal)}`

  return (
    <div className="plugin-install-progress w-100 mt-2">
      <div className="d-flex justify-content-between align-items-baseline gap-2 mb-1">
        <span className="plugin-install-progress__label small fw-semibold">{statusLabel}</span>
        <span className="plugin-install-progress__bytes small text-secondary text-nowrap">{bytesLabel}</span>
      </div>
      <ProgressBar
        className="plugin-install-progress__bar mb-2"
        now={overallPercent}
        min={0}
        max={100}
        label={isExtracting ? '…' : `${overallPercent}%`}
        striped={isExtracting}
        animated={isExtracting}
      />
      <button
        type="button"
        className="btn btn-sm btn-outline-secondary w-100"
        disabled={cancelling}
        onClick={onCancel}
      >
        {cancelling ? t('Cancelling installation…') : t('Cancel installation')}
      </button>
    </div>
  )
}

function PluginCard({
  entry,
  installed,
  enabled,
  installing,
  progress,
  cancelling,
  onInstall,
  onCancelInstall,
  onUninstall,
  onToggleEnabled,
}: {
  entry: PluginCatalogEntry
  installed: boolean
  enabled: boolean
  installing: boolean
  progress: PluginDownloadProgress | null
  cancelling: boolean
  onInstall: () => void
  onCancelInstall: () => void
  onUninstall: () => void
  onToggleEnabled: (next: boolean) => void
}) {
  const { t } = useTraslate()

  return (
    <div className="plugin-square-card plugin-square-card--active card border-secondary-subtle">
      <div className="card-body d-flex flex-column align-items-center text-center px-3 pb-3 pt-2">
        <img
          src={rerAiIcon}
          alt=""
          className="plugin-square-card__icon"
          draggable={false}
        />
        <h6 className="plugin-square-card__title mb-2">{pluginTitle(entry, t)}</h6>
        <p className="plugin-square-card__meta small mb-0">
          <span className="badge text-bg-secondary">v{entry.version}</span>
          <span className="plugin-square-card__size text-secondary ms-2">
            {formatPluginSizeLabel(entry.downloadSizeLabel)}
          </span>
        </p>

        {installing && (
          <PluginInstallProgress
            progress={progress}
            cancelling={cancelling}
            onCancel={onCancelInstall}
          />
        )}

        {!installing && (
          <div className="plugin-square-card__actions w-100 mt-auto pt-3">
            {!installed && (
              <button type="button" className="btn btn-sm btn-primary w-100" onClick={onInstall}>
                {t('Install')}
              </button>
            )}
            {installed && (
              <div className="d-flex flex-column align-items-center gap-2">
                <div className="form-check form-switch mb-0">
                  <input
                    className="form-check-input"
                    type="checkbox"
                    id={`plugin-enabled-${entry.id}`}
                    checked={enabled}
                    onChange={(e) => onToggleEnabled(e.target.checked)}
                  />
                  <label className="form-check-label small" htmlFor={`plugin-enabled-${entry.id}`}>
                    {t('Enabled')}
                  </label>
                </div>
                <button
                  type="button"
                  className="btn btn-sm btn-outline-danger w-100"
                  onClick={onUninstall}
                >
                  {t('Uninstall')}
                </button>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  )
}

function ComingSoonPluginCard({ title }: { title?: string }) {
  const { t } = useTraslate()

  return (
    <div className="plugin-square-card plugin-square-card--soon card border-secondary-subtle">
      <div className="card-body d-flex flex-column align-items-center justify-content-center text-center p-3">
        <Hourglass size={title ? 40 : 44} className="plugin-square-card__soon-icon mb-2" aria-hidden />
        {title && <h6 className="plugin-square-card__title mb-2">{title}</h6>}
        <span className="badge text-bg-secondary plugin-square-card__soon-badge">{t('Coming soon')}</span>
      </div>
    </div>
  )
}

export function PluginsModalBody() {
  const { t } = useTraslate()
  const {
    catalog,
    downloadProgress,
    clearDownloadProgress,
    install,
    cancelInstall,
    uninstall,
    setEnabled,
    isInstalled,
    isEnabled,
    llmStatus,
  } = usePlugins()
  const [busyId, setBusyId] = useState<PluginId | null>(null)
  const [cancelling, setCancelling] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const runInstall = async (pluginId: PluginId) => {
    setError(null)
    setCancelling(false)
    clearDownloadProgress()
    setBusyId(pluginId)
    const result = await install(pluginId)
    setBusyId(null)
    setCancelling(false)
    if (!result.ok && !result.cancelled) {
      setError(result.error ?? t('Install failed'))
    }
  }

  const runCancelInstall = () => {
    setCancelling(true)
    void cancelInstall()
  }

  const runUninstall = async (pluginId: PluginId) => {
    setError(null)
    setBusyId(pluginId)
    const result = await uninstall(pluginId)
    setBusyId(null)
    if (!result.ok) setError(result.error ?? t('Uninstall failed'))
  }

  return (
    <div className="plugins-modal-body py-2">
      {error && (
        <div className="alert alert-danger py-2 small mb-3" role="alert">
          {error}
        </div>
      )}

      {llmStatus.error && (
        <div className="alert alert-warning py-2 small mb-3" role="alert">
          {llmStatus.error}
        </div>
      )}

      <div className="plugins-modal-grid">
        {catalog.map((entry) => (
          <PluginCard
            key={entry.id}
            entry={entry}
            installed={isInstalled(entry.id)}
            enabled={isEnabled(entry.id)}
            installing={busyId === entry.id || llmStatus.status === 'downloading'}
            progress={downloadProgress?.pluginId === entry.id ? downloadProgress : null}
            cancelling={cancelling && busyId === entry.id}
            onInstall={() => void runInstall(entry.id)}
            onCancelInstall={runCancelInstall}
            onUninstall={() => void runUninstall(entry.id)}
            onToggleEnabled={(next) => void setEnabled(entry.id, next)}
          />
        ))}
        <ComingSoonPluginCard title={t('2D frame generation AI')} />
        <ComingSoonPluginCard />
      </div>
    </div>
  )
}
