import type { MutableRefObject } from 'react';
import { DEFAULT_GRAVITY_MAGNITUDE, type BackgroundInfo, type BluePrintEntry, type DebugMetrics, type EntityCategory, type ProjectSaveData, type SavedControlBindings, type SavedPlayerTransform, type SoundInfo, type SpriteInfo } from '@shared-types';

export interface Entity {
	id: number
}

export interface SelectedEntity {
	id: number
	name: string
	position: [number, number, number]
	rotation: [number, number, number, number]
	scale: [number, number, number]
	physicsEnabled: boolean
	physicsType: string
	path?: string
	animations?: {
		name: string
		fps: number
		loop: boolean
		is_default?: boolean
		facing_right?: boolean
		logical_w: number
		logical_h: number
		frames: {
			path: string
			pivot_x: number
			pivot_y: number
		}[]
	}[]
	scripts?: { name: string; source: string }[]
}

export interface LogEntry {
	id: number
	text: string
	isError: boolean
}

export interface ScenarioEntry {
	id: number
	path: string
}

export type CharacterEntry = ScenarioEntry;

export interface WorldConfig {
	worldWidth: number
	worldHeight: number
	worldDepth: number
	gridVisible: boolean
	gridCellSize: number
	gravity: number
	targetFps: number
}

export const DEFAULT_WORLD_CONFIG: WorldConfig = {
	worldWidth: 100,
	worldHeight: 50,
	worldDepth: 100,
	gridVisible: true,
	gridCellSize: 1,
	gravity: DEFAULT_GRAVITY_MAGNITUDE,
	targetFps: 60,
};

export interface EngineState {
	engineReady: boolean
	engineError: string | null
	previewPlaying: boolean
	/** Overlay mientras el motor ejecuta `import_scene` (2D). */
	sceneImportLoading: boolean
	/** Incrementa al recibir `first_person_view_changed` (refrescar UI de cámara FP). */
	playCharacterViewSyncSeq: number
	log: LogEntry[]
	entities: Entity[]
	selectedEntity: SelectedEntity | null
	hoveredEntityId: number | null
	backgroundPath: string | null
	scenarioEntities: ScenarioEntry[]
	characterEntities: CharacterEntry[]
	worldConfig: WorldConfig
	colliderEntities: ScenarioEntry[]
	executionAreaEntities: ScenarioEntry[]
	toolProgress: number | null
	animationPlaying: Map<number, boolean>
	sprites: SpriteInfo[]
	loadedSpritesInfo: Map<string, { name: string }>
	models: import('@shared-types').ModelInfo[]
	loadedModelsInfo: Map<string, { name: string }>
	sounds: SoundInfo[]
	backgrounds: BackgroundInfo[]
	debugMetrics: DebugMetrics | null
	debugMode: boolean
	blueprints: BluePrintEntry[]
	multiSelectedIds: number[]
}

export type EngineAction =
	| { type: 'SET_READY' }
	| { type: 'SET_ERROR'; payload: string }
	| { type: 'SET_PREVIEW_PLAYING'; payload: boolean }
	| { type: 'SET_SCENE_IMPORT_LOADING'; payload: boolean }
	| { type: 'SYNC_PLAY_CHARACTER_VIEW' }
	| { type: 'ADD_LOG'; payload: LogEntry }
	| { type: 'ADD_ENTITY'; payload: number }
	| { type: 'SELECT_ENTITY'; payload: SelectedEntity }
	| { type: 'DESELECT_ENTITY' }
	| { type: 'ENGINE_STOPPED'; payload: number | undefined }
	| { type: 'CLEAR_ENTITIES' }
	| { type: 'RESET_ENGINE' }
	| { type: 'ADD_SCENARIO'; payload: ScenarioEntry }
	| { type: 'REMOVE_SCENARIO'; payload: number }
	| { type: 'ADD_CHARACTER'; payload: CharacterEntry }
	| { type: 'REMOVE_CHARACTER'; payload: number }
	| { type: 'SET_HOVER'; payload: number | null }
	| { type: 'SET_BACKGROUND'; payload: string | null }
	| { type: 'SET_WORLD_CONFIG'; payload: Partial<WorldConfig> }
	| { type: 'ADD_COLLIDER'; payload: ScenarioEntry }
	| { type: 'REMOVE_COLLIDER'; payload: number }
	| { type: 'ADD_EXECUTION_AREA'; payload: ScenarioEntry }
	| { type: 'REMOVE_EXECUTION_AREA'; payload: number }
	| { type: 'SET_TOOL_PROGRESS'; payload: number | null }
	| { type: 'SET_ANIMATION_PLAYING'; payload: { entityId: number; playing: boolean } }
	| { type: 'UPDATE_ENTITY_ANIMATIONS'; payload: { entityId: number; animations: NonNullable<SelectedEntity['animations']> } }
	| { type: 'UPDATE_SELECTED_PHYSICS'; payload: { entityId: number; enabled: boolean; bodyType: string } }
	| { type: 'ADD_SPRITE'; payload: SpriteInfo }
	| { type: 'REMOVE_SPRITE'; payload: string }
	| { type: 'SET_SPRITES'; payload: SpriteInfo[] }
	| { type: 'ADD_SPRITE_INFO'; payload: { path: string; name: string } }
	| { type: 'REMOVE_SPRITE_INFO'; payload: string }
	| { type: 'SET_LOADED_SPRITES_INFO'; payload: Array<{ path: string; name: string }> }
	| { type: 'ADD_MODEL_INFO'; payload: { path: string; name: string } }
	| { type: 'REMOVE_MODEL_INFO'; payload: string }
	| { type: 'SET_MODELS'; payload: import('@shared-types').ModelInfo[] }
	| { type: 'SET_DEBUG_MODE'; payload: boolean }
	| { type: 'SET_DEBUG_METRICS'; payload: DebugMetrics }
	| { type: 'ADD_BLUEPRINT'; payload: BluePrintEntry }
	| { type: 'SET_BLUEPRINTS'; payload: BluePrintEntry[] }
	| { type: 'SET_MULTI_SELECT'; payload: number[] }
	| { type: 'ADD_SOUND'; payload: SoundInfo }
	| { type: 'REMOVE_SOUND'; payload: string }
	| { type: 'SET_SOUNDS'; payload: SoundInfo[] }
	| { type: 'ADD_BACKGROUND'; payload: BackgroundInfo }
	| { type: 'REMOVE_BACKGROUND'; payload: string }
	| { type: 'SET_BACKGROUNDS'; payload: BackgroundInfo[] }
	| {
			type: 'IMPORT_SCENE_STATE'
			payload: {
				scenarioEntities: ScenarioEntry[]
				characterEntities: CharacterEntry[]
				colliderEntities: ScenarioEntry[]
				executionAreaEntities: ScenarioEntry[]
				entities: { id: number }[]
				backgroundPath: string | null
				sprites: SpriteInfo[]
			}
	  };

export const initialState: EngineState = {
	engineReady: false,
	engineError: null,
	previewPlaying: false,
	sceneImportLoading: false,
	playCharacterViewSyncSeq: 0,
	log: [],
	entities: [],
	selectedEntity: null,
	hoveredEntityId: null,
	backgroundPath: null,
	scenarioEntities: [],
	characterEntities: [],
	worldConfig: DEFAULT_WORLD_CONFIG,
	colliderEntities: [],
	executionAreaEntities: [],
	toolProgress: null,
	animationPlaying: new Map(),
	sprites: [],
	loadedSpritesInfo: new Map(),
	models: [],
	loadedModelsInfo: new Map(),
	sounds: [],
	backgrounds: [],
	debugMetrics: null,
	debugMode: false,
	blueprints: [],
	multiSelectedIds: [],
};

export function engineReducer(state: EngineState, action: EngineAction): EngineState {
	const handlers: Record<string, (prevState: EngineState, nextAction: any) => EngineState> = {
		SET_READY: (prevState) => ({ ...prevState, engineReady: true, engineError: null, previewPlaying: false }),
		SET_ERROR: (prevState, nextAction) => ({ ...prevState, engineError: nextAction.payload }),
		SET_PREVIEW_PLAYING: (prevState, nextAction) => ({ ...prevState, previewPlaying: nextAction.payload }),
		SET_SCENE_IMPORT_LOADING: (prevState, nextAction) => ({
			...prevState,
			sceneImportLoading: nextAction.payload,
		}),
		SYNC_PLAY_CHARACTER_VIEW: (prevState) => ({ ...prevState, playCharacterViewSyncSeq: prevState.playCharacterViewSyncSeq + 1 }),
		ADD_LOG: (prevState, nextAction) => ({ ...prevState, log: [...prevState.log.slice(-199), nextAction.payload] }),
		ADD_ENTITY: (prevState, nextAction) =>
			prevState.entities.some((entity) => entity.id === nextAction.payload)
				? prevState
				: { ...prevState, entities: [...prevState.entities, { id: nextAction.payload }] },
		SELECT_ENTITY: (prevState, nextAction) => ({ ...prevState, selectedEntity: nextAction.payload }),
		DESELECT_ENTITY: (prevState) => ({ ...prevState, selectedEntity: null, multiSelectedIds: [] }),
		ENGINE_STOPPED: (prevState, nextAction) => {
			const code = nextAction.payload;
			const error = code !== 0 && code != null
				? `El motor terminó inesperadamente (código ${code}).`
				: null;
			return { ...prevState, engineReady: false, previewPlaying: false, ...(error ? { engineError: error } : {}) };
		},
		CLEAR_ENTITIES: (prevState) => ({ ...prevState, entities: [], multiSelectedIds: [] }),
		RESET_ENGINE: (prevState) => ({ ...prevState, engineReady: false, engineError: null, previewPlaying: false, entities: [], multiSelectedIds: [] }),
		ADD_SCENARIO: (prevState, nextAction) => ({ ...prevState, scenarioEntities: [...prevState.scenarioEntities, nextAction.payload] }),
		REMOVE_SCENARIO: (prevState, nextAction) => ({
			...prevState,
			scenarioEntities: prevState.scenarioEntities.filter((scenario) => scenario.id !== nextAction.payload),
		}),
		ADD_CHARACTER: (prevState, nextAction) => ({ ...prevState, characterEntities: [...prevState.characterEntities, nextAction.payload] }),
		REMOVE_CHARACTER: (prevState, nextAction) => ({
			...prevState,
			characterEntities: prevState.characterEntities.filter((character) => character.id !== nextAction.payload),
		}),
		SET_HOVER: (prevState, nextAction) => ({ ...prevState, hoveredEntityId: nextAction.payload }),
		SET_BACKGROUND: (prevState, nextAction) => ({ ...prevState, backgroundPath: nextAction.payload }),
		SET_WORLD_CONFIG: (prevState, nextAction) => ({
			...prevState,
			worldConfig: { ...prevState.worldConfig, ...nextAction.payload },
		}),
		ADD_COLLIDER: (prevState, nextAction) => ({ ...prevState, colliderEntities: [...prevState.colliderEntities, nextAction.payload] }),
		REMOVE_COLLIDER: (prevState, nextAction) => ({
			...prevState,
			colliderEntities: prevState.colliderEntities.filter((collider) => collider.id !== nextAction.payload),
		}),
		ADD_EXECUTION_AREA: (prevState, nextAction) => ({ ...prevState, executionAreaEntities: [...prevState.executionAreaEntities, nextAction.payload] }),
		REMOVE_EXECUTION_AREA: (prevState, nextAction) => ({
			...prevState,
			executionAreaEntities: prevState.executionAreaEntities.filter((area) => area.id !== nextAction.payload),
		}),
		SET_TOOL_PROGRESS: (prevState, nextAction) => ({ ...prevState, toolProgress: nextAction.payload }),
		SET_ANIMATION_PLAYING: (prevState, nextAction) => {
			const nextMap = new Map(prevState.animationPlaying);
			nextMap.set(nextAction.payload.entityId, nextAction.payload.playing);
			return { ...prevState, animationPlaying: nextMap };
		},
		UPDATE_ENTITY_ANIMATIONS: (prevState, nextAction) => {
			if (prevState.selectedEntity?.id !== nextAction.payload.entityId) return prevState;
			return {
				...prevState,
				selectedEntity: { ...prevState.selectedEntity, animations: nextAction.payload.animations },
			};
		},
		UPDATE_SELECTED_PHYSICS: (prevState, nextAction) => {
			if (prevState.selectedEntity?.id !== nextAction.payload.entityId) return prevState;
			return {
				...prevState,
				selectedEntity: {
					...prevState.selectedEntity,
					physicsEnabled: nextAction.payload.enabled,
					physicsType: nextAction.payload.bodyType,
				},
			};
		},
		ADD_SPRITE: (prevState, nextAction) =>
			prevState.sprites.some((sprite) => sprite.path === nextAction.payload.path)
				? prevState
				: { ...prevState, sprites: [...prevState.sprites, nextAction.payload] },
		REMOVE_SPRITE: (prevState, nextAction) => ({ ...prevState, sprites: prevState.sprites.filter((sprite) => sprite.path !== nextAction.payload) }),
		SET_SPRITES: (prevState, nextAction) => ({ ...prevState, sprites: nextAction.payload }),
		ADD_SPRITE_INFO: (prevState, nextAction) => {
			const nextMap = new Map(prevState.loadedSpritesInfo);
			nextMap.set(nextAction.payload.path, { name: nextAction.payload.name });
			return { ...prevState, loadedSpritesInfo: nextMap };
		},
		REMOVE_SPRITE_INFO: (prevState, nextAction) => {
			const nextMap = new Map(prevState.loadedSpritesInfo);
			nextMap.delete(nextAction.payload);
			return { ...prevState, loadedSpritesInfo: nextMap };
		},
		SET_LOADED_SPRITES_INFO: (prevState, nextAction) => {
			const nextMap = new Map<string, { name: string }>();
			for (const item of nextAction.payload) {
				nextMap.set(item.path, { name: item.name });
			}
			return { ...prevState, loadedSpritesInfo: nextMap };
		},
		ADD_MODEL_INFO: (prevState, nextAction) => {
			const nextMap = new Map(prevState.loadedModelsInfo);
			nextMap.set(nextAction.payload.path, { name: nextAction.payload.name });
			const entry = { path: nextAction.payload.path, name: nextAction.payload.name };
			const exists = prevState.models.some((m) => m.path === entry.path);
			return {
				...prevState,
				loadedModelsInfo: nextMap,
				models: exists ? prevState.models : [...prevState.models, entry],
			};
		},
		REMOVE_MODEL_INFO: (prevState, nextAction) => {
			const nextMap = new Map(prevState.loadedModelsInfo);
			nextMap.delete(nextAction.payload);
			return {
				...prevState,
				loadedModelsInfo: nextMap,
				models: prevState.models.filter((m) => m.path !== nextAction.payload),
			};
		},
		SET_MODELS: (prevState, nextAction) => ({ ...prevState, models: nextAction.payload }),
		SET_DEBUG_MODE: (prevState, nextAction) => ({ ...prevState, debugMode: nextAction.payload }),
		SET_DEBUG_METRICS: (prevState, nextAction) => ({ ...prevState, debugMetrics: nextAction.payload }),
		ADD_BLUEPRINT: (prevState, nextAction) => ({ ...prevState, blueprints: [...prevState.blueprints, nextAction.payload] }),
		SET_BLUEPRINTS: (prevState, nextAction) => ({ ...prevState, blueprints: nextAction.payload }),
		SET_MULTI_SELECT: (prevState, nextAction) => ({ ...prevState, multiSelectedIds: nextAction.payload }),
		ADD_SOUND: (prevState, nextAction) =>
			prevState.sounds.some((s) => s.path === nextAction.payload.path)
				? prevState
				: { ...prevState, sounds: [...prevState.sounds, nextAction.payload] },
		REMOVE_SOUND: (prevState, nextAction) => ({ ...prevState, sounds: prevState.sounds.filter((s) => s.path !== nextAction.payload) }),
		SET_SOUNDS: (prevState, nextAction) => ({ ...prevState, sounds: nextAction.payload }),
		ADD_BACKGROUND: (prevState, nextAction) =>
			prevState.backgrounds.some((b) => b.path === nextAction.payload.path)
				? prevState
				: { ...prevState, backgrounds: [...prevState.backgrounds, nextAction.payload] },
		REMOVE_BACKGROUND: (prevState, nextAction) => ({ ...prevState, backgrounds: prevState.backgrounds.filter((b) => b.path !== nextAction.payload) }),
		SET_BACKGROUNDS: (prevState, nextAction) => ({ ...prevState, backgrounds: nextAction.payload }),
		IMPORT_SCENE_STATE: (prevState, nextAction) => ({
			...prevState,
			scenarioEntities: nextAction.payload.scenarioEntities,
			characterEntities: nextAction.payload.characterEntities,
			colliderEntities: nextAction.payload.colliderEntities,
			executionAreaEntities: nextAction.payload.executionAreaEntities,
			entities: nextAction.payload.entities,
			backgroundPath: nextAction.payload.backgroundPath,
			sprites: nextAction.payload.sprites,
			selectedEntity: null,
			multiSelectedIds: [],
		}),
	};

	const handler = handlers[action.type as keyof typeof handlers];
	return handler ? handler(state, action) : state;
}

export type Transform = {
	position: [number, number, number]
	rotation: [number, number, number, number]
	scale: [number, number, number]
};

export type ColliderPoints = [[number, number], [number, number], [number, number], [number, number]];

export type EntityAnimations = NonNullable<SelectedEntity['animations']>;
export type EntityScripts = NonNullable<SelectedEntity['scripts']>;

export interface EntityMeta {
	kind: 'scenario' | 'character' | 'model' | 'collider' | 'execution_area'
	path: string
	name?: string
	physicsEnabled: boolean
	physicsType: string
	points?: ColliderPoints
	animations?: EntityAnimations
	scripts?: EntityScripts
	controlBindings?: SavedControlBindings
	/** ID de la blueprint desde la que fue instanciada esta entidad. */
	blueprintId?: string
	/** Entorno 3D creado desde acordeón Entorno (UI solo colisión). */
	entityCategory?: EntityCategory
	/** Modelo visual cargado (distinto de path lógico `[Player]` / `[EditorBox]`). */
	visualModelPath?: string
}

export interface PendingRestore {
	transform: Transform
	name?: string
	physicsEnabled: boolean
	physicsType: string
	animations?: any[]
	scripts?: EntityScripts
	controlBindings?: SavedControlBindings
	/** ID de la blueprint desde la que fue instanciada esta entidad. */
	blueprintId?: string
	entityCategory?: EntityCategory
	visualModelPath?: string
}

export interface Camera2dState {
	x: number
	y: number
	halfH: number
}

export interface EngineInternalRefs {
	readyTimer: MutableRefObject<ReturnType<typeof setTimeout> | null>
	resizeTimerRef: MutableRefObject<ReturnType<typeof setTimeout> | null>
	logIdRef: MutableRefObject<number>
	initialSaveRef: MutableRefObject<ProjectSaveData | null | undefined>
	entityTransformsRef: MutableRefObject<Record<number, Transform>>
	entityMetaRef: MutableRefObject<Record<number, EntityMeta>>
	pendingRestoresRef: MutableRefObject<Map<string, PendingRestore[]>>
	playerEntityIdRef: MutableRefObject<number | null>
	playCharacterViewRef: MutableRefObject<import('@shared-types').SavedPlayerTransform | null>
	pendingPlayCharacterViewRef: MutableRefObject<import('@shared-types').SavedPlayerTransform | null>
	pendingModelPathRef: MutableRefObject<string | null>
	pendingSpawnKindRef: MutableRefObject<EntityMeta['kind'] | null>
	pendingSpawnCategoryRef: MutableRefObject<EntityCategory | null>
	pendingModelLoadQueueRef: MutableRefObject<Array<{ modelPath: string; pending: PendingRestore }>>
	camera2dRef: MutableRefObject<Camera2dState | null>
	mainPlayerHandled: MutableRefObject<boolean>
	playerRemoved: MutableRefObject<boolean>
	pendingPlayerDups: MutableRefObject<Transform[]>
	pendingDupQ: MutableRefObject<Transform[]>
	pivotEditListenerRef: MutableRefObject<((framePath: string, px: number, py: number) => void) | null>
	quickBuildClickListenerRef: MutableRefObject<((x: number, y: number, fitToGrid: boolean, scale?: [number, number, number]) => void) | null>
	pendingEventsRef: MutableRefObject<Map<string, { resolve: (value: any) => void }>>
	blueprintsRef: MutableRefObject<BluePrintEntry[]>
	/** Escena 2D pendiente de sincronizar tras `scene_imported`. */
	pendingImportSceneRef: MutableRefObject<import('@shared-types').SavedScene | null>
	/** Evita duplicar estado React mientras el motor emite eventos de carga por entidad. */
	sceneImportInProgressRef: MutableRefObject<boolean>
	/** Overlay de carga durante ráfaga IPC 3D (pestañas / `ready`). */
	sceneBurstLoadInProgressRef: MutableRefObject<boolean>
	/** FP: esperar `play_character_view_changed` tras restore del jugador. */
	sceneBurstAwaitingPlayerViewRef: MutableRefObject<boolean>
	/** Colisionadores 3D enviados sin cola de restore. */
	sceneBurstPendingColliderCountRef: MutableRefObject<number>
}

export interface EngineContextValue extends EngineState {
	dispatch: (action: EngineAction) => void
	pendingImportSceneRef: MutableRefObject<import('@shared-types').SavedScene | null>
	sceneImportInProgressRef: MutableRefObject<boolean>
	sceneBurstLoadInProgressRef: MutableRefObject<boolean>
	sceneBurstAwaitingPlayerViewRef: MutableRefObject<boolean>
	sceneBurstPendingColliderCountRef: MutableRefObject<number>
	entityTransformsRef: MutableRefObject<Record<number, Transform>>
	entityMetaRef: MutableRefObject<Record<number, EntityMeta>>
	pendingRestoresRef: MutableRefObject<Map<string, PendingRestore[]>>
	playerEntityIdRef: MutableRefObject<number | null>
	playCharacterViewRef: MutableRefObject<SavedPlayerTransform | null>
	pendingPlayCharacterViewRef: MutableRefObject<SavedPlayerTransform | null>
	pendingModelPathRef: MutableRefObject<string | null>
	pendingSpawnKindRef: MutableRefObject<EntityMeta['kind'] | null>
	pendingSpawnCategoryRef: MutableRefObject<EntityCategory | null>
	pendingModelLoadQueueRef: MutableRefObject<Array<{ modelPath: string; pending: PendingRestore }>>
	mainPlayerHandled: MutableRefObject<boolean>
	camera2dRef: MutableRefObject<Camera2dState | null>
	send: (cmd: object) => void
	sendAsync: <T>(cmd: object, waitForEvent: string, onStart?: () => void) => Promise<T>
	setAnimationPlaying: (entityId: number, playing: boolean) => void
	loadModelAsset: (path: string, name: string) => void
	spawnModel: (path: string, kind?: EntityMeta['kind'], category?: EntityCategory) => void
	replaceEntityModel: (entityId: number, modelPath: string) => void
	removeModelAsset: (path: string) => void
	getModelsList: () => void
	reportBounds: () => void
	retryEngine: () => void
	removeScenario: (id: number) => void
	removeCharacter: (id: number) => void
	setWorldSize: (width: number, height: number, depth?: number) => void
	setGridVisible: (visible: boolean) => void
	setGridCellSize: (size: number) => void
	setGravity: (gravity: number) => void
	setTargetFps: (fps: number) => void
	removeCollider: (id: number) => void
	removeExecutionArea: (id: number) => void
	updateEntityAnimations: (id: number, animations: any[]) => any[]
	updateEntityScripts: (id: number, scripts: EntityScripts) => void
	setEntityPhysics: (id: number, enabled: boolean, bodyType: string) => void
	registerPivotEditListener: (fn: (framePath: string, px: number, py: number) => void) => void
	unregisterPivotEditListener: () => void
	loadSprite: (path: string, name: string) => void
	removeSprite: (path: string) => void
	getSpritesList: () => void
	loadCharacter: (path: string) => void
	setPreviewPlaying: (playing: boolean) => void
	setBackground: (path: string | null) => void
	loadSound: (path: string, name: string) => void
	removeSound: (path: string) => void
	loadBackgroundToLibrary: (path: string, name: string) => void
	removeBackgroundFromLibrary: (path: string) => void
	addBlueprint: (entry: BluePrintEntry) => void
	setBlueprints: (entries: BluePrintEntry[]) => void
	registerQuickBuildClickListener: (fn: (x: number, y: number, fitToGrid: boolean, scale?: [number, number, number]) => void) => void
	unregisterQuickBuildClickListener: () => void
	setDebugMode: (show: boolean) => void
}