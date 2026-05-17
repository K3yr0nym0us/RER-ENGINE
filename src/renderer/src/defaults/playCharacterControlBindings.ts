import type { SavedControlBindings } from '@shared-types'

const fpMove = (key: string, walkSpeed: number) => `local WALK_SPEED = ${walkSpeed}
engine.fp_set_walk_speed(WALK_SPEED)
engine.fp_press_key("${key}")
`

const fpSprint = (sprintMultiplier: number) => `local SPRINT_MULTIPLIER = ${sprintMultiplier}
engine.fp_set_sprint_multiplier(SPRINT_MULTIPLIER)
engine.fp_press_key("SHIFT")
`

const fpJump = (jumpSpeed: number) => `local JUMP_SPEED = ${jumpSpeed}
engine.fp_set_jump_speed(JUMP_SPEED)
engine.fp_jump()
`

/** Valores por defecto = mismos que las constantes del motor (4 / ×3 / 6). */
export const DEFAULT_PLAY_CHARACTER_CONTROL_BINDINGS: SavedControlBindings = {
	keyboard_mouse: {
		W: { name: 'fp_move_forward', source: fpMove('W', 4) },
		S: { name: 'fp_move_back', source: fpMove('S', 4) },
		A: { name: 'fp_move_left', source: fpMove('A', 4) },
		D: { name: 'fp_move_right', source: fpMove('D', 4) },
		SHIFT: { name: 'fp_sprint', source: fpSprint(3) },
		SPACE: { name: 'fp_jump', source: fpJump(6) },
	},
	gamepad: {},
}

/** @deprecated Use `DEFAULT_PLAY_CHARACTER_CONTROL_BINDINGS` */
export const DEFAULT_FIRST_PERSON_CONTROL_BINDINGS = DEFAULT_PLAY_CHARACTER_CONTROL_BINDINGS
