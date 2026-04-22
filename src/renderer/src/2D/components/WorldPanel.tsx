import { useState, useEffect } from 'react'
import { Accordion } from 'react-bootstrap'
import { Grid3x3, EyeFill, EyeSlashFill } from 'react-bootstrap-icons'
import type { WorldConfig } from '../../hooks/useEngine'

interface Props {
  engineReady:   boolean
  worldConfig:   WorldConfig
  onWorldSize:   (w: number, h: number) => void
  onGridVisible: (v: boolean)           => void
  onCellSize:    (s: number)            => void
}

export function WorldPanel({
  engineReady, worldConfig,
  onWorldSize, onGridVisible, onCellSize,
}: Props) {
  // Estado local para los inputs de tamaño (evitar enviar en cada tecla)
  const [widthStr,  setWidthStr]  = useState(String(worldConfig.worldWidth))
  const [heightStr, setHeightStr] = useState(String(worldConfig.worldHeight))

  // Sincronizar inputs cuando worldConfig cambia desde fuera (ej: carga de proyecto)
  useEffect(() => {
    setWidthStr(String(worldConfig.worldWidth))
    setHeightStr(String(worldConfig.worldHeight))
  }, [worldConfig.worldWidth, worldConfig.worldHeight])

  const commitSize = () => {
    const w = parseFloat(widthStr)
    const h = parseFloat(heightStr)
    if (!isNaN(w) && !isNaN(h) && w > 0 && h > 0) {
      onWorldSize(w, h)
    }
  }

  const handleKey = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') commitSize()
  }

  return (
    <Accordion.Item eventKey="mundo">
      <Accordion.Header>Mundo</Accordion.Header>
      <Accordion.Body className="py-2 px-2">

        {/* Tamaño del mundo */}
        <p className="text-secondary small mb-1 fw-semibold">Área de trabajo</p>
        <div className="d-flex gap-1 mb-2">
          <div className="flex-fill">
            <label className="form-label small text-secondary mb-0">Ancho (u)</label>
            <input
              type="number"
              className="form-control form-control-sm bg-dark text-light border-secondary"
              min={1} step={1}
              value={widthStr}
              disabled={!engineReady}
              onChange={(e) => setWidthStr(e.target.value)}
              onBlur={commitSize}
              onKeyDown={handleKey}
            />
          </div>
          <div className="flex-fill">
            <label className="form-label small text-secondary mb-0">Alto (u)</label>
            <input
              type="number"
              className="form-control form-control-sm bg-dark text-light border-secondary"
              min={1} step={1}
              value={heightStr}
              disabled={!engineReady}
              onChange={(e) => setHeightStr(e.target.value)}
              onBlur={commitSize}
              onKeyDown={handleKey}
            />
          </div>
        </div>

        <hr className="border-secondary my-2" />

        {/* Cuadrícula */}
        <div className="d-flex align-items-center justify-content-between mb-2">
          <span className="small fw-semibold text-secondary d-flex align-items-center gap-1">
            <Grid3x3 /> Cuadrícula
          </span>
          <button
            className={`btn btn-sm ${worldConfig.gridVisible ? 'btn-info' : 'btn-outline-secondary'}`}
            title={worldConfig.gridVisible ? 'Ocultar cuadrícula' : 'Mostrar cuadrícula'}
            disabled={!engineReady}
            onClick={() => onGridVisible(!worldConfig.gridVisible)}
          >
            {worldConfig.gridVisible ? <EyeFill /> : <EyeSlashFill />}
          </button>
        </div>

        {/* Tamaño de celda */}
        <label className="form-label small text-secondary mb-1 d-flex justify-content-between">
          <span>Tamaño de celda</span>
          <span className="text-info fw-bold">{worldConfig.gridCellSize.toFixed(2)} u</span>
        </label>
        <input
          type="range"
          className="form-range mb-2"
          min={0.25} max={10} step={0.25}
          value={worldConfig.gridCellSize}
          disabled={!engineReady}
          onChange={(e) => onCellSize(parseFloat(e.target.value))}
        />

      </Accordion.Body>
    </Accordion.Item>
  )
}
