import { useState, useEffect } from 'react';
import { Accordion } from 'react-bootstrap';
import { Globe2, Grid3x3, EyeFill, EyeSlashFill, Image, Lock, Unlock, CheckLg } from 'react-bootstrap-icons';

import { AppTooltip } from '@components';
import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';

export function WorldAccordion() {
  const { t } = useTraslate();
  const { engineReady, worldConfig, backgroundPath, backgrounds, setBackground, setWorldSize, setGridVisible, setGridCellSize, setGravity } = useContextEngine()
  const { openModal, closeModal } = useModal();
  const [widthStr,  setWidthStr]  = useState(String(worldConfig.worldWidth))
  const [heightStr, setHeightStr] = useState(String(worldConfig.worldHeight))
  const [gridCellStr, setGridCellStr] = useState(String(worldConfig.gridCellSize))
  const [gridSizeLocked, setGridSizeLocked] = useState(false)
  const [selectedBg, setSelectedBg] = useState(backgroundPath ?? '')

  useEffect(() => {
    setWidthStr(String(worldConfig.worldWidth))
    setHeightStr(String(worldConfig.worldHeight))
  }, [worldConfig.worldWidth, worldConfig.worldHeight])

  useEffect(() => {
    setGridCellStr(String(worldConfig.gridCellSize))
  }, [worldConfig.gridCellSize])

  useEffect(() => {
    setSelectedBg(backgroundPath ?? '')
  }, [backgroundPath])

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

  const handleApplyBackground = () => {
    if (!selectedBg) return;
    const selectedBackground = backgrounds.find((bg) => bg.path === selectedBg);
    openModal({
      title: t('Apply Background'),
      body: (
        <div className="text-center">
          <p>{t('Apply selected background to current scene?')}</p>
          <p><strong>{selectedBackground?.name ?? selectedBg}</strong></p>
          <div className="d-flex justify-content-end gap-2 mt-3">
            <button className="btn btn-secondary" onClick={() => closeModal()}>
              {t('Cancel')}
            </button>
            <button
              className="btn btn-primary"
              onClick={() => {
                setBackground(selectedBg);
                closeModal();
              }}
            >
              {t('Apply')}
            </button>
          </div>
        </div>
      ),
    });
  }

  return (
    <Accordion.Item eventKey="mundo">
      <Accordion.Header><Globe2 className="me-2" />{t('World')}</Accordion.Header>
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
        <div className="d-flex gap-1 mb-2">
          <select
            className="form-select form-select-sm bg-dark text-light border-secondary flex-fill"
            value={selectedBg}
            disabled={!engineReady || backgrounds.length === 0}
            onChange={(e) => setSelectedBg(e.target.value)}
          >
            {backgrounds.length === 0 && (
              <option value="">{t('No backgrounds loaded')}</option>
            )}
            {backgrounds.length > 0 && (
              <option value="">{t('— Select background —')}</option>
            )}
            {backgrounds.map((bg) => (
              <option key={bg.path} value={bg.path}>{bg.name}</option>
            ))}
          </select>
          <AppTooltip content={t('Apply background')} place="top">
            <button
              className="btn btn-sm btn-outline-info"
              disabled={!engineReady || !selectedBg}
              onClick={handleApplyBackground}
            >
              <CheckLg />
            </button>
          </AppTooltip>
        </div>

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
            <input
              id="grid-cell-size-number"
              type="number"
              className="form-control form-control-sm bg-dark text-light border-secondary"
              style={{ width: 55 }}
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