import { useEffect, useState } from 'react';

import { useContextEngine } from '@engine';
import { useTraslate } from '@hooks';

import type { AppResourceUsage } from '@shared-types';

const RESOURCE_POLL_MS = 3000;
const LINE_STYLE: React.CSSProperties = {
  fontFamily: 'monospace',
  fontSize: 12,
  lineHeight: 1.55,
};

function formatPercent(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return '—';
  return `${value.toFixed(1)}%`;
}

function combinedAppCpuPercent(
  electron: AppResourceUsage | null,
  engineCpuPercent?: number,
): number | null {
  if (!electron && engineCpuPercent == null) return null;
  return (electron?.electronCpuPercent ?? 0) + (engineCpuPercent ?? 0);
}

function combinedAppGpuPercent(
  electron: AppResourceUsage | null,
  engineGpuPercent?: number,
): number | null {
  const electronGpu = electron?.electronGpuPercent;
  if (electronGpu == null && engineGpuPercent == null) return null;
  return Math.min(100, (electronGpu ?? 0) + (engineGpuPercent ?? 0));
}

function MetricLine({
  label,
  value,
  valueStyle,
}: {
  label: string;
  value: string;
  valueStyle?: React.CSSProperties;
}) {
  return (
    <div className="d-flex gap-1" style={LINE_STYLE}>
      <span className="text-secondary">{label}</span>
      <span style={valueStyle}>{value}</span>
    </div>
  );
}

export function MetricsPanel() {
  const { debugMetrics } = useContextEngine();
  const { t } = useTraslate();
  const [resourceUsage, setResourceUsage] = useState<AppResourceUsage | null>(null);

  useEffect(() => {
    let cancelled = false;

    const poll = async () => {
      try {
        const usage = await window.electronAPI.getAppResourceUsage();
        if (!cancelled) setResourceUsage(usage);
      } catch {
        if (!cancelled) setResourceUsage(null);
      }
    };

    void poll();
    const intervalId = window.setInterval(() => void poll(), RESOURCE_POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(intervalId);
    };
  }, []);

  const fpsColor =
    debugMetrics == null
      ? undefined
      : debugMetrics.fps >= 55
        ? '#4ade80'
        : debugMetrics.fps >= 30
          ? '#facc15'
          : '#f87171';

  const cpuPercent = combinedAppCpuPercent(resourceUsage, debugMetrics?.cpu_percent);
  const gpuPercent = combinedAppGpuPercent(resourceUsage, debugMetrics?.gpu_percent);

  return (
    <div className="metrics-panel d-flex flex-column px-2 py-2">
      <span
        className="fw-bold text-secondary d-block text-center w-100"
        style={{ fontSize: 13, letterSpacing: '0.04em' }}
      >
        {t('Usage metrics')}
      </span>

      <hr className="my-2" style={{ borderColor: '#202545', opacity: 1 }} />

      {debugMetrics ? (
        <div className="row g-1">
          <div className="col-5">
            <MetricLine label="FPS" value={debugMetrics.fps.toFixed(1)} valueStyle={{ color: fpsColor }} />
          </div>
          <div className="col-7">
            <MetricLine label="Frame Time" value={`${debugMetrics.frame_time_ms.toFixed(1)} ms`} />
          </div>
          <div className="col-5">
            <MetricLine label="Draw Calls" value={String(debugMetrics.draw_calls)} />
          </div>
          <div className="col-7">
            <MetricLine label="Physics Bodies" value={String(debugMetrics.physics_bodies)} />
          </div>
          <div className="col-5">
            <MetricLine label="CPU" value={formatPercent(cpuPercent)} />
          </div>
          <div className="col-7">
            <MetricLine label="GPU" value={formatPercent(gpuPercent)} />
          </div>
        </div>
      ) : (
        <span className="text-secondary text-center d-block mb-1" style={{ fontSize: 12 }}>
          {t('Waiting for metrics...')}
        </span>
      )}
    </div>
  );
}

export default MetricsPanel;
