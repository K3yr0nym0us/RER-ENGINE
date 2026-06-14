export const DEFAULT_3D_PLAYER_UI_SCREEN_ID = 'player-hud-01';

export const DEFAULT_3D_PLAYER_UI_SCREEN = {
	id: DEFAULT_3D_PLAYER_UI_SCREEN_ID,
	name: 'Player UI 01',
	active: true as const,
};

export const DEFAULT_2D_PLAYER_UI_SCREEN_ID = 'hud-01';

export const DEFAULT_2D_PLAYER_UI_SCREEN = {
	id: DEFAULT_2D_PLAYER_UI_SCREEN_ID,
	name: 'Player UI 01',
	active: true as const,
};

export function default3dPlayerUiScreens() {
	return [{ ...DEFAULT_3D_PLAYER_UI_SCREEN }];
}

export function default2dPlayerUiScreens() {
	return [{ ...DEFAULT_2D_PLAYER_UI_SCREEN }];
}
