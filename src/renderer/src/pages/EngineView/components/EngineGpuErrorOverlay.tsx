import { useState } from 'react';
import { ExclamationTriangleFill } from 'react-bootstrap-icons';

import { useContextEngine } from '@engine';
import { useTraslate } from '@hooks';

export function EngineGpuErrorOverlay() {
  const { engineReady, engineError } = useContextEngine();
  const { t } = useTraslate();
  const [showTechnical, setShowTechnical] = useState(false);

  if (engineReady || !engineError) {
    return null;
  }

  return (
    <div
      className="position-absolute top-0 start-0 w-100 h-100 d-flex align-items-center justify-content-center p-4"
      style={{ zIndex: 20, background: 'rgba(13, 13, 26, 0.92)' }}
      role="alert"
    >
      <div
        className="text-light border border-danger rounded p-4 shadow"
        style={{ maxWidth: 520, background: 'rgba(30, 20, 35, 0.98)' }}
      >
        <div className="d-flex align-items-start gap-3 mb-3">
          <ExclamationTriangleFill className="text-danger flex-shrink-0" size={28} />
          <div>
            <h5 className="mb-2">{t('Engine GPU init failed title')}</h5>
            <p className="mb-0 small text-secondary">
              {t('Engine GPU init failed intro')}
            </p>
          </div>
        </div>

        <ul className="small mb-3 ps-3">
          <li>{t('Engine GPU hint drivers')}</li>
          <li>{t('Engine GPU hint wsl')}</li>
          <li>{t('Engine GPU hint vulkaninfo')}</li>
          <li>{t('Engine GPU hint restart')}</li>
        </ul>

        <button
          type="button"
          className="btn btn-sm btn-outline-secondary"
          onClick={() => setShowTechnical((v) => !v)}
        >
          {showTechnical ? t('Hide technical details') : t('Show technical details')}
        </button>

        {showTechnical && (
          <pre
            className="mt-3 mb-0 p-2 rounded small text-warning-emphasis border border-secondary"
            style={{
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-word',
              maxHeight: 200,
              overflow: 'auto',
              background: 'rgba(0,0,0,0.35)',
            }}
          >
            {engineError}
          </pre>
        )}
      </div>
    </div>
  );
}
