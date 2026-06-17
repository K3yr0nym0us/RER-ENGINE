import type {
	BluePrintEntry,
	ConfigCamera,
	Entity3D,
	Entity3DCategory,
	EntitySocket3D,
	SavedPlayerTransform,
} from '@shared-types';
import {
	entityPathMarker,
	isEditorCameraEntity,
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

const MODEL_3D_EXT = /\.(glb|gltf|fbx)$/i;

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
				: entity3dCategory === 'weapon'
					? { entityCategory: 'weapon' as const }
					: entity3dCategory === 'projectile'
						? { entityCategory: 'projectile' as const }
						: entity3dCategory === 'character'
						? { entityCategory: 'character' as const }
						: {}),
		...(visualModelPath && visualModelPath !== path
			? { visualModelPath }
			: {}),
		...(entity.attach_parent_id != null
			? { attachParentId: entity.attach_parent_id }
			: {}),
		...(entity.attach_socket_host_id != null
			? { attachSocketHostId: entity.attach_socket_host_id }
			: {}),
		...(entity.attach_socket_name != null
			? { attachSocketName: entity.attach_socket_name }
			: {}),
		...(entity.sockets?.length ? { sockets: entity.sockets } : {}),
	};
}

export function entityHasSkinnedModel(meta?: Pick<EntityMeta, 'animations' | 'visualModelPath'>): boolean {
	if (meta?.animations?.some((a) => a.embedded_in_model)) return true
	const path = meta?.visualModelPath ?? ''
	return /\.(glb|gltf|fbx)$/i.test(path)
}

export function entityCanHaveSockets(
	entityId: number,
	meta: EntityMeta | undefined,
	editorCameraEntityId: number | null,
): boolean {
	if (!meta) return false
	if (meta.kind === 'collider' || meta.kind === 'execution_area') return false
	if (isEditorCameraEntity(entityId, meta, editorCameraEntityId)) return false
	return entityHasSkinnedModel(meta)
}

/** Host con sockets en multiselección (exactamente uno con sockets definidos). */
export function resolveSocketAttachHost(
	ids: number[],
	entityMeta: Record<number, EntityMeta>,
): { hostId: number; sockets: EntitySocket3D[] } | null {
	const hosts = ids.filter((id) => (entityMeta[id]?.sockets?.length ?? 0) > 0)
	if (hosts.length !== 1) return null
	const hostId = hosts[0]
	const sockets = entityMeta[hostId]?.sockets ?? []
	return sockets.length > 0 ? { hostId, sockets } : null
}

export interface SocketAttachmentLink {
	childId: number
	childName: string
	socketName: string
}

export function resolveEntityEditorName(
	entityId: number,
	entityMeta: Record<number, EntityMeta>,
	selectedEntity?: { id: number; name: string } | null,
): string {
	const meta = entityMeta[entityId]
	if (meta?.name?.trim()) return meta.name.trim()
	if (selectedEntity?.id === entityId && selectedEntity.name?.trim()) {
		return selectedEntity.name.trim()
	}
	return `Entity ${entityId}`
}

/** Vínculos activos de objetos enganchados a sockets del host. */
export function listSocketAttachmentsForHost(
	hostId: number,
	entityMeta: Record<number, EntityMeta>,
	selectedEntity?: { id: number; name: string } | null,
): SocketAttachmentLink[] {
	const out: SocketAttachmentLink[] = []
	for (const [idStr, meta] of Object.entries(entityMeta)) {
		const childId = Number(idStr)
		if (!Number.isFinite(childId)) continue
		if (meta.attachSocketHostId !== hostId || !meta.attachSocketName?.trim()) continue
		out.push({
			childId,
			childName: resolveEntityEditorName(childId, entityMeta, selectedEntity),
			socketName: meta.attachSocketName,
		})
	}
	return out.sort((a, b) => {
		const bySocket = a.socketName.localeCompare(b.socketName, undefined, { sensitivity: 'accent' })
		if (bySocket !== 0) return bySocket
		return a.childName.localeCompare(b.childName, undefined, { sensitivity: 'accent' })
	})
}

/** Host inicial para modal de vínculos (entidad seleccionada con sockets). */
export function resolveSocketLinksModalHost(
	engine: Pick<import('@engine').EngineContextValue, 'selectedEntity' | 'entityMetaRef' | 'editorCameraEntityIdRef'>,
): number | null {
	const id = engine.selectedEntity?.id
	if (id == null) return null
	const meta = engine.entityMetaRef.current[id]
	if ((meta?.sockets?.length ?? 0) === 0) return null
	if (!entityCanHaveSockets(id, meta, engine.editorCameraEntityIdRef.current)) return null
	return id
}

/** Hijos candidatos para attach a socket (no host, no forbidden kinds). */
export function socketAttachChildCandidates(
	ids: number[],
	hostId: number,
	entityMeta: Record<number, EntityMeta>,
): number[] {
	return ids.filter((id) => {
		if (id === hostId) return false
		const meta = entityMeta[id]
		if (!meta) return false
		if (meta.kind === 'collider' || meta.kind === 'execution_area') return false
		if (meta.entity3dCategory === 'ground' || meta.entity3dCategory === 'sun') return false
		return true
	})
}

/** True si cada hijo de la selección ya está fusionado a un padre dentro de la misma selección. */
export function isMultiSelectionMerged(
	ids: number[],
	entityMeta: Record<number, EntityMeta>,
): boolean {
	if (ids.length < 2) return false;
	const selected = new Set(ids);
	let attachedChildren = 0;
	let parentsInSelection = 0;
	for (const id of ids) {
		const parentId = entityMeta[id]?.attachParentId;
		if (parentId != null && selected.has(parentId)) {
			attachedChildren += 1;
		}
	}
	for (const id of ids) {
		if (ids.some((other) => entityMeta[other]?.attachParentId === id)) {
			parentsInSelection += 1;
		}
	}
	return attachedChildren > 0 && attachedChildren === ids.length - parentsInSelection;
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
