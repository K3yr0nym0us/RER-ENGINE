import { createElement } from 'react'

import type { EngineContextValue } from '@engine'
import type { OpenModalElectronOptions } from '../hooks/useModalElectron'
import type {
	EntityPropertiesAction,
	EntityPropertiesState,
} from './entityPropertiesTypes'
import {
	blueprintCategoryFromEntity,
	blueprintEntityCategoryForEngine,
	nextBlueprintTemplateName,
	resolveBlueprintModelPath,
} from '../utils/blueprintModelPath'
import {
	isEditorCameraEntity,
	isEnvironmentEntity,
	isPlayerEntity,
} from '@shared-types'
import {
	isMultiSelectionMerged,
} from '../utils/entity3dEditorSync'
import type { BluePrintEntry } from '@shared-types'
import { CreateEntityFromModelModalBody } from '../pages/EngineView/components/sidebar/EntitiesAccordion/components/CreateEntityFromModelModalBody'
import { CreateEntityFromSpriteModalBody } from '../pages/EngineView/components/sidebar/EntitiesAccordion/components/CreateEntityFromSpriteModalBody'
import { SpritePreviewModalBody } from '@components'
import ScriptEditorModalBody from '../components/SpritePreviewModalBody/components/ScriptEditorModalBody'
import { VisualScriptingModalBody } from '../visualScripting/components/VisualScriptingModalBody'
import { ModalConfirmBody } from './ModalConfirmBody'
import { createEmptyEntityVisualGraph, saveEntityVisualGraph } from '../visualScripting/entityVisualScript'
import { resolveSceneEntitiesForVisualScript } from '../visualScripting/resolveSceneEntities'
import { buildPlayAnimationFrameCmd } from '../context/useContextEngine/hooks/applyPendingRestoreToEngine'
import type { TransformSendCommand } from '../pages/EngineView/components/sidebar/PropertiesAccordion/TransformPanel'

export const activeEntityPropertiesHandlerRef = { current: null as string | null }

export interface EntityPropertiesSessionDeps {
	getEngine: () => EngineContextValue
	openModal: (opts: OpenModalElectronOptions) => void
	closeModal: () => void
	pushPatch: (handlerId: string, state: EntityPropertiesState) => void
	onCloseModal: () => void
	t: (key: string) => string
}

interface Session {
	handlerId: string
	deps: EntityPropertiesSessionDeps
}

const sessions = new Map<string, Session>()

export function registerEntityPropertiesSession(handlerId: string, deps: EntityPropertiesSessionDeps): void {
	sessions.set(handlerId, { handlerId, deps })
	activeEntityPropertiesHandlerRef.current = handlerId
}

export function unregisterEntityPropertiesSession(handlerId: string): void {
	sessions.delete(handlerId)
	if (activeEntityPropertiesHandlerRef.current === handlerId) {
		activeEntityPropertiesHandlerRef.current = null
	}
}

export function buildEntityPropertiesState(engine: EngineContextValue): EntityPropertiesState {
	const { selectedEntity, multiSelectedIds, scenarioEntities, characterEntities } = engine
	if (!selectedEntity) {
		return {
			projectType: engine.projectType ?? '2D',
			selectedEntity: null,
			multiSelectedIds: [...multiSelectedIds],
			multiSelectAlreadyMerged: isMultiSelectionMerged(
				multiSelectedIds,
				engine.entityMetaRef.current,
			),
			isScenario: false,
			isCharacter: false,
			isEnvironment: false,
			isPlayer: false,
			isEditorCamera: false,
			isCollider: false,
			isExecutionArea: false,
			isFromBlueprint: false,
			linkedBlueprintName: null,
			scripts: [],
			animationPlayingIds: [],
			playingAnimationName: null,
		}
	}

	const entityMeta = engine.entityMetaRef.current[selectedEntity.id]
	const isScenario = scenarioEntities.some((s) => s.id === selectedEntity.id)
	const isEnvironment = isEnvironmentEntity(isScenario, entityMeta)
	const isPlayer = isPlayerEntity(selectedEntity.id, entityMeta, engine.playerEntityIdRef.current)
	const isEditorCamera = isEditorCameraEntity(
		selectedEntity.id,
		entityMeta,
		engine.editorCameraEntityIdRef.current,
	)
	const isCharacter = characterEntities.some((c) => c.id === selectedEntity.id)
	const isCollider = entityMeta?.kind === 'collider'
	const isExecutionArea = entityMeta?.kind === 'execution_area'
	const isFromBlueprint = !!entityMeta?.blueprintId
	const linkedBlueprintName = isFromBlueprint
		? (engine.blueprints.find((bp) => bp.id === entityMeta?.blueprintId)?.name ?? null)
		: null

	const metaAnimations = entityMeta?.animations ?? []
	const embedded = metaAnimations.filter((a) => a.embedded_in_model)
	const animations =
		embedded.length > 0 ? embedded : (selectedEntity.animations ?? metaAnimations)

	return {
		projectType: engine.projectType ?? '2D',
		selectedEntity: {
			id: selectedEntity.id,
			name: selectedEntity.name,
			position: selectedEntity.position,
			rotation: selectedEntity.rotation,
			scale: selectedEntity.scale,
			physicsEnabled: selectedEntity.physicsEnabled,
			physicsType: selectedEntity.physicsType,
			animations: animations as EntityPropertiesState['selectedEntity'] extends infer E
				? E extends { animations?: infer A }
					? A
					: never
				: never,
		},
		multiSelectedIds: [...multiSelectedIds],
		multiSelectAlreadyMerged: isMultiSelectionMerged(
			multiSelectedIds,
			engine.entityMetaRef.current,
		),
		isScenario,
		isCharacter,
		isEnvironment,
		isPlayer,
		isEditorCamera,
		isCollider,
		isExecutionArea,
		isFromBlueprint,
		linkedBlueprintName,
		scripts: entityMeta?.scripts ?? [],
		animationPlayingIds: entityMeta?.playingAnimationName
			? [selectedEntity.id]
			: [],
		playingAnimationName: entityMeta?.playingAnimationName ?? null,
	}
}

export function requestEntityAnimationPlayStateSync(
	engine: EngineContextValue,
	entityId: number,
): void {
	engine.send({ cmd: 'query_entity_animation_play_state', entity_id: entityId } as never)
}

export function pushEntityPropertiesPatch(handlerId: string): void {
	const session = sessions.get(handlerId)
	if (!session) return
	session.deps.pushPatch(handlerId, buildEntityPropertiesState(session.deps.getEngine()))
}

function handleTransform(engine: EngineContextValue, cmd: TransformSendCommand): void {
	const selected = engine.selectedEntity
	if (cmd.cmd === 'set_transform' && selected && cmd.id === selected.id) {
		engine.updateEntityTransform(selected.id, {
			...(cmd.position !== undefined ? { position: cmd.position } : {}),
			...(cmd.position_axis !== undefined ? { positionAxis: cmd.position_axis } : {}),
			...(cmd.rotation !== undefined ? { rotation: cmd.rotation } : {}),
			...(cmd.scale !== undefined ? { scale: cmd.scale } : {}),
			...(cmd.scale_axis !== undefined ? { scaleAxis: cmd.scale_axis } : {}),
			...(cmd.body_rotation_only ? { bodyRotationOnly: true } : {}),
			...(cmd.rotation_euler_delta !== undefined
				? { rotationEulerDelta: cmd.rotation_euler_delta }
				: {}),
			...(cmd.rotation_euler_degrees !== undefined
				? { rotationEulerDegrees: cmd.rotation_euler_degrees }
				: {}),
		})
		return
	}
	engine.send(cmd)
}

function removeEntityByKind(engine: EngineContextValue, id: number): void {
	const meta = engine.entityMetaRef.current[id]
	const kind = meta?.kind
	if (engine.scenarioEntities.some((s) => s.id === id)) engine.removeScenario(id)
	else if (engine.characterEntities.some((c) => c.id === id)) engine.removeCharacter(id)
	else if (kind === 'collider') engine.removeCollider(id)
	else if (kind === 'execution_area') engine.removeExecutionArea(id)
	else engine.removeEntity(id)
}

export async function runEntityPropertiesAction(
	handlerId: string,
	action: EntityPropertiesAction,
): Promise<void> {
	const session = sessions.get(handlerId)
	if (!session) return
	const engine = session.deps.getEngine()
	const { openModal, closeModal, t } = session.deps

	switch (action.action) {
		case 'close': {
			session.deps.onCloseModal()
			session.deps.closeModal()
			return
		}
		case 'setEntityName': {
			const trimmed = action.name.trim()
			if (!trimmed) return
			if (engine.entityMetaRef.current[action.id]) {
				engine.entityMetaRef.current[action.id].name = trimmed
			}
			engine.send({ cmd: 'set_entity_name', id: action.id, name: trimmed })
			pushEntityPropertiesPatch(handlerId)
			return
		}
		case 'setTransform':
			handleTransform(engine, action.cmd)
			pushEntityPropertiesPatch(handlerId)
			return
		case 'setPhysics':
			engine.setEntityPhysics(action.id, action.enabled, action.bodyType)
			pushEntityPropertiesPatch(handlerId)
			return
		case 'removeEntity':
			removeEntityByKind(engine, action.id)
			return
		case 'removeMultiple':
			action.ids.forEach((id) => {
				if (isPlayerEntity(id, engine.entityMetaRef.current[id], engine.playerEntityIdRef.current)) return
				if (
					isEditorCameraEntity(
						id,
						engine.entityMetaRef.current[id],
						engine.editorCameraEntityIdRef.current,
					)
				) {
					return
				}
				removeEntityByKind(engine, id)
			})
			return
		case 'mergeEntities':
			engine.send({ cmd: 'merge_entities', ids: action.ids })
			return
		case 'replaceModel':
			engine.replaceEntityModel(action.id, action.path)
			pushEntityPropertiesPatch(handlerId)
			return
		case 'convertToBlueprint': {
			const selectedEntity = engine.selectedEntity
			if (!selectedEntity || selectedEntity.id !== action.id) return
			const meta = engine.entityMetaRef.current[action.id]
			const kind = meta?.kind ?? 'model'
			const transform = engine.entityTransformsRef.current[action.id]
			const path = meta?.path ?? ''
			const modelPath = resolveBlueprintModelPath({
				path,
				model: meta?.visualModelPath ?? path,
				visualModelPath: meta?.visualModelPath,
			})
			const resolvedCategory = blueprintCategoryFromEntity(
				isEnvironmentEntity(
					engine.scenarioEntities.some((s) => s.id === action.id),
					meta,
				),
				kind,
				meta?.entityCategory,
				meta?.entity3dCategory,
				selectedEntity.name,
				engine.models,
				modelPath,
			)
			const bpName = nextBlueprintTemplateName(resolvedCategory, [
				...engine.blueprints.map((b) => b.name),
				...Object.values(engine.entityMetaRef.current)
					.map((m) => m.name)
					.filter((n): n is string => Boolean(n)),
			])
			const entity_category = blueprintEntityCategoryForEngine(resolvedCategory)
			const draft: BluePrintEntry = {
				id: `bp_${Date.now()}_${Math.random().toString(36).slice(2, 7)}`,
				name: bpName,
				category: resolvedCategory,
				kind,
				path,
				scale: transform?.scale ?? [1, 1, 1],
				rotation: transform?.rotation ?? [0, 0, 0, 1],
				colision: meta?.physicsEnabled ?? true,
				physics_enabled: meta?.physicsEnabled,
				physics_type: meta?.physicsType,
				animations: meta?.animations,
				scripts: meta?.scripts,
				control_bindings: meta?.controlBindings,
				visualModelPath: meta?.visualModelPath,
				...(entity_category ? { entity_category } : {}),
			}
			const model = resolveBlueprintModelPath(draft)
			const entry: BluePrintEntry = { ...draft, model, path: model }
			engine.addBlueprint(entry)
			if (engine.entityMetaRef.current[action.id]) {
				engine.entityMetaRef.current[action.id].blueprintId = entry.id
			}
			pushEntityPropertiesPatch(handlerId)
			return
		}
		case 'updateAnimations':
			engine.updateEntityAnimations(action.id, action.animations)
			pushEntityPropertiesPatch(handlerId)
			return
		case 'send':
			engine.send(action.cmd as never)
			pushEntityPropertiesPatch(handlerId)
			return
		case 'sendAsync':
			await engine.sendAsync(action.cmd as never, action.waitEvent as never)
			pushEntityPropertiesPatch(handlerId)
			return
		case 'setAnimationPlaying':
			engine.setAnimationPlaying(action.id, action.playing, action.animationName)
			pushEntityPropertiesPatch(handlerId)
			return
		case 'updateScripts':
			engine.updateEntityScripts(action.id, action.scripts)
			pushEntityPropertiesPatch(handlerId)
			return
		case 'openNestedModal':
			await openEntityPropertiesNestedModal(handlerId, action.kind, action.payload, {
				engine,
				openModal,
				closeModal,
				t,
			})
			pushEntityPropertiesPatch(handlerId)
			return
		default:
			return
	}
}

async function openEntityPropertiesNestedModal(
	handlerId: string,
	kind: EntityPropertiesAction extends { action: 'openNestedModal' }
		? EntityPropertiesAction['kind']
		: never,
	payload: Record<string, unknown>,
	ctx: {
		engine: EngineContextValue
		openModal: (opts: OpenModalElectronOptions) => void
		closeModal: () => void
		t: (key: string) => string
	},
): Promise<void> {
	const { engine, openModal, closeModal, t } = ctx
	const selectedEntity = engine.selectedEntity
	if (!selectedEntity) return

	switch (kind) {
		case 'confirm': {
			const onConfirm = payload.onConfirm as (() => void) | undefined
			openModal({
				title: (payload.title as string) ?? t('Confirm action'),
				size: 'sm',
				body: createElement(ModalConfirmBody, {
					message: payload.message,
					confirmLabel: payload.confirmLabel as string,
					confirmVariant: payload.confirmVariant as 'primary' | 'danger' | undefined,
					onConfirm: () => {
						onConfirm?.()
						closeModal()
					},
				}),
			})
			return
		}
		case 'replaceModel': {
			const isPlayer = payload.isPlayer as boolean
			const isCharacter = payload.isCharacter as boolean
			const isEnvironment = payload.isEnvironment as boolean
			openModal({
				title: t('Replace model'),
				size: 'md',
				body: createElement(CreateEntityFromModelModalBody, {
					hintText: payload.hintText as string,
					intent: isPlayer || isCharacter ? 'character' : isEnvironment ? 'environment' : 'object',
					onSpawn: (path: string) => {
						engine.replaceEntityModel(selectedEntity.id, path)
						closeModal()
						pushEntityPropertiesPatch(handlerId)
					},
				}),
			})
			return
		}
		case 'convertBlueprint':
			openModal({
				title: t('Convert to Blueprint'),
				size: 'md',
				body: createElement(ModalConfirmBody, {
					confirmVariant: 'primary',
					confirmLabel: t('Confirm'),
					messageSpec: {
						template: 'convertBlueprint',
						entityName: selectedEntity.name,
					},
					onConfirm: () => {
						void runEntityPropertiesAction(handlerId, {
							action: 'convertToBlueprint',
							id: selectedEntity.id,
						})
						closeModal()
					},
				}),
			})
			return
		case 'createAnimation': {
			const entityId = payload.entityId as number
			openModal({
				title: t('New animation'),
				body: createElement(CreateEntityFromSpriteModalBody, {
					sprites: engine.sprites,
					previewTitle: t('Configure animation'),
					onCreateEntity: ({
						spritePath,
						animation,
					}: {
						spritePath: string
						animation: {
							name: string
							fps: number
							loop: boolean
							defaultAnimation?: boolean
							isCancelable?: boolean
							facingRight?: boolean
							audioPath?: string
							scripts?: { name: string; source: string }[]
							selectionMode?: string
							gridSize?: number
							cellOffsetX?: number
							cellOffsetY?: number
							frames: Array<{ x: number; y: number; width: number; height: number; pivot_x?: number; pivot_y?: number }>
						}
					}) => {
						const meta = engine.entityMetaRef.current[entityId]
						const current = meta?.animations ?? []
						const mappedFrames = animation.frames.map((f) => ({
							path: spritePath,
							...(f.pivot_x != null ? { pivot_x: f.pivot_x } : {}),
							...(f.pivot_y != null ? { pivot_y: f.pivot_y } : {}),
							src_x: f.x,
							src_y: f.y,
							src_w: f.width,
							src_h: f.height,
						}))
						const logical_w = Math.max(1, ...animation.frames.map((f) => f.width))
						const logical_h = Math.max(1, ...animation.frames.map((f) => f.height))
						const markDefault = animation.defaultAnimation === true
						const newAnimation = {
							id: `anim_${Date.now()}`,
							name: animation.name,
							fps: animation.fps,
							loop: animation.loop,
							is_default: markDefault,
							is_cancelable: animation.isCancelable,
							facing_right: animation.facingRight,
							logical_w,
							logical_h,
							audio_path: animation.audioPath,
							scripts: animation.scripts,
							selection_mode: animation.selectionMode as 'cell' | 'box' | undefined,
							grid_size: animation.gridSize,
							cell_offset_x: animation.cellOffsetX,
							cell_offset_y: animation.cellOffsetY,
							frames: mappedFrames,
						}
						const next = markDefault
							? [...current.map((a) => ({ ...a, is_default: false })), newAnimation]
							: [...current, newAnimation]
						const resolved = engine.updateEntityAnimations(entityId, next)
						const synced = resolved.find((a) => a.name === newAnimation.name) ?? newAnimation
						const first = synced.frames[0]
						if (first) {
							engine.send(buildPlayAnimationFrameCmd(entityId, synced, first))
						}
						closeModal()
						pushEntityPropertiesPatch(handlerId)
					},
				}),
			})
			return
		}
		case 'editAnimation': {
			const entityId = payload.entityId as number
			const animationIndex = payload.animationIndex as number
			const spritePath = payload.spritePath as string
			const meta = engine.entityMetaRef.current[entityId]
			const animations = meta?.animations ?? []
			const anim = animations[animationIndex]
			openModal({
				title: `${t('Edit animation:')} ${payload.animationName as string}`,
				size: 'xl',
				body: createElement(SpritePreviewModalBody, {
					src: spritePath,
					initialAnimationName: payload.animationName as string,
					initialFrames: payload.initialFrames,
					initialFps: payload.initialFps as number,
					initialLoop: payload.initialLoop as boolean,
					initialIsDefaultAnimation: payload.initialIsDefault as boolean,
					initialIsCancelable: payload.initialIsCancelable as boolean,
					initialFacingRight: payload.initialFacingRight as boolean,
					initialAudioPath: payload.initialAudioPath as string | undefined,
					initialScripts: payload.initialScripts,
					initialSelectionMode: payload.initialSelectionMode,
					initialGridSize: payload.initialGridSize as number | undefined,
					initialCellOffsetX: payload.initialCellOffsetX as number | undefined,
					initialCellOffsetY: payload.initialCellOffsetY as number | undefined,
					onConfirm: (config: {
						animationName: string
						fps: number
						loop: boolean
						defaultAnimation: boolean
						isCancelable: boolean
						facingRight: boolean
						audioPath?: string
						scripts?: { name: string; source: string }[]
						selectionMode?: 'cell' | 'box'
						gridSize?: number
						cellOffsetX?: number
						cellOffsetY?: number
						frames: Array<{ x: number; y: number; width: number; height: number; pivot_x?: number; pivot_y?: number }>
					}) => {
						if (!anim) {
							closeModal()
							return
						}
						const updatedAnimation = {
							...anim,
							name: config.animationName,
							fps: config.fps,
							loop: config.loop,
							is_default: config.defaultAnimation,
							is_cancelable: config.isCancelable,
							facing_right: config.facingRight,
							audio_path: config.audioPath,
							scripts: config.scripts,
							selection_mode: config.selectionMode,
							grid_size: config.gridSize,
							cell_offset_x: config.cellOffsetX,
							cell_offset_y: config.cellOffsetY,
							frames: config.frames.map((f) => ({
								path: spritePath,
								...(f.pivot_x != null ? { pivot_x: f.pivot_x } : {}),
								...(f.pivot_y != null ? { pivot_y: f.pivot_y } : {}),
								src_x: f.x,
								src_y: f.y,
								src_w: f.width,
								src_h: f.height,
							})),
						}
						const next = animations.map((a, i) => {
							if (i === animationIndex) return updatedAnimation
							if (config.defaultAnimation) return { ...a, is_default: false }
							return a
						})
						const resolved = engine.updateEntityAnimations(entityId, next)
						const synced = resolved.find((a) => a.name === updatedAnimation.name) ?? updatedAnimation
						const first = synced.frames[0]
						if (first) {
							engine.send(buildPlayAnimationFrameCmd(entityId, synced, first))
						}
						closeModal()
						pushEntityPropertiesPatch(handlerId)
					},
				}),
			})
			return
		}
		case 'scriptEditor': {
			const entityId = payload.entityId as number
			const replacing = payload.replacing as string | undefined
			const meta = engine.entityMetaRef.current[entityId]
			const currentScripts = meta?.scripts ?? []
			openModal({
				title: (payload.title as string) ?? t('New Rhai script'),
				size: 'lg',
				body: createElement(ScriptEditorModalBody, {
					initialData: payload.initialData,
					onSave: (data: { name: string; source: string }) => {
						const next = replacing
							? currentScripts.map((s) => (s.name === replacing ? data : s))
							: [...currentScripts, data]
						engine.updateEntityScripts(entityId, next)
						engine.send({ cmd: 'load_script', id: entityId, path: data.name, source: data.source })
						closeModal()
						pushEntityPropertiesPatch(handlerId)
					},
					onCancel: closeModal,
				}),
			})
			return
		}
		case 'visualScripting': {
			const entityId = selectedEntity.id
			const meta = engine.entityMetaRef.current[entityId]
			const initialGraph = meta?.visualGraph ?? createEmptyEntityVisualGraph(entityId)
			const sceneEntities = resolveSceneEntitiesForVisualScript({
				entityMeta: engine.entityMetaRef.current,
				entityTransforms: engine.entityTransformsRef.current,
			})
			openModal({
				title: t('Entity logic'),
				size: 'xl',
				body: createElement(VisualScriptingModalBody, {
					context: 'entity',
					entityId,
					entityName: selectedEntity.name ?? meta?.name,
					sceneEntities,
					blueprints: engine.blueprints,
					initialGraph,
					onSave: (graph: Parameters<typeof saveEntityVisualGraph>[1]) => {
						const saveResult = saveEntityVisualGraph(entityId, graph)
						if (!saveResult.ok || !saveResult.rhaiSource) {
							return { ok: false, errors: saveResult.errors }
						}
						engine.updateEntityVisualGraph(entityId, graph, saveResult.rhaiSource)
						closeModal()
						return { ok: true }
					},
					onCancel: closeModal,
				}),
			})
			return
		}
		case 'deleteAnimation':
		case 'deleteScript':
			openModal({
				title: t('Confirm deletion'),
				size: 'sm',
				body: createElement(ModalConfirmBody, {
					message: payload.message,
					confirmLabel: (payload.confirmLabel as string) ?? t('Yes, delete'),
					onConfirm: () => {
						const callback = payload.onConfirm as (() => void) | undefined
						callback?.()
						closeModal()
					},
				}),
			})
			return
		default:
			return
	}
}

export { buildPlayAnimationFrameCmd }
