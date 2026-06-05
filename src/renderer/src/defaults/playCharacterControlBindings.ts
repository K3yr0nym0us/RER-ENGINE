import type { SavedControlBindings } from '@shared-types'

/** El motor aplica `control_key` al ejecutar el binding; el script solo ajusta parámetros. */
const fpMove = (walkSpeed: number) => `let WALK_SPEED = ${walkSpeed};
engine.fp_set_walk_speed(WALK_SPEED);
`

const fpSprint = (sprintMultiplier: number) => `let SPRINT_MULTIPLIER = ${sprintMultiplier};
engine.fp_set_sprint_multiplier(SPRINT_MULTIPLIER);
`

const fpJump = (jumpSpeed: number) => `let JUMP_SPEED = ${jumpSpeed};
engine.fp_set_jump_speed(JUMP_SPEED);
engine.fp_jump();
`

/** Valores por defecto = mismos que las constantes del motor (4 / ×3 / 6). */
export const DEFAULT_PLAY_CHARACTER_CONTROL_BINDINGS: SavedControlBindings = {
	keyboard_mouse: {
		W: { name: 'fp_move', source: fpMove(4) },
		S: { name: 'fp_move', source: fpMove(4) },
		A: { name: 'fp_move', source: fpMove(4) },
		D: { name: 'fp_move', source: fpMove(4) },
		SHIFT: { name: 'fp_sprint', source: fpSprint(3) },
		SPACE: { name: 'fp_jump', source: fpJump(6) },
	},
	gamepad: {},
}
