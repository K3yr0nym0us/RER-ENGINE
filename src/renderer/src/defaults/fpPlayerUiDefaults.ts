export const DEFAULT_FP_PLAYER_UI_SCREEN_ID = 'fp-hud-01';

export const DEFAULT_FP_PLAYER_UI_SCREEN = {
	id: DEFAULT_FP_PLAYER_UI_SCREEN_ID,
	name: 'Player UI 01',
	active: true as const,
};

export const DEFAULT_2D_PLAYER_UI_SCREEN_ID = 'hud-01';

export const DEFAULT_2D_PLAYER_UI_SCREEN = {
	id: DEFAULT_2D_PLAYER_UI_SCREEN_ID,
	name: 'Player UI 01',
	active: true as const,
};

export function defaultFpPlayerUiScreens() {
	return [{ ...DEFAULT_FP_PLAYER_UI_SCREEN }];
}

export function default2dPlayerUiScreens() {
	return [{ ...DEFAULT_2D_PLAYER_UI_SCREEN }];
}
