import type { EngineInternalRefs, PendingRestore } from '../types';

/** IPC `play_animation_frame` con pivot opcional (el motor resuelve si falta). */
export function buildPlayAnimationFrameCmd(
	entityId: number,
	anim: { logical_w?: number; logical_h?: number },
	frame: {
		path: string
		pivot_x?: number
		pivot_y?: number
		src_x?: number
		src_y?: number
		src_w?: number
		src_h?: number
	},
) {
	const logicalW = anim.logical_w ?? 64;
	const logicalH = anim.logical_h ?? 64;
	return {
		cmd: 'play_animation_frame' as const,
		id: entityId,
		path: frame.path,
		logical_w: logicalW,
		logical_h: logicalH,
		src_x: frame.src_x,
		src_y: frame.src_y,
		src_w: frame.src_w,
		src_h: frame.src_h,
		...(frame.pivot_x != null ? { pivot_x: frame.pivot_x } : {}),
		...(frame.pivot_y != null ? { pivot_y: frame.pivot_y } : {}),
	};
}

export function applyPendingRestoreMeta(
	refs: EngineInternalRefs,
	entityId: number,
	pending: PendingRestore,
) {
	const meta = refs.entityMetaRef.current[entityId];
	if (!meta) return;

	if (pending.name?.trim()) meta.name = pending.name;
	if (pending.physicsEnabled) {
		meta.physicsEnabled = true;
		meta.physicsType = pending.physicsType;
	}
	if (pending.animations) meta.animations = pending.animations;
	if (pending.scripts) meta.scripts = pending.scripts;
	if (pending.visualGraph) meta.visualGraph = pending.visualGraph;
	if (pending.visualScriptRhai) meta.visualScriptRhai = pending.visualScriptRhai;
	if (pending.controlBindings) meta.controlBindings = pending.controlBindings;
	if (pending.blueprintId) meta.blueprintId = pending.blueprintId;
	if (pending.entityCategory) meta.entityCategory = pending.entityCategory;
	if (pending.visualModelPath) meta.visualModelPath = pending.visualModelPath;
	refs.entityTransformsRef.current[entityId] = pending.transform;
}

export function sendApplyEntityRestore(
	entityId: number,
	pending: PendingRestore,
	options?: { omitScale?: boolean; skipTransform?: boolean; applyInitialAnimationFrame?: boolean },
) {
	window.engine.send({
		cmd: 'apply_entity_restore',
		id: entityId,
		...(pending.name?.trim() ? { name: pending.name } : {}),
		transform: pending.transform,
		...(pending.physicsEnabled
			? { physics: { enabled: true, body_type: pending.physicsType } }
			: {}),
		...(pending.animations?.length
			? {
					animations: pending.animations.map((anim: any) => ({
						name: anim.name,
						frames: anim.frames,
						fps: anim.fps,
						loop_: anim.loop,
						flip_horizontal: !(anim.facing_right ?? true),
						audio_path: anim.audio_path ?? null,
						scripts: (anim.scripts ?? []).map((s: { name: string; source: string }) => ({
							name: s.name,
							source: s.source,
						})),
						is_cancelable: anim.is_cancelable ?? true,
						is_default: !!anim.is_default,
					})),
				}
			: {}),
		...(pending.scripts?.length
			? {
					scripts: pending.scripts.map((s) => ({ path: s.name, source: s.source })),
				}
			: {}),
		...(pending.controlBindings ? { control_bindings: pending.controlBindings } : {}),
		omit_scale: options?.omitScale ?? false,
		skip_transform: options?.skipTransform ?? false,
		...(options?.applyInitialAnimationFrame !== undefined
			? { apply_initial_animation_frame: options.applyInitialAnimationFrame }
			: {}),
	} as never);
}
