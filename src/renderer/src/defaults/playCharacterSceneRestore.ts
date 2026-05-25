import type { PlayCharacterViewChanged, PlayCameraFollowMode, SavedEntity, SavedPlayerTransform } from '@shared-types';
import { FIRST_PERSON_PLAYER_BODY_SCALE, isPlayerPath } from '@shared-types';
import type { MutableRefObject } from 'react';
import type { PendingRestore, Transform } from '../context/useContextEngine/types';

/** Aplica al estado React lo que reporta el motor (sin derivar poses en TS). */
export function applyPlayCharacterViewFromEngine(
	ev: PlayCharacterViewChanged,
	playCharacterViewRef: MutableRefObject<SavedPlayerTransform | null>,
	entityTransformsRef: MutableRefObject<Record<number, Transform>>,
	playerEntityIdRef?: MutableRefObject<number | null>,
	editorCameraEntityIdRef?: MutableRefObject<number | null>,
	pendingSavedBodyRotation?: [number, number, number, number] | null,
) {
	if (ev.player_id != null && playerEntityIdRef) {
		playerEntityIdRef.current = ev.player_id;
	}
	if (ev.editor_camera_id != null && editorCameraEntityIdRef) {
		editorCameraEntityIdRef.current = ev.editor_camera_id;
	}
	const prev = playCharacterViewRef.current;
	const syncViewport = ev.sync_editor_viewport === true;
	// Tras replace_entity_model el motor emite sync_editor_viewport=false; no pisar body del .save.
	const savedBody = pendingSavedBodyRotation ?? prev?.body_rotation;
	const keepSavedBodyRotation = !syncViewport && savedBody != null;
	playCharacterViewRef.current = {
		position: syncViewport ? ev.position : (prev?.position ?? ev.position),
		camera_eye_position: ev.camera_eye_position ?? prev?.camera_eye_position,
		fps_camera_yaw: ev.fps_camera_yaw ?? prev?.fps_camera_yaw,
		fps_camera_pitch: ev.fps_camera_pitch ?? prev?.fps_camera_pitch,
		scale: ev.body_scale,
		yaw: syncViewport ? ev.yaw : (prev?.yaw ?? ev.yaw),
		pitch: syncViewport ? ev.pitch : (prev?.pitch ?? ev.pitch),
		fov_y: ev.fov_y,
		frustum_distance: ev.frustum_distance,
		camera_follow_mode: ev.camera_follow_mode ?? prev?.camera_follow_mode ?? 'move_with_character',
		body_rotation: keepSavedBodyRotation
			? savedBody
			: (ev.body_rotation ?? prev?.body_rotation),
		body_scale: ev.body_scale ?? prev?.body_scale,
		...(prev?.visual_model_path ? { visual_model_path: prev.visual_model_path } : {}),
		...(prev?.control_bindings ? { control_bindings: prev.control_bindings } : {}),
	};
	if (ev.player_id != null) {
		entityTransformsRef.current[ev.player_id] = {
			position: ev.body_center,
			rotation: keepSavedBodyRotation ? savedBody! : ev.body_rotation,
			scale: ev.body_scale,
		};
	}
	if (ev.editor_camera_id != null && ev.editor_orbit_target) {
		entityTransformsRef.current[ev.editor_camera_id] = {
			position: ev.editor_orbit_target,
			rotation: [0, 0, 0, 1],
			scale: [1, 1, 1],
		};
	}
}

/** Parche parcial de la cámara FPS (panel Cámara, tiempo real). Solo el campo modificado. */
export type PlayCharacterCameraPatch = {
	positionAxis?: { axis: number; value: number }
	yaw?: number
	fov_y?: number
	frustum_distance?: number
	camera_follow_mode?: PlayCameraFollowMode
}

/** Envía un cambio parcial de la cámara FPS; el motor lee el resto del estado actual. */
export function applyPlayCharacterCameraPatch(patch: PlayCharacterCameraPatch) {
	window.engine.send({
		cmd: 'set_play_character_view',
		camera_only: true,
		...(patch.positionAxis !== undefined ? { position_axis: patch.positionAxis } : {}),
		...(patch.yaw !== undefined ? { yaw: patch.yaw } : {}),
		...(patch.fov_y !== undefined ? { fov_y: patch.fov_y } : {}),
		...(patch.frustum_distance !== undefined ? { frustum_distance: patch.frustum_distance } : {}),
		...(patch.camera_follow_mode !== undefined ? { camera_follow_mode: patch.camera_follow_mode } : {}),
	} as never);
}

/** Restaura vista del jugador desde `.save` (motor aplica defaults para campos omitidos). */
export function applySavedPlayCharacterView(
	view: SavedPlayerTransform | null | undefined,
) {
	if (!view?.position) return;
	window.engine.send({
		cmd: 'set_play_character_view',
		position: view.position,
		...(view.yaw !== undefined ? { yaw: view.yaw } : {}),
		...(view.pitch !== undefined ? { pitch: view.pitch } : {}),
		...(view.fov_y !== undefined ? { fov_y: view.fov_y } : {}),
		...(view.frustum_distance !== undefined ? { frustum_distance: view.frustum_distance } : {}),
		...(view.camera_follow_mode ? { camera_follow_mode: view.camera_follow_mode } : {}),
		...(view.body_rotation ? { body_rotation: view.body_rotation } : {}),
		...(view.body_scale ? { body_scale: view.body_scale } : {}),
		...(view.camera_eye_position ? { camera_eye_position: view.camera_eye_position } : {}),
		...(view.fps_camera_yaw !== undefined ? { fps_camera_yaw: view.fps_camera_yaw } : {}),
		...(view.fps_camera_pitch !== undefined ? { fps_camera_pitch: view.fps_camera_pitch } : {}),
	} as never);
}

export function savedPlayCharacterViewForRestore(
	pending: SavedPlayerTransform | null | undefined,
	fallback: SavedPlayerTransform | null | undefined,
): SavedPlayerTransform | null | undefined {
	return pending ?? fallback;
}

type SceneSlice = {
	entities?: SavedEntity[];
	playerTransform?: SavedPlayerTransform | null;
};

/** Cola restore + `load_character` cuando el save no incluye entidad `[Player]`. */
export function ensurePlayCharacterOnLoad(
	scene: SceneSlice,
	pendingRestoresRef: MutableRefObject<Map<string, PendingRestore[]>>,
	send: (cmd: unknown) => void,
	options?: { onBurstOp?: () => void },
) {
	const savedPlayer = scene.playerTransform;
	const playerInEntities = (scene.entities ?? []).some(
		(e) => e.kind === 'character' && isPlayerPath(e.path),
	);
	const queue = pendingRestoresRef.current.get('[Player]') ?? [];
	const alreadyQueued = queue.length > 0;

	if (!playerInEntities && savedPlayer) {
		if (!alreadyQueued) {
			const pending: PendingRestore = {
				transform: {
					position: [0, FIRST_PERSON_PLAYER_BODY_SCALE[1] * 0.5, 0],
					rotation: [0, 0, 0, 1],
					scale: FIRST_PERSON_PLAYER_BODY_SCALE,
				},
				name: 'Player',
				physicsEnabled: true,
				physicsType: 'dynamic',
				controlBindings: savedPlayer.control_bindings,
				visualModelPath: savedPlayer.visual_model_path,
			};
			queue.push(pending);
			pendingRestoresRef.current.set('[Player]', queue);
		}
		options?.onBurstOp?.();
		send({ cmd: 'load_character', path: '[Player]' });
	}
}
