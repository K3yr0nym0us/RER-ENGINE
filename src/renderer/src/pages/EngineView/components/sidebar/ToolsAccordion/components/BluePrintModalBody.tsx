import { useState, useRef, useEffect } from 'react'

import { Modal, Nav } from 'react-bootstrap'
import { Grid3x3GapFill, TrashFill } from 'react-bootstrap-icons'
import { useContextEngine } from '@engine'
import { useModal } from '@modal'
import { useQuickBuild } from '../../../../../../context/QuickBuildContext'
import { useSpritePreviewImage } from '../../../../../../hooks/useSpritePreviewImage'
import type { BluePrintCategory, BluePrintEntry } from '@shared-types'

export function BluePrintModalBody() {
  const [activeCategory, setActiveCategory] = useState<BluePrintCategory>('personaje')
  const [pendingDelete, setPendingDelete] = useState<BluePrintEntry | null>(null)
  const { blueprints, setBlueprints } = useContextEngine()
  const { activeBluePrint, setActiveBluePrint } = useQuickBuild()
  const { closeModal } = useModal()

  const filtered = blueprints.filter(bp => bp.category === activeCategory)

  const handleSelect = (bp: BluePrintEntry) => {
    setActiveBluePrint(bp)
    closeModal()
  }

  const handleDeleteRequest = (bp: BluePrintEntry) => {
    setPendingDelete(bp)
  }

  const handleDeleteConfirm = () => {
    if (!pendingDelete) return
    setBlueprints(blueprints.filter(bp => bp.id !== pendingDelete.id))
    if (activeBluePrint?.id === pendingDelete.id) {
      setActiveBluePrint(null)
    }
    setPendingDelete(null)
  }

  return (
    <>
      <div>
      <Nav
        variant="tabs"
        className="mb-3"
        activeKey={activeCategory}
        onSelect={k => setActiveCategory((k ?? 'personaje') as BluePrintCategory)}
      >
        <Nav.Item>
          <Nav.Link eventKey="personaje">Personaje</Nav.Link>
        </Nav.Item>
        <Nav.Item>
          <Nav.Link eventKey="entorno">Entorno</Nav.Link>
        </Nav.Item>
        <Nav.Item>
          <Nav.Link eventKey="objetos">Objetos</Nav.Link>
        </Nav.Item>
      </Nav>

      {filtered.length === 0 ? (
        <p className="text-secondary fst-italic small text-center py-4 mb-0">
          Sin blueprints en esta categoría
        </p>
      ) : (
        <>
          <p className="text-secondary small mb-2">
            Selecciona una blueprint para activar el modo de construcción rápida.
          </p>
          <div className="d-flex flex-wrap gap-2">
            {filtered.map(bp => (
              <BluePrintCard
                key={bp.id}
                bp={bp}
                onSelect={handleSelect}
                onDeleteRequest={handleDeleteRequest}
              />
            ))}
          </div>
        </>
      )}
      </div>

      <Modal
        show={pendingDelete !== null}
        onHide={() => setPendingDelete(null)}
        centered
        backdrop={false}
      >
        <Modal.Header closeButton>
          <Modal.Title>Eliminar blueprint</Modal.Title>
        </Modal.Header>
        <Modal.Body>
          <p className="mb-3">
            Esta acción eliminará la blueprint <strong>{pendingDelete?.name}</strong>.
          </p>
          <p className="text-secondary small mb-4">No se puede deshacer.</p>
          <div className="d-flex justify-content-end gap-2">
            <button className="btn btn-secondary" onClick={() => setPendingDelete(null)}>
              Cancelar
            </button>
            <button className="btn btn-danger" onClick={handleDeleteConfirm}>
              Eliminar
            </button>
          </div>
        </Modal.Body>
      </Modal>
    </>
  )
}

// ---------------------------------------------------------------------------
// Tarjeta individual — muestra solo el primer frame como preview
// ---------------------------------------------------------------------------

const PREVIEW_SIZE = 48

function BluePrintCard({
  bp,
  onSelect,
  onDeleteRequest,
}: {
  bp: BluePrintEntry
  onSelect: (bp: BluePrintEntry) => void
  onDeleteRequest: (bp: BluePrintEntry) => void
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null)

  const firstFrame = bp.animations?.[0]?.frames?.[0]
  // Si no hay frame de animación, usamos la ruta principal del blueprint como preview
  const framePath = firstFrame?.path ?? bp.path

  const { imageSrc } = useSpritePreviewImage(framePath)

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas || !imageSrc) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return

    const img = new window.Image()
    img.onload = () => {
      ctx.clearRect(0, 0, PREVIEW_SIZE, PREVIEW_SIZE)

      const hasCrop = firstFrame?.src_w != null && firstFrame?.src_h != null
      if (hasCrop) {
        const { src_x = 0, src_y = 0, src_w = img.width, src_h = img.height } = firstFrame!
        ctx.drawImage(img, src_x, src_y, src_w, src_h, 0, 0, PREVIEW_SIZE, PREVIEW_SIZE)
      } else {
        const scale = Math.min(PREVIEW_SIZE / img.width, PREVIEW_SIZE / img.height)
        const dw = img.width * scale
        const dh = img.height * scale
        ctx.drawImage(img, (PREVIEW_SIZE - dw) / 2, (PREVIEW_SIZE - dh) / 2, dw, dh)
      }
    }
    img.src = imageSrc
  }, [imageSrc, firstFrame])

  return (
    <div className="position-relative" style={{ width: 80, height: 80 }}>
      <button
        className="btn btn-outline-secondary d-flex flex-column align-items-center justify-content-center gap-1 p-1"
        style={{ width: 80, height: 80, borderRadius: 8, overflow: 'hidden' }}
        title={bp.name}
        onClick={() => onSelect(bp)}
      >
        {/* Mostrar canvas si tenemos una ruta (con frame o sin animaciones) */}
        {framePath ? (
          <canvas
            ref={canvasRef}
            width={PREVIEW_SIZE}
            height={PREVIEW_SIZE}
            style={{ flexShrink: 0, imageRendering: 'pixelated' }}
          />
        ) : (
          <Grid3x3GapFill size={24} className="flex-shrink-0" />
        )}
        <span style={{ fontSize: 10, lineHeight: 1.2 }} className="text-truncate w-100 text-center">
          {bp.name}
        </span>
      </button>

      <button
        type="button"
        className="btn btn-sm btn-danger position-absolute d-flex align-items-center justify-content-center"
        style={{ top: 4, right: 4, width: 20, height: 20, borderRadius: 999, padding: 0 }}
        title="Eliminar blueprint"
        onClick={(e) => {
          e.stopPropagation()
          onDeleteRequest(bp)
        }}
      >
        <TrashFill size={10} />
      </button>
    </div>
  )
}