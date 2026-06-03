import type { ModalElectronOpenRequest } from '@shared-types'

export const MODAL_CALLBACK_KEYS = [
	'onSelect',
	'onConfirm',
	'onSave',
	'onCreate',
	'onRename',
	'onDone',
	'onCreateEntity',
	'onSpawn',
	'onCancel',
	'onOpenScriptEditor',
] as const

export type ModalCallbackKey = (typeof MODAL_CALLBACK_KEYS)[number]

type ResultHandler = (result: unknown) => void
type CallbackHandlerMap = Map<ModalCallbackKey, ResultHandler>

const pendingCallbackHandlers = new Map<string, CallbackHandlerMap>()

function invokeCallback(fn: (value: unknown) => void, result: unknown): void {
	if (result !== undefined && result !== null) {
		fn(result)
	} else {
		;(fn as () => void)()
	}
}

export function collectModalCallbackKeys(props: Record<string, unknown>): ModalCallbackKey[] {
	const keys: ModalCallbackKey[] = []
	for (const key of MODAL_CALLBACK_KEYS) {
		if (typeof props[key] === 'function') keys.push(key)
	}
	return keys
}

export function registerModalCallbacksFromProps(
	handlerId: string,
	props: Record<string, unknown>,
): ModalCallbackKey[] {
	const keys = collectModalCallbackKeys(props)
	if (keys.length === 0) {
		const existing = pendingCallbackHandlers.get(handlerId)
		return existing ? [...existing.keys()] : []
	}

	const map = pendingCallbackHandlers.get(handlerId) ?? new Map()
	for (const key of keys) {
		const fn = props[key] as (value: unknown) => void
		map.set(key, (result: unknown) => invokeCallback(fn, result))
	}
	pendingCallbackHandlers.set(handlerId, map)
	return [...map.keys()]
}

export function dispatchModalElectronResult(
	handlerId: string,
	result: unknown,
	callbackKey?: string,
): void {
	const map = pendingCallbackHandlers.get(handlerId)
	if (!map) return

	if (callbackKey) {
		map.get(callbackKey as ModalCallbackKey)?.(result)
	} else if (map.size === 1) {
		map.values().next().value?.(result)
	} else {
		const preferred = ['onSelect', 'onSave', 'onConfirm', 'onCreateEntity', 'onSpawn', 'onCreate', 'onRename']
		for (const key of preferred) {
			const handler = map.get(key as ModalCallbackKey)
			if (handler) {
				handler(result)
				break
			}
		}
	}

	pendingCallbackHandlers.delete(handlerId)
}

export function clearModalCallbackHandlers(handlerId: string): void {
	pendingCallbackHandlers.delete(handlerId)
}

export function wireModalCallbacksForHost(
	payload: ModalElectronOpenRequest,
	onClose: () => void,
): Record<string, unknown> {
	const hostProps: Record<string, unknown> = {
		...payload.props,
		onClose,
		handlerId: payload.handlerId,
		parentHandlerId: payload.handlerId,
		fonts: payload.fonts,
		hudImages: payload.hudImages,
		sprites: payload.sprites,
		models: payload.models,
		blueprints: payload.blueprints,
		linkedEntityCounts: payload.linkedEntityCounts,
	}

	const complete = (callbackKey: ModalCallbackKey, result?: unknown) => {
		window.electronAPI.completeModalElectron(payload.handlerId, result, callbackKey)
	}

	for (const key of payload.callbackKeys ?? []) {
		switch (key) {
			case 'onSelect':
			case 'onSpawn':
			case 'onCreate':
			case 'onRename':
			case 'onCreateEntity':
				hostProps[key] = (value: unknown) => complete(key, value)
				break
			case 'onSave':
			case 'onDone':
				hostProps[key] = (value: unknown) => complete(key, value)
				break
			case 'onConfirm':
				hostProps[key] = (value: unknown) => complete(key, value)
				break
			case 'onCancel':
				hostProps[key] = () => complete(key)
				break
			case 'onOpenScriptEditor':
				hostProps[key] = (value: unknown) => complete(key, value)
				break
			default:
				break
		}
	}

	if (!payload.callbackKeys?.includes('onCancel')) {
		hostProps.onCancel = onClose
	}

	return hostProps
}
