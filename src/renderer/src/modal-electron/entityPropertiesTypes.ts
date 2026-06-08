import type { TransformSendCommand } from '../pages/EngineView/components/sidebar/PropertiesAccordion/TransformPanel'
import type { ScriptEntry } from '@hooks'

export interface EntityPropertiesAnimation {
	id?: string
	name: string
	fps: number
	loop: boolean
	embedded_in_model?: boolean
	is_default?: boolean
	is_cancelable?: boolean
	facing_right?: boolean
	logical_w: number
	logical_h: number
	audio_path?: string
	scripts?: { name: string; source: string }[]
	frames: Array<{
		path: string
		pivot_x: number
		pivot_y: number
		src_x?: number
		src_y?: number
		src_w?: number
		src_h?: number
	}>
	selection_mode?: 'cell' | 'box'
	grid_size?: number
	cell_offset_x?: number
	cell_offset_y?: number
}

export interface EntityPropertiesEntity {
	id: number
	name: string
	position: [number, number, number]
	rotation: [number, number, number, number]
	scale: [number, number, number]
	physicsEnabled: boolean
	physicsType: string
	animations?: EntityPropertiesAnimation[]
}

export interface EntityPropertiesState {
	projectType: string
	selectedEntity: EntityPropertiesEntity | null
	multiSelectedIds: number[]
	isScenario: boolean
	isCharacter: boolean
	isEnvironment: boolean
	isPlayer: boolean
	isEditorCamera: boolean
	isCollider: boolean
	isExecutionArea: boolean
	isFromBlueprint: boolean
	linkedBlueprintName: string | null
	scripts: ScriptEntry[]
	animationPlayingIds: number[]
}

export type EntityPropertiesNestedModalKind =
	| 'confirm'
	| 'replaceModel'
	| 'convertBlueprint'
	| 'createAnimation'
	| 'editAnimation'
	| 'spritePreview'
	| 'scriptEditor'
	| 'visualScripting'
	| 'deleteAnimation'
	| 'deleteScript'

export type EntityPropertiesAction =
	| { action: 'setEntityName'; id: number; name: string }
	| { action: 'setTransform'; cmd: TransformSendCommand }
	| { action: 'setPhysics'; id: number; enabled: boolean; bodyType: string }
	| { action: 'removeEntity'; id: number }
	| { action: 'removeMultiple'; ids: number[] }
	| { action: 'replaceModel'; id: number; path: string }
	| { action: 'convertToBlueprint'; id: number }
	| { action: 'updateAnimations'; id: number; animations: EntityPropertiesAnimation[] }
	| { action: 'send'; cmd: Record<string, unknown> }
	| { action: 'sendAsync'; cmd: Record<string, unknown>; waitEvent: string }
	| { action: 'setAnimationPlaying'; id: number; playing: boolean }
	| { action: 'updateScripts'; id: number; scripts: ScriptEntry[] }
	| { action: 'openNestedModal'; kind: EntityPropertiesNestedModalKind; payload: Record<string, unknown> }
	| { action: 'close' }
