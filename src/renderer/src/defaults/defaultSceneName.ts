/** Mismo formato que `rer_engine_shared::editor_defaults::default_scene_name`. */
export function defaultSceneName(sceneId: number): string {
	const id = Math.max(0, Math.floor(sceneId));
	return `Scene-${String(id).padStart(2, '0')}`;
}
