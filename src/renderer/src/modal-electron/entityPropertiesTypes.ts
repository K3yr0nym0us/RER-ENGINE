import type { TransformSendCommand } from '../pages/EngineView/components/sidebar/PropertiesAccordion/TransformPanel'
import type { ScriptEntry } from '@hooks'
import type { EntityBonePhysics3D, ProjectileConfig3D } from '@shared-types'

export type BonePhysicsMode = EntityBonePhysics3D['mode']

export interface EntityPropertiesBonePhysicsUi {
	entries: EntityBonePhysics3D[]
	selectedBoneName: string | null
	draftMode: BonePhysicsMode
	bonePickActive: boolean
}

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
	multiSelectAlreadyMerged: boolean
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
	playingAnimationName: string | null
	canHaveBonePhysics: boolean
	bonePhysics: EntityPropertiesBonePhysicsUi | null
	isProjectile: boolean
	projectileConfig: ProjectileConfig3D | null
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
	| { action: 'setProjectileConfig'; id: number; speed: number; lifetimeS: number }
	| { action: 'fireProjectile'; templateId: number; dir: [number, number, number]; fromId?: number }
	| { action: 'removeEntity'; id: number }
	| { action: 'removeMultiple'; ids: number[] }
	| { action: 'mergeEntities'; ids: number[] }
	| { action: 'replaceModel'; id: number; path: string }
	| { action: 'convertToBlueprint'; id: number }
	| { action: 'updateAnimations'; id: number; animations: EntityPropertiesAnimation[] }
	| { action: 'send'; cmd: Record<string, unknown> }
	| { action: 'sendAsync'; cmd: Record<string, unknown>; waitEvent: string }
	| { action: 'setAnimationPlaying'; id: number; playing: boolean; animationName?: string | null }
	| { action: 'updateScripts'; id: number; scripts: ScriptEntry[] }
	| { action: 'openNestedModal'; kind: EntityPropertiesNestedModalKind; payload: Record<string, unknown> }
	| { action: 'setBonesTabActive'; active: boolean }
	| { action: 'setBonePickMode'; active: boolean }
	| { action: 'setBoneDraftMode'; mode: BonePhysicsMode }
	| { action: 'applyBonePhysics' }
	| { action: 'removeBonePhysics'; boneName: string }
	| { action: 'setBoneEntryMode'; boneName: string; mode: BonePhysicsMode }
	| { action: 'requestBonePhysicsList' }
	| { action: 'close' }
