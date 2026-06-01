import {
  Bug,
  ClockFill,
  FloppyFill,
  PlayFill,
  StopFill,
} from 'react-bootstrap-icons';

import { AppTooltip } from '@components';
import { useContextEngine } from '@engine';
import { useTraslate } from '@hooks';

interface Props {
  projectType: string
  handleSave: () => void
  toggleAutoSave: () => void
  hasSavedOnce: boolean
  autoSaveEnabled: boolean
}

export function TopBarEngine({ projectType, handleSave, toggleAutoSave, hasSavedOnce, autoSaveEnabled }: Props) {
  const { engineReady, engineError, previewPlaying, setPreviewPlaying, debugMode, setDebugMode } = useContextEngine();
  const { t } = useTraslate();
  const isStopActive = !previewPlaying;

  const statusBadge = engineReady
    ? <span className="badge bg-success">◉</span>
    : engineError
      ? <span className="badge bg-danger">{t('Error')}</span>
      : <span className="badge bg-warning text-dark">{t('Starting…')}</span>;

  return (
    <div className="custom-controls-bar border-bottom border-secondary-subtle">
      <div className="p-2 d-flex align-items-center gap-2 justify-content-between">
        <div className="d-flex align-items-center gap-2">
          <AppTooltip content={previewPlaying ? t('Playing') : t('Start test')} place="left">
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
          <AppTooltip content={isStopActive ? t('Active editor') : t('Stop test')} place="left">
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
          <AppTooltip content={debugMode ? t('Deactivate debug mode') : t('Activate debug mode')} place="right">
            <button
              className={`btn btn-sm ${debugMode ? 'btn-success active' : 'btn-outline-success'}`}
              disabled={!engineReady}
              onClick={() => setDebugMode(!debugMode)}
              type="button"
            >
              <Bug size={16} />
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
          <button
            className="btn btn-sm btn-outline-light d-flex align-items-center gap-2"
            disabled={!engineReady}
            onClick={handleSave}
            type="button"
          >
            <FloppyFill size={13} />
            <span style={{ fontSize: 14 }}>{t('Save')}</span>
          </button>
          <AppTooltip
            content={hasSavedOnce
              ? `${autoSaveEnabled ? t('Disable auto-save') : t('Enable auto-save')} (5 min)`
              : `${t('Save first')} (5 min)`}
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
              <span style={{ fontSize: 14 }}>{autoSaveEnabled ? 'Auto (on)' : 'Auto (off)'}</span>
            </button>
          </AppTooltip>
        </div>
      </div>
    </div>
  );
}

export default TopBarEngine;