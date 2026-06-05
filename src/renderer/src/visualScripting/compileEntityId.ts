import type { VisualGraphDocument } from '@shared-types'

import type { VisualGraphContext } from './nodeDefinitions'

export function graphContext(doc: VisualGraphDocument): VisualGraphContext {
  return doc.context ?? 'scene'
}

/** Expresión Rhai del id de entidad (entity.id en lógica de entidad propia). */
export function compileEntityIdExpr(doc: VisualGraphDocument, entityId: number): string {
  const ownerId = doc.entityId ?? 0
  if (graphContext(doc) === 'entity' && (entityId <= 0 || entityId === ownerId)) {
    return 'entity.id'
  }
  return String(entityId)
}

export function compileRhaiExprField(value: unknown, fallback: string): string {
  const expr = String(value ?? '').trim()
  return expr || fallback
}
