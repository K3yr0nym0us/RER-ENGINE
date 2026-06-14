import type { GameStyle } from '@shared-types';

export interface CameraModeOption {
	type: GameStyle;
	labelKey: string;
	available: boolean;
}

/** Opciones del selector de cámara 3D (sidebar Camera). */
export function get3DCameraModeOptions(): CameraModeOption[] {
	return [
		{ type: 'first-person', labelKey: 'First Person', available: true },
		{ type: 'second-person', labelKey: 'Second Person', available: false },
		{ type: 'third-person', labelKey: 'Third Person', available: false },
		{ type: 'top-down', labelKey: 'Top Down', available: false },
		{ type: 'side-scroller', labelKey: 'Side Scroller', available: false },
		{ type: 'isometric', labelKey: 'Isometric', available: false },
	];
}
