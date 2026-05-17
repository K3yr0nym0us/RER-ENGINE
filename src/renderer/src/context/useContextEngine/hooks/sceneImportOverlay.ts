import type { Dispatch, MutableRefObject } from 'react';

import type { GameStyle, SavedScene } from '@shared-types';
import { isPlayerPath } from '@shared-types';

import type { EngineAction, EngineInternalRefs } from '../types';

export function beginSceneImportLoading(
	dispatch: Dispatch<EngineAction>,
	sceneImportInProgressRef: MutableRefObject<boolean>,
) {
	sceneImportInProgressRef.current = true;
	dispatch({ type: 'SET_SCENE_IMPORT_LOADING', payload: true });
	window.electronAPI?.hideEngineViewport?.();
}

export function endSceneImportLoading(
	dispatch: Dispatch<EngineAction>,
	sceneImportInProgressRef: MutableRefObject<boolean>,
	pendingImportSceneRef: MutableRefObject<SavedScene | null>,
	reportBounds: () => void,
) {
	sceneImportInProgressRef.current = false;
	pendingImportSceneRef.current = null;
	dispatch({ type: 'SET_SCENE_IMPORT_LOADING', payload: false });
	setTimeout(() => {
		window.electronAPI?.restoreEngineViewport?.();
		reportBounds();
	}, 0);
}

/** Carga 3D por ráfaga IPC (cambio de pestaña o `ready` inicial), no `import_scene` 2D. */
export function needsSceneBurstLoad(
	projectType: string | undefined,
	gameStyle: GameStyle | undefined,
	scene: Pick<SavedScene, 'entities' | 'playerTransform'>,
): boolean {
	if (projectType !== '3D') return false;
	if ((scene.entities?.length ?? 0) > 0) return true;
	const savedPlayer = scene.playerTransform;
	const playerInEntities = (scene.entities ?? []).some(
		(e) => e.kind === 'character' && isPlayerPath(e.path),
	);
	return gameStyle === 'first-person' && !!savedPlayer && !playerInEntities;
}

export function beginSceneBurstLoad(
	dispatch: Dispatch<EngineAction>,
	sceneBurstLoadInProgressRef: MutableRefObject<boolean>,
) {
	if (sceneBurstLoadInProgressRef.current) return;
	sceneBurstLoadInProgressRef.current = true;
	dispatch({ type: 'SET_SCENE_IMPORT_LOADING', payload: true });
	window.electronAPI?.hideEngineViewport?.();
}

export function endSceneBurstLoad(
	dispatch: Dispatch<EngineAction>,
	sceneBurstLoadInProgressRef: MutableRefObject<boolean>,
	reportBounds: () => void,
) {
	if (!sceneBurstLoadInProgressRef.current) return;
	sceneBurstLoadInProgressRef.current = false;
	dispatch({ type: 'SET_SCENE_IMPORT_LOADING', payload: false });
	setTimeout(() => {
		window.electronAPI?.restoreEngineViewport?.();
		reportBounds();
	}, 0);
}

type SceneBurstRefs = Pick<
	EngineInternalRefs,
	| 'pendingRestoresRef'
	| 'pendingModelLoadQueueRef'
	| 'pendingPlayCharacterViewRef'
	| 'mainPlayerHandled'
	| 'sceneBurstAwaitingPlayerViewRef'
	| 'sceneBurstPendingColliderCountRef'
>;

export function hasPendingSceneBurstWork(refs: SceneBurstRefs): boolean {
	if (refs.pendingModelLoadQueueRef.current.length > 0) return true;
	if (refs.sceneBurstPendingColliderCountRef.current > 0) return true;
	for (const queue of refs.pendingRestoresRef.current.values()) {
		if (queue.length > 0) return true;
	}
	if (refs.pendingPlayCharacterViewRef.current && !refs.mainPlayerHandled.current) return true;
	if (refs.sceneBurstAwaitingPlayerViewRef.current) return true;
	return false;
}

export function tryEndSceneBurstLoad(
	dispatch: Dispatch<EngineAction>,
	sceneBurstLoadInProgressRef: MutableRefObject<boolean>,
	refs: SceneBurstRefs,
	reportBounds: () => void,
) {
	if (!sceneBurstLoadInProgressRef.current) return;
	if (hasPendingSceneBurstWork(refs)) return;
	endSceneBurstLoad(dispatch, sceneBurstLoadInProgressRef, reportBounds);
}

export function trackSceneBurstCollider(
	refs: Pick<EngineInternalRefs, 'sceneBurstPendingColliderCountRef'>,
) {
	refs.sceneBurstPendingColliderCountRef.current += 1;
}
