import type { SavedEntity, SavedPlayerTransform } from '@shared-types';
import { FIRST_PERSON_PLAYER_BODY_SCALE, isPlayerPath } from '@shared-types';
import type { MutableRefObject } from 'react';
import type { PendingRestore, Transform } from '../context/useContextEngine/types';

/** Pitch de órbita en editor (detrás del personaje). */
export const FP_EDITOR_ORBIT_PITCH = 0.25;
export const FP_DEFAULT_YAW = -Math.PI / 2;
export const FP_DEFAULT_FOV_Y = (45 * Math.PI) / 180;
export const FP_DEFAULT_FRUSTUM_DISTANCE = 2.5;

export interface FirstPersonViewChangedEvent {
	event: 'first_person_view_changed';
	player_id: number | null;
	position: [number, number, number];
	yaw: number;
	pitch: number;
	fov_y: number;
	frustum_distance: number;
	body_center: [number, number, number];
	body_rotation: [number, number, number, number];
	body_scale: [number, number, number];
}

/** Aplica al estado React lo que reporta el motor (sin derivar poses en TS). */
export function applyFirstPersonViewFromEngine(
	ev: FirstPersonViewChangedEvent,
	firstPersonViewRef: MutableRefObject<SavedPlayerTransform | null>,
	entityTransformsRef: MutableRefObject<Record<number, Transform>>,
	playerEntityIdRef?: MutableRefObject<number | null>,
) {
	if (ev.player_id != null && playerEntityIdRef) {
		playerEntityIdRef.current = ev.player_id;
	}
	const prev = firstPersonViewRef.current;
	firstPersonViewRef.current = {
		position: ev.position,
		scale: ev.body_scale,
		yaw: ev.yaw,
		pitch: ev.pitch,
		fov_y: ev.fov_y,
		frustum_distance: ev.frustum_distance,
		...(prev?.visual_model_path ? { visual_model_path: prev.visual_model_path } : {}),
		...(prev?.control_bindings ? { control_bindings: prev.control_bindings } : {}),
	};
	if (ev.player_id != null) {
		entityTransformsRef.current[ev.player_id] = {
			position: ev.body_center,
			rotation: ev.body_rotation,
			scale: ev.body_scale,
		};
	}
}

/** Pide al motor la vista FP; el frontend actualiza refs al recibir `first_person_view_changed`. */
export function applySavedFirstPersonView(
	view: SavedPlayerTransform | null | undefined,
	_options?: { editorOrbit?: boolean },
) {
	if (!view?.position) return;
	const yaw = view.yaw ?? FP_DEFAULT_YAW;
	const pitch =
		_options?.editorOrbit !== false
			? FP_EDITOR_ORBIT_PITCH
			: (view.pitch ?? FP_EDITOR_ORBIT_PITCH);
	window.engine.send({
		cmd: 'set_first_person_view',
		position: view.position,
		yaw,
		pitch,
		fov_y: view.fov_y ?? FP_DEFAULT_FOV_Y,
		frustum_distance: view.frustum_distance ?? FP_DEFAULT_FRUSTUM_DISTANCE,
	} as never);
}

type SceneSlice = {
	entities?: SavedEntity[];
	playerTransform?: SavedPlayerTransform | null;
};

/** Cola restore + `load_character` cuando el save no incluye entidad `[Player]`. */
export function ensureFirstPersonPlayerOnLoad(
	scene: SceneSlice,
	pendingRestoresRef: MutableRefObject<Map<string, PendingRestore[]>>,
	send: (cmd: unknown) => void,
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
		send({ cmd: 'load_character', path: '[Player]' });
	}
}
