import type { ProjectType } from '@shared-types';

export interface SetSceneEngineCommand {
	cmd: 'set_scene';
	scene: '2D' | '3D';
	save_path: string;
}

/** Único IPC de arranque/cambio de escena: dimensión 2D|3D + ruta al `.save` (vacío si proyecto nuevo). */
export function buildSetSceneCommand(
	projectType: ProjectType | string | undefined,
	savePath: string | null | undefined,
): SetSceneEngineCommand {
	return {
		cmd: 'set_scene',
		scene: projectType === '3D' ? '3D' : '2D',
		save_path: savePath ?? '',
	};
}
