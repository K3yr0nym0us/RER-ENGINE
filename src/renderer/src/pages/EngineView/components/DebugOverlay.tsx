import { useContextEngine } from '@engine'

/**
 * HUD de métricas de rendimiento del motor.
 * Se posiciona sobre el viewport (position: absolute, esquina superior-derecha).
 * Solo se monta cuando el overlay está habilitado.
 */
export function DebugOverlay() {
  const { debugMetrics, engineReady } = useContextEngine()

  if (!engineReady || !debugMetrics) return null

  const { fps, frame_time_ms, draw_calls, physics_bodies } = debugMetrics

  return (
    <div
      style={{
        position: 'absolute',
        top: 8,
        right: 8,
        zIndex: 10,
        background: 'rgba(0, 0, 0, 0.55)',
        backdropFilter: 'blur(4px)',
        border: '1px solid rgba(255,255,255,0.1)',
        borderRadius: 6,
        padding: '5px 10px',
        fontFamily: 'monospace',
        fontSize: 12,
        lineHeight: 1.7,
        color: '#e2e8f0',
        pointerEvents: 'none',
        userSelect: 'none',
        minWidth: 130,
      }}
    >
      <div style={{ color: fps >= 55 ? '#4ade80' : fps >= 30 ? '#facc15' : '#f87171' }}>
        FPS: <strong>{fps.toFixed(1)}</strong>
      </div>
      <div>
        Frame: <strong>{frame_time_ms.toFixed(2)} ms</strong>
      </div>
      <div>
        Draw calls: <strong>{draw_calls}</strong>
      </div>
      <div>
        Bodies: <strong>{physics_bodies}</strong>
      </div>
    </div>
  )
}

export default DebugOverlay
