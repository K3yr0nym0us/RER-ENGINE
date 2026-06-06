import { useRef, useState } from 'react'

import type { Blueprint3D, Entity3D, VisualGraphContext, VisualGraphDocument } from '@shared-types'

import { useTraslate } from '@hooks'
import { createEmptyEntityVisualGraph } from '../entityVisualScript'
import { createEmptyVisualGraph } from '../sceneVisualScript'
import { VisualScriptCanvas } from './VisualScriptCanvas'

export interface VisualScriptingModalBodyProps {
  context?: VisualGraphContext
  sceneId?: number
  sceneName?: string
  entityId?: number
  entityName?: string
  /** Entidades resueltas en la ventana principal (IPC → modal Electron). */
  sceneEntities?: Entity3D[]
  /** Blueprints del proyecto (agrupación Environment en panel lateral). */
  blueprints?: Blueprint3D[]
  initialGraph?: VisualGraphDocument
  onSave: (graph: VisualGraphDocument) => { ok: boolean; errors?: string[] }
  onCancel: () => void
}

export function VisualScriptingModalBody({
  context = 'scene',
  sceneId,
  sceneName,
  entityId,
  entityName,
  sceneEntities,
  blueprints,
  initialGraph,
  onSave,
  onCancel,
}: VisualScriptingModalBodyProps) {
  const { t } = useTraslate()
  const defaultGraph = context === 'entity' && entityId != null
    ? createEmptyEntityVisualGraph(entityId)
    : createEmptyVisualGraph(sceneId ?? 1)
  const graphRef = useRef<VisualGraphDocument>(initialGraph ?? defaultGraph)
  const [error, setError] = useState<string | null>(null)

  const handleSave = () => {
    setError(null)
    const result = onSave(graphRef.current)
    if (!result.ok) {
      setError(result.errors?.map((msg) => t(msg)).join(' · ') ?? t('Error saving graph'))
    }
  }

  const titleLine = context === 'entity'
    ? (
      <p className="small text-secondary mb-2 flex-shrink-0">
        {t('Entity')}: <strong>{entityName ?? entityId}</strong> (id {entityId})
      </p>
    )
    : sceneName && (
      <p className="small text-secondary mb-2 flex-shrink-0">
        {t('Scene')}: <strong>{sceneName}</strong> (id {sceneId})
      </p>
    )

  return (
    <div className="visual-scripting-modal visual-scripting-modal--resizable d-flex flex-column flex-grow-1 min-h-0">
      {titleLine}
      {error && <div className="alert alert-danger py-1 small flex-shrink-0">{error}</div>}
      <VisualScriptCanvas
        context={context}
        sceneId={sceneId}
        entityId={entityId}
        entityName={entityName}
        sceneEntities={sceneEntities ?? []}
        blueprints={blueprints}
        initialGraph={graphRef.current}
        fill
        onGraphChange={(doc) => {
          graphRef.current = doc
        }}
      />
      <div className="d-flex justify-content-end gap-2 mt-3 flex-shrink-0">
        <button type="button" className="btn btn-sm btn-outline-secondary" onClick={onCancel}>
          {t('Cancel')}
        </button>
        <button type="button" className="btn btn-sm btn-primary" onClick={handleSave}>
          {t('Save and apply')}
        </button>
      </div>
    </div>
  )
}

export default VisualScriptingModalBody
