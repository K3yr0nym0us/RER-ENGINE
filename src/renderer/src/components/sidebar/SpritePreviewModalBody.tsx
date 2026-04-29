import { useRef, useState, useEffect } from 'react';

const CANVAS_SIZE = 500;
const DEFAULT_BOX = { x: 0, y: 0, width: 64, height: 64 };
type SelectionMode = 'cell' | 'box';

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
    <div style={{ display: 'flex', minHeight: 300 }}>
      {/* Barra lateral izquierda */}
      <div className="border rounded me-3 text-center" style={{ minWidth: 160, borderRight: '1px solid #eee', padding: 12 }}>
        <h5>Propiedades del Sprite</h5>
        <hr className="mt-3 me-0 ms-0" />
        <div className="mb-2">
          <b>Modo de selección</b>
          <div className="d-flex justify-content-evenly pt-2">
            <label className="form-check form-check-inline">
              <input
                type="radio"
                className="form-check-input"
                checked={selectionMode === 'cell'}
                onChange={() => setSelectionMode('cell')}
              />
              <span className="form-check-label">Celdas</span>
            </label>
            <label className="form-check form-check-inline">
              <input
                type="radio"
                className="form-check-input"
                checked={selectionMode === 'box'}
                onChange={() => setSelectionMode('box')}
              />
              <span className="form-check-label">Recuadro</span>
            </label>
          </div>
        </div>

        {selectionMode === 'cell' && (
          <>
            <label className="form-label">Tamaño de celda</label>
            <div className="d-flex align-items-center gap-2">
              <input
                type="range"
                min={8}
                max={CANVAS_SIZE}
                step={1}
                value={gridSize}
                onChange={e => setGridSize(Number(e.target.value))}
                className="form-range"
                style={{ flex: 1 }}
              />
              <input
                type="number"
                min={8}
                max={CANVAS_SIZE}
                step={1}
                value={gridSize}
                onChange={e => setGridSize(Number(e.target.value))}
                className="form-control form-control-sm"
                style={{ width: 60 }}
              />
              <span>px</span>
            </div>

            <div className="mt-2">
              <label className="form-label">Desplazar cuadrícula</label>

              <div className="d-flex align-items-center gap-2 mb-1">
                <span className="small">X</span>
                <input
                  type="range"
                  min={-CANVAS_SIZE}
                  max={CANVAS_SIZE}
                  step={1}
                  value={cellOffsetX}
                  onChange={e => setCellOffsetX(Number(e.target.value))}
                  className="form-range"
                  style={{ flex: 1 }}
                />
                <input
                  type="number"
                  min={-CANVAS_SIZE}
                  max={CANVAS_SIZE}
                  step={1}
                  value={cellOffsetX}
                  onChange={e => setCellOffsetX(Number(e.target.value))}
                  className="form-control form-control-sm"
                  style={{ width: 60 }}
                />
                <span>px</span>
              </div>

              <div className="d-flex align-items-center gap-2">
                <span className="small">Y</span>
                <input
                  type="range"
                  min={-CANVAS_SIZE}
                  max={CANVAS_SIZE}
                  step={1}
                  value={cellOffsetY}
                  onChange={e => setCellOffsetY(Number(e.target.value))}
                  className="form-range"
                  style={{ flex: 1 }}
                />
                <input
                  type="number"
                  min={-CANVAS_SIZE}
                  max={CANVAS_SIZE}
                  step={1}
                  value={cellOffsetY}
                  onChange={e => setCellOffsetY(Number(e.target.value))}
                  className="form-control form-control-sm"
                  style={{ width: 60 }}
                />
                <span>px</span>
              </div>
            </div>
          </>
        )}

        {selectionMode === 'box' && (
          <div className="mt-2">
            <label className="form-label">Tamaño recuadro</label>

            <div className="d-flex align-items-center gap-2 mb-2">
              <input
                type="range"
                min={8}
                max={CANVAS_SIZE}
                step={1}
                value={box.width}
                onChange={e => handleBoxWidthChange(Number(e.target.value))}
                className="form-range"
                style={{ flex: 1 }}
              />
              <input
                type="number"
                min={8}
                max={CANVAS_SIZE}
                step={1}
                value={box.width}
                onChange={e => handleBoxWidthChange(Number(e.target.value))}
                className="form-control form-control-sm"
                style={{ width: 60 }}
              />
              <span>px</span>
            </div>

            <div className="d-flex align-items-center gap-2">
              <input
                type="range"
                min={8}
                max={CANVAS_SIZE}
                step={1}
                value={box.height}
                onChange={e => handleBoxHeightChange(Number(e.target.value))}
                className="form-range"
                style={{ flex: 1 }}
                disabled={keepAspect}
              />
              <input
                type="number"
                min={8}
                max={CANVAS_SIZE}
                step={1}
                value={box.height}
                onChange={e => handleBoxHeightChange(Number(e.target.value))}
                className="form-control form-control-sm"
                style={{ width: 60 }}
                disabled={keepAspect}
              />
              <span>px</span>
            </div>

            <button
              className={`btn btn-sm mt-2 ${keepAspect ? 'btn-primary' : 'btn-outline-primary'}`}
              onClick={() => setKeepAspect(k => !k)}
              type="button"
            >
              {keepAspect ? 'Mantener proporciones (ON)' : 'Mantener proporciones (OFF)'}
            </button>
          </div>
        )}
      </div>

      {/* Canvas */}
      <div
        style={{
          flex: 1,
          display: 'flex',
          justifyContent: 'center',
          alignItems: 'center',
          minHeight: CANVAS_SIZE,
        }}
      >
        <canvas
          ref={canvasRef}
          width={CANVAS_SIZE}
          height={CANVAS_SIZE}
          style={{
            border: '1px solid #ccc',
            background: '#222',
            width: '50vh',
            height: '50vh',
            display: 'block',
            cursor: selectionMode === 'box' ? 'crosshair' : 'default'
          }}
          onMouseMove={selectionMode === 'box' ? handleMouseMove : undefined}
          onMouseLeave={selectionMode === 'box' ? handleMouseLeave : undefined}
          onClick={handleCanvasClick}
        />
      </div>

      {/* Sidebar derecho */}
      <div 
        className="border rounded ms-3 text-center" 
        style={{ minWidth: '15vw', borderLeft: '1px solid #eee', padding: 12 }}
      >
        <h4>Previsualización</h4>
        <hr className="mt-3 ms-0 me-0" />
      </div>
    </div>
  );
}