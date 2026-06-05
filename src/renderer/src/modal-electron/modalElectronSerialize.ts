import { isValidElement } from 'react'

import type { Entity3D } from '@shared-types'

import { sanitizeSceneEntitiesForModal } from '../visualScripting/resolveSceneEntities'
import { extractModalConfirmMessageSpec } from './modalConfirmMessageSpec'

const REACT_ELEMENT_TYPE = Symbol.for('react.element')

function isReactElement(value: unknown): boolean {
	if (typeof value !== 'object' || value === null) return false
	const record = value as Record<string | symbol, unknown>
	return record.$$typeof === REACT_ELEMENT_TYPE
}

function isCloneableValue(value: unknown): boolean {
	if (value === null) return true
	const valueType = typeof value
	if (valueType === 'string' || valueType === 'number' || valueType === 'boolean') return true
	if (valueType === 'function' || valueType === 'symbol' || valueType === 'undefined') return false
	if (isReactElement(value)) return false
	if (value instanceof Date) return true
	if (Array.isArray(value)) return value.every(isCloneableValue)
	if (valueType === 'object') {
		if (typeof Node !== 'undefined' && value instanceof Node) return false
		return Object.values(value as Record<string, unknown>).every(isCloneableValue)
	}
	return false
}

function clonePlainValue(value: unknown): unknown {
	if (Array.isArray(value)) {
		return value.map(clonePlainValue)
	}
	if (value !== null && typeof value === 'object') {
		const out: Record<string, unknown> = {}
		for (const [key, entry] of Object.entries(value as Record<string, unknown>)) {
			if (isCloneableValue(entry)) {
				out[key] = clonePlainValue(entry)
			}
		}
		return out
	}
	return value
}

export function serializeModalProps(props: Record<string, unknown>): Record<string, unknown> {
	const out: Record<string, unknown> = {}
	for (const [key, value] of Object.entries(props)) {
		if (!isCloneableValue(value)) continue
		out[key] = clonePlainValue(value)
	}
	return out
}

/** Props listas para `ipcRenderer` según el componente modal. */
export function prepareModalElectronProps(
	componentKey: string,
	props: Record<string, unknown>,
): Record<string, unknown> {
	if (componentKey === 'ModalConfirmBody') {
		const { message, messageSpec: existingSpec, ...rest } = props
		const spec =
			existingSpec != null
				? existingSpec
				: message != null
					? extractModalConfirmMessageSpec(message)
					: undefined
		return {
			...serializeModalProps(rest),
			...(spec ? { messageSpec: clonePlainValue(spec) } : {}),
		}
	}

	if (componentKey === 'VisualScriptingModalBody') {
		const { sceneEntities, ...rest } = props
		const sanitized = Array.isArray(sceneEntities)
			? sanitizeSceneEntitiesForModal(sceneEntities as Entity3D[])
			: []
		return {
			...serializeModalProps(rest),
			sceneEntities: sanitized,
		}
	}

	return serializeModalProps(props)
}
