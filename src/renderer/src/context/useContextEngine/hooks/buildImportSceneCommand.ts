import type {
	BluePrintEntry,
	SavedAnimation,
	SavedEntity,
	SavedScene,
	SavedScript,
} from '@shared-types';
import { DEFAULT_GRAVITY_MAGNITUDE, isEditorCameraPath, isPlayerPath } from '@shared-types';

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
		const bp = entity.blueprint_id
			? (blueprints ?? []).find((b) => b.id === entity.blueprint_id) ?? null
			: null;
		const restore = resolveEntityRestore(entity, blueprints);
		const transform = resolveEntityTransform(entity, blueprints);
		refs.entityTransformsRef.current[entity.id] = transform;

		const meta: EntityMeta = {
			kind: entity.kind,
			path: entity.path,
			name: entity.name,
			physicsEnabled: restore.physicsEnabled,
			physicsType: restore.physicsType,
			...(entity.points ? { points: entity.points } : {}),
			...((bp?.animations ?? entity.animations)
				? { animations: (bp?.animations ?? entity.animations) as EntityMeta['animations'] }
				: {}),
			...(restore.scripts ? { scripts: restore.scripts as EntityMeta['scripts'] } : {}),
			...(restore.controlBindings ? { controlBindings: restore.controlBindings } : {}),
			...(entity.blueprint_id ? { blueprintId: entity.blueprint_id } : {}),
			...(entity.entity_category ? { entityCategory: entity.entity_category } : {}),
			...(entity.visual_model_path ? { visualModelPath: entity.visual_model_path } : {}),
		};
		refs.entityMetaRef.current[entity.id] = meta;
		entityIds.push(entity.id);

		const entry = { id: entity.id, path: entity.path };
		switch (entity.kind) {
			case 'scenario':
				scenarioEntities.push(entry);
				break;
			case 'character':
				characterEntities.push(entry);
				break;
			case 'collider':
				colliderEntities.push(entry);
				break;
			case 'execution_area':
				executionAreaEntities.push(entry);
				break;
			default:
				break;
		}
	}

	refs.camera2dRef.current = scene.camera2d ?? DEFAULT_CAMERA_2D;

	const player = scene.entities.find((e) => isPlayerPath(e.path));
	if (player) {
		refs.playerEntityIdRef.current = player.id;
		refs.mainPlayerHandled.current = true;
		refs.playerRemoved.current = false;
	} else {
		refs.playerEntityIdRef.current = null;
		refs.mainPlayerHandled.current = false;
	}
	const editorCam = scene.entities.find((e) => isEditorCameraPath(e.path));
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
