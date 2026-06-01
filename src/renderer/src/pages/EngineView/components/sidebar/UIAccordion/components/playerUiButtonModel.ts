/** Forma del botón HUD (editor UI jugador). */
export type UiButtonShapeType =
	| 'square'
	| 'rectangle'
	| 'diamond'
	| 'triangle'
	| 'circle';

export interface PlayerUiButtonConfig {
	type: UiButtonShapeType;
	/** Redondeo de esquinas (px); en círculo se ignora visualmente. */
	round: number;
	backgroundColor: string;
	texturePath: string | null;
	/** 0 = transparente, 100 = opaco. */
	transparencyBackground: number;
	text: string;
	textColor: string;
	transparencyText: number;
	fontPath: string;
	fontName: string;
	borderColor: string;
	borderWeight: number;
}

export const UI_BUTTON_SHAPE_OPTIONS: UiButtonShapeType[] = [
	'square',
	'rectangle',
	'diamond',
	'triangle',
	'circle',
];

export const DEFAULT_PLAYER_UI_BUTTON_CONFIG: PlayerUiButtonConfig = {
	type: 'rectangle',
	round: 8,
	backgroundColor: '#2563eb',
	texturePath: null,
	transparencyBackground: 100,
	text: 'Button',
	textColor: '#ffffff',
	transparencyText: 100,
	fontPath: '',
	fontName: '',
	borderColor: '#e2e8f0',
	borderWeight: 2,
};

export function hexToRgba(hex: string, alphaPercent: number): string {
	const normalized = normalizeHexColor(hex);
	if (!normalized) {
		return `rgba(37, 99, 235, ${clampPercent(alphaPercent) / 100})`;
	}
	const r = parseInt(normalized.slice(1, 3), 16);
	const g = parseInt(normalized.slice(3, 5), 16);
	const b = parseInt(normalized.slice(5, 7), 16);
	return `rgba(${r}, ${g}, ${b}, ${clampPercent(alphaPercent) / 100})`;
}

export function normalizeHexColor(value: string): string | null {
	let v = value.trim();
	if (!v.startsWith('#')) v = `#${v}`;
	if (/^#[0-9a-fA-F]{6}$/.test(v)) return v.toLowerCase();
	if (/^#[0-9a-fA-F]{3}$/.test(v)) {
		const r = v[1];
		const g = v[2];
		const b = v[3];
		return `#${r}${r}${g}${g}${b}${b}`.toLowerCase();
	}
	return null;
}

export function clampPercent(n: number): number {
	if (!Number.isFinite(n)) return 100;
	return Math.min(100, Math.max(0, n));
}

export function clampRound(n: number): number {
	if (!Number.isFinite(n)) return 0;
	return Math.min(64, Math.max(0, Math.round(n)));
}

export function clampBorderWeight(n: number): number {
	if (!Number.isFinite(n)) return 0;
	return Math.min(24, Math.max(0, n));
}

export function buttonPreviewMetrics(type: UiButtonShapeType): { width: number; height: number } {
	switch (type) {
		case 'square':
			return { width: 120, height: 120 };
		case 'diamond':
			return { width: 120, height: 120 };
		case 'triangle':
			return { width: 140, height: 110 };
		case 'circle':
			return { width: 120, height: 120 };
		default:
			return { width: 160, height: 72 };
	}
}

export function buttonShapeClipPath(type: UiButtonShapeType): string | undefined {
	switch (type) {
		case 'diamond':
			return 'polygon(50% 0%, 100% 50%, 50% 100%, 0% 50%)';
		case 'triangle':
			return 'polygon(50% 8%, 92% 92%, 8% 92%)';
		default:
			return undefined;
	}
}

export function buttonShapeBorderRadius(
	type: UiButtonShapeType,
	round: number,
): string | number {
	if (type === 'circle') return '50%';
	if (type === 'diamond' || type === 'triangle') return 0;
	return clampRound(round);
}
