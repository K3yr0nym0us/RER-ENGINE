import type {
	BluePrintEntry,
	Entity3D,
	SavedAnimation,
	SavedEntity,
	SavedPlayerTransform,
	SavedScene,
	SavedScript,
} from '@shared-types';
import { DEFAULT_GRAVITY_MAGNITUDE, entityPathMarker } from '@shared-types';
import { entity3dToMeta } from '../../../utils/entity3dEditorSync';

import type { EngineAction, EngineInternalRefs, EntityMeta, Transform } from '../types';

/** Debe coincidir con `setup_2d_platformer` / `Camera2D` inicial del motor. */
const DEFAULT_CAMERA_2D = { x: 0, y: 0, halfH: 3.5 };

function mapRestoreAnimations(animations: SavedAnimation[] | undefined) {
	if (!animations?.length) return undefined;
	return animations.map((anim) => ({
		name: anim.name,
		frames: anim.frames,
		fps: anim.fps,
		loop_: anim.loop,
		flip_horizontal: !(anim.facing_right ?? true),
		audio_path: anim.audio_path ?? null,
		scripts: (anim.scripts ?? []).map((s) => ({ name: s.name, source: s.source })),
		is_cancelable: anim.is_cancelable ?? true,
		is_default: !!anim.is_default,
	}));
}

function mapRestoreScripts(scripts: SavedScript[] | undefined) {
	if (!scripts?.length) return undefined;
	return scripts.map((s) => ({ path: s.name, source: s.source }));
}

export function resolveEntityTransform(
	entity: SavedEntity,
	blueprints?: BluePrintEntry[],
): Transform {
	const restore = resolveEntityRestore(entity, blueprints);
	return {
		position: entity.position,
		rotation: restore.rotation,
		scale: restore.scale,
	};
}

/** Transform guardado por entidad (carga 3D; blueprint no sobreescribe pos/rot/escala). */
export function resolveSavedEntityTransform(entity: SavedEntity): Transform {
	return {
		position: entity.position,
		rotation: entity.rotation ?? [0, 0, 0, 1],
		scale: entity.scale,
	};
}

function resolveEntityRestore(entity: SavedEntity, blueprints?: BluePrintEntry[]) {
	const bp = entity.blueprint_id
		? (blueprints ?? []).find((b) => b.id === entity.blueprint_id) ?? null
		: null;
	const isPlayer = isPlayerPath(entity.path);
	const physicsEnabled = isPlayer
		? true
		: (bp?.physics_enabled ?? entity.physics_enabled ?? false);
	const physicsType = isPlayer
		? 'dynamic'
		: (bp?.physics_type ?? entity.physics_type ?? 'static');

	return {
		animations: mapRestoreAnimations(bp?.animations ?? entity.animations),
		scripts: mapRestoreScripts(bp?.scripts ?? entity.scripts),
		controlBindings: bp?.control_bindings ?? entity.control_bindings,
		physicsEnabled,
		physicsType,
		scale: bp?.scale ?? entity.scale,
		rotation: bp?.rotation ?? entity.rotation,
	};
}

export function buildImportSceneEntity(
	entity: SavedEntity,
	blueprints?: BluePrintEntry[],
) {
	const restore = resolveEntityRestore(entity, blueprints);
	const isPlayer = isPlayerPath(entity.path);

	return {
		id: entity.id,
		kind: entity.kind,
		path: entity.path,
		...(entity.name?.trim() ? { name: entity.name } : {}),
		transform: {
			position: entity.position,
			rotation: restore.rotation,
			scale: restore.scale,
		},
		...(restore.physicsEnabled
			? { physics: { enabled: true, body_type: restore.physicsType } }
			: {}),
		...(restore.animations ? { animations: restore.animations } : {}),
		...(restore.scripts ? { scripts: restore.scripts } : {}),
		...(restore.controlBindings ? { control_bindings: restore.controlBindings } : {}),
		...(entity.points ? { points: entity.points } : {}),
		omit_scale: isPlayer,
		skip_transform: false,
		apply_initial_animation_frame: true,
	};
}

/** Proyecto 2D abierto desde `.save`: el motor carga desde `extract_dir`. */
export function is2dProjectLoadedByEngine(
	projectType: string | undefined,
	extractDir: string | null | undefined,
): boolean {
	return projectType === '2D' && Boolean(extractDir?.trim());
}

/** Proyecto 3D abierto desde `.save`: el motor carga desde `extract_dir`. */
export function is3dProjectLoadedByEngine(
	projectType: string | undefined,
	extractDir: string | null | undefined,
): boolean {
	return projectType === '3D' && Boolean(extractDir?.trim());
}

/** Proyecto existente: Electron extrajo el `.save` y el motor lee `extract_dir`. */
export function isProjectOpenedFromSave(extractDir: string | null | undefined): boolean {
	return Boolean(extractDir?.trim());
}

export function buildImportSceneCommand(scene: SavedScene, blueprints?: BluePrintEntry[]) {
	const camera = scene.camera2d ?? DEFAULT_CAMERA_2D;
	return {
		cmd: 'import_scene' as const,
		scene: '2D',
		world: {
			world_width: scene.world.worldWidth,
			world_height: scene.world.worldHeight,
			grid_visible: scene.world.gridVisible,
			grid_cell_size: scene.world.gridCellSize,
			target_fps: Number.isFinite(scene.world?.targetFps) ? scene.world.targetFps : 60,
			gravity: scene.world.gravity ?? DEFAULT_GRAVITY_MAGNITUDE,
		},
		background_path: scene.backgroundPath,
		camera2d: {
			x: camera.x,
			y: camera.y,
			half_h: camera.halfH,
		},
		sprites: scene.sprites ?? [],
		entities: scene.entities.map((entity) => buildImportSceneEntity(entity, blueprints)),
	};
}

/** Meta del jugador desde `player` del manifest (no está en `entities`). */
export function syncPlayerEntityMetaFromPlayer(
	refs: EngineInternalRefs,
	playerId: number,
	player: Entity3D,
) {
	refs.entityMetaRef.current[playerId] = entity3dToMeta(player);
}

/** @deprecated Runtime FP agregado; preferir `syncPlayerEntityMetaFromPlayer`. */
export function syncPlayerEntityMetaFromTransform(
	refs: EngineInternalRefs,
	playerId: number,
	playerTransform: SavedPlayerTransform,
) {
	const existing = refs.entityMetaRef.current[playerId];
	refs.entityMetaRef.current[playerId] = {
		kind: 'character',
		path: existing?.path ?? '[Player]',
		name: existing?.name ?? 'Player',
		physicsEnabled: true,
		physicsType: 'dynamic',
		...(playerTransform.control_bindings
			? { controlBindings: playerTransform.control_bindings }
			: {}),
		...(playerTransform.scripts?.length
			? {
					scripts: playerTransform.scripts.map((s) => ({
						name: s.name,
						source: s.source,
					})),
				}
			: {}),
		...(playerTransform.visual_model_path
			? { visualModelPath: playerTransform.visual_model_path }
			: {}),
	};
}

export function syncEditorStateFromSavedScene(
	scene: SavedScene,
	refs: EngineInternalRefs,
	dispatch: (action: EngineAction) => void,
	blueprints?: BluePrintEntry[],
) {
	const scenarioEntities: { id: number; path: string }[] = [];
	const characterEntities: { id: number; path: string }[] = [];
	const colliderEntities: { id: number; path: string }[] = [];
	const executionAreaEntities: { id: number; path: string }[] = [];
	const entityIds: number[] = [];

	refs.entityMetaRef.current = {};
	refs.entityTransformsRef.current = {};

	for (const entity of scene.entities) {
		const meta = entity3dToMeta(entity);
		refs.entityMetaRef.current[entity.id] = meta;
		refs.entityTransformsRef.current[entity.id] = {
			position: entity.position,
			rotation: entity.rotation ?? [0, 0, 0, 1],
			scale: entity.scale,
		};
		entityIds.push(entity.id);

		const entry = { id: entity.id, path: meta.path };
		switch (entity.category) {
			case 'character':
				characterEntities.push(entry);
				break;
			case 'environment':
			case 'object':
				scenarioEntities.push(entry);
				break;
			case 'sun':
				scenarioEntities.push(entry);
				break;
			default:
				break;
		}
	}

	refs.camera2dRef.current = scene.camera2d ?? DEFAULT_CAMERA_2D;

	if (scene.player) {
		refs.mainPlayerHandled.current = false;
		refs.playerRemoved.current = false;
		const playerId = refs.playerEntityIdRef.current;
		if (playerId != null) {
			syncPlayerEntityMetaFromPlayer(refs, playerId, scene.player);
			refs.mainPlayerHandled.current = true;
		}
	} else {
		refs.playerEntityIdRef.current = null;
		refs.mainPlayerHandled.current = false;
	}
	const editorCam = scene.entities.find((e) => entityPathMarker(e.model) === '[EditorCamera]');
	refs.editorCameraEntityIdRef.current = editorCam?.id ?? null;

	dispatch({
		type: 'IMPORT_SCENE_STATE',
		payload: {
			scenarioEntities,
			characterEntities,
			colliderEntities,
			executionAreaEntities,
			entities: entityIds.map((id) => ({ id })),
			backgroundPath: scene.backgroundPath,
			sprites: [],
		},
	});
}
