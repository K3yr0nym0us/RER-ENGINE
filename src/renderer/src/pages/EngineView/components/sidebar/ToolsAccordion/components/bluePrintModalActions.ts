import type { MutableRefObject } from 'react'

import type { Blueprint3D, BluePrintEntry } from '@shared-types'
import type { EntityMeta, Transform } from '@engine'

export interface BluePrintModalActionDeps {
	blueprints: BluePrintEntry[]
	setBlueprints: (next: BluePrintEntry[]) => void
	activeBluePrint: BluePrintEntry | null
	setActiveBluePrint: (bp: BluePrintEntry | null) => void
	entityMetaRef: MutableRefObject<Record<number, EntityMeta>>
	entityTransformsRef: MutableRefObject<Record<number, Transform>>
	removeScenario: (id: number) => void
	removeCharacter: (id: number) => void
	removeEntity: (id: number) => void
	removeCollider: (id: number) => void
	removeExecutionArea: (id: number) => void
}

function getLinkedEntityIds(
	entityMetaRef: MutableRefObject<Record<number, EntityMeta>>,
	bpId: string,
): number[] {
	return Object.entries(entityMetaRef.current)
		.filter(([, meta]) => meta.blueprintId === bpId)
		.map(([id]) => Number(id))
}

export function deleteBlueprintWithEntities(
	pendingDelete: BluePrintEntry,
	deps: BluePrintModalActionDeps,
): BluePrintEntry[] {
	const ids = getLinkedEntityIds(deps.entityMetaRef, pendingDelete.id)
	ids.forEach((id) => {
		const kind = deps.entityMetaRef.current[id]?.kind
		if (kind === 'scenario') deps.removeScenario(id)
		else if (kind === 'character') deps.removeCharacter(id)
		else if (kind === 'collider') deps.removeCollider(id)
		else if (kind === 'execution_area') deps.removeExecutionArea(id)
		else if (kind === 'model' || kind === 'directional_light') deps.removeEntity(id)
	})
	const next = deps.blueprints.filter((bp) => bp.id !== pendingDelete.id)
	deps.setBlueprints(next)
	if (deps.activeBluePrint?.id === pendingDelete.id) {
		deps.setActiveBluePrint(null)
	}
	return next
}

export function deleteBlueprintKeepEntities(
	pendingDelete: BluePrintEntry,
	deps: BluePrintModalActionDeps,
): BluePrintEntry[] {
	const ids = getLinkedEntityIds(deps.entityMetaRef, pendingDelete.id)
	ids.forEach((id) => {
		const meta = deps.entityMetaRef.current[id]
		if (!meta) return
		meta.physicsEnabled = pendingDelete.physics_enabled ?? meta.physicsEnabled
		meta.physicsType = pendingDelete.physics_type ?? meta.physicsType
		meta.animations = pendingDelete.animations ?? meta.animations
		meta.scripts = pendingDelete.scripts ?? meta.scripts
		meta.controlBindings = pendingDelete.control_bindings ?? meta.controlBindings
		const tr = deps.entityTransformsRef.current[id]
		if (tr) {
			tr.scale = [...pendingDelete.scale] as [number, number, number]
			if (pendingDelete.rotation) {
				tr.rotation = [...pendingDelete.rotation] as [number, number, number, number]
			}
		}
		delete meta.blueprintId
	})
	const next = deps.blueprints.filter((bp) => bp.id !== pendingDelete.id)
	deps.setBlueprints(next)
	if (deps.activeBluePrint?.id === pendingDelete.id) {
		deps.setActiveBluePrint(null)
	}
	return next
}

export type BluePrintModalDelegateAction =
	| { action: 'deleteWithEntities'; blueprint: BluePrintEntry }
	| { action: 'deleteKeepEntities'; blueprint: BluePrintEntry }

export function runBluePrintModalDelegate(
	data: BluePrintModalDelegateAction,
	deps: BluePrintModalActionDeps,
): { blueprints: Blueprint3D[] } {
	switch (data.action) {
		case 'deleteWithEntities':
			return { blueprints: deleteBlueprintWithEntities(data.blueprint, deps) }
		case 'deleteKeepEntities':
			return { blueprints: deleteBlueprintKeepEntities(data.blueprint, deps) }
		default:
			return { blueprints: deps.blueprints }
	}
}
