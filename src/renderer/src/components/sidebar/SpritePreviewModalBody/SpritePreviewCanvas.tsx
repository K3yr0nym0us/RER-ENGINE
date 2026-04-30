import { forwardRef } from 'react';

interface SpritePreviewCanvasProps {
  selectionMode: 'cell' | 'box';
  onClick: (e: React.MouseEvent<HTMLCanvasElement>) => void;
  onMouseMove?: (e: React.MouseEvent<HTMLCanvasElement>) => void;
  onMouseLeave?: (e: React.MouseEvent<HTMLCanvasElement>) => void;
  CANVAS_SIZE: number;
}

export const SpritePreviewCanvas = forwardRef<HTMLCanvasElement, SpritePreviewCanvasProps>(
  ({ selectionMode, onClick, onMouseMove, onMouseLeave, CANVAS_SIZE }, ref) => {
    return (
      <div
        className="text-center"
      >
        <canvas
          ref={ref}
          width={CANVAS_SIZE}
          height={CANVAS_SIZE}
          style={{
            border: '2px solid #444',
            background: '#222',
            maxWidth: '100%',
            maxHeight: '65vh',
            cursor: selectionMode === 'box' ? 'crosshair' : 'default'
          }}
          onMouseMove={onMouseMove}
          onMouseLeave={onMouseLeave}
          onClick={onClick}
        />
      </div>
    );
  }
);
