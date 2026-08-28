import type { MutableRefObject } from 'react';
import { default2dPlayerUiScreens, default3dPlayerUiScreens } from '../../defaults/fpPlayerUiDefaults';
import type { ModelLoadOverlayKind } from './hooks/sceneImportOverlay';
import {
	DEFAULT_GRAVITY_MAGNITUDE,
	DEFAULT_LIGHT_AMBIENT,
	DEFAULT_LIGHT_INTENSITY,
	DEFAULT_SHADOW_DARKNESS,
	type BackgroundInfo,
	type BluePrintEntry,
	type DebugMetrics,
	type EngineCommand2D,
	type EngineCommand3D,
	type Entity3DCategory,
	type EntityCategory,
	type ModelCategory,
	type ModelInfo,
	type ProjectLoaded2dPayload,
	type ProjectLoaded3dPayload,
	type SavedAnimation,
	type SavedControlBindings,
	type SavedPlayerTransform,
	type SavedScene,
	type FontInfo,
	type GameStyle,
	type HudImageInfo,
	type SoundInfo,
	type SpriteInfo,
} from '@shared-types';
import {
	DEFAULT_PLAYER_UI_BUTTON_CONFIG,
	type PlayerUiButtonConfig,
} from '../../pages/EngineView/components/sidebar/UIAccordion/components/playerUiButtonModel';

export type { PlayerUiButtonConfig };

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
	/** Ruta del modelo visual 3D (FBX/GLB) en la entidad. */
	visualModelPath?: string
	animations?: SavedAnimation[]
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

export type GraphicsTextureTier = 'low' | 'medium' | 'high' | 'ultra';

export function normalizeGraphicsTextureTier(value: unknown): GraphicsTextureTier {
	const s = String(value ?? 'medium').trim().toLowerCase();
	if (s === 'low' || s === 'medium' || s === 'high' || s === 'ultra') return s;
	return 'medium';
}

export type ReflectionTier = 'off' | 'low' | 'medium' | 'high' | 'ultra';

export function normalizeReflectionTier(value: unknown): ReflectionTier {
	const s = String(value ?? 'off').trim().toLowerCase();
	if (s === 'off' || s === 'disabled' || s === 'none') return 'off';
	if (s === 'low') return 'low';
	if (s === 'medium' || s === 'medio') return 'medium';
	if (s === 'high') return 'high';
	if (s === 'ultra') return 'ultra';
	return 'off';
}

/** RT es independiente del tier; solo se lee del .save si está guardado explícitamente. */
export function resolveReflectionRaytracingFromSave(
	_tier: ReflectionTier,
	wire: unknown,
): boolean {
	if (wire !== undefined && wire !== null) return Boolean(wire);
	return false;
}

export type ShadowTier = 'off' | 'low' | 'medium' | 'high' | 'ultra';

export function normalizeShadowTier(value: unknown): ShadowTier {
	const s = String(value ?? 'low').trim().toLowerCase();
	if (s === 'off' || s === 'apagado') return 'off';
	if (s === 'low' || s === 'bajo') return 'low';
	if (s === 'medium' || s === 'medio') return 'medium';
	if (s === 'high' || s === 'alto') return 'high';
	if (s === 'ultra') return 'ultra';
	return 'low';
}

/** Low = Off (1 muestra). Medium 2x, High 4x, Ultra 8x (clamp GPU). */
export type MsaaTier = 'low' | 'medium' | 'high' | 'ultra';

export function normalizeMsaaTier(value: unknown): MsaaTier {
	const s = String(value ?? 'low').trim().toLowerCase();
	if (s === 'low' || s === 'bajo' || s === 'off' || s === 'apagado') return 'low';
	if (s === 'medium' || s === 'medio') return 'medium';
	if (s === 'high' || s === 'alto') return 'high';
	if (s === 'ultra') return 'ultra';
	return 'low';
}

export type ReflectionDebugView = 'final' | 'ssr_debug' | 'ssr_miss_green' | 'ssr_exit_reason' | 'ssr_vector_rgb' | 'ssr_hit_class' | 'ssr_path_px' | 'ssr_march_refl_dir' | 'ssr_hit_uv' | 'ssr_hit_sample_color' | 'ssr_proj_depth_delta' | 'ssr_ray_overlay'

export function normalizeReflectionDebugView(value: unknown): ReflectionDebugView {
	const s = String(value ?? 'final').trim().toLowerCase()
	if (
		s === 'ssr_debug'
		|| s === 'ssrdebug'
		|| s === 'ssr'
		|| s === 'debug'
		|| s === 'ssr_hits'
		|| s === 'ssrhits'
		|| s === 'hits'
	) {
		return 'ssr_debug'
	}
	if (s === 'ssr_miss_green' || s === 'miss_green' || s === 'missgreen' || s === 'green') {
		return 'ssr_miss_green'
	}
	if (s === 'ssr_exit_reason' || s === 'exit_reason' || s === 'exitreason') {
		return 'ssr_exit_reason'
	}
	if (s === 'ssr_vector_rgb' || s === 'vector_rgb' || s === 'vectorrgb' || s === 'refl_vector') {
		return 'ssr_vector_rgb'
	}
	if (
		s === 'ssr_hit_class'
		|| s === 'ssr_hitclass'
		|| s === 'hit_class'
		|| s === 'self_hit'
		|| s === 'ssr_self_hit'
	) {
		return 'ssr_hit_class'
	}
	if (s === 'ssr_path_px' || s === 'ssr_pathpx' || s === 'path_px' || s === 'ssr_ray_path') {
		return 'ssr_path_px'
	}
	if (
		s === 'ssr_march_refl_dir'
		|| s === 'ssr_refl_dir'
		|| s === 'ssr_r_world'
		|| s === 'r_world'
		|| s === 'march_refl_dir'
	) {
		return 'ssr_march_refl_dir'
	}
	if (s === 'ssr_hit_uv' || s === 'ssr_hituv' || s === 'hit_uv' || s === 'ssr_sample_uv') {
		return 'ssr_hit_uv'
	}
	if (
		s === 'ssr_hit_sample_color'
		|| s === 'ssr_sample_color'
		|| s === 'hit_sample_color'
		|| s === 'ssr_lit_at_hit'
	) {
		return 'ssr_hit_sample_color'
	}
	if (
		s === 'ssr_proj_depth_delta'
		|| s === 'ssr_start_cs_z_delta'
		|| s === 'proj_depth_delta'
		|| s === 'start_cs_z_delta'
	) {
		return 'ssr_proj_depth_delta'
	}
	if (
		s === 'ssr_ray_overlay'
		|| s === 'ssr_rayoverlay'
		|| s === 'ssr_rays'
		|| s === 'ray_overlay'
	) {
		return 'ssr_ray_overlay'
	}
	return 'final'
}

export interface WorldConfig {
	worldWidth: number
	worldHeight: number
	worldDepth: number
	/** 3D: radio de la esfera de límites del mundo. */
	worldRadius: number
	gridVisible: boolean
	gridCellSize: number
	gravity: number
	targetFps: number
	lightAmbient: number
	lightIntensity: number
	shadowDarkness: number
	graphicsTextureTier: GraphicsTextureTier
	textureDetailDistance: number
	reflectionTier: ReflectionTier
	/** Tier que corre el motor si difiere del pedido (p. ej. High → Medium sin RT). */
	reflectionTierEffective?: ReflectionTier
	/** Si la GPU expone ray query (evento reflection_tier_effective). */
	reflectionRtAvailable?: boolean
	/** Ray tracing HW (toggle independiente del tier). */
	reflectionRaytracing: boolean
	/** Reflection probes (cubemap IBL; independiente del SSR). */
	reflectionProbes: boolean
	reflectionDebugView: ReflectionDebugView
	ssrDebugMode: boolean
	shadowTier: ShadowTier
	msaaTier: MsaaTier
	taaEnabled: boolean
	taaBlend: number
	taaJitterScale: number
}

export const DEFAULT_WORLD_CONFIG: WorldConfig = {
	worldWidth: 100,
	worldHeight: 50,
	worldDepth: 100,
	worldRadius: 50,
	gridVisible: true,
	gridCellSize: 1,
	gravity: DEFAULT_GRAVITY_MAGNITUDE,
	targetFps: 60,
	lightAmbient: DEFAULT_LIGHT_AMBIENT,
	lightIntensity: DEFAULT_LIGHT_INTENSITY,
	shadowDarkness: DEFAULT_SHADOW_DARKNESS,
	graphicsTextureTier: 'medium',
	textureDetailDistance: 10,
	reflectionTier: 'off',
	reflectionRaytracing: false,
	reflectionProbes: false,
	reflectionDebugView: 'final',
	ssrDebugMode: false,
	shadowTier: 'low',
	msaaTier: 'low',
	taaEnabled: true,
	taaBlend: 0.62,
	taaJitterScale: 1.0,
};

export type UiScreenScope = 'player' | 'menu';

/** Normaliza flags `active` al cargar proyecto (sin forzar ninguna activa). */
export function normalizePlayerUiScreens(screens: UiScreenEntry[]): UiScreenEntry[] {
	return screens.map((s) => ({ ...s, active: Boolean(s.active) }));
}

export interface UiScreenEntry {
	id: string;
	name: string;
	/** Pantalla HUD mostrada en play (solo scope `player`). */
	active?: boolean;
}

export interface PlayerUiHudElementMeta {
	zIndex: number;
	locked: boolean;
}

export interface PlayerUiTextBoxEntry extends PlayerUiHudElementMeta {
	id: number;
	fontName: string;
	text: string;
}

export type EditingUiElementKind = 'text' | 'button' | 'image' | 'object';

export interface EditingUiButtonEntry extends PlayerUiHudElementMeta {
	id: number;
	config: PlayerUiButtonConfig;
}

export interface PlayerUiImageEntry extends PlayerUiHudElementMeta {
	id: number;
	imageName: string;
}

export interface PlayerUiObjectEntry extends PlayerUiHudElementMeta {
	id: number;
	vertexCount: number;
	fillColor?: [number, number, number, number];
	texturePath?: string | null;
	textureName?: string;
}

export type EditingUiElement =
	| (PlayerUiTextBoxEntry & { kind: 'text' })
	| ({ kind: 'button' } & EditingUiButtonEntry)
	| (PlayerUiImageEntry & { kind: 'image' })
	| (PlayerUiObjectEntry & { kind: 'object' });

export function mergeEditingUiTextElements(
	elements: EditingUiElement[],
	textBoxes: PlayerUiTextBoxEntry[],
): EditingUiElement[] {
	const other = elements.filter((e) => e.kind !== 'text');
	return [...other, ...textBoxes.map((box) => ({ kind: 'text' as const, ...box }))];
}

export function mergeEditingUiButtonElements(
	elements: EditingUiElement[],
	buttons: Array<{
		zIndex: number;
		locked: boolean;
		id: number;
		text: string;
		fontName: string;
	}>,
	defaultConfig: PlayerUiButtonConfig,
): EditingUiElement[] {
	const other = elements.filter((e) => e.kind !== 'button');
	return [
		...other,
		...buttons.map((b) => ({
			kind: 'button' as const,
			id: b.id,
			config: {
				...defaultConfig,
				text: b.text,
				fontName: b.fontName,
			},
			zIndex: b.zIndex,
			locked: b.locked,
		})),
	];
}

export function mergeEditingUiImageElements(
	elements: EditingUiElement[],
	images: PlayerUiImageEntry[],
): EditingUiElement[] {
	const other = elements.filter((e) => e.kind !== 'image');
	return [
		...other,
		...images.map((img) => ({ kind: 'image' as const, ...img })),
	];
}

export function mergeEditingUiObjectElements(
	elements: EditingUiElement[],
	objects: PlayerUiObjectEntry[],
): EditingUiElement[] {
	const other = elements.filter((e) => e.kind !== 'object');
	return [
		...other,
		...objects.map((obj) => ({ kind: 'object' as const, ...obj })),
	];
}

export function filterEditingUiElementsByKind<K extends EditingUiElementKind>(
	elements: EditingUiElement[],
	kind: K,
): Extract<EditingUiElement, { kind: K }>[] {
	return elements.filter((e): e is Extract<EditingUiElement, { kind: K }> => e.kind === kind);
}

/** Lista completa del motor (`player_ui_text_boxes_list`) → estado del sidebar, como `SET_MODELS`. */
export function buildEditingUiElementsFromEngineList(input: {
	textBoxes: PlayerUiTextBoxEntry[];
	buttons: Array<{
		id: number;
		text: string;
		fontName: string;
		zIndex: number;
		locked: boolean;
	}>;
	images: PlayerUiImageEntry[];
	objects: PlayerUiObjectEntry[];
	buttonDefaultConfig: PlayerUiButtonConfig;
}): EditingUiElement[] {
	return [
		...input.textBoxes.map((box) => ({ kind: 'text' as const, ...box })),
		...input.buttons.map((b) => ({
			kind: 'button' as const,
			id: b.id,
			config: {
				...input.buttonDefaultConfig,
				text: b.text,
				fontName: b.fontName,
			},
			zIndex: b.zIndex,
			locked: b.locked,
		})),
		...input.images.map((img) => ({ kind: 'image' as const, ...img })),
		...input.objects.map((obj) => ({ kind: 'object' as const, ...obj })),
	];
}

export interface EngineState {
	engineReady: boolean
	engineError: string | null
	previewPlaying: boolean
	/** Overlay mientras el motor ejecuta `import_scene` (2D). */
	sceneImportLoading: boolean
	/** Incrementa al recibir `play_character_view_changed` (refrescar UI de cámara play character). */
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
	models: ModelInfo[]
	loadedModelsInfo: Map<string, { name: string; category?: ModelCategory; model_id?: string; asset?: string }>
	sounds: SoundInfo[]
	fonts: FontInfo[]
	hudImages: HudImageInfo[]
	backgrounds: BackgroundInfo[]
	debugMetrics: DebugMetrics | null
	debugMode: boolean
	blueprints: BluePrintEntry[]
	multiSelectedIds: number[]
	/** Incrementa al aplicar metadatos 2D enviados por el motor (`project_loaded_2d`). */
	projectLoaded2dSeq: number
	/** Incrementa al aplicar metadatos 3D enviados por el motor (`project_loaded_3d`). */
	projectLoaded3dSeq: number
	playerUiScreens: UiScreenEntry[]
	menuUiScreens: UiScreenEntry[]
	playerUiEditingId: string | null
	menuUiEditingId: string | null
	/** Elementos de la pantalla UI en edición (texto sincronizado con el motor). */
	editingUiElements: EditingUiElement[]
	/** Incrementa al terminar o cancelar el dibujo de objeto HUD en el viewport. */
	playerUiObjectDrawEndTick: number
}

export type EngineAction =
	| { type: 'SET_READY' }
	| { type: 'SET_ERROR'; payload: string }
	| { type: 'SET_PREVIEW_PLAYING'; payload: boolean }
	| { type: 'SET_UI_SCREEN_EDITING'; payload: { playerId: string | null; menuId: string | null } }
	| { type: 'ADD_UI_SCREEN'; payload: { scope: UiScreenScope; entry: UiScreenEntry } }
	| { type: 'REMOVE_UI_SCREEN'; payload: { scope: UiScreenScope; id: string } }
	| { type: 'RENAME_UI_SCREEN'; payload: { scope: UiScreenScope; id: string; name: string } }
	| { type: 'SET_ACTIVE_PLAYER_UI_SCREEN'; payload: string | null }
	| { type: 'INIT_DEFAULT_3D_PLAYER_UI' }
	| { type: 'INIT_DEFAULT_2D_PLAYER_UI' }
	| { type: 'CLEAR_EDITING_UI_ELEMENTS' }
	| { type: 'SET_EDITING_UI_ELEMENTS'; payload: EditingUiElement[] }
	| { type: 'SET_EDITING_UI_TEXT_BOXES'; payload: PlayerUiTextBoxEntry[] }
	| {
			type: 'SET_EDITING_UI_BUTTONS'
			payload: Array<{
				id: number
				text: string
				fontName: string
				zIndex: number
				locked: boolean
			}>
	  }
	| { type: 'SET_EDITING_UI_IMAGES'; payload: PlayerUiImageEntry[] }
	| { type: 'SET_EDITING_UI_OBJECTS'; payload: PlayerUiObjectEntry[] }
	| {
			type: 'ADD_PLAYER_UI_TEXT_BOX'
			payload: PlayerUiTextBoxEntry
	  }
	| { type: 'UPDATE_PLAYER_UI_TEXT_BOX'; payload: { id: number; text: string } }
	| { type: 'REMOVE_PLAYER_UI_TEXT_BOX'; payload: number }
	| {
			type: 'ADD_PLAYER_UI_BUTTON'
			payload: {
				id: number
				config: PlayerUiButtonConfig
				zIndex?: number
				locked?: boolean
			}
	  }
	| { type: 'REMOVE_PLAYER_UI_BUTTON'; payload: number }
	| { type: 'ADD_PLAYER_UI_IMAGE'; payload: PlayerUiImageEntry }
	| { type: 'REMOVE_PLAYER_UI_IMAGE'; payload: number }
	| { type: 'REMOVE_PLAYER_UI_OBJECT'; payload: number }
	| { type: 'REMOVE_EDITING_UI_PLACEHOLDER'; payload: { kind: 'button'; id: number } }
	| { type: 'SET_SCENE_IMPORT_LOADING'; payload: boolean }
	| { type: 'SYNC_PLAY_CHARACTER_VIEW' }
	| { type: 'ADD_LOG'; payload: LogEntry }
	| { type: 'ADD_ENTITY'; payload: number }
	| { type: 'REMOVE_ENTITY'; payload: number }
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
	| { type: 'PLAYER_UI_OBJECT_DRAW_END' }
	| { type: 'SET_ANIMATION_PLAYING'; payload: { entityId: number; playing: boolean } }
	| {
		type: 'UPDATE_ENTITY_ANIMATIONS'
		payload: {
			entityId: number
			animations: NonNullable<SelectedEntity['animations']>
			visualModelPath?: string
		}
	}
	| { type: 'UPDATE_SELECTED_PHYSICS'; payload: { entityId: number; enabled: boolean; bodyType: string } }
	| {
		type: 'UPDATE_SELECTED_TRANSFORM';
		payload: {
			entityId: number;
			position: [number, number, number];
			rotation: [number, number, number, number];
			scale: [number, number, number];
		};
	}
	| { type: 'ADD_SPRITE'; payload: SpriteInfo }
	| { type: 'REMOVE_SPRITE'; payload: string }
	| { type: 'SET_SPRITES'; payload: SpriteInfo[] }
	| { type: 'ADD_SPRITE_INFO'; payload: { path: string; name: string } }
	| { type: 'REMOVE_SPRITE_INFO'; payload: string }
	| { type: 'SET_LOADED_SPRITES_INFO'; payload: Array<{ path: string; name: string }> }
	| {
		type: 'ADD_MODEL_INFO';
		payload: {
			path: string;
			name: string;
			loading?: boolean;
			category?: ModelCategory;
			model_id?: string;
			asset?: string;
			state?: string;
		}
	}
	| { type: 'SYNC_MODEL_PRELOAD'; payload: { path: string; name: string; category?: ModelCategory } }
	| {
		type: 'MARK_MODEL_READY';
		payload: {
			path: string;
			name: string;
			model_id?: string;
			asset?: string;
			state?: string;
		}
	}
	| { type: 'REMOVE_MODEL_INFO'; payload: string }
	| { type: 'SET_MODELS'; payload: ModelInfo[] }
	| { type: 'SET_DEBUG_MODE'; payload: boolean }
	| { type: 'SET_DEBUG_METRICS'; payload: DebugMetrics }
	| { type: 'ADD_BLUEPRINT'; payload: BluePrintEntry }
	| { type: 'SET_BLUEPRINTS'; payload: BluePrintEntry[] }
	| { type: 'SET_MULTI_SELECT'; payload: number[] }
	| { type: 'ADD_SOUND'; payload: SoundInfo }
	| { type: 'REMOVE_SOUND'; payload: string }
	| { type: 'SET_SOUNDS'; payload: SoundInfo[] }
	| { type: 'ADD_FONT'; payload: FontInfo }
	| { type: 'REMOVE_FONT'; payload: string }
	| { type: 'SET_FONTS'; payload: FontInfo[] }
	| { type: 'ADD_HUD_IMAGE'; payload: HudImageInfo }
	| { type: 'REMOVE_HUD_IMAGE'; payload: string }
	| { type: 'SET_HUD_IMAGES'; payload: HudImageInfo[] }
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
	  }
	| { type: 'APPLY_PROJECT_LOADED_2D'; payload: ProjectLoaded2dPayload }
	| { type: 'APPLY_PROJECT_LOADED_3D'; payload: ProjectLoaded3dPayload };

export const initialState: EngineState = {
	engineReady: false,
	engineError: null,
	previewPlaying: false,
	sceneImportLoading: true,
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
	fonts: [],
	hudImages: [],
	backgrounds: [],
	debugMetrics: null,
	debugMode: false,
	blueprints: [],
	multiSelectedIds: [],
	projectLoaded2dSeq: 0,
	projectLoaded3dSeq: 0,
	playerUiScreens: [],
	menuUiScreens: [],
	playerUiEditingId: null,
	menuUiEditingId: null,
	editingUiElements: [],
	playerUiObjectDrawEndTick: 0,
};

export function engineReducer(state: EngineState, action: EngineAction): EngineState {
	type EngineHandlers = {
		[K in EngineAction['type']]?: (
			prevState: EngineState,
			nextAction: Extract<EngineAction, { type: K }>,
		) => EngineState
	}

	const handlers: EngineHandlers = {
		SET_READY: (prevState) => ({
			...prevState,
			engineReady: true,
			engineError: null,
			previewPlaying: false,
			playerUiEditingId: null,
			menuUiEditingId: null,
		}),
		SET_ERROR: (prevState, nextAction) => ({
			...prevState,
			engineError: nextAction.payload,
			sceneImportLoading: false,
		}),
		SET_PREVIEW_PLAYING: (prevState, nextAction) => ({ ...prevState, previewPlaying: nextAction.payload }),
		SET_UI_SCREEN_EDITING: (prevState, nextAction) => {
			const nextPlayer = nextAction.payload.playerId;
			const nextMenu = nextAction.payload.menuId;
			const editing = Boolean(nextPlayer || nextMenu);
			return {
				...prevState,
				playerUiEditingId: nextPlayer,
				menuUiEditingId: nextMenu,
				// Solo vaciar al salir de edición; la lista del motor reemplaza todo al entrar.
				editingUiElements: editing ? prevState.editingUiElements : [],
			};
		},
		SET_EDITING_UI_TEXT_BOXES: (prevState, nextAction) => ({
			...prevState,
			editingUiElements: mergeEditingUiTextElements(
				prevState.editingUiElements,
				nextAction.payload,
			),
		}),
		SET_EDITING_UI_BUTTONS: (prevState, nextAction) => ({
			...prevState,
			editingUiElements: mergeEditingUiButtonElements(
				prevState.editingUiElements,
				nextAction.payload,
				DEFAULT_PLAYER_UI_BUTTON_CONFIG,
			),
		}),
		SET_EDITING_UI_IMAGES: (prevState, nextAction) => ({
			...prevState,
			editingUiElements: mergeEditingUiImageElements(
				prevState.editingUiElements,
				nextAction.payload,
			),
		}),
		SET_EDITING_UI_OBJECTS: (prevState, nextAction) => ({
			...prevState,
			editingUiElements: mergeEditingUiObjectElements(
				prevState.editingUiElements,
				nextAction.payload,
			),
		}),
		SET_EDITING_UI_ELEMENTS: (prevState, nextAction) => ({
			...prevState,
			editingUiElements: nextAction.payload,
		}),
		CLEAR_EDITING_UI_ELEMENTS: (prevState) => ({
			...prevState,
			editingUiElements: [],
		}),
		ADD_PLAYER_UI_TEXT_BOX: (prevState, nextAction) => ({
			...prevState,
			editingUiElements: [
				...prevState.editingUiElements,
				{ kind: 'text', ...nextAction.payload },
			],
		}),
		UPDATE_PLAYER_UI_TEXT_BOX: (prevState, nextAction) => ({
			...prevState,
			editingUiElements: prevState.editingUiElements.map((el) =>
				el.kind === 'text' && el.id === nextAction.payload.id
					? { ...el, text: nextAction.payload.text }
					: el,
			),
		}),
		REMOVE_PLAYER_UI_TEXT_BOX: (prevState, nextAction) => ({
			...prevState,
			editingUiElements: prevState.editingUiElements.filter(
				(el) => el.kind !== 'text' || el.id !== nextAction.payload,
			),
		}),
		ADD_PLAYER_UI_BUTTON: (prevState, nextAction) => ({
			...prevState,
			editingUiElements: [
				...prevState.editingUiElements,
				{
					kind: 'button' as const,
					id: nextAction.payload.id,
					config: nextAction.payload.config,
					zIndex: nextAction.payload.zIndex ?? 0,
					locked: nextAction.payload.locked ?? false,
				},
			],
		}),
		REMOVE_PLAYER_UI_BUTTON: (prevState, nextAction) => ({
			...prevState,
			editingUiElements: prevState.editingUiElements.filter(
				(el) => el.kind !== 'button' || el.id !== nextAction.payload,
			),
		}),
		ADD_PLAYER_UI_IMAGE: (prevState, nextAction) => ({
			...prevState,
			editingUiElements: [
				...prevState.editingUiElements,
				{ kind: 'image' as const, ...nextAction.payload },
			],
		}),
		REMOVE_PLAYER_UI_IMAGE: (prevState, nextAction) => ({
			...prevState,
			editingUiElements: prevState.editingUiElements.filter(
				(el) => el.kind !== 'image' || el.id !== nextAction.payload,
			),
		}),
		REMOVE_PLAYER_UI_OBJECT: (prevState, nextAction) => ({
			...prevState,
			editingUiElements: prevState.editingUiElements.filter(
				(el) => el.kind !== 'object' || el.id !== nextAction.payload,
			),
		}),
		REMOVE_EDITING_UI_PLACEHOLDER: (prevState, nextAction) => ({
			...prevState,
			editingUiElements: prevState.editingUiElements.filter(
				(el) => el.kind !== 'button' || el.id !== nextAction.payload.id,
			),
		}),
		ADD_UI_SCREEN: (prevState, nextAction) => {
			const { scope, entry } = nextAction.payload;
			if (scope === 'player') {
				return {
					...prevState,
					playerUiScreens: [
						...prevState.playerUiScreens,
						{ ...entry, active: false },
					],
				};
			}
			return { ...prevState, menuUiScreens: [...prevState.menuUiScreens, entry] };
		},
		SET_ACTIVE_PLAYER_UI_SCREEN: (prevState, nextAction) => ({
			...prevState,
			playerUiScreens: prevState.playerUiScreens.map((screen) => ({
				...screen,
				active:
					nextAction.payload !== null && screen.id === nextAction.payload,
			})),
		}),
		INIT_DEFAULT_3D_PLAYER_UI: (prevState) => {
			if (prevState.playerUiScreens.length > 0) {
				return prevState;
			}
			return {
				...prevState,
				playerUiScreens: default3dPlayerUiScreens(),
			};
		},
		INIT_DEFAULT_2D_PLAYER_UI: (prevState) => {
			if (prevState.playerUiScreens.length > 0) {
				return prevState;
			}
			return {
				...prevState,
				playerUiScreens: default2dPlayerUiScreens(),
			};
		},
		REMOVE_UI_SCREEN: (prevState, nextAction) => {
			const { scope, id } = nextAction.payload;
			if (scope === 'player') {
				return {
					...prevState,
					playerUiScreens: prevState.playerUiScreens.filter(
						(screen) => screen.id !== id,
					),
					playerUiEditingId: prevState.playerUiEditingId === id ? null : prevState.playerUiEditingId,
				};
			}
			return {
				...prevState,
				menuUiScreens: prevState.menuUiScreens.filter((screen) => screen.id !== id),
				menuUiEditingId: prevState.menuUiEditingId === id ? null : prevState.menuUiEditingId,
			};
		},
		RENAME_UI_SCREEN: (prevState, nextAction) => {
			const { scope, id, name } = nextAction.payload;
			if (scope === 'player') {
				return {
					...prevState,
					playerUiScreens: prevState.playerUiScreens.map((screen) =>
						screen.id === id ? { ...screen, name } : screen,
					),
				};
			}
			return {
				...prevState,
				menuUiScreens: prevState.menuUiScreens.map((screen) =>
					screen.id === id ? { ...screen, name } : screen,
				),
			};
		},
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
		REMOVE_ENTITY: (prevState, nextAction) => {
			const id = nextAction.payload;
			const without = <T extends { id: number }>(list: T[]): T[] =>
				list.filter((entry) => entry.id !== id);
			const multiSelectedIds = prevState.multiSelectedIds.filter((selectedId) => selectedId !== id);
			const selectedEntity =
				prevState.selectedEntity?.id === id ? null : prevState.selectedEntity;
			return {
				...prevState,
				entities: without(prevState.entities),
				characterEntities: without(prevState.characterEntities),
				scenarioEntities: without(prevState.scenarioEntities),
				colliderEntities: without(prevState.colliderEntities),
				executionAreaEntities: without(prevState.executionAreaEntities),
				multiSelectedIds,
				selectedEntity,
			};
		},
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
		ADD_SCENARIO: (prevState, nextAction) => {
			const id = nextAction.payload.id;
			if (prevState.scenarioEntities.some((scenario) => scenario.id === id)) {
				return {
					...prevState,
					scenarioEntities: prevState.scenarioEntities.map((scenario) =>
						scenario.id === id ? nextAction.payload : scenario,
					),
				};
			}
			return { ...prevState, scenarioEntities: [...prevState.scenarioEntities, nextAction.payload] };
		},
		REMOVE_SCENARIO: (prevState, nextAction) => ({
			...prevState,
			scenarioEntities: prevState.scenarioEntities.filter((scenario) => scenario.id !== nextAction.payload),
		}),
		ADD_CHARACTER: (prevState, nextAction) => {
			const id = nextAction.payload.id;
			if (prevState.characterEntities.some((character) => character.id === id)) {
				return {
					...prevState,
					characterEntities: prevState.characterEntities.map((character) =>
						character.id === id ? nextAction.payload : character,
					),
				};
			}
			return { ...prevState, characterEntities: [...prevState.characterEntities, nextAction.payload] };
		},
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
		PLAYER_UI_OBJECT_DRAW_END: (prevState) => ({
			...prevState,
			playerUiObjectDrawEndTick: prevState.playerUiObjectDrawEndTick + 1,
		}),
		SET_ANIMATION_PLAYING: (prevState, nextAction) => {
			const nextMap = new Map(prevState.animationPlaying);
			nextMap.set(nextAction.payload.entityId, nextAction.payload.playing);
			return { ...prevState, animationPlaying: nextMap };
		},
		UPDATE_ENTITY_ANIMATIONS: (prevState, nextAction) => {
			const selected = prevState.selectedEntity;
			if (!selected || selected.id !== nextAction.payload.entityId) return prevState;
			return {
				...prevState,
				selectedEntity: {
					...selected,
					animations: nextAction.payload.animations,
					...(nextAction.payload.visualModelPath !== undefined
						? { visualModelPath: nextAction.payload.visualModelPath }
						: {}),
				},
			};
		},
		UPDATE_SELECTED_PHYSICS: (prevState, nextAction) => {
			const selected = prevState.selectedEntity;
			if (!selected || selected.id !== nextAction.payload.entityId) return prevState;
			return {
				...prevState,
				selectedEntity: {
					...selected,
					physicsEnabled: nextAction.payload.enabled,
					physicsType: nextAction.payload.bodyType,
				},
			};
		},
		UPDATE_SELECTED_TRANSFORM: (prevState, nextAction) => {
			const selected = prevState.selectedEntity;
			if (!selected || selected.id !== nextAction.payload.entityId) return prevState;
			return {
				...prevState,
				selectedEntity: {
					...selected,
					position: nextAction.payload.position,
					rotation: nextAction.payload.rotation,
					scale: nextAction.payload.scale,
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
			const entry = {
				path: nextAction.payload.path,
				name: nextAction.payload.name,
				...(nextAction.payload.loading ? { loading: true } : {}),
				...(nextAction.payload.category ? { category: nextAction.payload.category } : {}),
				...(nextAction.payload.model_id ? { model_id: nextAction.payload.model_id } : {}),
				...(nextAction.payload.asset ? { asset: nextAction.payload.asset } : {}),
				...(nextAction.payload.state ? { state: nextAction.payload.state } : {}),
			};
			const nextMap = new Map(prevState.loadedModelsInfo);
			nextMap.set(entry.path, {
				name: entry.name,
				...(entry.category ? { category: entry.category } : {}),
			});
			const exists = prevState.models.some((m) => m.path === entry.path);
			return {
				...prevState,
				loadedModelsInfo: nextMap,
				models: exists
					? prevState.models.map((m) =>
						m.path === entry.path ? { ...m, ...entry } : m,
					)
					: [...prevState.models, entry],
			};
		},
		SYNC_MODEL_PRELOAD: (prevState, nextAction) => {
			const { path, name, category: payloadCategory } = nextAction.payload;
			let synced = false;
			let matchedCategory: ModelCategory | undefined = payloadCategory;
			const models = prevState.models.map((m) => {
				if (!synced && m.loading && m.name === name) {
					synced = true;
					matchedCategory = m.category;
					if (m.path !== path) {
						prevState.loadedModelsInfo.delete(m.path);
					}
					return {
						path,
						name,
						loading: true,
						...(m.category ? { category: m.category } : {}),
					};
				}
				return m;
			});
			const prevInfo = prevState.loadedModelsInfo.get(path);
			const category = matchedCategory ?? prevInfo?.category;
			const nextMap = new Map(prevState.loadedModelsInfo);
			nextMap.set(path, {
				name,
				...(category ? { category } : {}),
			});
			return { ...prevState, models, loadedModelsInfo: nextMap };
		},
		MARK_MODEL_READY: (prevState, nextAction) => {
			const { path, name, model_id, asset, state } = nextAction.payload;
			const prevInfo = prevState.loadedModelsInfo.get(path);
			const models = prevState.models.map((m) =>
				m.path === path || (m.loading && m.name === name)
					? {
						path,
						name,
						loading: false,
						...(m.category ? { category: m.category } : {}),
						...(model_id ?? m.model_id ? { model_id: model_id ?? m.model_id } : {}),
						...(asset ?? m.asset ? { asset: asset ?? m.asset } : {}),
						state: state ?? 'ready',
					}
					: m,
			);
			const nextMap = new Map(prevState.loadedModelsInfo);
			nextMap.set(path, {
				name,
				...(prevInfo?.category ? { category: prevInfo.category } : {}),
			});
			return { ...prevState, models, loadedModelsInfo: nextMap };
		},
		REMOVE_MODEL_INFO: (prevState, nextAction) => {
			const removed = nextAction.payload;
			const nextMap = new Map(prevState.loadedModelsInfo);
			nextMap.delete(removed);
			for (const [key] of prevState.loadedModelsInfo) {
				if (key === removed) continue;
				const model = prevState.models.find((m) => m.path === key);
				if (model?.model_id === removed) nextMap.delete(key);
			}
			return {
				...prevState,
				loadedModelsInfo: nextMap,
				models: prevState.models.filter(
					(m) => m.path !== removed && m.model_id !== removed,
				),
			};
		},
		SET_MODELS: (prevState, nextAction) => {
			const models: ModelInfo[] = nextAction.payload.map((m) => {
				const fromState = prevState.models.find(
					(x) => x.path === m.path || (x.loading && x.name === m.name),
				);
				const category = m.category ?? fromState?.category;
				const loading = fromState?.loading;
				return {
					...m,
					...(category ? { category } : {}),
					...(loading ? { loading } : {}),
				};
			});
			const nextMap = new Map<string, { name: string; category?: ModelCategory; model_id?: string; asset?: string }>();
			for (const m of models) {
				nextMap.set(m.path, {
					name: m.name,
					...(m.category ? { category: m.category } : {}),
					...(m.model_id ? { model_id: m.model_id } : {}),
					...(m.asset ? { asset: m.asset } : {}),
				});
			}
			return { ...prevState, models, loadedModelsInfo: nextMap };
		},
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
		ADD_FONT: (prevState, nextAction) =>
			prevState.fonts.some((f) => f.path === nextAction.payload.path)
				? prevState
				: { ...prevState, fonts: [...prevState.fonts, nextAction.payload] },
		REMOVE_FONT: (prevState, nextAction) => ({
			...prevState,
			fonts: prevState.fonts.filter((f) => f.path !== nextAction.payload),
		}),
		SET_FONTS: (prevState, nextAction) => ({ ...prevState, fonts: nextAction.payload }),
		ADD_HUD_IMAGE: (prevState, nextAction) =>
			prevState.hudImages.some((img) => img.path === nextAction.payload.path)
				? prevState
				: { ...prevState, hudImages: [...prevState.hudImages, nextAction.payload] },
		REMOVE_HUD_IMAGE: (prevState, nextAction) => ({
			...prevState,
			hudImages: prevState.hudImages.filter((img) => img.path !== nextAction.payload),
		}),
		SET_HUD_IMAGES: (prevState, nextAction) => ({ ...prevState, hudImages: nextAction.payload }),
		ADD_BACKGROUND: (prevState, nextAction) =>
			prevState.backgrounds.some((b) => b.path === nextAction.payload.path)
				? prevState
				: { ...prevState, backgrounds: [...prevState.backgrounds, nextAction.payload] },
		REMOVE_BACKGROUND: (prevState, nextAction) => ({ ...prevState, backgrounds: prevState.backgrounds.filter((b) => b.path !== nextAction.payload) }),
		SET_BACKGROUNDS: (prevState, nextAction) => ({ ...prevState, backgrounds: nextAction.payload }),
		APPLY_PROJECT_LOADED_2D: (prevState, nextAction) => {
			const p = nextAction.payload;
			const spriteMap = new Map<string, { name: string }>();
			for (const s of p.sprites) {
				spriteMap.set(s.path, { name: s.name });
			}
			return {
				...prevState,
				projectLoaded2dSeq: prevState.projectLoaded2dSeq + 1,
				blueprints: p.blueprints,
				worldConfig: {
					...prevState.worldConfig,
					worldWidth: p.world.worldWidth,
					worldHeight: p.world.worldHeight,
					worldDepth: p.world.worldDepth ?? prevState.worldConfig.worldDepth,
					worldRadius:
						p.world.worldRadius
						?? Math.min(
							p.world.worldWidth,
							p.world.worldHeight,
							p.world.worldDepth ?? prevState.worldConfig.worldDepth,
						) * 0.5,
					gridVisible: p.world.gridVisible,
					gridCellSize: p.world.gridCellSize,
					gravity: p.world.gravity ?? DEFAULT_GRAVITY_MAGNITUDE,
					targetFps: p.world.targetFps,
				},
				sounds: p.sounds,
				fonts: p.fonts ?? [],
				backgrounds: p.backgrounds,
				hudImages: p.hudImages ?? [],
				loadedSpritesInfo: spriteMap,
				backgroundPath: p.backgroundPath ?? null,
				playerUiScreens: normalizePlayerUiScreens(p.playerUiScreens ?? []),
				menuUiScreens: p.menuUiScreens ?? [],
			};
		},
		APPLY_PROJECT_LOADED_3D: (prevState, nextAction) => {
			const p = nextAction.payload;
			const modelMap = new Map<string, { name: string; category?: ModelCategory }>();
			const models: ModelInfo[] = [];
			for (const m of p.models) {
				const known = prevState.loadedModelsInfo.get(m.path);
				const category = m.category ?? known?.category;
				modelMap.set(m.path, {
					name: m.name,
					...(category ? { category } : {}),
					...(m.model_id ?? known?.model_id ? { model_id: m.model_id ?? known?.model_id } : {}),
					...(m.asset ?? known?.asset ? { asset: m.asset ?? known?.asset } : {}),
					...(m.model_id ? { state: 'ready' as const } : {}),
				});
				models.push({
					path: m.path,
					name: m.name,
					...(category ? { category } : {}),
					...(m.model_id ? { model_id: m.model_id } : {}),
					...(m.asset ? { asset: m.asset } : {}),
					...(m.model_id ? { state: 'ready' as const } : {}),
				});
			}
			return {
				...prevState,
				projectLoaded3dSeq: prevState.projectLoaded3dSeq + 1,
				blueprints: p.blueprints,
				models,
				worldConfig: {
					...prevState.worldConfig,
					worldWidth: p.world.worldWidth,
					worldHeight: p.world.worldHeight,
					worldDepth: p.world.worldDepth ?? prevState.worldConfig.worldDepth,
					worldRadius:
						p.world.worldRadius
						?? Math.min(
							p.world.worldWidth,
							p.world.worldHeight,
							p.world.worldDepth ?? prevState.worldConfig.worldDepth,
						) * 0.5,
					gridVisible: p.world.gridVisible,
					gridCellSize: p.world.gridCellSize,
					gravity: p.world.gravity ?? DEFAULT_GRAVITY_MAGNITUDE,
					targetFps: p.world.targetFps,
					lightAmbient: p.world.lightAmbient ?? DEFAULT_LIGHT_AMBIENT,
					lightIntensity: p.world.lightIntensity ?? DEFAULT_LIGHT_INTENSITY,
					shadowDarkness: p.world.shadowDarkness ?? DEFAULT_SHADOW_DARKNESS,
					graphicsTextureTier: normalizeGraphicsTextureTier(p.world.graphicsTextureTier),
					textureDetailDistance:
						typeof p.world.textureDetailDistance === 'number' &&
						Number.isFinite(p.world.textureDetailDistance)
							? p.world.textureDetailDistance
							: DEFAULT_WORLD_CONFIG.textureDetailDistance,
					reflectionTier: normalizeReflectionTier(p.world.reflectionTier),
					reflectionRaytracing: resolveReflectionRaytracingFromSave(
						normalizeReflectionTier(p.world.reflectionTier),
						p.world.reflectionRaytracing,
					),
					reflectionProbes: Boolean(p.world.reflectionProbes),
					shadowTier: normalizeShadowTier(p.world.shadowTier),
					msaaTier: normalizeMsaaTier(p.world.msaaTier),
				},
				sounds: p.sounds,
				fonts: p.fonts ?? [],
				backgrounds: p.backgrounds,
				hudImages: p.hudImages ?? [],
				loadedModelsInfo: modelMap,
				playerUiScreens: normalizePlayerUiScreens(p.playerUiScreens ?? []),
				menuUiScreens: p.menuUiScreens ?? [],
			};
		},
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

	const handler = handlers[action.type];
	if (!handler) return state;
	return (handler as (prevState: EngineState, nextAction: EngineAction) => EngineState)(state, action);
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
	kind: 'scenario' | 'character' | 'projectile' | 'model' | 'collider' | 'execution_area' | 'directional_light'
	path: string
	name?: string
	physicsEnabled: boolean
	physicsType: string
	points?: ColliderPoints
	animations?: EntityAnimations
	scripts?: EntityScripts
	visualGraph?: import('@shared-types').VisualGraphDocument
	visualScriptRhai?: string
	controlBindings?: SavedControlBindings
	/** ID de la blueprint desde la que fue instanciada esta entidad. */
	blueprintId?: string
	/** Entorno 3D creado desde acordeón Entorno (UI solo colisión). */
	entityCategory?: EntityCategory
	/** Categoría manifest (`Entity3DCategory`); fuente de verdad al guardar/copiar blueprint. */
	entity3dCategory?: Entity3DCategory
	/** Modelo visual cargado (distinto de path lógico `[Player]` / `[EditorBox]`). */
	visualModelPath?: string
	/** Padre de fusión 3D (esta entidad es hijo). */
	attachParentId?: number
	/** Hijo enganchado a socket de otra entidad. */
	attachSocketHostId?: number
	attachSocketName?: string
	/** Sockets definidos en esta entidad host. */
	sockets?: import('@shared-types').EntitySocket3D[]
	/** Física secundaria por hueso. */
	bonePhysics?: import('@shared-types').EntityBonePhysics3D[]
	/** Config de disparo (categoría projectile). */
	projectileConfig?: import('@shared-types').ProjectileConfig3D
	/** Huesos del modelo skinned (cache editor; invalidar al cambiar visualModelPath). */
	boneNames?: string[]
	/** El motor enlazó animación/esqueleto skinned (`model_clips_ready`). */
	skinnedModelBound?: boolean
	/** Animación en reproducción en el editor (preview desde propiedades). */
	playingAnimationName?: string
}

export interface PendingRestore {
	transform: Transform
	name?: string
	physicsEnabled: boolean
	physicsType: string
	animations?: EntityAnimations
	scripts?: EntityScripts
	visualGraph?: import('@shared-types').VisualGraphDocument
	visualScriptRhai?: string
	controlBindings?: SavedControlBindings
	/** ID de la blueprint desde la que fue instanciada esta entidad. */
	blueprintId?: string
	entityCategory?: EntityCategory
	visualModelPath?: string
}

export interface PendingBurstSpawnEntry {
	modelPath: string
	pending: PendingRestore
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
	initialSavePathRef: MutableRefObject<string | null | undefined>
	initialExtractDirRef: MutableRefObject<string | null | undefined>
	projectLoaded2dMetaRef: MutableRefObject<ProjectLoaded2dPayload | null>
	projectLoaded3dMetaRef: MutableRefObject<ProjectLoaded3dPayload | null>
	entityTransformsRef: MutableRefObject<Record<number, Transform>>
	entityMetaRef: MutableRefObject<Record<number, EntityMeta>>
	pendingRestoresRef: MutableRefObject<Map<string, PendingRestore[]>>
	playerEntityIdRef: MutableRefObject<number | null>
	editorCameraEntityIdRef: MutableRefObject<number | null>
	playCharacterViewRef: MutableRefObject<SavedPlayerTransform | null>
	pendingPlayCharacterViewRef: MutableRefObject<SavedPlayerTransform | null>
	pendingModelPathRef: MutableRefObject<string | null>
	pendingSpawnCategoryRef: MutableRefObject<EntityCategory | null>
	pendingModelLoadQueueRef: MutableRefObject<Array<{ modelPath: string; pending: PendingRestore }>>
	/** Restore pendiente por cada `spawn_cached_model` durante burst load (emparejado por path). */
	pendingBurstSpawnRestoreRef: MutableRefObject<PendingBurstSpawnEntry[]>
	camera2dRef: MutableRefObject<Camera2dState | null>
	mainPlayerHandled: MutableRefObject<boolean>
	playerRemoved: MutableRefObject<boolean>
	pendingPlayerDups: MutableRefObject<Transform[]>
	pendingDupQ: MutableRefObject<Transform[]>
	pivotEditListenerRef: MutableRefObject<((framePath: string, px: number, py: number) => void) | null>
	quickBuildClickListenerRef: MutableRefObject<((x: number, y: number, z: number, fitToGrid: boolean, scale?: [number, number, number]) => void) | null>
	/** Blueprint activa en construcción rápida (fallback si el motor no envía `blueprint_id`). */
	quickBuildActiveBlueprintIdRef: MutableRefObject<string | null>
	pendingEventsRef: MutableRefObject<Map<string, { resolve: (value: unknown) => void }>>
	blueprintsRef: MutableRefObject<BluePrintEntry[]>
	modelsRef: MutableRefObject<ModelInfo[]>
	updateEntityTransformRef: MutableRefObject<
		(id: number, patch: Partial<Transform>) => void
	>
	/** Escena 2D pendiente de sincronizar tras `scene_imported`. */
	pendingImportSceneRef: MutableRefObject<SavedScene | null>
	/** Evita duplicar estado React mientras el motor emite eventos de carga por entidad. */
	sceneImportInProgressRef: MutableRefObject<boolean>
	/** Overlay mientras el motor ejecuta `replace_entity_model` (GLB/FBX). */
	modelReplaceInProgressRef: MutableRefObject<boolean>
	/** Texto del overlay: modelo en recursos vs entidad vs escena. */
	modelLoadOverlayKindRef: MutableRefObject<ModelLoadOverlayKind | null>
	/** Precargas IPC de `load_model_asset` pendientes de `model_asset_loaded`. */
	modelAssetPreloadPendingRef: MutableRefObject<number>
	/** Overlay de carga durante ráfaga IPC 3D (cambio de escena / `ready`). */
	sceneBurstLoadInProgressRef: MutableRefObject<boolean>
	/** Colisionadores 3D enviados sin cola de restore. */
	sceneBurstPendingColliderCountRef: MutableRefObject<number>
	/** Operaciones IPC pendientes durante burst load 3D (ready / cambio escena). */
	sceneBurstPendingOpsRef: MutableRefObject<number>
	/** Arranque 3D sin `.save`: esperar IPC de entidades antes de mostrar el motor. */
	engineBootAwaitRef: MutableRefObject<boolean>
	engineBootIpcPendingRef: MutableRefObject<number>
	engineBootIpcSeenRef: MutableRefObject<number>
	engineBootFinishedRef: MutableRefObject<boolean>
	/**
	 * Arranque inicial: no quitar overlay hasta escenas listadas + primer `[Carga]`.
	 * Tras el primer reveal queda en false (cambios de escena mid-session no lo reactivan).
	 */
	bootRevealPendingRef: MutableRefObject<boolean>
	scenesTabsReadyRef: MutableRefObject<boolean>
	bootCargaLogSeenRef: MutableRefObject<boolean>
	/** Cambio de escena activa 3D: agrupar `entity_removed` en log `[Limpieza]`. */
	sceneWorldCleanupRef: MutableRefObject<{ active: boolean; summaryLogged: boolean }>
	/** Escena FP vacía tras limpieza: logs legibles para suelo/sol/jugador del motor. */
	fpSceneBaselineLogRef: MutableRefObject<boolean>
}

export interface EngineContextValue extends EngineState {
	projectType?: string
	gameStyle?: GameStyle
	dispatch: (action: EngineAction) => void
	pendingImportSceneRef: MutableRefObject<SavedScene | null>
	sceneImportInProgressRef: MutableRefObject<boolean>
	modelReplaceInProgressRef: MutableRefObject<boolean>
	modelLoadOverlayKindRef: MutableRefObject<ModelLoadOverlayKind | null>
	modelAssetPreloadPendingRef: MutableRefObject<number>
	modelsRef: MutableRefObject<ModelInfo[]>
	sceneBurstLoadInProgressRef: MutableRefObject<boolean>
	sceneBurstPendingColliderCountRef: MutableRefObject<number>
	sceneBurstPendingOpsRef: MutableRefObject<number>
	sceneWorldCleanupRef: MutableRefObject<{ active: boolean; summaryLogged: boolean }>
	fpSceneBaselineLogRef: MutableRefObject<boolean>
	bootRevealPendingRef: MutableRefObject<boolean>
	scenesTabsReadyRef: MutableRefObject<boolean>
	bootCargaLogSeenRef: MutableRefObject<boolean>
	engineBootAwaitRef: MutableRefObject<boolean>
	/** Marca tabs del acordeón listas y reintenta quitar el overlay de arranque. */
	notifyScenesTabsReady: (tabCount: number) => void
	entityTransformsRef: MutableRefObject<Record<number, Transform>>
	entityMetaRef: MutableRefObject<Record<number, EntityMeta>>
	pendingRestoresRef: MutableRefObject<Map<string, PendingRestore[]>>
	quickBuildActiveBlueprintIdRef: MutableRefObject<string | null>
	playerEntityIdRef: MutableRefObject<number | null>
	editorCameraEntityIdRef: MutableRefObject<number | null>
	playCharacterViewRef: MutableRefObject<SavedPlayerTransform | null>
	pendingPlayCharacterViewRef: MutableRefObject<SavedPlayerTransform | null>
	pendingModelPathRef: MutableRefObject<string | null>
	pendingSpawnCategoryRef: MutableRefObject<EntityCategory | null>
	pendingModelLoadQueueRef: MutableRefObject<Array<{ modelPath: string; pending: PendingRestore }>>
	pendingBurstSpawnRestoreRef: MutableRefObject<PendingBurstSpawnEntry[]>
	mainPlayerHandled: MutableRefObject<boolean>
	playerRemoved: MutableRefObject<boolean>
	camera2dRef: MutableRefObject<Camera2dState | null>
	projectLoaded2dMetaRef: MutableRefObject<ProjectLoaded2dPayload | null>
	projectLoaded3dMetaRef: MutableRefObject<ProjectLoaded3dPayload | null>
	send: (cmd: EngineCommand2D | EngineCommand3D) => void
	sendAsync: <T>(
		cmd: EngineCommand2D | EngineCommand3D,
		waitForEvent: string,
		onStart?: () => void,
	) => Promise<T>
	setAnimationPlaying: (entityId: number, playing: boolean, animationName?: string | null) => void
	loadModelAsset: (
		path: string,
		name: string,
		category?: ModelCategory,
	) => void
	spawnModel: (path: string, kind?: EntityMeta['kind'], category?: EntityCategory) => void
	replaceEntityModel: (entityId: number, modelPath: string) => void
	removeModelAsset: (path: string) => void
	getModelsList: () => void
	reportBounds: () => void
	retryEngine: () => void
	removeScenario: (id: number) => void
	removeCharacter: (id: number) => void
	removeEntity: (id: number) => void
	setWorldSize: (width: number, height: number, depth?: number) => void
	setWorldRadius: (radius: number) => void
	setGridVisible: (visible: boolean) => void
	setGridCellSize: (size: number) => void
	setGravity: (gravity: number) => void
	setDirectionalLight: (settings: {
		ambient?: number
		intensity?: number
		shadowDarkness?: number
	}) => void
	setTargetFps: (fps: number) => void
	setGraphicsTextureTier: (tier: GraphicsTextureTier) => void
	setReflectionTier: (tier: ReflectionTier) => void
	setReflectionRaytracing: (enabled: boolean) => void
	setReflectionProbes: (enabled: boolean) => void
	spawnReflectionProbe: () => void
	setReflectionDebugView: (view: ReflectionDebugView) => void
	setSsrDebugMode: (enabled: boolean) => void
	setShadowTier: (tier: ShadowTier) => void
	setMsaaTier: (tier: MsaaTier) => void
	setTaaEnabled: (enabled: boolean) => void
	setTaaParams: (params: { blend: number; jitterScale: number; enabled: boolean }) => void
	setTextureDetailDistance: (distanceM: number) => void
	removeCollider: (id: number) => void
	removeExecutionArea: (id: number) => void
	updateEntityAnimations: (id: number, animations: EntityAnimations) => EntityAnimations
	updateEntityScripts: (id: number, scripts: EntityScripts) => void
	updateEntityVisualGraph: (
		id: number,
		graph: import('@shared-types').VisualGraphDocument,
		rhaiSource: string,
	) => void
	setEntityPhysics: (id: number, enabled: boolean, bodyType: string) => void
	setProjectileConfig: (
		id: number,
		speed: number,
		lifetimeS: number,
		extras?: {
			affectedByGravity?: boolean
			gravityScale?: number
			alignToVelocity?: boolean
			muzzleSocket?: string | null
			bounceable?: boolean
			maxBounces?: number
			bounceSpeedLoss?: number
		},
	) => void
	fireProjectile: (
		templateId: number,
		dir: [number, number, number],
		fromId?: number,
	) => void
	updateEntityTransform: (
		id: number,
		patch: Partial<Transform> & {
			position?: [number, number, number];
			rotation?: [number, number, number, number];
			scale?: [number, number, number];
		},
	) => void
	registerPivotEditListener: (fn: (framePath: string, px: number, py: number) => void) => void
	unregisterPivotEditListener: () => void
	loadSprite: (path: string, name: string) => void
	removeSprite: (path: string) => void
	getSpritesList: () => void
	loadCharacter: (path: string) => void
	setPreviewPlaying: (playing: boolean) => void
	addUiScreen: (scope: UiScreenScope, name: string) => string | null
	removeUiScreen: (scope: UiScreenScope, id: string) => void
	renameUiScreen: (scope: UiScreenScope, id: string, name: string) => void
	setActivePlayerUiScreen: (screenId: string | null) => void
	syncPlayerUiScreensToEngine: (screens: UiScreenEntry[]) => void
	beginUiScreenEdit: (scope: UiScreenScope, id: string) => void
	endUiScreenEdit: () => void
	addPlayerUiTextBox: (fontPath: string) => void
	removePlayerUiTextBox: (id?: number) => void
	addEditingUiButton: (config: PlayerUiButtonConfig) => void
	addPlayerUiImage: (imagePath: string) => void
	removePlayerUiImage: (id?: number) => void
	removePlayerUiObject: (id?: number) => void
	setPlayerUiHudElementProps: (
		elementKind: EditingUiElementKind,
		id: number,
		props: { locked?: boolean; z_index?: number },
	) => void
	setPlayerUiObjectStyle: (
		id: number,
		style: {
			fill_color?: [number, number, number, number];
			texture_path?: string | null;
			clear_texture?: boolean;
			live?: boolean;
			skip_undo?: boolean;
		},
	) => void
	removeEditingUiPlaceholder: (kind: 'button', id: number) => void
	loadHudImage: (path: string, name: string) => void
	removeHudImage: (path: string) => void
	setBackground: (path: string | null) => void
	loadSound: (path: string, name: string) => void
	removeSound: (path: string) => void
	loadFont: (path: string, name: string) => void
	removeFont: (path: string) => void
	loadBackgroundToLibrary: (path: string, name: string) => void
	removeBackgroundFromLibrary: (path: string) => void
	addBlueprint: (entry: BluePrintEntry) => void
	setBlueprints: (entries: BluePrintEntry[]) => void
	registerQuickBuildClickListener: (fn: (x: number, y: number, z: number, fitToGrid: boolean, scale?: [number, number, number]) => void) => void
	unregisterQuickBuildClickListener: () => void
	setDebugMode: (show: boolean) => void
}

export { BluePrintEntry };
