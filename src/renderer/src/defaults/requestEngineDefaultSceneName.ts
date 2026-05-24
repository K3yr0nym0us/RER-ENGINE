const DEFAULT_SCENE_NAME_TIMEOUT_MS = 5_000;

/** Ask the engine for the default editor scene tab name (`Scene-01`, …). */
export function requestEngineDefaultSceneName(id: number): Promise<string> {
	return new Promise((resolve, reject) => {
		const onEngineEvent = (event: { event: string; id?: number; name?: string; message?: string }) => {
			if (event.event === 'default_scene_name_ready' && event.id === id) {
				cleanup();
				resolve(String(event.name ?? ''));
			}
			if (event.event === 'error') {
				cleanup();
				reject(new Error(event.message ?? 'Error al obtener nombre de escena del motor'));
			}
		};

		const cleanup = () => {
			window.clearTimeout(timeout);
			window.engine.off(onEngineEvent);
		};

		const timeout = window.setTimeout(() => {
			cleanup();
			reject(new Error('Timeout esperando default_scene_name_ready del motor'));
		}, DEFAULT_SCENE_NAME_TIMEOUT_MS);

		window.engine.on(onEngineEvent);
		window.engine.send({ cmd: 'get_default_scene_name', id } as never);
	});
}
