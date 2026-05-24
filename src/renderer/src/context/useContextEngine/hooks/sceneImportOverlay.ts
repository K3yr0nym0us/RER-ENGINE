import type { Dispatch, MutableRefObject } from 'react';

import type { GameStyle, SavedScene } from '@shared-types';
import { isPlayerPath } from '@shared-types';

import type { EngineAction, EngineInternalRefs, PendingRestore } from '../types';

export type ModelLoadOverlayKind = 'model' | 'entity' | 'scene';

type SceneBurstRefs = Pick<
	EngineInternalRefs,
	| 'pendingRestoresRef'
	| 'pendingModelLoadQueueRef'
	| 'pendingPlayCharacterViewRef'
	| 'mainPlayerHandled'
	| 'sceneBurstAwaitingPlayerViewRef'
	| 'sceneBurstPendingColliderCountRef'
	| 'sceneBurstPendingOpsRef'
>;

export function trackSceneBurstOp(
	refs: Pick<EngineInternalRefs, 'sceneBurstPendingOpsRef'>,
) {
	refs.sceneBurstPendingOpsRef.current += 1;
}

export function completeSceneBurstOp(
	refs: Pick<EngineInternalRefs, 'sceneBurstPendingOpsRef'>,
) {
	if (refs.sceneBurstPendingOpsRef.current > 0) {
		refs.sceneBurstPendingOpsRef.current -= 1;
	}
}

/** Precargas GPU (`load_model_asset` → `model_asset_loaded`) durante ráfaga de escena. */
export function trackSceneBurstModelPreloads(
	refs: Pick<EngineInternalRefs, 'sceneBurstPendingOpsRef'>,
	count: number,
) {
	if (count > 0) refs.sceneBurstPendingOpsRef.current += count;
}

export function modelPathBasename(path: string): string {
	return path.split(/[/\\]/).pop()?.toLowerCase() ?? path.toLowerCase();
}

export function pathsMatchForBurstRestore(a: string, b: string): boolean {
	if (a === b) return true;
	return modelPathBasename(a) === modelPathBasename(b);
}

export function takePendingModelLoadByPath(
	queue: Array<{ modelPath: string; pending: PendingRestore }>,
	loadedPath: string,
): { modelPath: string; pending: PendingRestore } | null {
	const idx = queue.findIndex((item) => pathsMatchForBurstRestore(item.modelPath, loadedPath));
	if (idx < 0) return null;
	return queue.splice(idx, 1)[0] ?? null;
}

export function takePendingRestoreByPath(
	restores: Map<string, PendingRestore[]>,
	loadedPath: string,
): { path: string; pending: PendingRestore } | null {
	for (const [key, queue] of restores.entries()) {
		if (queue.length === 0 || key.startsWith('[')) continue;
		if (pathsMatchForBurstRestore(key, loadedPath)) {
			const pending = queue.shift()!;
			if (queue.length === 0) restores.delete(key);
			return { path: key, pending };
		}
	}
	return null;
}

/** Al consumir un restore por path, vacía también la cola duplicada en `pendingModelLoadQueueRef`. */
export function drainPendingRestoreSlot(
	restores: Map<string, PendingRestore[]>,
	modelQueue: Array<{ modelPath: string; pending: PendingRestore }>,
	path: string,
) {
	for (const [key, queue] of restores.entries()) {
		if (queue.length === 0 || key.startsWith('[')) continue;
		if (pathsMatchForBurstRestore(key, path)) {
			queue.shift();
			if (queue.length === 0) restores.delete(key);
			break;
		}
	}
	const idx = modelQueue.findIndex((item) => pathsMatchForBurstRestore(item.modelPath, path));
	if (idx >= 0) modelQueue.splice(idx, 1);
}

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
	refs?: Pick<EngineInternalRefs, 'sceneBurstPendingOpsRef'>,
) {
	if (sceneBurstLoadInProgressRef.current) return;
	sceneBurstLoadInProgressRef.current = true;
	if (refs) refs.sceneBurstPendingOpsRef.current = 0;
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

export function hasPendingSceneBurstWork(refs: SceneBurstRefs): boolean {
	if (refs.sceneBurstPendingOpsRef.current > 0) return true;
	if (refs.sceneBurstPendingColliderCountRef.current > 0) return true;
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
	if (hasPendingSceneBurstWork(refs)) {
		if (refs.sceneBurstPendingOpsRef.current === 0) {
			const orphanModels = refs.pendingModelLoadQueueRef.current.length;
			const orphanRestores = [...refs.pendingRestoresRef.current.entries()]
				.filter(([, q]) => q.length > 0)
				.map(([k, q]) => `${k}(${q.length})`);
			if (orphanModels > 0 || orphanRestores.length > 0) {
				console.warn(
					'[burst_load] colas huérfanas sin contador:',
					{ orphanModels, orphanRestores },
				);
				refs.pendingModelLoadQueueRef.current = [];
				for (const [key, queue] of refs.pendingRestoresRef.current.entries()) {
					if (!key.startsWith('[')) queue.length = 0;
				}
			}
		}
		if (hasPendingSceneBurstWork(refs)) return;
	}
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
