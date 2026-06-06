import { clampPercent, normalizeHexColor } from './playerUiButtonModel';

export type FillColorRgba = [number, number, number, number];

export type PlayerUiObjectStyleCommitOptions = {
	/** Preview en viewport sin reset de atlas ni sync IPC completo. */
	live?: boolean;
	/** Omitir snapshot de undo (p. ej. ticks intermedios del slider). */
	skip_undo?: boolean;
};

export const DEFAULT_OBJECT_FILL_COLOR: FillColorRgba = [0.28, 0.55, 0.92, 0.72];

export function fillColorToHex(fill: FillColorRgba): string {
	const r = Math.round(fill[0] * 255);
	const g = Math.round(fill[1] * 255);
	const b = Math.round(fill[2] * 255);
	return `#${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${b.toString(16).padStart(2, '0')}`;
}

/** 0 % = sólido, 100 % = totalmente transparente. */
export function fillColorToTransparencyPercent(fill: FillColorRgba): number {
	return Math.round(clampPercent(100 - fill[3] * 100));
}

export function hexAndTransparencyToFillColor(hex: string, transparencyPercent: number): FillColorRgba {
	const normalized = normalizeHexColor(hex) ?? fillColorToHex(DEFAULT_OBJECT_FILL_COLOR);
	const r = parseInt(normalized.slice(1, 3), 16) / 255;
	const g = parseInt(normalized.slice(3, 5), 16) / 255;
	const b = parseInt(normalized.slice(5, 7), 16) / 255;
	const alpha = 1 - clampPercent(transparencyPercent) / 100;
	return [r, g, b, alpha];
}
