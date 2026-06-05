import type { SavedScene, VisualGraphDocument } from '@shared-types';

export interface SceneProjectState {
  scenes: SavedScene[];
  activeSceneId: number;
}

let sceneProjectState: SceneProjectState | null = null;

export function setSceneProjectState(next: SceneProjectState): void {
  sceneProjectState = next;
}

export function getSceneProjectState(): SceneProjectState | null {
  return sceneProjectState;
}

export function getSceneVisualGraph(sceneId: number): VisualGraphDocument | undefined {
  return sceneProjectState?.scenes.find((s) => s.id === sceneId)?.visualGraph;
}

export function updateSceneVisualGraph(
  sceneId: number,
  graph: VisualGraphDocument,
  visualScriptRhai?: string,
): void {
  if (!sceneProjectState) return;
  sceneProjectState = {
    ...sceneProjectState,
    scenes: sceneProjectState.scenes.map((s) =>
      s.id === sceneId
        ? {
            ...s,
            visualGraph: graph,
            ...(visualScriptRhai !== undefined ? { visualScriptRhai } : {}),
          }
        : s,
    ),
  };
}

export function getSceneScriptRhai(sceneId: number): string | undefined {
  return sceneProjectState?.scenes.find((s) => s.id === sceneId)?.sceneScriptRhai;
}

export function updateSceneScriptRhai(sceneId: number, source: string): void {
  if (!sceneProjectState) return;
  sceneProjectState = {
    ...sceneProjectState,
    scenes: sceneProjectState.scenes.map((s) =>
      s.id === sceneId ? { ...s, sceneScriptRhai: source } : s,
    ),
  };
}
