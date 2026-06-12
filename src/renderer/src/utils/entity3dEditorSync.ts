import type {
	BluePrintEntry,
	ConfigCamera,
	Entity3D,
	Entity3DCategory,
	SavedPlayerTransform,
} from '@shared-types';
import {
	entityPathMarker,
	isEditorCameraPath,
	isGroundPath,
	isPlayerPath,
	isSunPath,
} from '@shared-types';
import type { EntityMeta, PendingRestore, Transform } from '../context/useContextEngine/types';
import {
	inferEntity3dCategoryFromName,
	reconcileCategoryWithName,
} from './blueprintModelPath';

function kindFromCategory(category: Entity3DCategory): EntityMeta['kind'] {
	switch (category) {
		case 'sun':
			return 'directional_light';
		case 'character':
		case 'player':
			return 'character';
		default:
			return 'model';
	}
}

/**
 * Categoría manifest para listados de editor (nodos, UI).
 * Corrige `object` genérico del motor usando nombre y path lógico.
 */
export function resolveEntity3dCategoryForScene(
	entity: Pick<Entity3D, 'name' | 'category' | 'model'>,
	meta?: Pick<EntityMeta, 'name' | 'entity3dCategory' | 'path' | 'kind'>,
): Entity3DCategory {
	const paths = [meta?.path, entity.model].filter(Boolean) as string[]
	for (const path of paths) {
		if (isPlayerPath(path)) return 'player'
		if (isGroundPath(path)) return 'ground'
		if (isSunPath(path)) return 'sun'
	}

	const name = entity.name ?? meta?.name
	const fromName = inferEntity3dCategoryFromName(name)
	if (fromName) return fromName

	if (meta?.kind === 'character') return 'character'
	if (meta?.kind === 'scenario') return 'environment'

	const base: Entity3DCategory = meta?.entity3dCategory ?? entity.category ?? 'object'
	return reconcileCategoryWithName(base, name)
}

/** Excluye entidades solo de editor (cámara orbital, etc.). */
export function isEditorOnlySceneEntity(
	entity: Pick<Entity3D, 'model'>,
	meta?: Pick<EntityMeta, 'path'>,
): boolean {
	const paths = [meta?.path, entity.model].filter(Boolean) as string[]
	return paths.some((path) => isEditorCameraPath(path) || entityPathMarker(path) === '[EditorCamera]')
}

/** Path/marker para IPC y colas de restore (`[Sun]`, `.glb`, etc.). */
export function entity3dSpawnPath(entity: Entity3D): string {
	return entityPathMarker(entity.model) ?? entity.model;
}

export function entity3dTransform(entity: Entity3D): Transform {
	return {
		position: entity.position,
		rotation: entity.rotation ?? [0, 0, 0, 1],
		scale: entity.scale,
	};
}

export function entity3dPendingRestore(
	entity: Entity3D,
	blueprints?: BluePrintEntry[],
): PendingRestore {
	const meta = entity3dToMeta(entity);
	const bp = entity.blueprint_id
		? (blueprints ?? []).find((b) => b.id === entity.blueprint_id) ?? null
		: null;
	return {
		transform: entity3dTransform(entity),
		name: entity.name,
		physicsEnabled: meta.physicsEnabled,
		physicsType: meta.physicsType,
		animations: bp?.animations ?? entity.animations,
		scripts: bp?.scripts ?? entity.scripts,
		visualGraph: entity.visualGraph,
		visualScriptRhai: entity.visualScriptRhai,
		controlBindings: bp?.control_bindings ?? entity.controls,
		blueprintId: entity.blueprint_id,
		entityCategory: meta.entityCategory,
		visualModelPath: meta.visualModelPath,
	};
}

const MODEL_3D_EXT = /\.(glb|gltf)$/i;

/** Meta de editor desde entidad 3D del manifest / snapshot del motor. */
export function entity3dToMeta(entity: Entity3D): EntityMeta {
	const marker = entityPathMarker(entity.model);
	const isPlayer = entity.category === 'player';
	const path =
		marker ??
		(isPlayer ? '[Player]' : entity.model);
	let visualModelPath: string | undefined;
	if (marker && entity.model !== path) {
		visualModelPath = entity.model;
	} else if (!marker && isPlayer && MODEL_3D_EXT.test(entity.model)) {
		// Player FP: el manifest guarda el GLB en `model`, no en un marcador `[Player]`.
		visualModelPath = entity.model;
	} else if (!marker && !isPlayer) {
		visualModelPath = entity.model;
	}

	const entity3dCategory = reconcileCategoryWithName(
		entity.category,
		entity.name,
	);

	return {
		kind: kindFromCategory(entity3dCategory),
		path,
		name: entity.name,
		entity3dCategory,
		physicsEnabled: entity.colision ?? entity.physics_type != null,
		physicsType: entity.physics_type ?? 'static',
		...(entity.animations?.length ? { animations: entity.animations } : {}),
		...(entity.scripts?.length ? { scripts: entity.scripts } : {}),
		...(entity.visualGraph ? { visualGraph: entity.visualGraph } : {}),
		...(entity.visualScriptRhai ? { visualScriptRhai: entity.visualScriptRhai } : {}),
		...(entity.controls ? { controlBindings: entity.controls } : {}),
		...(entity.blueprint_id ? { blueprintId: entity.blueprint_id } : {}),
		...(entity3dCategory === 'environment'
			? { entityCategory: 'environment' as const }
			: entity3dCategory === 'object'
				? { entityCategory: 'object' as const }
				: entity3dCategory === 'character'
					? { entityCategory: 'character' as const }
					: {}),
		...(visualModelPath && visualModelPath !== path
			? { visualModelPath }
			: {}),
	};
}

/** Vista runtime FP (refs del editor) desde `player` + `config_camera` del manifest. */
export function playViewFromPlayerAndCamera(
	player: Entity3D,
	cam: ConfigCamera,
): SavedPlayerTransform {
	const marker = entityPathMarker(player.model);
	const visual =
		marker && player.model !== '[Player]'
			? player.model
			: !marker && MODEL_3D_EXT.test(player.model)
				? player.model
				: undefined;

	return {
		position: player.position,
		scale: player.scale,
		yaw: cam.yaw,
		pitch: cam.pitch,
		fov_y: cam.fov_y,
		frustum_distance: cam.frustum_distance,
		camera_follow_mode: cam.camera_follow_mode,
		control_bindings: player.controls,
		scripts: player.scripts,
		body_rotation: player.rotation,
		body_scale: player.scale,
		camera_eye_position: cam.camera_eye_position,
		fps_camera_yaw: cam.fps_camera_yaw,
		fps_camera_pitch: cam.fps_camera_pitch,
		...(visual ? { visual_model_path: visual } : {}),
	};
}
