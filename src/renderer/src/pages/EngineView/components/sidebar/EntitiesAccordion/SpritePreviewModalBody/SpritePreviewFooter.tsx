interface SpritePreviewFooterProps {
  validationError: string | null;
  selectedFrameCount: number;
  hasImageSrc: boolean;
  onConfirm?: () => void;
  onCancel?: () => void;
}

export function SpritePreviewFooter({
  validationError,
  selectedFrameCount,
  hasImageSrc,
  onConfirm,
  onCancel,
}: SpritePreviewFooterProps) {
  return (
    <>
      {validationError && (
        <div className="alert alert-danger py-2 px-3 mt-2 mb-0">
          {validationError}
        </div>
      )}

      {(onConfirm || onCancel) && (
        <div className="d-flex gap-2 justify-content-end mt-3 px-3">
          {onCancel && (
            <button className="btn btn-secondary btn-sm" onClick={onCancel}>
              Cancelar
            </button>
          )}
          {onConfirm && (
            <button
              className="btn btn-primary btn-sm"
              onClick={onConfirm}
              disabled={selectedFrameCount === 0 || !hasImageSrc}
            >
              Confirmar
            </button>
          )}
        </div>
      )}
    </>
  );
}
