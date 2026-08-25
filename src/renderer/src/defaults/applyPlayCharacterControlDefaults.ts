import type { MutableRefObject } from 'react'
import type { EngineCommand2D, EngineCommand3D, SavedControlBindings } from '@shared-types'
import type { EntityMeta } from '../context/useContextEngine/types'
import { DEFAULT_PLAY_CHARACTER_CONTROL_BINDINGS } from './playCharacterControlBindings'

function hasKeyboardBindings(bindings?: SavedControlBindings): boolean {
	if (!bindings?.keyboard_mouse) return false
	return Object.keys(bindings.keyboard_mouse).length > 0
}

export function applyPlayCharacterControlDefaultsIfEmpty(
	entityId: number,
	entityMetaRef: MutableRefObject<Record<number, EntityMeta>>,
	send: (cmd: EngineCommand2D | EngineCommand3D) => void,
): SavedControlBindings | null {
	const meta = entityMetaRef.current[entityId]
	if (hasKeyboardBindings(meta?.controlBindings)) {
		return null
	}

	const defaults = DEFAULT_PLAY_CHARACTER_CONTROL_BINDINGS
	const nextMeta: EntityMeta = {
		...meta,
		kind: meta?.kind ?? 'character',
		path: meta?.path ?? '[Player]',
		name: meta?.name ?? 'Player',
		physicsEnabled: meta?.physicsEnabled ?? false,
		physicsType: meta?.physicsType ?? '',
		controlBindings: defaults,
	}
	entityMetaRef.current[entityId] = nextMeta
	send({ cmd: 'set_control_bindings', id: entityId, bindings: defaults })
	return defaults
}
