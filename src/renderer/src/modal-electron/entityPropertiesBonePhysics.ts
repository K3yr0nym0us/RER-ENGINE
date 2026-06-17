import type { EntityBonePhysics3D } from '@shared-types'
import type { EngineContextValue } from '@engine'
import { entityCanHaveBonePhysics, entityCanHaveSockets, invalidateEntityBoneNames } from '../utils/entity3dEditorSync'
import type { EntityPropertiesBonePhysicsUi } from './entityPropertiesTypes'

export type BonePhysicsMode = EntityBonePhysics3D['mode']

export interface EntityPropertiesBonePhysicsSession {
	boneEntityId: number | null
	bonePickActive: boolean
	selectedBoneName: string | null
	draftMode: BonePhysicsMode
	bonesTabActive: boolean
}

export function createBonePhysicsSession(): EntityPropertiesBonePhysicsSession {
	return {
		boneEntityId: null,
		bonePickActive: false,
		selectedBoneName: null,
		draftMode: 'dynamic',
		bonesTabActive: false,
	}
}

export function entityPropertiesCanHaveBonePhysics(
	engine: EngineContextValue,
	entityId: number | null | undefined,
): boolean {
	if (entityId == null) return false
	const meta = engine.entityMetaRef.current[entityId]
	return entityCanHaveBonePhysics(entityId, meta, engine.editorCameraEntityIdRef.current)
}

export function requestEntityBoneNamesIfNeeded(
	engine: EngineContextValue,
	entityId: number | null | undefined,
): void {
	if (entityId == null) return
	const meta = engine.entityMetaRef.current[entityId]
	if (!entityCanHaveSockets(entityId, meta, engine.editorCameraEntityIdRef.current)) return
	if (meta?.boneNames !== undefined) return
	engine.send({ cmd: 'list_entity_bones', entity_id: entityId })
}

export function invalidateEntityBoneNamesForEntity(
	engine: EngineContextValue,
	entityId: number | null | undefined,
): void {
	if (entityId == null) return
	invalidateEntityBoneNames(engine.entityMetaRef.current[entityId])
}

export function buildEntityPropertiesBonePhysicsUi(
	engine: EngineContextValue,
	entityId: number | null,
	session: EntityPropertiesBonePhysicsSession,
): EntityPropertiesBonePhysicsUi | null {
	if (entityId == null || !entityPropertiesCanHaveBonePhysics(engine, entityId)) {
		return null
	}
	return {
		entries: engine.entityMetaRef.current[entityId]?.bonePhysics ?? [],
		selectedBoneName: session.selectedBoneName,
		draftMode: session.draftMode,
		bonePickActive: session.bonePickActive,
	}
}

function syncEditorSkeleton(engine: EngineContextValue, entityId: number, active: boolean): void {
	engine.send({
		cmd: 'set_bone_physics_editor_entity',
		entity_id: entityId,
		active,
	})
}

export function disableEntityBonePick(
	engine: EngineContextValue,
	entityId: number | null,
	session: EntityPropertiesBonePhysicsSession,
): void {
	if (!session.bonePickActive || entityId == null) return
	session.bonePickActive = false
	engine.send({
		cmd: 'set_bone_physics_pick_mode',
		entity_id: entityId,
		active: false,
	})
}

export function syncEntityPropertiesBonePhysicsEditor(
	engine: EngineContextValue,
	entityId: number | null,
	session: EntityPropertiesBonePhysicsSession,
): void {
	if (entityId != null && session.bonesTabActive && entityPropertiesCanHaveBonePhysics(engine, entityId)) {
		syncEditorSkeleton(engine, entityId, true)
		engine.send({ cmd: 'list_entity_bone_physics', entity_id: entityId })
		return
	}
	if (entityId != null) {
		disableEntityBonePick(engine, entityId, session)
		syncEditorSkeleton(engine, entityId, false)
	}
}

export function resetBonePhysicsSessionForEntity(
	session: EntityPropertiesBonePhysicsSession,
	entityId: number | null,
): void {
	if (session.boneEntityId === entityId) return
	session.boneEntityId = entityId
	session.bonePickActive = false
	session.selectedBoneName = null
	session.draftMode = 'dynamic'
}

export function teardownEntityPropertiesBonePhysics(
	engine: EngineContextValue,
	session: EntityPropertiesBonePhysicsSession,
): void {
	disableEntityBonePick(engine, session.boneEntityId, session)
	if (session.boneEntityId != null) {
		syncEditorSkeleton(engine, session.boneEntityId, false)
	}
	session.bonesTabActive = false
	session.boneEntityId = null
	session.selectedBoneName = null
	session.bonePickActive = false
}

export function onEntityPropertiesBonePicked(
	engine: EngineContextValue,
	session: EntityPropertiesBonePhysicsSession,
	entityId: number,
	boneName: string,
): void {
	if (session.boneEntityId !== entityId) return
	session.selectedBoneName = boneName
	const found = engine.entityMetaRef.current[entityId]?.bonePhysics?.find(
		(e) => e.bone_name === boneName,
	)
	session.draftMode = found?.mode ?? 'dynamic'
	session.bonePickActive = false
	engine.send({
		cmd: 'set_bone_physics_pick_mode',
		entity_id: entityId,
		active: false,
	})
}
