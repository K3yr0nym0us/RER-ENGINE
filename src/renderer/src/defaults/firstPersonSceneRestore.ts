import type { SavedEntity, SavedPlayerTransform } from '@shared-types';
import { FIRST_PERSON_PLAYER_BODY_SCALE, isPlayerPath } from '@shared-types';
import type { MutableRefObject } from 'react';
import type { PendingRestore } from '../context/useContextEngine/types';

/** Pitch de órbita en editor (detrás del personaje). */
export const FP_EDITOR_ORBIT_PITCH = 0.25;
export const FP_DEFAULT_YAW = -Math.PI / 2;

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
		entityTransformsRef.current[playerId] = {
			position: view.position,
			rotation: [0, 0, 0, 1],
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
