import type { MutableRefObject } from 'react';
import type { ProjectSaveData, SavedControlBindings, SpriteInfo } from '@shared-types';

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
	gridVisible: boolean
	gridCellSize: number
}

export const DEFAULT_WORLD_CONFIG: WorldConfig = {
	worldWidth: 100,
	worldHeight: 50,
	gridVisible: true,
	gridCellSize: 1,
};

export interface EngineState {
	engineReady: boolean
	engineError: string | null
	previewPlaying: boolean
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
}

export type EngineAction =
	| { type: 'SET_READY' }
	| { type: 'SET_ERROR'; payload: string }
	| { type: 'SET_PREVIEW_PLAYING'; payload: boolean }
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
	| { type: 'UPDATE_SELECTED_PHYSICS'; payload: { entityId: number; enabled: boolean; bodyType: string } }
	| { type: 'ADD_SPRITE'; payload: SpriteInfo }
	| { type: 'REMOVE_SPRITE'; payload: string }
	| { type: 'SET_SPRITES'; payload: SpriteInfo[] }
	| { type: 'ADD_SPRITE_INFO'; payload: { path: string; name: string } }
	| { type: 'REMOVE_SPRITE_INFO'; payload: string }
	| { type: 'SET_LOADED_SPRITES_INFO'; payload: Array<{ path: string; name: string }> };

export const initialState: EngineState = {
	engineReady: false,
	engineError: null,
	previewPlaying: false,
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
};

export function engineReducer(state: EngineState, action: EngineAction): EngineState {
	const handlers: Record<string, (prevState: EngineState, nextAction: any) => EngineState> = {
		SET_READY: (prevState) => ({ ...prevState, engineReady: true, engineError: null, previewPlaying: false }),
		SET_ERROR: (prevState, nextAction) => ({ ...prevState, engineError: nextAction.payload }),
		SET_PREVIEW_PLAYING: (prevState, nextAction) => ({ ...prevState, previewPlaying: nextAction.payload }),
		ADD_LOG: (prevState, nextAction) => ({ ...prevState, log: [...prevState.log.slice(-199), nextAction.payload] }),
		ADD_ENTITY: (prevState, nextAction) =>
			prevState.entities.some((entity) => entity.id === nextAction.payload)
				? prevState
				: { ...prevState, entities: [...prevState.entities, { id: nextAction.payload }] },
		SELECT_ENTITY: (prevState, nextAction) => ({ ...prevState, selectedEntity: nextAction.payload }),
		DESELECT_ENTITY: (prevState) => ({ ...prevState, selectedEntity: null }),
		ENGINE_STOPPED: (prevState, nextAction) => {
			const code = nextAction.payload;
			const error = code !== 0 && code != null
				? `El motor terminó inesperadamente (código ${code}).`
				: null;
			return { ...prevState, engineReady: false, previewPlaying: false, ...(error ? { engineError: error } : {}) };
		},
		CLEAR_ENTITIES: (prevState) => ({ ...prevState, entities: [] }),
		RESET_ENGINE: (prevState) => ({ ...prevState, engineReady: false, engineError: null, previewPlaying: false, entities: [] }),
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
		ADD_SPRITE: (prevState, nextAction) => ({ ...prevState, sprites: [...prevState.sprites, nextAction.payload] }),
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
}

export interface PendingRestore {
	transform: Transform
	name?: string
	physicsEnabled: boolean
	physicsType: string
	animations?: any[]
	scripts?: EntityScripts
	controlBindings?: SavedControlBindings
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
	camera2dRef: MutableRefObject<Camera2dState | null>
	mainPlayerHandled: MutableRefObject<boolean>
	playerRemoved: MutableRefObject<boolean>
	pendingPlayerDups: MutableRefObject<Transform[]>
	pendingDupQ: MutableRefObject<Transform[]>
	pivotEditListenerRef: MutableRefObject<((framePath: string, px: number, py: number) => void) | null>
	pendingEventsRef: MutableRefObject<Map<string, { resolve: (value: any) => void }>>
}

export interface EngineContextValue extends EngineState {
	entityTransformsRef: MutableRefObject<Record<number, Transform>>
	entityMetaRef: MutableRefObject<Record<number, EntityMeta>>
	pendingRestoresRef: MutableRefObject<Map<string, PendingRestore[]>>
	playerEntityIdRef: MutableRefObject<number | null>
	camera2dRef: MutableRefObject<Camera2dState | null>
	send: (cmd: object) => void
	sendAsync: <T>(cmd: object, waitForEvent: string, onStart?: () => void) => Promise<T>
	setAnimationPlaying: (entityId: number, playing: boolean) => void
	loadModel: (path: string) => void
	reportBounds: () => void
	retryEngine: () => void
	removeScenario: (id: number) => void
	duplicateScenario: (id: number) => void
	removeCharacter: (id: number) => void
	duplicateCharacter: (id: number) => void
	setWorldSize: (width: number, height: number) => void
	setGridVisible: (visible: boolean) => void
	setGridCellSize: (size: number) => void
	removeCollider: (id: number) => void
	removeExecutionArea: (id: number) => void
	updateEntityAnimations: (id: number, animations: any[]) => void
	updateEntityScripts: (id: number, scripts: EntityScripts) => void
	registerPivotEditListener: (fn: (framePath: string, px: number, py: number) => void) => void
	unregisterPivotEditListener: () => void
	loadSprite: (path: string, name: string) => void
	removeSprite: (path: string) => void
	getSpritesList: () => void
	loadCharacter: (path: string) => void
	setPreviewPlaying: (playing: boolean) => void
	setBackground: (path: string | null) => void
}