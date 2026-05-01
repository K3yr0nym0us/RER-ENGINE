import type { SavedScene } from '@shared-types';

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
