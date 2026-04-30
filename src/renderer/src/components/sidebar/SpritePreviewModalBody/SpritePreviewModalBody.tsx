import { useState, useCallback, useRef, useEffect } from 'react';
import { SpritePreviewLeftPanel } from './SpritePreviewLeftPanel';
import { SpritePreviewCanvas } from './SpritePreviewCanvas';
import { SpritePreviewRightPanel } from './SpritePreviewRightPanel';

const CANVAS_SIZE = 500;
export type SelectionMode = 'cell' | 'box';

export function SpritePreviewModalBody({ src }: { src: string }) {
  const [cellOffsetX, setCellOffsetX] = useState(0);
  const [cellOffsetY, setCellOffsetY] = useState(0);
  const [gridSize, setGridSize] = useState(32);
  const [selectionMode, setSelectionMode] = useState<SelectionMode>('cell');
  const [selectedCells, setSelectedCells] = useState<{ x: number, y: number }[]>([]);
  const [boxes, setBoxes] = useState<{ x: number, y: number, width: number, height: number }[]>([]);
  const [currentBox, setCurrentBox] = useState({ x: 0, y: 0, width: 64, height: 64 });

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'z') {
        e.preventDefault();
        setBoxes(prev => prev.slice(0, -1));
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  const handleRemoveBox = useCallback((index: number) => {
    setBoxes(prev => prev.filter((_, i) => i !== index));
  }, []);

  const handleCanvasClick = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    if (selectionMode === 'cell') {
      const rect = e.currentTarget.getBoundingClientRect();
      const x = Math.floor((e.clientX - rect.left + cellOffsetX) / gridSize);
      const y = Math.floor((e.clientY - rect.top + cellOffsetY) / gridSize);

      setSelectedCells(prev => {
        const exists = prev.some(cell => cell.x === x && cell.y === y);
        return exists
          ? prev.filter(cell => !(cell.x === x && cell.y === y))
          : [...prev, { x, y }];
      });
    } else if (selectionMode === 'box') {
      setBoxes(prev => [...prev, { ...currentBox }]);
    }
  }, [selectionMode, cellOffsetX, cellOffsetY, gridSize, currentBox]);

  const handleMouseMove = useCallback((e: React.MouseEvent<HTMLCanvasElement>) => {
    if (selectionMode !== 'box') return;

    const rect = e.currentTarget.getBoundingClientRect();
    const mouseX = Math.floor(e.clientX - rect.left);
    const mouseY = Math.floor(e.clientY - rect.top);

    setCurrentBox(b => {
      const width = b.width;
      const height = b.height;
      let x = mouseX - width / 2;
      let y = mouseY - height / 2;
      x = Math.max(0, Math.min(x, CANVAS_SIZE - width));
      y = Math.max(0, Math.min(y, CANVAS_SIZE - height));
      return { ...b, x, y };
    });
  }, [selectionMode]);

  const handleBoxChange = useCallback((box: { x: number; y: number; width: number; height: number }) => {
    setCurrentBox(box);
  }, []);

  const handleAddBox = useCallback(() => {
    setBoxes(prev => [...prev, { ...currentBox }]);
  }, [currentBox]);

  return (
    <div data-bs-theme="dark" className="row g-3 p-3 rounded-3" style={{ minHeight: 300 }}>
      <div className="col-3">
        <SpritePreviewLeftPanel
          selectionMode={selectionMode}
          setSelectionMode={setSelectionMode}
          gridSize={gridSize}
          setGridSize={setGridSize}
          cellOffsetX={cellOffsetX}
          setCellOffsetX={setCellOffsetX}
          cellOffsetY={cellOffsetY}
          setCellOffsetY={setCellOffsetY}
          CANVAS_SIZE={CANVAS_SIZE}
          onBoxChange={handleBoxChange}
          onAddBox={handleAddBox}
        />
      </div>

      <div className="col">
        <SpritePreviewCanvas
          src={src}
          selectionMode={selectionMode}
          selectedCells={selectedCells}
          boxes={boxes}
          box={currentBox}
          gridSize={gridSize}
          cellOffsetX={cellOffsetX}
          cellOffsetY={cellOffsetY}
          onCanvasClick={handleCanvasClick}
          onMouseMove={selectionMode === 'box' ? handleMouseMove : undefined}
          CANVAS_SIZE={CANVAS_SIZE}
        />
      </div>

      <div className="col-3">
        <SpritePreviewRightPanel
          src={src}
          selectionMode={selectionMode}
          selectedCells={selectedCells}
          boxes={boxes}
          gridSize={gridSize}
          cellOffsetX={cellOffsetX}
          cellOffsetY={cellOffsetY}
          onRemoveBox={handleRemoveBox}
        />
      </div>
    </div>
  );
}
