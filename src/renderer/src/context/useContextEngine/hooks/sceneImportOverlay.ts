import type { Dispatch, MutableRefObject } from 'react';

import type { SavedScene } from '@shared-types';

import type { EngineAction } from '../types';

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
