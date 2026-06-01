import type {
	EngineSaveSceneSnapshot,
	Entity3D,
	GameStyle,
	ProjectSaveData,
	ProjectType,
	SavedAnimation,
	SavedScene,
	FontInfo,
	SoundInfo,
	BackgroundInfo,
} from '@shared-types';
import type { EntityMeta } from '../context/useContextEngine/types';
import { getSceneProjectState } from '../pages/EngineView/sceneStateStore';
import { requestEngineDefaultSceneName } from './requestEngineDefaultSceneName';
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

/** Copia entidad del snapshot del motor; el front solo fusiona metadatos de editor (celdas de animación). */
function entityFromEngineSnapshot(raw: Entity3D): Entity3D {
	return {
		...raw,
		animations: normalizeEngineAnimations(raw.animations as EngineAnim[] | undefined),
	};
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
		models: scene.models,
	};
}

/** Escena activa desde el motor (p. ej. antes de cambiar de pestaña en 3D). */
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
	backgrounds: BackgroundInfo[]
	entityMeta: Record<number, EntityMeta>
	initialGameStyle?: GameStyle
}

/** Combina snapshot del motor con metadatos solo del editor (pestañas, blueprints, idioma). */
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
		backgrounds,
		entityMeta,
		initialGameStyle,
	} = options;

	const sceneState = getSceneProjectState();
	let activeSceneId = 1;
	let activeSceneName = sceneState?.scenes.find((tab) => tab.id === sceneState.activeSceneId)?.name ?? '';
	if (sceneState && sceneState.scenes.length > 0) {
		activeSceneId = sceneState.activeSceneId;
	}
	if (!activeSceneName.trim()) {
		activeSceneName = await requestEngineDefaultSceneName(activeSceneId);
	}

	const activeScene = engineSceneToSavedScene(engineScene, activeSceneId, activeSceneName, entityMeta);

	let scenes: SavedScene[] = [activeScene];

	if (sceneState && sceneState.scenes.length > 0) {
		scenes = sceneState.scenes.map((tab) =>
			tab.id === activeSceneId
				? { ...tab, ...activeScene, id: tab.id, name: tab.name }
				: tab,
		);
	}

	const root = scenes.find((s) => s.id === activeSceneId) ?? scenes[0];

	const savedBlueprints =
		projectType === '3D'
			? blueprints.map(blueprintToSave)
			: blueprints;

	return {
		version: 1,
		type: projectType,
		gameStyle: initialGameStyle ?? gameStyle,
		scenes,
		activeSceneId,
		world: root.world,
		backgroundPath: root.backgroundPath,
		entities: root.entities,
		player: root.player,
		config_camera: root.config_camera,
		config_editor_camera: root.config_editor_camera,
		camera2d: root.camera2d,
		savedAt: new Date().toISOString(),
		sprites: root.sprites,
		models: root.models,
		sounds,
		fonts,
		backgrounds,
		blueprints: savedBlueprints,
		language: locale,
	};
}
