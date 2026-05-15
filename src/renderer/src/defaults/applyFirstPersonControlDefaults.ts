import type { MutableRefObject } from 'react'
import type { SavedControlBindings } from '@shared-types'
import type { EntityMeta } from '../context/useContextEngine/types'
import { DEFAULT_FIRST_PERSON_CONTROL_BINDINGS } from './firstPersonControlBindings'

function hasKeyboardBindings(bindings?: SavedControlBindings): boolean {
	if (!bindings?.keyboard_mouse) return false
	return Object.keys(bindings.keyboard_mouse).length > 0
}

export function applyFirstPersonControlDefaultsIfEmpty(
	entityId: number,
	entityMetaRef: MutableRefObject<Record<number, EntityMeta>>,
	send: (cmd: object) => void,
): SavedControlBindings | null {
	const meta = entityMetaRef.current[entityId]
	if (hasKeyboardBindings(meta?.controlBindings)) {
		return null
	}

	const defaults = DEFAULT_FIRST_PERSON_CONTROL_BINDINGS
	const nextMeta: EntityMeta = {
		kind: 'character',
		path: meta?.path ?? '[Player]',
		name: meta?.name ?? 'Player',
		physicsEnabled: meta?.physicsEnabled ?? false,
		physicsType: meta?.physicsType ?? '',
		...meta,
		controlBindings: defaults,
	}
	entityMetaRef.current[entityId] = nextMeta
	send({ cmd: 'set_control_bindings', id: entityId, bindings: defaults })
	return defaults
}
