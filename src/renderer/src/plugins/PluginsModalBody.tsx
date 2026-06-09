import { useState } from 'react'
import { Robot } from 'react-bootstrap-icons'

import { useTraslate } from '@hooks'
import type { PluginCatalogEntry, PluginId } from '@shared-types'
import { usePlugins } from './usePlugins'

function PluginCard({
  entry,
  installed,
  enabled,
  installing,
  confirming,
  progress,
  onRequestInstall,
  onCancelConfirm,
  onConfirmInstall,
  onUninstall,
  onToggleEnabled,
}: {
  entry: PluginCatalogEntry
  installed: boolean
  enabled: boolean
  installing: boolean
  confirming: boolean
  progress: { phase: string; percent: number } | null
  onRequestInstall: () => void
  onCancelConfirm: () => void
  onConfirmInstall: () => void
  onUninstall: () => void
  onToggleEnabled: (next: boolean) => void
}) {
  const { t } = useTraslate()

  return (
    <div className="card border-secondary-subtle">
      <div className="card-body">
        <div className="d-flex align-items-start gap-3">
          <Robot size={32} className="text-primary flex-shrink-0 mt-1" />
          <div className="flex-grow-1">
            <h6 className="mb-1">{entry.name}</h6>
            <p className="text-secondary small mb-2">{entry.description}</p>
            <p className="small mb-2">
              <span className="badge text-bg-secondary me-1">v{entry.version}</span>
              <span className="text-secondary">{entry.downloadSizeLabel}</span>
            </p>

            {confirming && (
              <div className="border rounded p-3 mb-2 bg-body-secondary">
                <p className="small mb-2">{t('Download is on-demand from the official Hugging Face repository.')}</p>
                <ul className="small text-secondary mb-3">
                  <li>
                    <a href={entry.model.repoUrl} target="_blank" rel="noreferrer">
                      {entry.model.repo}
                    </a>
                  </li>
                  <li>
                    {t('File')}: <code>{entry.model.filename}</code>
                  </li>
                  <li>{t('Requires disk space and RAM. Windows only in v1.')}</li>
                </ul>
                <div className="d-flex gap-2">
                  <button type="button" className="btn btn-sm btn-secondary" onClick={onCancelConfirm}>
                    {t('Cancel')}
                  </button>
                  <button type="button" className="btn btn-sm btn-primary" onClick={onConfirmInstall}>
                    {t('Install plugin')}
                  </button>
                </div>
              </div>
            )}

            {installing && progress && (
              <div className="mb-2">
                <div className="small text-secondary mb-1">
                  {progress.phase === 'model' ? t('Downloading model…') : t('Downloading runtime…')}{' '}
                  {progress.percent}%
                </div>
                <div className="progress" style={{ height: 6 }}>
                  <div
                    className="progress-bar progress-bar-striped progress-bar-animated"
                    style={{ width: `${progress.percent}%` }}
                  />
                </div>
              </div>
            )}

            <div className="d-flex flex-wrap gap-2 align-items-center">
              {!installed && !confirming && (
                <button
                  type="button"
                  className="btn btn-sm btn-primary"
                  disabled={installing}
                  onClick={onRequestInstall}
                >
                  {installing ? t('Installing…') : t('Install')}
                </button>
              )}
              {installed && (
                <>
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
                    className="btn btn-sm btn-outline-danger"
                    disabled={installing}
                    onClick={onUninstall}
                  >
                    {t('Uninstall')}
                  </button>
                </>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

export function PluginsModalBody() {
  const { t } = useTraslate()
  const { catalog, downloadProgress, install, uninstall, setEnabled, isInstalled, isEnabled, llmStatus } =
    usePlugins()
  const [busyId, setBusyId] = useState<PluginId | null>(null)
  const [confirmId, setConfirmId] = useState<PluginId | null>(null)
  const [error, setError] = useState<string | null>(null)

  const runInstall = async (pluginId: PluginId) => {
    setError(null)
    setConfirmId(null)
    setBusyId(pluginId)
    const result = await install(pluginId)
    setBusyId(null)
    if (!result.ok) setError(result.error ?? t('Install failed'))
  }

  const runUninstall = async (pluginId: PluginId) => {
    setError(null)
    setBusyId(pluginId)
    const result = await uninstall(pluginId)
    setBusyId(null)
    if (!result.ok) setError(result.error ?? t('Uninstall failed'))
  }

  return (
    <div className="d-flex flex-column gap-3">
      <p className="text-secondary small mb-0">
        {t('Optional plugins are downloaded on demand. The base editor does not include AI models or runtimes.')}
      </p>

      {error && (
        <div className="alert alert-danger py-2 small mb-0" role="alert">
          {error}
        </div>
      )}

      {llmStatus.error && (
        <div className="alert alert-warning py-2 small mb-0" role="alert">
          {llmStatus.error}
        </div>
      )}

      {catalog.map((entry) => (
        <PluginCard
          key={entry.id}
          entry={entry}
          installed={isInstalled(entry.id)}
          enabled={isEnabled(entry.id)}
          confirming={confirmId === entry.id}
          installing={busyId === entry.id || llmStatus.status === 'downloading'}
          progress={
            downloadProgress?.pluginId === entry.id
              ? { phase: downloadProgress.phase, percent: downloadProgress.percent }
              : null
          }
          onRequestInstall={() => setConfirmId(entry.id)}
          onCancelConfirm={() => setConfirmId(null)}
          onConfirmInstall={() => void runInstall(entry.id)}
          onUninstall={() => void runUninstall(entry.id)}
          onToggleEnabled={(next) => void setEnabled(entry.id, next)}
        />
      ))}
    </div>
  )
}
