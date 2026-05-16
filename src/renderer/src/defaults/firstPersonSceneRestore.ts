import type { SavedEntity, SavedPlayerTransform } from '@shared-types';
import { FIRST_PERSON_PLAYER_BODY_SCALE, isPlayerPath } from '@shared-types';
import type { MutableRefObject } from 'react';
import type { PendingRestore } from '../context/useContextEngine/types';

/** Pitch de órbita en editor (detrás del personaje). */
export const FP_EDITOR_ORBIT_PITCH = 0.25;
export const FP_DEFAULT_YAW = -Math.PI / 2;

/** Mitad de altura del cuerpo (debe coincidir con FIRST_PERSON_BODY_HALF_H del motor). */
const FP_BODY_HALF_H = FIRST_PERSON_PLAYER_BODY_SCALE[1] * 0.5;
const FEET_OFFSET_LOCAL: [number, number, number] = [0, -FP_BODY_HALF_H, 0];

function rotateVec3ByQuat(
	v: [number, number, number],
	q: [number, number, number, number],
): [number, number, number] {
	const [qx, qy, qz, qw] = q;
	const [vx, vy, vz] = v;
	const ix = qw * vx + qy * vz - qz * vy;
	const iy = qw * vy + qz * vx - qx * vz;
	const iz = qw * vz + qx * vy - qy * vx;
	const iw = -qx * vx - qy * vy - qz * vz;
	return [
		ix * qw + iw * -qx + iy * -qz - iz * -qy,
		iy * qw + iw * -qy + iz * -qx - ix * -qz,
		iz * qw + iw * -qz + ix * -qy - iy * -qx,
	];
}

export function feetFromPlayerBodyCenter(
	center: [number, number, number],
	rotation: [number, number, number, number] = [0, 0, 0, 1],
): [number, number, number] {
	const off = rotateVec3ByQuat(FEET_OFFSET_LOCAL, rotation);
	return [center[0] + off[0], center[1] + off[1], center[2] + off[2]];
}

export function bodyCenterFromFeet(
	feet: [number, number, number],
	rotation: [number, number, number, number] = [0, 0, 0, 1],
): [number, number, number] {
	const off = rotateVec3ByQuat(FEET_OFFSET_LOCAL, rotation);
	return [feet[0] - off[0], feet[1] - off[1], feet[2] - off[2]];
}

/** Mantiene `firstPersonViewRef` alineado con el transform del jugador (centro → pies). */
export function syncFirstPersonViewRefFromPlayer(
	firstPersonViewRef: MutableRefObject<SavedPlayerTransform | null>,
	playerId: number,
	entityTransformsRef: MutableRefObject<
		Record<number, import('../context/useContextEngine/types').Transform>
	>,
) {
	const t = entityTransformsRef.current[playerId];
	if (!t) return;
	const feet = feetFromPlayerBodyCenter(t.position, t.rotation);
	const prev = firstPersonViewRef.current;
	firstPersonViewRef.current = {
		position: feet,
		scale: FIRST_PERSON_PLAYER_BODY_SCALE,
		yaw: prev?.yaw ?? FP_DEFAULT_YAW,
		pitch: prev?.pitch ?? FP_EDITOR_ORBIT_PITCH,
		...(prev?.visual_model_path ? { visual_model_path: prev.visual_model_path } : {}),
	};
}

type SceneSlice = {
	entities?: SavedEntity[]
	playerTransform?: SavedPlayerTransform | null
};

export function applySavedFirstPersonView(
	view: SavedPlayerTransform | null | undefined,
	playerId: number | null,
	entityTransformsRef: MutableRefObject<Record<number, import('../context/useContextEngine/types').Transform>>,
	options?: { editorOrbit?: boolean },
) {
	if (!view?.position) return;
	const yaw = view.yaw ?? FP_DEFAULT_YAW;
	const pitch = options?.editorOrbit !== false
		? FP_EDITOR_ORBIT_PITCH
		: (view.pitch ?? FP_EDITOR_ORBIT_PITCH);
	window.engine.send({
		cmd: 'set_first_person_spawn',
		position: view.position,
		yaw,
		pitch,
	} as never);
	if (playerId != null) {
		const prev = entityTransformsRef.current[playerId];
		const rot = prev?.rotation ?? [0, 0, 0, 1];
		entityTransformsRef.current[playerId] = {
			position: bodyCenterFromFeet(view.position, rot),
			rotation: rot,
			scale: FIRST_PERSON_PLAYER_BODY_SCALE,
		};
	}
}

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
			const playerEntity = (scene.entities ?? []).find(
				(e) => e.kind === 'character' && isPlayerPath(e.path),
			);
			const pending: PendingRestore = {
				transform: {
					position: savedPlayer.position,
					rotation: [0, 0, 0, 1],
					scale: FIRST_PERSON_PLAYER_BODY_SCALE,
				},
				name: playerEntity?.name ?? 'Player',
				physicsEnabled: true,
				physicsType: 'dynamic',
				scripts: playerEntity?.scripts,
				controlBindings: playerEntity?.control_bindings,
				visualModelPath: savedPlayer.visual_model_path ?? playerEntity?.visual_model_path,
			};
			queue.push(pending);
			pendingRestoresRef.current.set('[Player]', queue);
		}
		send({ cmd: 'load_character', path: '[Player]' });
	}
}

export function buildSavedPlayerTransform(
	fpView: SavedPlayerTransform | null | undefined,
	feetPosition: [number, number, number] | undefined,
	visualModelPath?: string,
): SavedPlayerTransform | null {
	if (!feetPosition) return null;
	return {
		position: feetPosition,
		scale: FIRST_PERSON_PLAYER_BODY_SCALE,
		yaw: fpView?.yaw ?? FP_DEFAULT_YAW,
		pitch: fpView?.pitch ?? FP_EDITOR_ORBIT_PITCH,
		...(visualModelPath ? { visual_model_path: visualModelPath } : {}),
	};
}
