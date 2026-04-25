import { 
  ClockFill, 
  FloppyFill, 
  PauseFill, 
  PlayFill, 
  StopFill 
} from 'react-bootstrap-icons';

export function TopBarEngine({ engineError, projectType, engineReady, hasSavedOnce, autoSaveEnabled, handleSave, toggleAutoSave }: {
  engineError: string | null
  projectType: string
  engineReady: boolean
  hasSavedOnce: boolean
  autoSaveEnabled: boolean
  handleSave: () => void
  toggleAutoSave: () => void
}) {

  const statusBadge = engineReady
    ? <span className="badge bg-success">◉</span>
    : engineError
      ? <span className="badge bg-danger">Error</span>
      : <span className="badge bg-warning text-dark">Iniciando…</span>

  const typeBadgeClass = `engine-type-badge engine-type-badge--${projectType === '3D' ? '3d' : '2d'}`

  return (
    <div className="p-2 d-flex align-items-center gap-2 custom-controls-bar border-bottom border-secondary-subtle justify-content-between">
      <div className="d-flex align-items-center gap-2">
        <button className="btn btn-outline-light btn-sm" title="Play">
          <PlayFill size={16} />
        </button>
        <button className="btn btn-outline-light btn-sm" title="Pause">
          <PauseFill size={16} />
        </button>
        <button className="btn btn-outline-light btn-sm" title="Stop">
          <StopFill size={16} />
        </button>
      </div>
      <div className="d-flex align-items-center">
        <span style={{ fontSize: 16, fontWeight: 700, color: '#c084fc', letterSpacing: '0.03em' }}>
          ⬡ RER-ENGINE
        </span>
        <div className="d-flex align-items-center gap-2 ms-2">
          <span className={typeBadgeClass}>{projectType}</span>
          {statusBadge}
        </div>
      </div>
      <div className="d-flex align-items-center gap-2">
        <button
          className="btn btn-sm btn-outline-light d-flex align-items-center gap-2"
          title="Guardar proyecto"
          disabled={!engineReady}
          onClick={handleSave}
        >
          <FloppyFill size={13} />
          <span style={{ fontSize: 11 }}>Guardar</span>
        </button>
        <button
          className={`btn btn-sm d-flex align-items-center gap-1 ${
            autoSaveEnabled ? 'btn-warning text-dark' : 'btn-outline-secondary'
          }`}
          title={hasSavedOnce ? (autoSaveEnabled ? 'Desactivar auto-guardado (cada 5 min)' : 'Activar auto-guardado (cada 5 min)') : 'Guarda el proyecto al menos una vez para activar'}
          disabled={!hasSavedOnce || !engineReady}
          onClick={toggleAutoSave}
          style={{ whiteSpace: 'nowrap' }}
        >
          <ClockFill size={11} />
          <span style={{ fontSize: 10 }}>Auto</span>
        </button>
      </div>
    </div>
  )
}

export default TopBarEngine;
