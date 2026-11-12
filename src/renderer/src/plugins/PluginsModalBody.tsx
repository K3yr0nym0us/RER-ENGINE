import { useCallback, useEffect, useState } from 'react'
import { ProgressBar } from 'react-bootstrap'
import { Hourglass } from 'react-bootstrap-icons'

import { useTraslate } from '@hooks'
import type {
  AssistantGoConnectResult,
  AssistantModelOption,
  AssistantModelsResult,
  AssistantSettings,
  PluginCatalogEntry,
  PluginDownloadProgress,
  PluginId,
} from '@shared-types'
import rerAiIcon from '../../../resources/RER-AI.png'
import { formatDownloadBytes, pluginInstallStatusLabel } from './pluginInstallUi'
import { usePlugins, type PluginLlmStatusSnapshot } from './usePlugins'

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
  const isIndeterminate =
    progress?.phase === 'extracting' || progress?.phase === 'preparing'

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
        label={isIndeterminate ? '…' : `${overallPercent}%`}
        striped={isIndeterminate}
        animated={isIndeterminate}
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

interface AssistantModelConnectionProps {
  llmStatus: PluginLlmStatusSnapshot
  listModels: () => Promise<AssistantModelsResult>
  setModel: (modelRef: string | null) => Promise<void>
  getAssistantSettings: () => Promise<AssistantSettings>
  connectGo: (apiKey: string) => Promise<AssistantGoConnectResult>
  disconnectGo: () => Promise<AssistantGoConnectResult>
  startOpenCodeLogin: () => Promise<{ ok: boolean; url?: string; error?: string }>
}

function AssistantModelConnection({
  llmStatus,
  listModels,
  setModel,
  getAssistantSettings,
  connectGo,
  disconnectGo,
  startOpenCodeLogin,
}: AssistantModelConnectionProps) {
  const { t } = useTraslate()
  const [settings, setSettings] = useState<AssistantSettings>({ selectedModel: null, goConnected: false })
  const [models, setModels] = useState<AssistantModelOption[]>([])
  const [modelsError, setModelsError] = useState<string | null>(null)
  const [apiKey, setApiKey] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const serverRunning = llmStatus.status === 'running' || llmStatus.status === 'starting'

  const load = useCallback(async () => {
    const [settingsResult, modelsResult] = await Promise.all([
      getAssistantSettings(),
      listModels(),
    ])
    setSettings(settingsResult)
    if (modelsResult.ok) {
      setModels(modelsResult.models ?? [])
      setModelsError(null)
    } else {
      setModels([])
      setModelsError(modelsResult.error ?? t('Model catalog unavailable'))
    }
  }, [getAssistantSettings, listModels, t])

  useEffect(() => {
    if (!serverRunning) return
    void load()
  }, [serverRunning, llmStatus.status, load])

  const freeModels = models.filter((model) => model.free)
  const goModels = models.filter((model) => !model.free)

  const handleModelChange = async (value: string) => {
    const next = value || null
    setSettings((prev) => ({ ...prev, selectedModel: next }))
    await setModel(next)
  }

  const runConnect = async () => {
    setBusy(true)
    setError(null)
    const result = await connectGo(apiKey.trim())
    setBusy(false)
    if (result.ok) {
      setApiKey('')
      await load()
    } else {
      setError(result.error ?? t('Connection failed'))
    }
  }

  const runDisconnect = async () => {
    setBusy(true)
    setError(null)
    const result = await disconnectGo()
    setBusy(false)
    if (result.ok) {
      await load()
    } else {
      setError(result.error ?? t('Connection failed'))
    }
  }

  return (
    <div className="plugin-assistant-settings card border-secondary-subtle mt-3">
      <div className="card-body py-3">
        <h6 className="plugin-assistant-settings__title mb-3">{t('Model and connection')}</h6>

        {!serverRunning && (
          <p className="small text-secondary mb-0">
            {t('Enable the assistant plugin to configure models.')}
          </p>
        )}

        {serverRunning && (
          <>
            <div className="row g-2 align-items-center mb-2">
              <div className="col-sm-3">
                <label className="col-form-label small mb-0" htmlFor="assistant-model-select">
                  {t('Model')}
                </label>
              </div>
              <div className="col-sm-9">
                <select
                  id="assistant-model-select"
                  className="form-select form-select-sm"
                  value={settings.selectedModel ?? ''}
                  onChange={(e) => void handleModelChange(e.target.value)}
                >
                  <option value="">{t('Automatic (first free model)')}</option>
                  {freeModels.length > 0 && (
                    <optgroup label={t('Free models')}>
                      {freeModels.map((model) => (
                        <option key={model.ref} value={model.ref}>
                          {model.name}
                        </option>
                      ))}
                    </optgroup>
                  )}
                  {goModels.length > 0 && (
                    <optgroup label={t('OpenCode Go models')}>
                      {goModels.map((model) => (
                        <option key={model.ref} value={model.ref}>
                          {model.name}
                        </option>
                      ))}
                    </optgroup>
                  )}
                </select>
              </div>
            </div>

            {settings.goConnected ? (
              <div className="row g-2 align-items-center">
                <div className="col-sm-3">
                  <span className="small">{t('OpenCode Go')}</span>
                </div>
                <div className="col-sm-9 d-flex align-items-center gap-2">
                  <span className="badge text-bg-success">{t('Connected')}</span>
                  <button
                    type="button"
                    className="btn btn-sm btn-outline-secondary"
                    disabled={busy}
                    onClick={() => void runDisconnect()}
                  >
                    {t('Disconnect')}
                  </button>
                </div>
              </div>
            ) : (
              <div className="row g-2 align-items-center">
                <div className="col-sm-3">
                  <label className="col-form-label small mb-0" htmlFor="assistant-go-token">
                    {t('OpenCode Go token')}
                  </label>
                </div>
                <div className="col-sm-9 d-flex gap-2">
                  <input
                    id="assistant-go-token"
                    type="password"
                    className="form-control form-control-sm"
                    placeholder={t('Paste your OpenCode Go API key')}
                    value={apiKey}
                    onChange={(e) => setApiKey(e.target.value)}
                    autoComplete="off"
                  />
                  <button
                    type="button"
                    className="btn btn-sm btn-primary flex-shrink-0"
                    disabled={busy || apiKey.trim().length < 8}
                    onClick={() => void runConnect()}
                  >
                    {t('Connect')}
                  </button>
                </div>
              </div>
            )}

            <p className="small text-secondary mt-2 mb-0">
              {t('The token is stored in the isolated OpenCode profile, never in RER-ENGINE.')}
            </p>

            {freeModels.length === 0 && (
              <div className="mt-2">
                <p className="small mb-1">{t('No free models available. Connect your OpenCode account.')}</p>
                <button
                  type="button"
                  className="btn btn-sm btn-outline-primary"
                  disabled={busy}
                  onClick={() => void startOpenCodeLogin()}
                >
                  {t('Connect OpenCode account')}
                </button>
              </div>
            )}

            {modelsError && (
              <p className="small text-warning mt-2 mb-0">{modelsError}</p>
            )}
            {error && (
              <p className="small text-danger mt-2 mb-0">{error}</p>
            )}
          </>
        )}
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
    listModels,
    setModel,
    getAssistantSettings,
    connectGo,
    disconnectGo,
    startOpenCodeLogin,
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

  const hasAssistantPlugin = catalog.some((entry) => entry.id === 'local-ai-assistant')

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

      {hasAssistantPlugin && (
        <AssistantModelConnection
          llmStatus={llmStatus}
          listModels={listModels}
          setModel={setModel}
          getAssistantSettings={getAssistantSettings}
          connectGo={connectGo}
          disconnectGo={disconnectGo}
          startOpenCodeLogin={startOpenCodeLogin}
        />
      )}
    </div>
  )
}
