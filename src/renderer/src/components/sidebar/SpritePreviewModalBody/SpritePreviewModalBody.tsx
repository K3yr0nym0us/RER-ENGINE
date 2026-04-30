import { useRef, useState, useEffect } from 'react';

import { SpritePreviewLeftPanel } from './SpritePreviewLeftPanel';
import { SpritePreviewCanvas } from './SpritePreviewCanvas';
import { SpritePreviewRightPanel } from './SpritePreviewRightPanel';

const CANVAS_SIZE = 500;
const DEFAULT_BOX = { x: 0, y: 0, width: 64, height: 64 };
export type SelectionMode = 'cell' | 'box';

export function SpritePreviewModalBody({ src }: { src: string }) {
  const [cellOffsetX, setCellOffsetX] = useState(0);
  const [cellOffsetY, setCellOffsetY] = useState(0);
  const [gridSize, setGridSize] = useState(32);
  const [selectionMode, setSelectionMode] = useState<SelectionMode>('cell');
  const [selectedCells, setSelectedCells] = useState<{ x: number, y: number }[]>([]);
  const [box, setBox] = useState(DEFAULT_BOX);
  const [keepAspect, setKeepAspect] = useState(true);
  const [boxes, setBoxes] = useState<{ x: number, y: number, width: number, height: number }[]>([]);
  const canvasRef = useRef<HTMLCanvasElement>(null);

  // --- Funciones de interacción ---
  const handleCanvasClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (selectionMode === 'cell') {
      const rect = e.currentTarget.getBoundingClientRect();

      // ✅ FIX: offset correcto
      const x = Math.floor((e.clientX - rect.left + cellOffsetX) / gridSize);
      const y = Math.floor((e.clientY - rect.top + cellOffsetY) / gridSize);

      const exists = selectedCells.some(cell => cell.x === x && cell.y === y);
      setSelectedCells(exists
        ? selectedCells.filter(cell => !(cell.x === x && cell.y === y))
        : [...selectedCells, { x, y }]
      );
    } else if (selectionMode === 'box') {
      setBoxes(prev => [...prev, { ...box }]);
    }
  };

  const handleMouseMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (selectionMode !== 'box') return;
    if (box != DEFAULT_BOX) {
      setBox(DEFAULT_BOX);
    }
    const rect = e.currentTarget.getBoundingClientRect();
    const mouseX = Math.floor(e.clientX - rect.left);
    const mouseY = Math.floor(e.clientY - rect.top);
    setBox(b => {
      let width = b.width;
      let height = b.height;
      if (keepAspect) height = width;
      let x = mouseX - width / 2;
      let y = mouseY - height / 2;
      x = Math.max(0, Math.min(x, CANVAS_SIZE - width));
      y = Math.max(0, Math.min(y, CANVAS_SIZE - height));
      return { ...b, x, y, width, height };
    });
  };

  const handleMouseLeave = () => {
    if (selectionMode === 'box') {
      const canvas = canvasRef.current;
      if (canvas) {
        const ctx = canvas.getContext('2d');
        if (ctx) {
          ctx.clearRect(0, 0, CANVAS_SIZE, CANVAS_SIZE);
          const img = new window.Image();
          img.src = src;
          img.onload = () => {
            const scale = Math.min(CANVAS_SIZE / img.width, CANVAS_SIZE / img.height);
            const drawWidth = img.width * scale;
            const drawHeight = img.height * scale;
            const offsetX = (CANVAS_SIZE - drawWidth) / 2;
            const offsetY = (CANVAS_SIZE - drawHeight) / 2;
            ctx.drawImage(img, offsetX, offsetY, drawWidth, drawHeight);
            ctx.strokeStyle = 'rgba(0,200,255,0.8)';
            ctx.lineWidth = 2;
            setBox({ x: 0, y: 0, width: 0, height: 0 });
            boxes.forEach(b =>
              ctx.strokeRect(b.x, b.y, b.width, b.height)
            );
          };
        }
      }
    }
  };

  const handleBoxWidthChange = (width: number) => {
    setBox(b => {
      let newHeight = b.height;
      if (keepAspect) newHeight = width;
      return {
        ...b,
        width,
        height: newHeight,
        x: Math.min(b.x, CANVAS_SIZE - width),
        y: Math.min(b.y, CANVAS_SIZE - newHeight)
      };
    });
  };

  const handleBoxHeightChange = (height: number) => {
    setBox(b => {
      let newWidth = b.width;
      if (keepAspect) newWidth = height;
      return {
        ...b,
        width: newWidth,
        height,
        x: Math.min(b.x, CANVAS_SIZE - newWidth),
        y: Math.min(b.y, CANVAS_SIZE - height)
      };
    });
  };

  // --- useEffect ---
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    ctx.clearRect(0, 0, CANVAS_SIZE, CANVAS_SIZE);

    const img = new window.Image();
    img.src = src;
    img.onload = () => {
      const scale = Math.min(CANVAS_SIZE / img.width, CANVAS_SIZE / img.height);
      const drawWidth = img.width * scale;
      const drawHeight = img.height * scale;
      const offsetX = (CANVAS_SIZE - drawWidth) / 2;
      const offsetY = (CANVAS_SIZE - drawHeight) / 2;

      ctx.clearRect(0, 0, CANVAS_SIZE, CANVAS_SIZE);
      ctx.drawImage(img, offsetX, offsetY, drawWidth, drawHeight);

      if (selectionMode === 'cell') {
        ctx.strokeStyle = 'rgba(255,255,255,0.64)';
        ctx.lineWidth = 1;

        // ✅ FIX: grid correcta con offsets
        const startX = Math.floor(cellOffsetX / gridSize) * gridSize;
        for (let x = startX; x < CANVAS_SIZE + gridSize; x += gridSize) {
          const drawX = x - cellOffsetX;
          ctx.beginPath();
          ctx.moveTo(drawX, 0);
          ctx.lineTo(drawX, CANVAS_SIZE);
          ctx.stroke();
        }

        const startY = Math.floor(cellOffsetY / gridSize) * gridSize;
        for (let y = startY; y < CANVAS_SIZE + gridSize; y += gridSize) {
          const drawY = y - cellOffsetY;
          ctx.beginPath();
          ctx.moveTo(0, drawY);
          ctx.lineTo(CANVAS_SIZE, drawY);
          ctx.stroke();
        }

        ctx.fillStyle = 'rgba(0,200,255,0.25)';
        selectedCells.forEach(cell => {
          ctx.fillRect(
            cell.x * gridSize - cellOffsetX,
            cell.y * gridSize - cellOffsetY,
            gridSize,
            gridSize
          );
        });
      }

      if (selectionMode === 'box') {
        ctx.strokeStyle = 'rgba(0,200,255,0.8)';
        ctx.lineWidth = 2;
        boxes.forEach(b =>
          ctx.strokeRect(b.x, b.y, b.width, b.height)
        );
        ctx.setLineDash([6, 4]);
        ctx.strokeRect(box.x, box.y, box.width, box.height);
        ctx.setLineDash([]);
      }
    };
  }, [src, gridSize, selectionMode, selectedCells, box, boxes, keepAspect, cellOffsetX, cellOffsetY]);

  // --- Render ---
  return (
    <div data-bs-theme="dark" className="row g-3 p-3 bg-dark rounded-3" style={{ minHeight: 300 }}>
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
          box={box}
          handleBoxWidthChange={handleBoxWidthChange}
          handleBoxHeightChange={handleBoxHeightChange}
          keepAspect={keepAspect}
          setKeepAspect={setKeepAspect}
          CANVAS_SIZE={CANVAS_SIZE}
        />
      </div>

      <div className="col">
        <SpritePreviewCanvas
          ref={canvasRef}
          selectionMode={selectionMode}
          onClick={handleCanvasClick}
          onMouseMove={selectionMode === 'box' ? handleMouseMove : undefined}
          onMouseLeave={selectionMode === 'box' ? handleMouseLeave : undefined}
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
        />
      </div>
    </div>
  );
}
