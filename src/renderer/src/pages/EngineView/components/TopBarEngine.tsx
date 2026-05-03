import { useState } from 'react';
import {
  Activity,
  ClockFill,
  FloppyFill,
  PlayFill,
  StopFill,
} from 'react-bootstrap-icons';

import AppTooltip from '../../../components/AppTooltip';
import { useContextEngine } from '@engine';

interface Props {
  projectType: string
  handleSave: () => void
  toggleAutoSave: () => void
  debugOverlayVisible: boolean
  onToggleDebugOverlay: () => void
}

export function TopBarEngine({ projectType, handleSave, toggleAutoSave, debugOverlayVisible, onToggleDebugOverlay }: Props) {
  const { engineReady, engineError, previewPlaying, setPreviewPlaying, debugMetrics } = useContextEngine();
  const isStopActive = !previewPlaying;
  const [hasSavedOnce] = useState(false);
  const [autoSaveEnabled] = useState(false);

  const statusBadge = engineReady
    ? <span className="badge bg-success">◉</span>
    : engineError
      ? <span className="badge bg-danger">Error</span>
      : <span className="badge bg-warning text-dark">Iniciando…</span>;

  return (
    <div className="custom-controls-bar border-bottom border-secondary-subtle">
      <div className="p-2 d-flex align-items-center gap-2 justify-content-between">
        <div className="d-flex align-items-center gap-2">
          <AppTooltip content={previewPlaying ? 'Jugando' : 'Iniciar prueba'} place="bottom">
            <button
              className={`btn btn-sm ${previewPlaying ? 'btn-success active' : 'btn-outline-light'}`}
              disabled={!engineReady || previewPlaying}
              onClick={() => setPreviewPlaying(true)}
              aria-pressed={previewPlaying}
              type="button"
            >
              <PlayFill size={16} />
            </button>
          </AppTooltip>
          <AppTooltip content={isStopActive ? 'Editor activo' : 'Detener prueba'} place="bottom">
            <button
              className={`btn btn-sm ${isStopActive ? 'btn-danger active' : 'btn-outline-light'}`}
              disabled={!engineReady || isStopActive}
              onClick={() => setPreviewPlaying(false)}
              aria-pressed={isStopActive}
              type="button"
            >
              <StopFill size={16} />
            </button>
          </AppTooltip>
        </div>
        <div className="d-flex align-items-center">
          <span style={{ fontSize: 16, fontWeight: 700, color: '#c084fc', letterSpacing: '0.03em' }}>
            ⬡ RER-ENGINE
          </span>
          <div className="d-flex align-items-center gap-2 ms-2">
            <span className={`engine-type-badge engine-type-badge--${projectType}`}>
              {projectType}
            </span>
            {statusBadge}
          </div>
        </div>
        <div className="d-flex align-items-center gap-2">
          {debugOverlayVisible && debugMetrics && (
            <div
              style={{
                fontFamily: 'monospace',
                fontSize: 11,
                color: '#94a3b8',
                display: 'flex',
                gap: 10,
                whiteSpace: 'nowrap',
              }}
            >
              <span style={{ color: debugMetrics.fps >= 55 ? '#4ade80' : debugMetrics.fps >= 30 ? '#facc15' : '#f87171' }}>
                {debugMetrics.fps.toFixed(1)} fps
              </span>
              <span>{debugMetrics.frame_time_ms.toFixed(1)} ms</span>
              <span>{debugMetrics.draw_calls} dc</span>
              <span>{debugMetrics.physics_bodies} bod</span>
            </div>
          )}
          <AppTooltip
            content={debugOverlayVisible ? 'Ocultar métricas' : 'Mostrar métricas'}
            place="left"
          >
            <button
              className={`btn btn-sm d-flex align-items-center gap-1 ${
                debugOverlayVisible ? 'btn-info text-dark' : 'btn-outline-secondary'
              }`}
              onClick={onToggleDebugOverlay}
              type="button"
            >
              <Activity size={13} />
            </button>
          </AppTooltip>
          <button
            className="btn btn-sm btn-outline-light d-flex align-items-center gap-2"
            disabled={!engineReady}
            onClick={handleSave}
            type="button"
          >
            <FloppyFill size={13} />
            <span style={{ fontSize: 14 }}>Guardar</span>
          </button>
          <AppTooltip
            content={hasSavedOnce ? (autoSaveEnabled ? 'Desactivar auto-guardado' : 'Activar auto-guardado') : 'Guarda primero'}
            place="left"
          >
            <button
              className={`btn btn-sm d-flex align-items-center gap-1 ${
                autoSaveEnabled ? 'btn-warning text-dark' : 'btn-outline-secondary'
              }`}
              disabled={!hasSavedOnce || !engineReady}
              onClick={toggleAutoSave}
              style={{ whiteSpace: 'nowrap' }}
              type="button"
            >
              <ClockFill size={11} />
              <span style={{ fontSize: 14 }}>Auto</span>
            </button>
          </AppTooltip>
        </div>
      </div>
    </div>
  );
}

export default TopBarEngine;