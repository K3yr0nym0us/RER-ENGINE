import type { SavedScene, VisualGraphDocument } from '@shared-types'

import { compileGraphToRhai } from './compileGraphToRhai'
import {
  getSceneProjectState,
  updateSceneScriptRhai,
  updateSceneVisualGraph,
} from '../pages/EngineView/sceneStateStore'

function hasVisualGraphContent(graph?: VisualGraphDocument): boolean {
  return (graph?.nodes?.length ?? 0) > 0
}

/** Resuelve el Rhai activo de la escena (nodos visuales tienen prioridad). */
export function resolveSceneRhaiSource(scene: SavedScene | undefined): string {
  if (!scene) return ''
  if (hasVisualGraphContent(scene.visualGraph)) {
    if (scene.visualScriptRhai?.trim()) return scene.visualScriptRhai
    if (scene.visualGraph) {
      const sceneEntities = scene.entities?.map((e) => ({
        id: e.id,
        name: e.name,
        category: e.category,
      }))
      const { source, errors } = compileGraphToRhai(scene.visualGraph, { sceneEntities })
      if (errors.length === 0) return source
    }
    return ''
  }
  return scene.sceneScriptRhai?.trim() ?? ''
}

/** Envía al motor el script Rhai de la escena indicada. */
export function pushSceneVisualScriptToEngine(sceneId: number, rhaiSource: string): void {
  window.engine.send({
    cmd: 'load_scene_visual_script',
    scene_id: sceneId,
    source: rhaiSource,
  } as never)
}

/** Compila el grafo y lo carga en el motor. */
export function applyVisualGraphToEngine(
  sceneId: number,
  graph: VisualGraphDocument,
): { ok: boolean; errors: string[]; rhaiSource?: string } {
  const scene = getSceneProjectState()?.scenes.find((s) => s.id === sceneId)
  const sceneEntities = scene?.entities?.map((e) => ({
    id: e.id,
    name: e.name,
    category: e.category,
  }))
  const { source, errors } = compileGraphToRhai(graph, { sceneEntities })
  if (errors.length > 0) {
    return { ok: false, errors }
  }
  pushSceneVisualScriptToEngine(sceneId, source)
  return { ok: true, errors: [], rhaiSource: source }
}

/** Persiste grafo + caché Rhai en sceneStateStore y sincroniza motor si es escena activa. */
export function saveSceneVisualGraph(
  sceneId: number,
  graph: VisualGraphDocument,
  options?: { pushToEngine?: boolean },
): { ok: boolean; errors: string[]; rhaiSource?: string } {
  const scene = getSceneProjectState()?.scenes.find((s) => s.id === sceneId)
  const sceneEntities = scene?.entities?.map((e) => ({
    id: e.id,
    name: e.name,
    category: e.category,
  }))
  const { source, errors } = compileGraphToRhai(graph, { sceneEntities })
  if (errors.length > 0) {
    return { ok: false, errors }
  }
  updateSceneVisualGraph(sceneId, graph, source)
  const state = getSceneProjectState()
  const push = options?.pushToEngine !== false
  if (push && state?.activeSceneId === sceneId) {
    pushSceneVisualScriptToEngine(sceneId, source)
  }
  return { ok: true, errors: [], rhaiSource: source }
}

/** Persiste script Rhai manual de escena y sincroniza motor si aplica. */
export function saveSceneScriptRhai(
  sceneId: number,
  source: string,
  options?: { pushToEngine?: boolean },
): void {
  updateSceneScriptRhai(sceneId, source)
  const state = getSceneProjectState()
  const push = options?.pushToEngine !== false
  if (!push || !state || state.activeSceneId !== sceneId) return
  const scene = state.scenes.find((s) => s.id === sceneId)
  if (hasVisualGraphContent(scene?.visualGraph)) return
  pushSceneVisualScriptToEngine(sceneId, source)
}

/** Carga en el motor el script de la escena activa (o la indicada). */
export function reloadActiveSceneVisualScript(sceneId?: number): void {
  const state = getSceneProjectState()
  if (!state) return
  const id = sceneId ?? state.activeSceneId
  const scene = state.scenes.find((s) => s.id === id)
  pushSceneVisualScriptToEngine(id, resolveSceneRhaiSource(scene))
}

export function createEmptyVisualGraph(sceneId: number): VisualGraphDocument {
  return {
    version: 1,
    context: 'scene',
    sceneId,
    nodes: [],
    edges: [],
  }
}
