import { useState } from 'react';
import { Activity } from 'react-bootstrap-icons';

import { useContextEngine } from '@engine';
import { useTraslate } from '@hooks';

export function MetricsPanel() {
  const { engineReady, debugMetrics } = useContextEngine();
  const { t } = useTraslate();
  const [metricsVisible, setMetricsVisible] = useState(true);

  return (
    <div className="metrics-panel d-flex flex-column h-100 px-3 py-2">
      <div className="d-flex align-items-center justify-content-between mb-2">
        <span className="text-secondary" style={{ fontSize: 12, letterSpacing: '0.04em' }}>
          {t('Metrics')}
        </span>
        <button
          className={`btn btn-sm d-flex align-items-center gap-1 ${
            metricsVisible ? 'btn-info text-dark' : 'btn-outline-secondary'
          }`}
          onClick={() => setMetricsVisible((v) => !v)}
          disabled={!engineReady}
          type="button"
        >
          <Activity size={13} />
          <span>{metricsVisible ? t('Disable metrics') : t('Enable metrics')}</span>
        </button>
      </div>

      {metricsVisible && debugMetrics && (
        <div className="row g-1">
          <div className="col-6">
            <div className="text-secondary" style={{ fontSize: 10 }}>Frames Per Second</div>
            <div style={{ fontFamily: 'monospace', fontSize: 12, color: debugMetrics.fps >= 55 ? '#4ade80' : debugMetrics.fps >= 30 ? '#facc15' : '#f87171' }}>
              {debugMetrics.fps.toFixed(1)}
            </div>
          </div>
          <div className="col-6">
            <div className="text-secondary" style={{ fontSize: 10 }}>Frame Time (ms)</div>
            <div style={{ fontFamily: 'monospace', fontSize: 12 }}>{debugMetrics.frame_time_ms.toFixed(1)}</div>
          </div>
          <div className="col-6">
            <div className="text-secondary" style={{ fontSize: 10 }}>Draw Calls</div>
            <div style={{ fontFamily: 'monospace', fontSize: 12 }}>{debugMetrics.draw_calls}</div>
          </div>
          <div className="col-6">
            <div className="text-secondary" style={{ fontSize: 10 }}>Physics Bodies</div>
            <div style={{ fontFamily: 'monospace', fontSize: 12 }}>{debugMetrics.physics_bodies}</div>
          </div>
        </div>
      )}

      {(!metricsVisible || !debugMetrics) && (
        <span className="text-secondary" style={{ fontSize: 12 }}>
          {metricsVisible ? t('Waiting for metrics...') : t('Metrics hidden')}
        </span>
      )}
    </div>
  );
}

export default MetricsPanel;
