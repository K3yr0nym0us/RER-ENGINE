import { useState, useEffect } from 'react';
import { Accordion } from 'react-bootstrap';
import { Grid3x3, EyeFill, EyeSlashFill, Image, Lock, Unlock } from 'react-bootstrap-icons';

import { AppTooltip } from '@components';
import { useContextEngine } from '@engine';
import { useTraslate } from '@hooks';

export function WorldAccordion() {
  const { t } = useTraslate();
  const { engineReady, worldConfig, backgroundPath, setWorldSize, setGridVisible, setGridCellSize, setGravity, send } = useContextEngine()
  const [widthStr,  setWidthStr]  = useState(String(worldConfig.worldWidth))
  const [heightStr, setHeightStr] = useState(String(worldConfig.worldHeight))
  const [gridCellStr, setGridCellStr] = useState(String(worldConfig.gridCellSize))
  const [gridSizeLocked, setGridSizeLocked] = useState(false)

  useEffect(() => {
    setWidthStr(String(worldConfig.worldWidth))
    setHeightStr(String(worldConfig.worldHeight))
  }, [worldConfig.worldWidth, worldConfig.worldHeight])

  useEffect(() => {
    setGridCellStr(String(worldConfig.gridCellSize))
  }, [worldConfig.gridCellSize])

  const loadBackground = () => {
    window.electronAPI.openBackgroundDialog().then((p: string | null) => {
      if (p) send({ cmd: 'load_background', path: p })
    })
  }

  const commitSize = () => {
    const w = parseFloat(widthStr)
    const h = parseFloat(heightStr)
    if (!isNaN(w) && !isNaN(h) && w > 0 && h > 0) {
      setWorldSize(w, h)
    }
  }

  const handleKey = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') commitSize()
  }

  const commitGridCell = () => {
    const size = parseFloat(gridCellStr)
    if (!isNaN(size) && size > 0) {
      setGridCellSize(size)
    }
  }

  const handleGridCellKey = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') commitGridCell()
  }

  const handleGridCellChange = (rawValue: string) => {
    setGridCellStr(rawValue)
    const size = parseFloat(rawValue)
    if (!isNaN(size) && size > 0) {
      setGridCellSize(size)
    }
  }

  return (
    <Accordion.Item eventKey="mundo">
      <Accordion.Header>{t('World')}</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <p className="text-secondary small mb-1 fw-semibold">{t('Workspace')}</p>
        <div className="d-flex gap-1 mb-2">
          <div className="flex-fill">
            <label className="form-label small text-secondary mb-0" htmlFor="world-width">{t('Width (u)')}</label>
            <input id="world-width" type="number" className="form-control form-control-sm bg-dark text-light border-secondary" min={1} step={1} value={widthStr} disabled={!engineReady} onChange={(e) => setWidthStr(e.target.value)} onBlur={commitSize} onKeyDown={handleKey} />
          </div>
          <div className="flex-fill">
            <label className="form-label small text-secondary mb-0" htmlFor="world-height">{t('Height (u)')}</label>
            <input id="world-height" type="number" className="form-control form-control-sm bg-dark text-light border-secondary" min={1} step={1} value={heightStr} disabled={!engineReady} onChange={(e) => setHeightStr(e.target.value)} onBlur={commitSize} onKeyDown={handleKey} />
          </div>
        </div>

        <hr className="border-secondary my-2" />

        <p className="text-secondary small mb-1 fw-semibold d-flex align-items-center gap-1">
          <Image /> {t('World background')}
        </p>
        <button
          className="btn btn-outline-info btn-sm w-100 fw-bold mb-1"
          disabled={!engineReady}
          onClick={loadBackground}
        >
          {backgroundPath ? t('Change background') : t('+ Load background (PNG/GIF)')}
        </button>
        {backgroundPath && (
          <AppTooltip content={backgroundPath} place="top">
            <p className="text-secondary small text-truncate mb-0 px-1">
              {backgroundPath.split('/').pop()}
            </p>
          </AppTooltip>
        )}

        <hr className="border-secondary my-2" />

        <div className="d-flex align-items-center justify-content-between mb-2">
          <span className="small fw-semibold text-secondary d-flex align-items-center gap-1">
            <Grid3x3 /> {t('Grid')}
          </span>
          <AppTooltip content={worldConfig.gridVisible ? t('Hide grid') : t('Show grid')} place="top">
            <button
              className={`btn btn-sm ${worldConfig.gridVisible ? 'btn-info' : 'btn-outline-secondary'}`}
              disabled={!engineReady}
              onClick={() => setGridVisible(!worldConfig.gridVisible)}
            >
              {worldConfig.gridVisible ? <EyeFill /> : <EyeSlashFill />}
            </button>
          </AppTooltip>
        </div>

        <div className="form-label small text-secondary mb-1 d-flex align-items-center justify-content-between gap-2">
          <span>{t('Cell size')}</span>
          <div className="d-flex align-items-center gap-2">
            <span className="text-info fw-bold">{worldConfig.gridCellSize.toFixed(2)} u</span>
            <input
              id="grid-cell-size-number"
              type="number"
              className="form-control form-control-sm bg-dark text-light border-secondary"
              style={{ width: 79 }}
              min={0.05}
              step={0.01}
              value={gridCellStr}
              disabled={!engineReady || gridSizeLocked}
              onChange={(e) => handleGridCellChange(e.target.value)}
              onBlur={commitGridCell}
              onKeyDown={handleGridCellKey}
              aria-label={t('Exact cell size')}
            />
            <AppTooltip content={gridSizeLocked ? t('Unlock grid size') : t('Lock grid size')} place="top">
              <button
                type="button"
                className={`btn btn-sm ${gridSizeLocked ? 'btn-info' : 'btn-outline-secondary'}`}
                onClick={() => setGridSizeLocked((v) => !v)}
                aria-pressed={gridSizeLocked}
                disabled={!engineReady}
              >
                {gridSizeLocked ? <Lock size={13} /> : <Unlock size={13} />}
              </button>
            </AppTooltip>
          </div>
        </div>
        <div className="mb-2">
          <input
            id="grid-cell-size-range"
            type="range"
            className="form-range mb-0"
            min={0.25} max={10} step={0.25}
            value={worldConfig.gridCellSize}
            disabled={!engineReady || gridSizeLocked}
            onChange={(e) => {
              setGridCellStr(e.target.value)
              setGridCellSize(parseFloat(e.target.value))
            }}
          />
        </div>

        <hr className="border-secondary my-2" />

        <label className="form-label small text-secondary mb-1 d-flex justify-content-between" htmlFor="gravity-range">
          <span>{t('Gravity')}</span>
          <span className="text-info fw-bold">{(worldConfig.gravity ?? 9.81).toFixed(2)} u/s²</span>
        </label>
        <input
          id="gravity-range"
          type="range"
          className="form-range mb-2"
          min={0} max={50} step={0.5}
          value={worldConfig.gravity ?? 9.81}
          disabled={!engineReady}
          onChange={(e) => setGravity(parseFloat(e.target.value))}
        />
      </Accordion.Body>
    </Accordion.Item>
  )
}

export default WorldAccordion