import React, { createContext, useContext, useEffect, useReducer, useRef } from 'react';
import type { ProjectSaveData } from '@shared-types';
import { createEngineActions } from './hooks/createEngineActions';
import { useEngineEffects } from './hooks/useEngineEffects';
import {
	engineReducer,
	initialState,
	type Camera2dState,
	type ColliderPoints,
	type EngineContextValue,
	type EngineInternalRefs,
	type EntityMeta,
	type PendingRestore,
	type Transform,
} from './types';

const EngineContext = createContext<EngineContextValue | undefined>(undefined);

export function EngineProvider({
	children,
	viewportRef,
	projectType,
	initialSave,
}: {
	children: React.ReactNode;
	viewportRef: React.RefObject<HTMLDivElement | null>;
	projectType?: string;
	initialSave?: ProjectSaveData | null;
}) {
	const [state, dispatch] = useReducer(engineReducer, initialState);

	const refs: EngineInternalRefs = {
		readyTimer: useRef<ReturnType<typeof setTimeout> | null>(null),
		resizeTimerRef: useRef<ReturnType<typeof setTimeout> | null>(null),
		logIdRef: useRef(0),
		initialSaveRef: useRef(initialSave),
		entityTransformsRef: useRef<Record<number, Transform>>({}),
		entityMetaRef: useRef<Record<number, EntityMeta>>({}),
		pendingRestoresRef: useRef<Map<string, PendingRestore[]>>(new Map()),
		playerEntityIdRef: useRef<number | null>(null),
		camera2dRef: useRef<Camera2dState | null>(null),
		mainPlayerHandled: useRef(false),
		playerRemoved: useRef(false),
		pendingPlayerDups: useRef<Transform[]>([]),
		pendingDupQ: useRef<Transform[]>([]),
		pivotEditListenerRef: useRef<((framePath: string, px: number, py: number) => void) | null>(null),
		quickBuildClickListenerRef: useRef<((x: number, y: number, fitToGrid: boolean, scale?: [number, number, number]) => void) | null>(null),
		pendingEventsRef: useRef<Map<string, { resolve: (value: any) => void }>>(new Map()),
		blueprintsRef: useRef([]),
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
	});

	useEngineEffects({
		dispatch,
		refs,
		addLog,
		viewportRef,
		projectType,
		reportBounds,
		reportBoundsDebounced,
		applyInitialAnimationFrame: actions.applyInitialAnimationFrame,
	});

	// Cargar blueprints desde el guardado inicial al montar
	useEffect(() => {
		const saved = refs.initialSaveRef.current;
		if (saved?.blueprints && saved.blueprints.length > 0) {
			actions.setBlueprints(saved.blueprints);
		}
	// eslint-disable-next-line react-hooks/exhaustive-deps
	}, []);

	// Mantener blueprintsRef sincronizado con el estado para acceso desde acciones
	useEffect(() => {
		refs.blueprintsRef.current = state.blueprints;
	}, [state.blueprints, refs.blueprintsRef]);

	const value: EngineContextValue = {
		...state,
		entityTransformsRef: refs.entityTransformsRef,
		entityMetaRef: refs.entityMetaRef,
		pendingRestoresRef: refs.pendingRestoresRef,
		playerEntityIdRef: refs.playerEntityIdRef,
		camera2dRef: refs.camera2dRef,
		send,
		sendAsync: actions.sendAsync,
		setAnimationPlaying: actions.setAnimationPlaying,
		loadModel: actions.loadModel,
		reportBounds,
		retryEngine: actions.retryEngine,
		removeScenario: actions.removeScenario,
		removeCharacter: actions.removeCharacter,
		setWorldSize: actions.setWorldSize,
		setGridVisible: actions.setGridVisible,
		setGridCellSize: actions.setGridCellSize,
		setGravity: actions.setGravity,
		removeCollider: actions.removeCollider,
		removeExecutionArea: actions.removeExecutionArea,
		updateEntityAnimations: actions.updateEntityAnimations,
		updateEntityScripts: actions.updateEntityScripts,
		setEntityPhysics: actions.setEntityPhysics,
		registerPivotEditListener: actions.registerPivotEditListener,
		unregisterPivotEditListener: actions.unregisterPivotEditListener,
		loadSprite: actions.loadSprite,
		removeSprite: actions.removeSprite,
		getSpritesList: actions.getSpritesList,
		loadCharacter: actions.loadCharacter,
		setPreviewPlaying: actions.setPreviewPlaying,
		setBackground: actions.setBackground,
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