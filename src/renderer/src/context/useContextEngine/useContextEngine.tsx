import React, { createContext, useContext, useEffect, useReducer, useRef } from 'react';
import type {
	GameStyle,
	ModelInfo,
	ProjectLoaded2dPayload,
	ProjectLoaded3dPayload,
	SavedPlayerTransform,
	SavedScene,
	EntityCategory,
} from '@shared-types';
import { useLanguage } from '../LanguageContext';
import { createEngineActions } from './hooks/createEngineActions';
import { useEngineEffects } from './hooks/useEngineEffects';
import type { ModelLoadOverlayKind } from './hooks/sceneImportOverlay';
import {
	engineReducer,
	initialState,
	type Camera2dState,
	type EngineContextValue,
	type EngineInternalRefs,
	type EntityMeta,
	type PendingBurstSpawnEntry,
	type PendingRestore,
	type Transform,
} from './types';

const EngineContext = createContext<EngineContextValue | undefined>(undefined);

export function EngineProvider({
	children,
	viewportRef,
	projectType,
	gameStyle,
	initialSavePath,
	initialExtractDir,
}: {
	children: React.ReactNode;
	viewportRef: React.RefObject<HTMLDivElement | null>;
	projectType?: string;
	gameStyle?: GameStyle;
	initialSavePath?: string | null;
	initialExtractDir?: string | null;
}) {
	const { setLocale } = useLanguage();

	const [state, dispatch] = useReducer(engineReducer, initialState);

	const refs: EngineInternalRefs = {
		readyTimer: useRef<ReturnType<typeof setTimeout> | null>(null),
		resizeTimerRef: useRef<ReturnType<typeof setTimeout> | null>(null),
		logIdRef: useRef(0),
		initialSavePathRef: useRef(initialSavePath),
		initialExtractDirRef: useRef(initialExtractDir),
		projectLoaded2dMetaRef: useRef<ProjectLoaded2dPayload | null>(null),
		projectLoaded3dMetaRef: useRef<ProjectLoaded3dPayload | null>(null),
		entityTransformsRef: useRef<Record<number, Transform>>({}),
		entityMetaRef: useRef<Record<number, EntityMeta>>({}),
		pendingRestoresRef: useRef<Map<string, PendingRestore[]>>(new Map()),
		playerEntityIdRef: useRef<number | null>(null),
		editorCameraEntityIdRef: useRef<number | null>(null),
		playCharacterViewRef: useRef<SavedPlayerTransform | null>(null),
		pendingPlayCharacterViewRef: useRef<SavedPlayerTransform | null>(null),
		pendingModelPathRef: useRef<string | null>(null),
		pendingSpawnKindRef: useRef<EntityMeta['kind'] | null>(null),
		pendingSpawnCategoryRef: useRef<EntityCategory | null>(null),
		pendingModelLoadQueueRef: useRef<Array<{ modelPath: string; pending: PendingRestore }>>([]),
		pendingBurstSpawnRestoreRef: useRef<PendingBurstSpawnEntry[]>([]),
		camera2dRef: useRef<Camera2dState | null>(null),
		mainPlayerHandled: useRef(false),
		playerRemoved: useRef(false),
		pendingPlayerDups: useRef<Transform[]>([]),
		pendingDupQ: useRef<Transform[]>([]),
		pivotEditListenerRef: useRef<((framePath: string, px: number, py: number) => void) | null>(null),
		quickBuildClickListenerRef: useRef<((x: number, y: number, z: number, fitToGrid: boolean, scale?: [number, number, number]) => void) | null>(null),
		quickBuildActiveBlueprintIdRef: useRef<string | null>(null),
		pendingEventsRef: useRef<Map<string, { resolve: (value: any) => void }>>(new Map()),
		pendingImportSceneRef: useRef<SavedScene | null>(null),
		sceneImportInProgressRef: useRef(false),
		modelReplaceInProgressRef: useRef(false),
		modelLoadOverlayKindRef: useRef<ModelLoadOverlayKind | null>(null),
		modelAssetPreloadPendingRef: useRef(0),
		sceneBurstLoadInProgressRef: useRef(false),
		sceneBurstPendingColliderCountRef: useRef(0),
		sceneBurstPendingOpsRef: useRef(0),
		engineBootAwaitRef: useRef(false),
		engineBootIpcPendingRef: useRef(0),
		engineBootIpcSeenRef: useRef(0),
		engineBootFinishedRef: useRef(false),
		blueprintsRef: useRef([]),
		modelsRef: useRef<ModelInfo[]>([]),
		updateEntityTransformRef: useRef((_id: number, _patch: Partial<Transform>) => {}),
	};

	const addLog = (text: string, isError = false) => {
		refs.logIdRef.current += 1;
		dispatch({ type: 'ADD_LOG', payload: { id: refs.logIdRef.current, text, isError } });
	};

	const reportBounds = () => {
		if (!viewportRef.current) return;
		const rect = viewportRef.current.getBoundingClientRect();
		const dpr = window.devicePixelRatio ?? 1;
		window.electronAPI.sendViewportBounds({
			x: rect.x * dpr,
			y: rect.y * dpr,
			width: rect.width * dpr,
			height: rect.height * dpr,
		});
	};

	const reportBoundsDebounced = () => {
		if (refs.resizeTimerRef.current) clearTimeout(refs.resizeTimerRef.current);
		refs.resizeTimerRef.current = setTimeout(reportBounds, 200);
	};

	const send = (cmd: object) => window.engine.send(cmd as never);

	const actions = createEngineActions({
		dispatch,
		refs,
		addLog,
		reportBounds,
		send,
		projectType,
	});

	useEngineEffects({
		dispatch,
		refs,
		addLog,
		viewportRef,
		projectType,
		gameStyle,
		reportBounds,
		reportBoundsDebounced,
		applyInitialAnimationFrame: actions.applyInitialAnimationFrame,
		setLocale,
	});

	useEffect(() => {
		refs.initialSavePathRef.current = initialSavePath;
		refs.initialExtractDirRef.current = initialExtractDir;
	}, [initialSavePath, initialExtractDir]);

	// Mantener blueprintsRef sincronizado con el estado para acceso desde acciones
	useEffect(() => {
		refs.blueprintsRef.current = state.blueprints;
	}, [state.blueprints, refs.blueprintsRef]);

	useEffect(() => {
		refs.modelsRef.current = state.models;
	}, [state.models, refs.modelsRef]);

	refs.updateEntityTransformRef.current = actions.updateEntityTransform;

	const value: EngineContextValue = {
		...state,
		dispatch,
		pendingImportSceneRef: refs.pendingImportSceneRef,
		sceneImportInProgressRef: refs.sceneImportInProgressRef,
		modelReplaceInProgressRef: refs.modelReplaceInProgressRef,
		modelLoadOverlayKindRef: refs.modelLoadOverlayKindRef,
		modelAssetPreloadPendingRef: refs.modelAssetPreloadPendingRef,
		modelsRef: refs.modelsRef,
		sceneBurstLoadInProgressRef: refs.sceneBurstLoadInProgressRef,
		sceneBurstPendingColliderCountRef: refs.sceneBurstPendingColliderCountRef,
		sceneBurstPendingOpsRef: refs.sceneBurstPendingOpsRef,
		entityTransformsRef: refs.entityTransformsRef,
		entityMetaRef: refs.entityMetaRef,
		pendingRestoresRef: refs.pendingRestoresRef,
		quickBuildActiveBlueprintIdRef: refs.quickBuildActiveBlueprintIdRef,
		playerEntityIdRef: refs.playerEntityIdRef,
		editorCameraEntityIdRef: refs.editorCameraEntityIdRef,
		playCharacterViewRef: refs.playCharacterViewRef,
		pendingPlayCharacterViewRef: refs.pendingPlayCharacterViewRef,
		pendingModelPathRef: refs.pendingModelPathRef,
		pendingSpawnKindRef: refs.pendingSpawnKindRef,
		pendingSpawnCategoryRef: refs.pendingSpawnCategoryRef,
		pendingModelLoadQueueRef: refs.pendingModelLoadQueueRef,
		pendingBurstSpawnRestoreRef: refs.pendingBurstSpawnRestoreRef,
		mainPlayerHandled: refs.mainPlayerHandled,
		camera2dRef: refs.camera2dRef,
		projectLoaded2dMetaRef: refs.projectLoaded2dMetaRef,
		projectLoaded3dMetaRef: refs.projectLoaded3dMetaRef,
		send,
		sendAsync: actions.sendAsync,
		setAnimationPlaying: actions.setAnimationPlaying,
		models: state.models,
		loadModelAsset: actions.loadModelAsset,
		spawnModel: actions.spawnModel,
		replaceEntityModel: actions.replaceEntityModel,
		removeModelAsset: actions.removeModelAsset,
		getModelsList: actions.getModelsList,
		reportBounds,
		retryEngine: actions.retryEngine,
		removeScenario: actions.removeScenario,
		removeCharacter: actions.removeCharacter,
		removeEntity: actions.removeEntity,
		setWorldSize: actions.setWorldSize,
		setGridVisible: actions.setGridVisible,
		setGridCellSize: actions.setGridCellSize,
		setGravity: actions.setGravity,
		setDirectionalLight: actions.setDirectionalLight,
		setTargetFps: actions.setTargetFps,
		removeCollider: actions.removeCollider,
		removeExecutionArea: actions.removeExecutionArea,
		updateEntityAnimations: actions.updateEntityAnimations,
		updateEntityScripts: actions.updateEntityScripts,
		setEntityPhysics: actions.setEntityPhysics,
		updateEntityTransform: actions.updateEntityTransform,
		registerPivotEditListener: actions.registerPivotEditListener,
		unregisterPivotEditListener: actions.unregisterPivotEditListener,
		loadSprite: actions.loadSprite,
		removeSprite: actions.removeSprite,
		getSpritesList: actions.getSpritesList,
		loadCharacter: actions.loadCharacter,
		setPreviewPlaying: actions.setPreviewPlaying,
		addUiScreen: actions.addUiScreen,
		removeUiScreen: actions.removeUiScreen,
		beginUiScreenEdit: actions.beginUiScreenEdit,
		endUiScreenEdit: actions.endUiScreenEdit,
		addPlayerUiTextBox: actions.addPlayerUiTextBox,
		removePlayerUiTextBox: actions.removePlayerUiTextBox,
		addEditingUiButton: actions.addEditingUiButton,
		addPlayerUiImage: actions.addPlayerUiImage,
		removePlayerUiImage: actions.removePlayerUiImage,
		setPlayerUiHudElementProps: actions.setPlayerUiHudElementProps,
		removeEditingUiPlaceholder: actions.removeEditingUiPlaceholder,
		loadHudImage: actions.loadHudImage,
		removeHudImage: actions.removeHudImage,
		setDebugMode: actions.setDebugMode,
		setBackground: actions.setBackground,
		loadSound: actions.loadSound,
		removeSound: actions.removeSound,
		loadFont: actions.loadFont,
		removeFont: actions.removeFont,
		loadBackgroundToLibrary: actions.loadBackgroundToLibrary,
		removeBackgroundFromLibrary: actions.removeBackgroundFromLibrary,
		addBlueprint: actions.addBlueprint,
		setBlueprints: actions.setBlueprints,
		registerQuickBuildClickListener: actions.registerQuickBuildClickListener,
		unregisterQuickBuildClickListener: actions.unregisterQuickBuildClickListener,
	};

	return (
		<EngineContext.Provider value={value}>
			{children}
		</EngineContext.Provider>
	);
}

export function useContextEngine() {
	const ctx = useContext(EngineContext);
	if (!ctx) throw new Error('useContextEngine debe usarse dentro de <EngineProvider>');
	return ctx;
}