import { useState, useCallback } from 'react';
import { SelectionMode, type ScriptEntry } from './';
import { Link, Unlock, MusicNoteBeamed, FileEarmarkCode, PencilSquare, SkipEndFill, Trash } from 'react-bootstrap-icons';
import { AppTooltip } from '@components';
import { useTraslate } from '@hooks';
import type { SoundInfo } from '@shared-types';

interface SpritePreviewLeftPanelProps {
  selectionMode: SelectionMode;
  setSelectionMode: (mode: SelectionMode) => void;
  gridSize: number;
  setGridSize: (size: number) => void;
  cellOffsetX: number;
  setCellOffsetX: (offset: number) => void;
  cellOffsetY: number;
  setCellOffsetY: (offset: number) => void;
  CANVAS_SIZE: number;
  onBoxChange: (box: { x: number; y: number; width: number; height: number }) => void;
  sounds: SoundInfo[];
  audioPath?: string;
  onAudioChange: (path: string) => void;
  scripts: ScriptEntry[];
  onAddScript: () => void;
  onEditScript: (name: string) => void;
  onRemoveScript: (name: string) => void;
  isCancelable: boolean;
  onIsCancelableChange: (value: boolean) => void;
}

const DEFAULT_BOX = { x: 0, y: 0, width: 64, height: 64 };

export function SpritePreviewLeftPanel({
  selectionMode,
  setSelectionMode,
  gridSize,
  setGridSize,
  cellOffsetX,
  setCellOffsetX,
  cellOffsetY,
  setCellOffsetY,
  CANVAS_SIZE,
  onBoxChange,
  sounds,
  audioPath,
  onAudioChange,
  scripts,
  onAddScript,
  onEditScript,
  onRemoveScript,
  isCancelable,
  onIsCancelableChange,
}: SpritePreviewLeftPanelProps) {
  const [box, setBox] = useState(DEFAULT_BOX);
  const [keepAspect, setKeepAspect] = useState(true);
  const { t } = useTraslate();

  const handleBoxWidthChange = useCallback((width: number) => {
    const newHeight = keepAspect ? width : box.height;
    const updated = { ...box, width, height: newHeight };
    setBox(updated);
    onBoxChange(updated);
  }, [keepAspect, onBoxChange, box]);

  const handleBoxHeightChange = useCallback((height: number) => {
    const newWidth = keepAspect ? height : box.width;
    const updated = { ...box, width: newWidth, height };
    setBox(updated);
    onBoxChange(updated);
  }, [keepAspect, onBoxChange, box]);

  return (
    <div className="bg-dark text-light border border-secondary rounded p-3 h-100">
      <h5 className="text-light text-center mb-3">{t('Properties')}</h5>
      <hr className="border-secondary opacity-50 mb-3" />

      <div className="mb-3">
        <label className="text-light fw-bold d-block mb-2" id="label-modo-seleccion" htmlFor="mode-cell">{t('Selection mode')}</label>
        <div className="d-flex gap-4 justify-content-center">
          <div className="form-check d-flex align-items-center gap-1">
            <input
              className="form-check-input"
              type="radio"
              checked={selectionMode === 'cell'}
              onChange={() => setSelectionMode('cell')}
              id="mode-cell"
              aria-labelledby="label-modo-seleccion mode-cell-label"
            />
            <label className="form-check-label" htmlFor="mode-cell" id="mode-cell-label">
              {t('Cells')}
            </label>
          </div>
          <div className="form-check d-flex align-items-center gap-1">
            <input
              className="form-check-input"
              type="radio"
              checked={selectionMode === 'box'}
              onChange={() => setSelectionMode('box')}
              id="mode-box"
              aria-labelledby="label-modo-seleccion mode-box-label"
            />
            <label className="form-check-label" htmlFor="mode-box" id="mode-box-label">
              {t('Box')}
            </label>
          </div>
        </div>
      </div>

      {selectionMode === 'cell' && (
        <div className="mb-3">
            <label className="text-light fw-bold d-block mb-2" htmlFor="grid-size">{t('Cell size')}</label>
          <div className="d-flex align-items-center gap-2 mb-3">
            <input
              id="grid-size"
              type="range"
              className="form-range flex-fill"
              min={8}
              max={CANVAS_SIZE}
              step={1}
              value={gridSize}
              onChange={e => setGridSize(Number(e.target.value))}
            />
            <input
              id="grid-size-number"
              type="number"
              className="form-control form-control-sm bg-dark text-light border-secondary"
              style={{ width: 70 }}
              min={8}
              max={CANVAS_SIZE}
              step={1}
              value={gridSize}
              onChange={e => setGridSize(Number(e.target.value))}
            />
            <span className="text-secondary small">px</span>
          </div>

          <label className="text-light fw-bold d-block mb-2" htmlFor="offset-x-range">{t('Move grid')}</label>

          <div className="mb-2">
            <div className="d-flex align-items-center gap-2">
              <span className="text-secondary small fw-bold" style={{ width: 20 }}>X</span>
              <input
                id="offset-x-range"
                type="range"
                className="form-range flex-fill"
                min={-CANVAS_SIZE}
                max={CANVAS_SIZE}
                step={1}
                value={cellOffsetX}
                onChange={e => setCellOffsetX(Number(e.target.value))}
                aria-label={t('Move grid X')}
              />
              <input
                id="offset-x-number"
                type="number"
                className="form-control form-control-sm bg-dark text-light border-secondary"
                style={{ width: 70 }}
                min={-CANVAS_SIZE}
                max={CANVAS_SIZE}
                step={1}
                value={cellOffsetX}
                onChange={e => setCellOffsetX(Number(e.target.value))}
                aria-label={t('Move grid X (number)')}
              />
            </div>
          </div>

          <div className="mb-2">
            <div className="d-flex align-items-center gap-2">
              <span className="text-secondary small fw-bold" style={{ width: 20 }}>Y</span>
              <input
                type="range"
                className="form-range flex-fill"
                min={-CANVAS_SIZE}
                max={CANVAS_SIZE}
                step={1}
                value={cellOffsetY}
                onChange={e => setCellOffsetY(Number(e.target.value))}
              />
              <input
                type="number"
                className="form-control form-control-sm bg-dark text-light border-secondary"
                style={{ width: 70 }}
                min={-CANVAS_SIZE}
                max={CANVAS_SIZE}
                step={1}
                value={cellOffsetY}
                onChange={e => setCellOffsetY(Number(e.target.value))}
              />
            </div>
          </div>
        </div>
      )}

      {selectionMode === 'box' && (
        <div className="mb-3">
          <label className="text-light fw-bold d-block mb-2" htmlFor="box-width-range">{t('Box size')}</label>

          <div className="mb-2">
            <div className="d-flex align-items-center gap-2">
              <span className="text-secondary small fw-bold" style={{ width: 20 }}>W</span>
              <input
                id="box-width-range"
                type="range"
                className="form-range flex-fill"
                min={8}
                max={CANVAS_SIZE}
                step={1}
                value={box.width}
                onChange={e => handleBoxWidthChange(Number(e.target.value))}
                aria-label={t('Box size width')}
              />
              <input
                id="box-width-number"
                type="number"
                className="form-control form-control-sm bg-dark text-light border-secondary"
                style={{ width: 70 }}
                min={8}
                max={CANVAS_SIZE}
                step={1}
                value={box.width}
                onChange={e => handleBoxWidthChange(Number(e.target.value))}
                aria-label={t('Box size width (number)')}
              />
              <span className="text-secondary small">px</span>
            </div>
          </div>

          <div className="mb-2">
            <div className="d-flex align-items-center gap-2">
              <span className="text-secondary small fw-bold" style={{ width: 20 }}>H</span>
              <input
                type="range"
                className="form-range flex-fill"
                min={8}
                max={CANVAS_SIZE}
                step={1}
                value={box.height}
                onChange={e => handleBoxHeightChange(Number(e.target.value))}
                disabled={keepAspect}
              />
              <input
                type="number"
                className="form-control form-control-sm bg-dark text-light border-secondary"
                style={{ width: 70 }}
                min={8}
                max={CANVAS_SIZE}
                step={1}
                value={box.height}
                onChange={e => handleBoxHeightChange(Number(e.target.value))}
                disabled={keepAspect}
              />
              <span className="text-secondary small">px</span>
            </div>
          </div>

          <button
            onClick={() => setKeepAspect(!keepAspect)}
            className={`btn w-100 ${keepAspect ? 'btn-primary' : 'btn-outline-secondary'}`}
            type="button"
          >
            {keepAspect ? <><Link className="me-1" /> {t('Fixed proportion')}</> : <><Unlock className="me-1" /> {t('Free proportion')}</>}
          </button>
        </div>
      )}

      <hr className="border-secondary opacity-50 mb-3" />

      <div className="mb-3">
        <label className="text-light fw-bold d-block mb-2">{t('Animation audio')}</label>
        <div className="d-flex align-items-center gap-2">
          <MusicNoteBeamed className="text-secondary" />
          <select
            className="form-select form-select-sm bg-dark text-light border-secondary"
            value={audioPath ?? ''}
            onChange={(e) => onAudioChange(e.target.value)}
          >
            <option value="">{t('-- No audio --')}</option>
            {audioPath && !sounds.some((s) => s.path === audioPath) && (
              <option value={audioPath}>{audioPath.split(/[\\/]/).pop() ?? audioPath}</option>
            )}
            {sounds.map((sound) => (
              <option key={sound.path} value={sound.path}>{sound.name}</option>
            ))}
          </select>
        </div>
      </div>

      <hr className="border-secondary opacity-50 mb-3" />

      <div className="mb-3">
        <label className="text-light fw-bold d-block mb-2">{t('Lua Scripts')}</label>

        {scripts.length > 0 && (
          <div className="d-flex flex-column gap-1 mb-2">
            {scripts.map((s) => (
              <div key={s.name} className="d-flex align-items-center gap-1">
                <AppTooltip content={s.name} place="top">
                  <span className="text-info small text-truncate flex-fill" style={{ maxWidth: 110 }}>
                    <FileEarmarkCode className="me-1" />{s.name}
                  </span>
                </AppTooltip>
                <AppTooltip content={t('Edit script')} place="top">
                  <button className="btn btn-sm p-0 px-1 btn-outline-warning" type="button" onClick={() => onEditScript(s.name)}>
                    <PencilSquare size={12} />
                  </button>
                </AppTooltip>
                <AppTooltip content={t('Delete script')} place="top">
                  <button className="btn btn-sm p-0 px-1 btn-outline-danger" type="button" onClick={() => onRemoveScript(s.name)}>
                    <Trash size={12} />
                  </button>
                </AppTooltip>
              </div>
            ))}
          </div>
        )}

        <AppTooltip content={t('Add Lua script to this animation')} place="top">
          <button
            className="btn btn-outline-secondary btn-sm w-100"
            type="button"
            onClick={onAddScript}
          >
            <FileEarmarkCode className="me-1" /> {t('Add script')}
          </button>
        </AppTooltip>
      </div>

      <hr className="border-secondary opacity-50 mb-3" />

      <div className="mb-2 d-flex justify-content-center">
        <AppTooltip
          content={
            <>
              <strong>{t('Cancelable by other animations')}</strong><br />
              {t('Cancelable tooltip')}
            </>
          }
          place="right"
        >
          <div className="form-check d-flex align-items-center gap-2" style={{ cursor: 'pointer' }}>
            <input
              className="form-check-input"
              type="checkbox"
              id="is-cancelable-check"
              checked={isCancelable}
              onChange={e => onIsCancelableChange(e.target.checked)}
            />
            <label className="form-check-label d-flex align-items-center gap-1" htmlFor="is-cancelable-check" style={{ cursor: 'pointer' }}>
              <SkipEndFill size={13} className="text-warning" />
              <span className="text-light small">{t('Cancelable')}</span>
            </label>
          </div>
        </AppTooltip>
      </div>

    </div>
  );
}
