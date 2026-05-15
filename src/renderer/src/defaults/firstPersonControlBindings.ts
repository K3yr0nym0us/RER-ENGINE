import type { SavedControlBindings } from '@shared-types'

/**
 * Plantilla Lua por tecla (accordion Controles).
 * El motor aplica WASD, Shift y Space de forma nativa en primera persona;
 * estos scripts documentan el mapeo y sirven para personalizar con engine.fp_* si lo necesitas.
 */
const nativeNote = (label: string, key: string) =>
	`-- ${label} (${key})\n-- Comportamiento nativo del motor en primera persona.\n-- Opcional: engine.fp_press_key("${key}") para lógica personalizada.\n`

export const DEFAULT_FIRST_PERSON_CONTROL_BINDINGS: SavedControlBindings = {
	keyboard_mouse: {
		W: { name: 'fp_move_forward', source: nativeNote('Avanzar', 'W') },
		S: { name: 'fp_move_back', source: nativeNote('Retroceder', 'S') },
		A: { name: 'fp_move_left', source: nativeNote('Izquierda', 'A') },
		D: { name: 'fp_move_right', source: nativeNote('Derecha', 'D') },
		SHIFT: { name: 'fp_sprint', source: nativeNote('Sprint', 'SHIFT') },
		SPACE: { name: 'fp_jump', source: nativeNote('Salto', 'SPACE') },
	},
	gamepad: {},
}
