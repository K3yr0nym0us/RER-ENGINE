import type { EngineStartPayload } from '../shared-types/types'

/** Valida el payload con el que el launcher arranca el editor Rust. */
export function isEngineStartPayload(v: unknown): v is EngineStartPayload {
  if (typeof v !== 'object' || v === null) return false
  const o = v as Record<string, unknown>
  return (
    (o.projectType === '2D' || o.projectType === '3D' || o.projectType === 'save')
    && 'save_path' in o
  )
}
