import { createElement, isValidElement, useCallback, useEffect, useRef, type ReactNode } from 'react'

import { SpritePreviewModalBody } from '@components'
import type { BluePrintEntry, ModalElectronOpenRequest, ModalElectronSize } from '@shared-types'
import { useContextEngine } from '@engine'
import { useLanguage } from '../context/LanguageContext'
import { useQuickBuild } from '../context/QuickBuildContext'
import { buildEngineSnapshot } from '../modal-electron/buildEngineSnapshot'
import { getComponentKey, prepareModalElectronProps } from '../modal-electron/getComponentKey'
import { serializeModalProps } from '../modal-electron/modalElectronSerialize'
import { setModalElectronParentBridge } from '../modal-electron/modalElectronParentBridge'
import {
	dispatchModalElectronResult,
	registerModalCallbacksFromProps,
} from '../modal-electron/registerModalCallbacks'
import {
	runBluePrintModalDelegate,
	type BluePrintModalActionDeps,
	type BluePrintModalDelegateAction,
} from '../pages/EngineView/components/sidebar/ToolsAccordion/components/bluePrintModalActions'
import { consumePendingPlayerUiEditorSession } from '../modal-electron/playerUiEditorBridge'
import {
	buildPlayerUiEditorState,
	pushPlayerUiEditorPatch,
	registerPlayerUiEditorSession,
	resolvePlayerUiEditorState,
	runPlayerUiEditorAction,
} from '../modal-electron/playerUiEditorSessions'
import type { PlayerUiEditorState } from '../modal-electron/playerUiEditorTypes'
import {
	buildEntityPropertiesState,
	pushEntityPropertiesPatch,
	registerEntityPropertiesSession,
	requestEntityAnimationPlayStateSync,
	runEntityPropertiesAction,
	unregisterEntityPropertiesSession,
} from '../modal-electron/entityPropertiesModalSessions'
import {
	buildSocketConfigModalState,
	pushSocketConfigModalPatch,
	registerSocketConfigModalSession,
	runSocketConfigModalAction,
	unregisterSocketConfigModalSession,
	activeSocketConfigModalHandlerRef,
} from '../modal-electron/socketConfigModalSessions'
import { useTraslate } from './useTraslate'

/**
 * Abre ventanas modales Electron (ventana hija; el motor sigue visible).
 *
 * IMPORTANTE — al añadir un modal nuevo:
 *   1. Registrar el componente en modal-electron/modalElectronRegistry.tsx
 *   2. Seguir docs/MODAL_ELECTRON.yaml
 *
 * Si falta el registro aparece: "Componente modal no soportado: NombreComponente"
 */

export interface OpenModalElectronOptions {
	title: string
	body: ReactNode
	size?: ModalElectronSize
}

export type { ModalElectronSize }

type DelegateHandler = (
	data: BluePrintModalDelegateAction,
) => Promise<{ blueprints?: BluePrintEntry[] } | null>

type ParentNestedHandler = (payload: Record<string, unknown>) => void

const pendingDelegates = new Map<string, DelegateHandler>()
const pendingParentNested = new Map<string, Record<string, ParentNestedHandler>>()

let delegateListenerInstalled = false
let parentOpenListenerInstalled = false
let playerUiActionListenerInstalled = false
let playerUiStateListenerInstalled = false
let entityPropertiesActionListenerInstalled = false
let socketConfigModalActionListenerInstalled = false

/** Modal Electron visible: para parches en vivo (p. ej. modelos que terminan de precargar). */
const activeModalRef: { current: { handlerId: string; componentKey: string } | null } = {
	current: null,
}

const MODEL_PICKER_MODAL_KEY = 'CreateEntityFromModelModalBody'

function patchActiveModelPickerModal(models: import('@shared-types').ModelInfo[]): void {
	const active = activeModalRef.current
	if (!active || active.componentKey !== MODEL_PICKER_MODAL_KEY) return
	window.electronAPI.patchModalElectron({
		handlerId: active.handlerId,
		models: serializeModalProps({ models }).models as import('@shared-types').ModelInfo[],
	})
}

function createHandlerId(): string {
	return crypto.randomUUID()
}

export function useModalElectron() {
	const engine = useContextEngine()
	const { locale } = useLanguage()
	const { activeBluePrint, setActiveBluePrint } = useQuickBuild()
	const { t } = useTraslate()
	const tRef = useRef(t)
	tRef.current = t

	const engineRef = useRef(engine)
	engineRef.current = engine

	const bluePrintDepsRef = useRef<BluePrintModalActionDeps | null>(null)
	bluePrintDepsRef.current = {
		blueprints: engine.blueprints,
		setBlueprints: engine.setBlueprints,
		activeBluePrint,
		setActiveBluePrint,
		entityMetaRef: engine.entityMetaRef,
		entityTransformsRef: engine.entityTransformsRef,
		removeScenario: engine.removeScenario,
		removeCharacter: engine.removeCharacter,
		removeEntity: engine.removeEntity,
		removeCollider: engine.removeCollider,
		removeExecutionArea: engine.removeExecutionArea,
	}

	useEffect(() => {
		const remove = window.electronAPI.onModalElectronResult((handlerId, result, callbackKey) => {
			dispatchModalElectronResult(handlerId, result, callbackKey)
			pendingDelegates.delete(handlerId)
			pendingParentNested.delete(handlerId)
		})
		return remove
	}, [])

	useEffect(() => {
		if (delegateListenerInstalled) return
		delegateListenerInstalled = true
		window.electronAPI.onModalElectronDelegateRequest(async (req) => {
			const delegate = pendingDelegates.get(req.handlerId)
			if (!delegate) return null
			const { requestId: _rid, handlerId: _hid, ...action } = req
			return delegate(action as BluePrintModalDelegateAction)
		})
	}, [])

	const closeModal = useCallback(async () => {
		activeModalRef.current = null
		await window.electronAPI.closeModalElectron()
	}, [])

	const openModal = useCallback(async (opts: OpenModalElectronOptions) => {
		const { title, body, size } = opts

		if (!isValidElement(body)) {
			console.error('[useModalElectron] body debe ser un elemento React')
			return
		}

		const componentKey = getComponentKey(body.type)
		const handlerId = createHandlerId()
		const props = body.props as Record<string, unknown>

		let callbackKeys = registerModalCallbacksFromProps(handlerId, props)

		if (componentKey === 'BluePrintModalBody') {
			callbackKeys = registerModalCallbacksFromProps(handlerId, {
				onSelect: (result: unknown) => {
					if (result && typeof result === 'object' && 'id' in result) {
						setActiveBluePrint(result as BluePrintEntry)
					}
				},
			})
		}

		let entityPropertiesState: ReturnType<typeof buildEntityPropertiesState> | undefined
		if (componentKey === 'EntityPropertiesModalBody') {
			registerEntityPropertiesSession(handlerId, {
				getEngine: () => engineRef.current,
				openModal,
				closeModal,
				pushPatch: (hid, state) => {
					window.electronAPI.patchModalElectron({ handlerId: hid, entityPropertiesState: state })
				},
				onCloseModal: () => {
					engineRef.current.send({ cmd: 'deselect_entity' })
					unregisterEntityPropertiesSession(handlerId)
				},
				t: (key) => tRef.current(key),
			})
			entityPropertiesState = buildEntityPropertiesState(engineRef.current)
			const entityId = engineRef.current.selectedEntity?.id
			if (entityId != null) {
				requestEntityAnimationPlayStateSync(engineRef.current, entityId)
			}
		}

		let socketConfigModalState: ReturnType<typeof buildSocketConfigModalState> | undefined
		if (componentKey === 'SocketConfigModalBody') {
			socketConfigModalState = registerSocketConfigModalSession(handlerId, {
				getEngine: () => engineRef.current,
				closeModal,
				pushPatch: (hid, state) => {
					window.electronAPI.patchModalElectron({ handlerId: hid, socketConfigModalState: state })
				},
			})
		}

		let playerUiEditorState: PlayerUiEditorState | undefined
		if (componentKey === 'PlayerUiEditorModalBody') {
			const sessionDeps = consumePendingPlayerUiEditorSession(handlerId)
			if (sessionDeps) {
				registerPlayerUiEditorSession(handlerId, sessionDeps)
				sessionDeps.getEngine().beginUiScreenEdit('player', sessionDeps.screenId)
				playerUiEditorState = buildPlayerUiEditorState(sessionDeps)
			}
		}

		const preparedProps = prepareModalElectronProps(componentKey, props)
		const engineSnapshot = buildEngineSnapshot(componentKey, engineRef.current)
		const request: ModalElectronOpenRequest = {
			size,
			title,
			handlerId,
			componentKey,
			locale,
			resizable: componentKey === 'VisualScriptingModalBody',
			props: preparedProps,
			callbackKeys,
			...(entityPropertiesState ? { entityPropertiesState } : {}),
			...(socketConfigModalState ? { socketConfigModalState } : {}),
			...(playerUiEditorState ? { playerUiEditorState } : {}),
			...engineSnapshot,
			...(componentKey === 'VisualScriptingModalBody'
				? {
					sceneEntities: (
						Array.isArray(preparedProps.sceneEntities) && preparedProps.sceneEntities.length > 0
							? preparedProps.sceneEntities
							: engineSnapshot.sceneEntities
					) as ModalElectronOpenRequest['sceneEntities'],
				}
				: {}),
		}

		if (componentKey === 'BluePrintModalBody') {
			pendingDelegates.set(handlerId, async (data) => {
				const current = bluePrintDepsRef.current
				if (!current) return null
				return runBluePrintModalDelegate(data as BluePrintModalDelegateAction, current)
			})
		}

		if (componentKey === 'CreateEntityFromSpriteModalBody' && typeof props.onCreateEntity === 'function') {
			const onCreateEntity = props.onCreateEntity as (payload: unknown) => void
			pendingParentNested.set(handlerId, {
				openSpritePreview: (nested) => {
					const spritePath = String(nested.spritePath ?? '')
					const previewTitle = String(nested.previewTitle ?? 'Preview')
					void openModal({
						title: previewTitle,
						size: 'xl',
						body: createElement(SpritePreviewModalBody, {
							src: spritePath,
							onConfirm: (config) => {
								onCreateEntity({
									spritePath,
									animation: {
										name: config.animationName,
										frames: config.frames,
										fps: config.fps,
										loop: config.loop,
										facingRight: config.facingRight,
										audioPath: config.audioPath,
										scripts: config.scripts,
										isCancelable: config.isCancelable,
										defaultAnimation: config.defaultAnimation,
										selectionMode: config.selectionMode,
										gridSize: config.gridSize,
										cellOffsetX: config.cellOffsetX,
										cellOffsetY: config.cellOffsetY,
									},
								})
							},
							onCancel: closeModal,
						}),
					})
				},
			})
		}

		// IPC structured clone: solo datos planos (sin JSX ni funciones).
		const ipcPayload = JSON.parse(JSON.stringify(request)) as ModalElectronOpenRequest
		activeModalRef.current = { handlerId, componentKey }
		await window.electronAPI.openModalElectron(ipcPayload)
		if (componentKey === 'PlayerUiEditorModalBody') {
			pushPlayerUiEditorPatch(handlerId)
		}
		if (componentKey === 'EntityPropertiesModalBody') {
			pushEntityPropertiesPatch(handlerId)
		}
		if (componentKey === 'SocketConfigModalBody') {
			pushSocketConfigModalPatch(handlerId)
		}
		if (componentKey === MODEL_PICKER_MODAL_KEY) {
			patchActiveModelPickerModal(engineRef.current.models)
		}
	}, [closeModal, locale, setActiveBluePrint])

	useEffect(() => {
		patchActiveModelPickerModal(engine.models)
	}, [engine.models])

	useEffect(() => {
		setModalElectronParentBridge({ openModal })
		return () => setModalElectronParentBridge(null)
	}, [openModal])

	useEffect(() => {
		if (playerUiActionListenerInstalled) return
		playerUiActionListenerInstalled = true
		window.electronAPI.onModalElectronPlayerUiActionRequest(async (req) => {
			await runPlayerUiEditorAction(req.handlerId, req.action as import('../modal-electron/playerUiEditorTypes').PlayerUiEditorAction)
		})
	}, [])

	useEffect(() => {
		if (playerUiStateListenerInstalled) return
		playerUiStateListenerInstalled = true
		window.electronAPI.onModalElectronPlayerUiStateRequest((req) => {
			return resolvePlayerUiEditorState(req.handlerId)
		})
	}, [])

	useEffect(() => {
		if (entityPropertiesActionListenerInstalled) return
		entityPropertiesActionListenerInstalled = true
		window.electronAPI.onModalElectronEntityPropertiesActionRequest(async (req) => {
			await runEntityPropertiesAction(req.handlerId, req.action as import('../modal-electron/entityPropertiesTypes').EntityPropertiesAction)
		})
	}, [])

	useEffect(() => {
		if (socketConfigModalActionListenerInstalled) return
		socketConfigModalActionListenerInstalled = true
		window.electronAPI.onModalElectronSocketConfigModalActionRequest(async (req) => {
			await runSocketConfigModalAction(req.handlerId, req.action as import('../modal-electron/socketConfigModalTypes').SocketConfigModalAction)
		})
	}, [])

	useEffect(() => {
		if (parentOpenListenerInstalled) return
		parentOpenListenerInstalled = true
		window.electronAPI.onModalElectronParentOpenRequest((req) => {
			const nested = pendingParentNested.get(req.parentHandlerId)
			const action = nested?.[req.action]
			if (action) {
				action(req.payload ?? {})
			}
		})
	}, [])

	useEffect(() => {
		const remove = window.electronAPI.onModalElectronClosed((data) => {
			activeModalRef.current = null
			if (data.componentKey === 'SocketConfigModalBody' && activeSocketConfigModalHandlerRef.current) {
				unregisterSocketConfigModalSession(activeSocketConfigModalHandlerRef.current)
			}
		})
		return remove
	}, [])

	return { openModal, closeModal }
}
