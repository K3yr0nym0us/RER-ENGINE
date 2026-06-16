import type {
	EngineSaveSceneSnapshot,
	Entity3D,
	GameStyle,
	ModelCategory,
	ProjectSaveData,
	ProjectType,
	ResourceModelEntry,
	SavedAnimation,
	SavedPlayerUiTextBox,
	SavedPlayerUiButton,
	SavedPlayerUiImage,
	SavedPlayerUiObject,
	SavedScene,
	FontInfo,
	HudImageInfo,
	ModelInfo,
	SoundInfo,
	BackgroundInfo,
} from '@shared-types';
import { entityPathMarker } from '@shared-types';
import type { UiScreenEntry } from '../context/useContextEngine/types';
import type { EntityMeta } from '../context/useContextEngine/types';
import { getSceneProjectState } from '../pages/EngineView/sceneStateStore';
import { defaultSceneName } from './defaultSceneName';
import { blueprintToSave } from '../utils/blueprintModelPath';

const SAVE_SNAPSHOT_TIMEOUT_MS = 15_000;

export function requestEngineSaveSnapshot(): Promise<EngineSaveSceneSnapshot> {
	return new Promise((resolve, reject) => {
		const onEngineEvent = (event: { event: string; [key: string]: unknown }) => {
			if (event.event === 'save_snapshot_ready') {
				cleanup();
				resolve((event as unknown as { scene: EngineSaveSceneSnapshot }).scene);
			}
			if (event.event === 'error') {
				cleanup();
				reject(new Error((event as { message?: string }).message ?? 'Error al exportar escena'));
			}
		};

		const cleanup = () => {
			window.clearTimeout(timeout);
			window.engine.off(onEngineEvent);
		};

		const timeout = window.setTimeout(() => {
			cleanup();
			reject(new Error('Timeout esperando save_snapshot_ready del motor'));
		}, SAVE_SNAPSHOT_TIMEOUT_MS);

		window.engine.on(onEngineEvent);
		window.engine.send({ cmd: 'export_save_snapshot' } as never);
	});
}

type EngineAnim = SavedAnimation & { loop_?: boolean };

function normalizeEngineAnimations(
	animations: EngineAnim[] | undefined,
): SavedAnimation[] | undefined {
	if (!animations?.length) return undefined;
	return animations.map((anim) => {
		const loop = anim.loop_ ?? anim.loop;
		const { loop_: _ignored, ...rest } = anim;
		return { ...rest, loop: loop ?? false };
	});
}

type EngineEntitySnapshotWire = Entity3D & {
	kind?: string
	path?: string
	control_bindings?: Entity3D['controls']
	physics_enabled?: boolean
}

function pickEntityFusionFields(
	raw: EngineEntitySnapshotWire,
): Pick<
	Entity3D,
	'attach_parent_id' | 'attach_local_position' | 'attach_local_rotation' | 'attach_local_scale'
> {
	const out: Pick<
		Entity3D,
		'attach_parent_id' | 'attach_local_position' | 'attach_local_rotation' | 'attach_local_scale'
	> = {}
	if (raw.attach_parent_id != null) out.attach_parent_id = raw.attach_parent_id
	if (raw.attach_local_position != null) out.attach_local_position = raw.attach_local_position
	if (raw.attach_local_rotation != null) out.attach_local_rotation = raw.attach_local_rotation
	if (raw.attach_local_scale != null) out.attach_local_scale = raw.attach_local_scale
	return out
}

function kindToEntityCategory(kind?: string): Entity3D['category'] | undefined {
	switch (kind) {
		case 'character':
			return 'character'
		case 'scenario':
			return 'environment'
		case 'collider':
		case 'execution_area':
		case 'model':
			return 'object'
		default:
			return undefined
	}
}

/** Normaliza entidades 2D del motor (`kind`, `path`, `control_bindings`) al contrato `Entity3D`. */
function normalizeEngineEntitySnapshot(raw: EngineEntitySnapshotWire): Entity3D {
	const model = raw.model ?? raw.path ?? ''
	const category =
		raw.category
		?? kindToEntityCategory(raw.kind)
		?? 'object'
	const controls = raw.controls ?? raw.control_bindings
	const colision = raw.colision ?? Boolean(raw.physics_enabled)
	return {
		...raw,
		...pickEntityFusionFields(raw),
		model,
		category,
		colision,
		...(controls ? { controls } : {}),
		animations: normalizeEngineAnimations(raw.animations as EngineAnim[] | undefined),
	}
}

/** Copia entidad del snapshot del motor; el front solo fusiona metadatos de editor (celdas de animación). */
function entityFromEngineSnapshot(raw: Entity3D): Entity3D {
	return normalizeEngineEntitySnapshot(raw as EngineEntitySnapshotWire)
}

function mergeAnimationEditorMeta(
	engineAnimations: SavedAnimation[] | undefined,
	metaAnimations: EntityMeta['animations'],
): SavedAnimation[] | undefined {
	if (!engineAnimations?.length) {
		return metaAnimations as SavedAnimation[] | undefined;
	}
	if (!metaAnimations?.length) return engineAnimations;
	return engineAnimations.map((anim) => {
		const fromMeta = metaAnimations.find((m) => m.name === anim.name);
		if (!fromMeta) return anim;
		return {
			...anim,
			...(fromMeta.selection_mode != null ? { selection_mode: fromMeta.selection_mode } : {}),
			...(fromMeta.grid_size != null ? { grid_size: fromMeta.grid_size } : {}),
			...(fromMeta.cell_offset_x != null ? { cell_offset_x: fromMeta.cell_offset_x } : {}),
			...(fromMeta.cell_offset_y != null ? { cell_offset_y: fromMeta.cell_offset_y } : {}),
		};
	});
}

function mapEngineEntities(
	entities: Entity3D[],
	entityMeta: Record<number, EntityMeta>,
): Entity3D[] {
	return entities.map((raw) => {
		const entity = entityFromEngineSnapshot(raw);
		const meta = entityMeta[entity.id];
		if (!meta) return entity;
		const animations = mergeAnimationEditorMeta(entity.animations, meta.animations);
		return {
			...entity,
			...(animations ? { animations } : {}),
			...(meta.scripts?.length ? { scripts: meta.scripts } : {}),
			...(meta.visualGraph ? { visualGraph: meta.visualGraph } : {}),
			...(meta.visualScriptRhai ? { visualScriptRhai: meta.visualScriptRhai } : {}),
			...(entity.blueprint_id ?? meta.blueprintId
				? { blueprint_id: entity.blueprint_id ?? meta.blueprintId }
				: {}),
		};
	});
}

export function engineSceneToSavedScene(
	scene: EngineSaveSceneSnapshot,
	id: number,
	name: string,
	entityMeta: Record<number, EntityMeta>,
): SavedScene {
	const player = scene.player
		? entityFromEngineSnapshot(scene.player)
		: null;

	return {
		id,
		name,
		world: {
			worldWidth: scene.world.world_width,
			worldHeight: scene.world.world_height,
			worldDepth: scene.world.world_depth,
			gridVisible: scene.world.grid_visible,
			gridCellSize: scene.world.grid_cell_size,
			gravity: scene.world.gravity,
			targetFps: scene.world.target_fps,
			lightAmbient: scene.world.light_ambient ?? undefined,
			lightIntensity: scene.world.light_intensity ?? undefined,
			shadowDarkness: scene.world.shadow_darkness ?? undefined,
			graphicsTextureTier: scene.world.graphics_texture_tier ?? undefined,
			textureDetailDistance: scene.world.texture_detail_distance_m ?? undefined,
		},
		backgroundPath: scene.background_path ?? null,
		entities: mapEngineEntities(scene.entities, entityMeta),
		player,
		config_camera: scene.config_camera ?? null,
		config_editor_camera: scene.config_editor_camera ?? null,
		camera2d: scene.camera2d
			? { x: scene.camera2d.x, y: scene.camera2d.y, halfH: scene.camera2d.half_h }
			: null,
		sprites: scene.sprites ?? [],
		models: [],
	};
}

/** Escena activa desde el motor (p. ej. antes de cambiar de escena activa en 3D). */
export async function buildActiveSceneSnapshotFromEngine(
	id: number,
	name: string,
	entityMeta: Record<number, EntityMeta>,
): Promise<SavedScene> {
	const engineScene = await requestEngineSaveSnapshot();
	return engineSceneToSavedScene(engineScene, id, name, entityMeta);
}

export interface BuildProjectSaveOptions {
	projectType: ProjectType
	gameStyle: GameStyle
	locale: string
	blueprints: import('@shared-types').BluePrintEntry[]
	sounds: SoundInfo[]
	fonts: FontInfo[]
	hudImages: HudImageInfo[]
	models: ModelInfo[]
	backgrounds: BackgroundInfo[]
	entityMeta: Record<number, EntityMeta>
	initialGameStyle?: GameStyle
	playerUiScreens?: UiScreenEntry[]
	menuUiScreens?: UiScreenEntry[]
}

function mapEngineUiTextBoxesToSave(
	boxes: NonNullable<EngineSaveSceneSnapshot['player_ui_text_boxes']>,
): SavedPlayerUiTextBox[] {
	return boxes.map((b) => ({
		scope: b.scope,
		screen_id: b.screen_id,
		id: b.id,
		font_path: b.font_path,
		font_name: b.font_name,
		text: b.text,
		center_x: b.center_x,
		center_y: b.center_y,
		width: b.width,
		height: b.height,
		z_index: b.z_index ?? 0,
		locked: b.locked ?? false,
	}));
}

function mapEngineUiButtonsToSave(
	buttons: NonNullable<EngineSaveSceneSnapshot['player_ui_buttons']>,
): SavedPlayerUiButton[] {
	return buttons.map((b) => ({
		scope: b.scope,
		screen_id: b.screen_id,
		id: b.id,
		type: b.type,
		round: b.round,
		background_color: b.background_color,
		texture_path: b.texture_path ?? null,
		transparency_background: b.transparency_background,
		text: b.text,
		text_color: b.text_color,
		transparency_text: b.transparency_text,
		font_path: b.font_path,
		font_name: b.font_name,
		border_color: b.border_color,
		border_weight: b.border_weight,
		center_x: b.center_x,
		center_y: b.center_y,
		width: b.width,
		height: b.height,
		source_aspect: b.source_aspect,
		z_index: b.z_index ?? 0,
		locked: b.locked ?? false,
	}));
}

function mapEngineUiImagesToSave(
	images: NonNullable<EngineSaveSceneSnapshot['player_ui_images']>,
): SavedPlayerUiImage[] {
	return images.map((img) => ({
		scope: img.scope,
		screen_id: img.screen_id,
		id: img.id,
		image_path: img.image_path,
		image_name: img.image_name,
		center_x: img.center_x,
		center_y: img.center_y,
		width: img.width,
		height: img.height,
		source_aspect: img.source_aspect,
		z_index: img.z_index ?? 0,
		locked: img.locked ?? false,
	}));
}

function mapEngineUiObjectsToSave(
	objects: NonNullable<EngineSaveSceneSnapshot['player_ui_objects']>,
): SavedPlayerUiObject[] {
	return objects.map((obj) => ({
		scope: obj.scope,
		screen_id: obj.screen_id,
		id: obj.id,
		vertices: obj.vertices,
		fill_color: obj.fill_color,
		texture_path: obj.texture_path ?? undefined,
		z_index: obj.z_index ?? 0,
		locked: obj.locked ?? false,
	}));
}

function mergeLibraryAssets(
	fromEditor: Array<{ name: string; path: string }>,
	fromEngine?: Array<{ name: string; path: string }>,
): Array<{ name: string; path: string }> {
	const byPath = new Map<string, { name: string; path: string }>();
	for (const item of fromEditor) byPath.set(item.path, item);
	for (const item of fromEngine ?? []) byPath.set(item.path, item);
	return [...byPath.values()];
}

function modelBasename(p: string): string {
	return p.split(/[/\\]/).pop() ?? p
}

function attachModelIdsToEntities<T extends { model: string; model_id?: string }>(
	entities: T[],
	models: ModelInfo[],
): T[] {
	const byPath = new Map(
		models.filter((m) => m.model_id).map((m) => [m.path, m.model_id!] as const),
	)
	const byModelId = new Map(
		models.filter((m) => m.model_id).map((m) => [m.model_id!, m.model_id!] as const),
	)
	const byBasename = new Map(
		models
			.filter((m) => m.model_id)
			.map((m) => [modelBasename(m.path).toLowerCase(), m.model_id!] as const),
	)
	return entities.map((entity) => {
		const model_id =
			entity.model_id ??
			byModelId.get(entity.model) ??
			byPath.get(entity.model) ??
			byBasename.get(modelBasename(entity.model).toLowerCase())
		return model_id ? { ...entity, model_id } : entity
	})
}

/** Entidades 3D referencian `model_id` (runtime carga desde `.rerasset`, no GLB fuente). */
function rewriteImportedEntityPaths<T extends { model: string; model_id?: string }>(
	entities: T[],
): T[] {
	return entities.map((entity) => {
		if (!entity.model_id || entityPathMarker(entity.model)) {
			return entity
		}
		return { ...entity, model: entity.model_id }
	})
}

function attachModelIdsToBlueprints<T extends { model: string; model_id?: string }>(
	blueprints: T[],
	models: ModelInfo[],
): T[] {
	return attachModelIdsToEntities(blueprints, models)
}

function rewriteImportedBlueprintPaths<T extends { model: string; model_id?: string }>(
	blueprints: T[],
): T[] {
	return rewriteImportedEntityPaths(blueprints)
}

function importedModelAssetPath(modelId: string, asset?: string): string {
	return asset ?? `imported/models/${modelId}.rerasset`
}

const RERASSET_IMPORTER_VERSION = 2

/** Biblioteca 3D: solo `resources.models` con importación lista (motor → `model_store` filtrado). */
function buildResourcesModelsForSave(
	engineModels: EngineSaveSceneSnapshot['models'],
	editorModels: ModelInfo[],
): ResourceModelEntry[] {
	const byId = new Map<string, ResourceModelEntry>()
	for (const m of engineModels ?? []) {
		if (!m.model_id || !m.asset) continue
		byId.set(m.model_id, {
			id: m.model_id,
			name: m.name,
			type: (m.category ?? 'object') as ModelCategory,
			asset: importedModelAssetPath(m.model_id, m.asset),
			importer_version: RERASSET_IMPORTER_VERSION,
		})
	}
	for (const m of editorModels) {
		if (!m.model_id || m.state === 'importing' || m.state === 'failed') continue
		if (byId.has(m.model_id)) continue
		byId.set(m.model_id, {
			id: m.model_id,
			name: m.name,
			type: (m.category ?? 'object') as ModelCategory,
			asset: importedModelAssetPath(m.model_id, m.asset),
			importer_version: RERASSET_IMPORTER_VERSION,
		})
	}
	return [...byId.values()]
}

function mergeModelLibrary(
	fromEditor: ModelInfo[],
	fromEngine?: Array<{
		name: string
		path: string
		category?: ModelInfo['category']
		model_id?: string
		asset?: string
	}>,
): ModelInfo[] {
	const byPath = new Map<string, ModelInfo>()
	for (const item of fromEditor) {
		byPath.set(item.path, { ...item })
	}
	for (const item of fromEngine ?? []) {
		const prev = byPath.get(item.path)
		byPath.set(item.path, {
			name: item.name,
			path: item.path,
			...(prev?.category ?? item.category ? { category: prev?.category ?? item.category } : {}),
			...(prev?.model_id ?? item.model_id ? { model_id: prev?.model_id ?? item.model_id } : {}),
			...(prev?.asset ?? item.asset ? { asset: prev?.asset ?? item.asset } : {}),
			...(prev?.state ? { state: prev.state } : {}),
		})
	}
	return [...byPath.values()]
}

/** Combina snapshot del motor con metadatos solo del editor (escenas inactivas, blueprints, idioma). */
export async function buildProjectSaveFromEngineSnapshot(
	engineScene: EngineSaveSceneSnapshot,
	options: BuildProjectSaveOptions,
): Promise<ProjectSaveData> {
	const {
		projectType,
		gameStyle,
		locale,
		blueprints,
		sounds,
		fonts,
		hudImages,
		models,
		backgrounds,
		entityMeta,
		initialGameStyle,
		playerUiScreens = [],
		menuUiScreens = [],
	} = options;

	const sceneState = getSceneProjectState();
	let activeSceneId = 1;
	let activeSceneName = sceneState?.scenes.find((tab) => tab.id === sceneState.activeSceneId)?.name ?? '';
	if (sceneState && sceneState.scenes.length > 0) {
		activeSceneId = sceneState.activeSceneId;
	}
	if (!activeSceneName.trim()) {
		activeSceneName = defaultSceneName(activeSceneId);
	}

	const activeScene = engineSceneToSavedScene(engineScene, activeSceneId, activeSceneName, entityMeta);

	let scenes: SavedScene[] = [activeScene];

	if (sceneState && sceneState.scenes.length > 0) {
		scenes = sceneState.scenes.map((tab) =>
			tab.id === activeSceneId
				? { ...tab, ...activeScene, id: tab.id, name: tab.name, models: [] }
				: { ...tab, models: [] },
		);
	}

	const root = scenes.find((s) => s.id === activeSceneId) ?? scenes[0];

	const mergedFonts = mergeLibraryAssets(fonts, engineScene.fonts);
	const mergedHudImages = mergeLibraryAssets(hudImages, engineScene.hud_images);
	const mergedModels = mergeModelLibrary(
		models,
		engineScene.models?.map((m) => ({
			name: m.name,
			path: m.path,
			...(m.category ? { category: m.category as ModelInfo['category'] } : {}),
			...(m.model_id ? { model_id: m.model_id } : {}),
			...(m.asset ? { asset: m.asset } : {}),
		})),
	);

	const savedBlueprints =
		projectType === '3D'
			? rewriteImportedBlueprintPaths(
					attachModelIdsToBlueprints(blueprints.map(blueprintToSave), mergedModels),
				)
			: blueprints;

	const hasScenes = scenes.length > 0;

	const scenesWithModelIds = scenes.map((tab) => ({
		...tab,
		entities: attachModelIdsToEntities(tab.entities, mergedModels),
		player: tab.player
			? attachModelIdsToEntities([tab.player], mergedModels)[0] ?? tab.player
			: null,
	}));
	const scenesRewritten = scenesWithModelIds.map((tab) => ({
		...tab,
		entities: rewriteImportedEntityPaths(tab.entities),
		player: tab.player ? rewriteImportedEntityPaths([tab.player])[0] ?? tab.player : null,
	}));
	const rootWithIds =
		scenesRewritten.find((s) => s.id === activeSceneId) ?? scenesRewritten[0];

	const readyImported = mergedModels.filter(
		(m) => m.model_id && m.state !== 'importing' && m.state !== 'failed',
	);
	const is3d = projectType === '3D';
	const resourcesModels = is3d
		? buildResourcesModelsForSave(engineScene.models, readyImported)
		: [];

	return {
		version: 1,
		type: projectType,
		gameStyle: initialGameStyle ?? gameStyle,
		scenes: scenesRewritten,
		activeSceneId,
		world: hasScenes ? undefined : rootWithIds.world,
		backgroundPath: hasScenes ? null : rootWithIds.backgroundPath,
		entities: hasScenes ? [] : rootWithIds.entities,
		player: hasScenes ? null : rootWithIds.player,
		config_camera: hasScenes ? null : rootWithIds.config_camera,
		config_editor_camera: hasScenes ? null : rootWithIds.config_editor_camera,
		camera2d: hasScenes ? null : rootWithIds.camera2d,
		savedAt: new Date().toISOString(),
		sprites: hasScenes ? [] : rootWithIds.sprites,
		...(is3d ? { resources: { models: resourcesModels } } : {}),
		sounds,
		fonts: mergedFonts,
		hudImages: mergedHudImages.length ? mergedHudImages : undefined,
		backgrounds,
		blueprints: savedBlueprints,
		language: locale,
		playerUiScreens,
		menuUiScreens,
		playerUiTextBoxes: engineScene.player_ui_text_boxes?.length
			? mapEngineUiTextBoxesToSave(engineScene.player_ui_text_boxes)
			: undefined,
		playerUiButtons: engineScene.player_ui_buttons?.length
			? mapEngineUiButtonsToSave(engineScene.player_ui_buttons)
			: undefined,
		playerUiImages: engineScene.player_ui_images?.length
			? mapEngineUiImagesToSave(engineScene.player_ui_images)
			: undefined,
		playerUiObjects: engineScene.player_ui_objects?.length
			? mapEngineUiObjectsToSave(engineScene.player_ui_objects)
			: undefined,
	};
}
