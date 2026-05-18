import type {
	BluePrintEntry,
	EngineSaveEntitySnapshot,
	EngineSaveSceneSnapshot,
	EntityCategory,
	GameStyle,
	ProjectSaveData,
	ProjectType,
	SavedEntity,
	SavedScene,
	SoundInfo,
	BackgroundInfo,
} from '@shared-types';
import type { EntityMeta } from '../context/useContextEngine/types';
import { getSceneProjectState } from '../pages/EngineView/sceneStateStore';

const SAVE_SNAPSHOT_TIMEOUT_MS = 15_000;

export function requestEngineSaveSnapshot(): Promise<EngineSaveSceneSnapshot> {
	return new Promise((resolve, reject) => {
		const onEngineEvent = (event: { event: string; [key: string]: unknown }) => {
			if (event.event === 'save_snapshot_ready') {
				cleanup();
				resolve((event as { scene: EngineSaveSceneSnapshot }).scene);
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

function normalizeEngineEntity(raw: EngineSaveEntitySnapshot): SavedEntity {
	const animations = raw.animations?.map((anim) => {
		const loop = (anim.loop_ ?? anim.loop) as boolean | undefined;
		const { loop_: _ignored, ...rest } = anim;
		return {
			...rest,
			loop: loop ?? false,
		};
	}) as SavedEntity['animations'];

	return {
		id: raw.id,
		name: raw.name,
		kind: raw.kind as SavedEntity['kind'],
		path: raw.path,
		position: raw.position,
		rotation: raw.rotation,
		scale: raw.scale,
		physics_enabled: raw.physics_enabled,
		physics_type: raw.physics_type,
		points: raw.points,
		animations,
		scripts: raw.scripts,
		control_bindings: raw.control_bindings,
		visual_model_path: raw.visual_model_path,
	};
}

function mapEngineEntities(
	entities: EngineSaveEntitySnapshot[],
	entityMeta: Record<number, EntityMeta>,
): SavedEntity[] {
	return entities.map((raw) => {
		const entity = normalizeEngineEntity(raw);
		const meta = entityMeta[entity.id];
		if (!meta) return entity;
		return {
			...entity,
			...(meta.blueprintId ? { blueprint_id: meta.blueprintId } : {}),
			...(meta.entityCategory ? { entity_category: meta.entityCategory as EntityCategory } : {}),
		};
	});
}

export function engineSceneToSavedScene(
	scene: EngineSaveSceneSnapshot,
	id: number,
	name: string,
	entityMeta: Record<number, EntityMeta>,
): SavedScene {
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
		playerTransform: scene.player_transform ?? null,
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
	blueprints: BluePrintEntry[]
	sounds: SoundInfo[]
	backgrounds: BackgroundInfo[]
	entityMeta: Record<number, EntityMeta>
	initialGameStyle?: GameStyle
}

/** Combina snapshot del motor con metadatos solo del editor (pestañas, blueprints, idioma). */
export function buildProjectSaveFromEngineSnapshot(
	engineScene: EngineSaveSceneSnapshot,
	options: BuildProjectSaveOptions,
): ProjectSaveData {
	const {
		projectType,
		gameStyle,
		locale,
		blueprints,
		sounds,
		backgrounds,
		entityMeta,
		initialGameStyle,
	} = options;

	const activeScene = engineSceneToSavedScene(engineScene, 1, 'Escena 1', entityMeta);

	const sceneState = getSceneProjectState();
	let scenes: SavedScene[] = [activeScene];
	let activeSceneId = 1;

	if (sceneState && sceneState.scenes.length > 0) {
		activeSceneId = sceneState.activeSceneId;
		scenes = sceneState.scenes.map((tab) =>
			tab.id === activeSceneId
				? { ...tab, ...activeScene, id: tab.id, name: tab.name }
				: tab,
		);
	}

	const root = scenes.find((s) => s.id === activeSceneId) ?? scenes[0];

	return {
		version: 1,
		type: projectType,
		gameStyle: initialGameStyle ?? gameStyle,
		scenes,
		activeSceneId,
		world: root.world,
		backgroundPath: root.backgroundPath,
		entities: root.entities,
		playerTransform: root.playerTransform,
		camera2d: root.camera2d,
		savedAt: new Date().toISOString(),
		sprites: root.sprites,
		models: root.models,
		sounds,
		backgrounds,
		blueprints,
		language: locale,
	};
}
