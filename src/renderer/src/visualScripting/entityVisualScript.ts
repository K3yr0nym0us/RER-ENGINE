import type { VisualGraphDocument } from '@shared-types'

import { compileGraphToRhai } from './compileGraphToRhai'

const VISUAL_LOGIC_SCRIPT_NAME = 'visual_logic'

function hasVisualGraphContent(graph?: VisualGraphDocument): boolean {
  return (graph?.nodes?.length ?? 0) > 0
}

export function createEmptyEntityVisualGraph(entityId: number): VisualGraphDocument {
  return {
    version: 1,
    context: 'entity',
    entityId,
    nodes: [],
    edges: [],
  }
}

export function resolveEntityRhaiSource(
  visualGraph?: VisualGraphDocument,
  visualScriptRhai?: string,
): string {
  if (!hasVisualGraphContent(visualGraph)) return ''
  if (visualScriptRhai?.trim()) return visualScriptRhai
  if (visualGraph) {
    const { source, errors } = compileGraphToRhai(visualGraph)
    if (errors.length === 0) return source
  }
  return ''
}

export function saveEntityVisualGraph(
  _entityId: number,
  graph: VisualGraphDocument,
): { ok: boolean; errors: string[]; rhaiSource?: string } {
  const { source, errors } = compileGraphToRhai(graph)
  if (errors.length > 0) {
    return { ok: false, errors }
  }
  return { ok: true, errors: [], rhaiSource: source }
}

export { VISUAL_LOGIC_SCRIPT_NAME }
