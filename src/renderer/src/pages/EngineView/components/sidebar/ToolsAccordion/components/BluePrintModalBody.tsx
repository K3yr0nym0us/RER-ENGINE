import { useState, useRef, useEffect } from 'react'

import { Modal, Nav } from 'react-bootstrap'
import { Grid3x3GapFill, TrashFill } from 'react-bootstrap-icons'
import { useContextEngine } from '@engine'
import { useModal } from '@modal'
import { useQuickBuild } from '../../../../../../context/QuickBuildContext'
import { useSpritePreviewImage } from '@hooks'
import type { BluePrintCategory, BluePrintEntry } from '@shared-types'
import { useTraslate } from '@hooks'

export function BluePrintModalBody() {
  const { t } = useTraslate()
  const [activeCategory, setActiveCategory] = useState<BluePrintCategory>('personaje')
  const [pendingDelete, setPendingDelete] = useState<BluePrintEntry | null>(null)
  const {
    blueprints,
    setBlueprints,
    entityMetaRef,
    removeScenario,
    removeCharacter,
    removeCollider,
    removeExecutionArea,
  } = useContextEngine()
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

  /** Recoge todos los ids de entidades vinculadas a una blueprint */
  const getLinkedEntityIds = (bpId: string): number[] =>
    Object.entries(entityMetaRef.current)
      .filter(([, meta]) => meta.blueprintId === bpId)
      .map(([id]) => Number(id))

  /** Elimina la blueprint y BORRA todas las entidades vinculadas */
  const handleDeleteWithEntities = () => {
    if (!pendingDelete) return
    const ids = getLinkedEntityIds(pendingDelete.id)
    ids.forEach(id => {
      const kind = entityMetaRef.current[id]?.kind
      if (kind === 'scenario') removeScenario(id)
      else if (kind === 'character') removeCharacter(id)
      else if (kind === 'collider') removeCollider(id)
      else if (kind === 'execution_area') removeExecutionArea(id)
    })
    setBlueprints(blueprints.filter(bp => bp.id !== pendingDelete.id))
    if (activeBluePrint?.id === pendingDelete.id) setActiveBluePrint(null)
    setPendingDelete(null)
  }

  /** Elimina la blueprint y CONVIERTE todas las entidades en entidades únicas */
  const handleDeleteKeepEntities = () => {
    if (!pendingDelete) return
    const ids = getLinkedEntityIds(pendingDelete.id)
    ids.forEach(id => {
      const meta = entityMetaRef.current[id]
      if (!meta) return
      // Absorber propiedades de la blueprint antes de romper el vínculo
      meta.physicsEnabled  = pendingDelete.physics_enabled ?? meta.physicsEnabled
      meta.physicsType     = pendingDelete.physics_type    ?? meta.physicsType
      meta.animations      = pendingDelete.animations      ?? meta.animations
      meta.scripts         = pendingDelete.scripts         ?? meta.scripts
      meta.controlBindings = pendingDelete.control_bindings ?? meta.controlBindings
      delete meta.blueprintId
    })
    setBlueprints(blueprints.filter(bp => bp.id !== pendingDelete.id))
    if (activeBluePrint?.id === pendingDelete.id) setActiveBluePrint(null)
    setPendingDelete(null)
  }

  const linkedCount = pendingDelete ? getLinkedEntityIds(pendingDelete.id).length : 0

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
          <Nav.Link eventKey="personaje">{t('Character')}</Nav.Link>
        </Nav.Item>
        <Nav.Item>
          <Nav.Link eventKey="entorno">{t('Environment')}</Nav.Link>
        </Nav.Item>
        <Nav.Item>
          <Nav.Link eventKey="objetos">{t('Objects')}</Nav.Link>
        </Nav.Item>
      </Nav>

      {filtered.length === 0 ? (
        <p className="text-secondary fst-italic small text-center py-4 mb-0">
          {t('No blueprints in this category')}
        </p>
      ) : (
        <>
          <p className="text-secondary small mb-2">
            {t('Select a blueprint to activate quick build mode.')}
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
          <Modal.Title>{t('Delete blueprint')}</Modal.Title>
        </Modal.Header>
        <Modal.Body>
          <p className="mb-2">
            {t('This action will delete the blueprint')} <strong>{pendingDelete?.name}</strong>.
          </p>
          {linkedCount > 0 ? (
            <>
              <p className="mb-4">
                {t('There are')} <strong>{linkedCount}</strong> {t('entities based on this blueprint. What do you want to do with them?')}
              </p>
              <div className="d-flex flex-column gap-2">
                <button className="btn btn-danger" onClick={handleDeleteWithEntities}>
                  {t('Delete all entities')} ({linkedCount})
                </button>
                <button className="btn btn-warning text-dark" onClick={handleDeleteKeepEntities}>
                  {t('Convert to standalone entities')}
                </button>
                <button className="btn btn-secondary" onClick={() => setPendingDelete(null)}>
                  {t('Cancel')}
                </button>
              </div>
            </>
          ) : (
            <>
              <p className="text-secondary small mb-4">{t('Cannot be undone.')}</p>
              <div className="d-flex justify-content-end gap-2">
                <button className="btn btn-secondary" onClick={() => setPendingDelete(null)}>
                  {t('Cancel')}
                </button>
                <button className="btn btn-danger" onClick={handleDeleteWithEntities}>
                  {t('Delete')}
                </button>
              </div>
            </>
          )}
        </Modal.Body>
      </Modal>
    </>
  )
}

// ---------------------------------------------------------------------------
// Tarjeta individual - muestra solo el primer frame como preview
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
  const { t } = useTraslate()
  const canvasRef = useRef<HTMLCanvasElement>(null)

  const firstFrame = bp.animations?.[0]?.frames?.[0]
  // Si no hay frame de animacion, usamos la ruta principal del blueprint como preview
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
        title={t('Delete blueprint')}
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