import type { Dispatch, MutableRefObject } from 'react';

import type { GameStyle, SavedScene } from '@shared-types';
import { isPlayerPath } from '@shared-types';

import type { EngineAction, EngineInternalRefs } from '../types';

export type ModelLoadOverlayKind = 'model' | 'entity' | 'scene';

type BlockingLoadRefs = {
	sceneImport: MutableRefObject<boolean>;
	burst: MutableRefObject<boolean>;
	modelReplace: MutableRefObject<boolean>;
};

function syncBlockingLoadOverlay(
	dispatch: Dispatch<EngineAction>,
	refs: BlockingLoadRefs,
	reportBounds: () => void,
) {
	const stillBusy =
		refs.sceneImport.current || refs.burst.current || refs.modelReplace.current;
	dispatch({ type: 'SET_SCENE_IMPORT_LOADING', payload: stillBusy });
	if (!stillBusy) {
		setTimeout(() => {
			window.electronAPI?.restoreEngineViewport?.();
			reportBounds();
		}, 0);
	}
}

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
	sceneBurstLoadInProgressRef: MutableRefObject<boolean>,
	modelReplaceInProgressRef: MutableRefObject<boolean>,
	reportBounds: () => void,
) {
	sceneImportInProgressRef.current = false;
	pendingImportSceneRef.current = null;
	syncBlockingLoadOverlay(
		dispatch,
		{
			sceneImport: sceneImportInProgressRef,
			burst: sceneBurstLoadInProgressRef,
			modelReplace: modelReplaceInProgressRef,
		},
		reportBounds,
	);
}

/** Overlay mientras el motor reemplaza un modelo 3D (GLB/FBX pesado en hilo principal). */
export function beginModelReplaceLoading(
	dispatch: Dispatch<EngineAction>,
	modelReplaceInProgressRef: MutableRefObject<boolean>,
	kind: ModelLoadOverlayKind = 'model',
	modelLoadOverlayKindRef?: MutableRefObject<ModelLoadOverlayKind | null>,
) {
	modelReplaceInProgressRef.current = true;
	if (modelLoadOverlayKindRef) {
		modelLoadOverlayKindRef.current = kind;
	}
	dispatch({ type: 'SET_SCENE_IMPORT_LOADING', payload: true });
	window.electronAPI?.hideEngineViewport?.();
}

export function trackModelAssetPreloadStart(
	dispatch: Dispatch<EngineAction>,
	refs: Pick<EngineInternalRefs, 'modelAssetPreloadPendingRef' | 'modelReplaceInProgressRef' | 'modelLoadOverlayKindRef'>,
) {
	refs.modelAssetPreloadPendingRef.current += 1;
	beginModelReplaceLoading(
		dispatch,
		refs.modelReplaceInProgressRef,
		'model',
		refs.modelLoadOverlayKindRef,
	);
}

export function trackModelAssetPreloadEnd(
	dispatch: Dispatch<EngineAction>,
	refs: Pick<
		EngineInternalRefs,
		| 'modelAssetPreloadPendingRef'
		| 'modelReplaceInProgressRef'
		| 'sceneImportInProgressRef'
		| 'sceneBurstLoadInProgressRef'
		| 'modelLoadOverlayKindRef'
	>,
	reportBounds: () => void,
) {
	if (refs.modelAssetPreloadPendingRef.current > 0) {
		refs.modelAssetPreloadPendingRef.current -= 1;
	}
	if (refs.modelAssetPreloadPendingRef.current <= 0) {
		refs.modelAssetPreloadPendingRef.current = 0;
		endModelReplaceLoading(
			dispatch,
			refs.modelReplaceInProgressRef,
			refs.sceneImportInProgressRef,
			refs.sceneBurstLoadInProgressRef,
			reportBounds,
			refs.modelLoadOverlayKindRef,
		);
	}
}

export function endModelReplaceLoading(
	dispatch: Dispatch<EngineAction>,
	modelReplaceInProgressRef: MutableRefObject<boolean>,
	sceneImportInProgressRef: MutableRefObject<boolean>,
	sceneBurstLoadInProgressRef: MutableRefObject<boolean>,
	reportBounds: () => void,
	modelLoadOverlayKindRef?: MutableRefObject<ModelLoadOverlayKind | null>,
) {
	if (!modelReplaceInProgressRef.current) return;
	modelReplaceInProgressRef.current = false;
	if (modelLoadOverlayKindRef) {
		modelLoadOverlayKindRef.current = null;
	}
	syncBlockingLoadOverlay(
		dispatch,
		{
			sceneImport: sceneImportInProgressRef,
			burst: sceneBurstLoadInProgressRef,
			modelReplace: modelReplaceInProgressRef,
		},
		reportBounds,
	);
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
	sceneImportInProgressRef: MutableRefObject<boolean>,
	modelReplaceInProgressRef: MutableRefObject<boolean>,
	reportBounds: () => void,
) {
	if (!sceneBurstLoadInProgressRef.current) return;
	sceneBurstLoadInProgressRef.current = false;
	syncBlockingLoadOverlay(
		dispatch,
		{
			sceneImport: sceneImportInProgressRef,
			burst: sceneBurstLoadInProgressRef,
			modelReplace: modelReplaceInProgressRef,
		},
		reportBounds,
	);
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
	return false;
}

export function tryEndSceneBurstLoad(
	dispatch: Dispatch<EngineAction>,
	sceneBurstLoadInProgressRef: MutableRefObject<boolean>,
	refs: SceneBurstRefs,
	sceneImportInProgressRef: MutableRefObject<boolean>,
	modelReplaceInProgressRef: MutableRefObject<boolean>,
	reportBounds: () => void,
) {
	if (!sceneBurstLoadInProgressRef.current) return;
	if (hasPendingSceneBurstWork(refs)) return;
	endSceneBurstLoad(
		dispatch,
		sceneBurstLoadInProgressRef,
		sceneImportInProgressRef,
		modelReplaceInProgressRef,
		reportBounds,
	);
}

export function trackSceneBurstCollider(
	refs: Pick<EngineInternalRefs, 'sceneBurstPendingColliderCountRef'>,
) {
	refs.sceneBurstPendingColliderCountRef.current += 1;
}
