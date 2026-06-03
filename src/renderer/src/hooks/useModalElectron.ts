import { createElement, isValidElement, useCallback, useEffect, useRef, type ReactNode } from 'react'

import { SpritePreviewModalBody } from '@components'
import type { BluePrintEntry, ModalElectronOpenRequest, ModalElectronSize } from '@shared-types'
import { useContextEngine } from '@engine'
import { useQuickBuild } from '../context/QuickBuildContext'
import { buildEngineSnapshot } from '../modal-electron/buildEngineSnapshot'
import { getComponentKey, prepareModalElectronProps } from '../modal-electron/getComponentKey'
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
	registerPlayerUiEditorSession,
	runPlayerUiEditorAction,
} from '../modal-electron/playerUiEditorSessions'
import type { PlayerUiEditorState } from '../modal-electron/playerUiEditorTypes'

export type { ModalElectronSize }

export interface OpenModalElectronOptions {
	title: string
	body: ReactNode
	size?: ModalElectronSize
}

type DelegateHandler = (data: BluePrintModalDelegateAction) => Promise<{ blueprints?: BluePrintEntry[] } | null>

type ParentNestedHandler = (payload: Record<string, unknown>) => void

const pendingDelegates = new Map<string, DelegateHandler>()
const pendingParentNested = new Map<string, Record<string, ParentNestedHandler>>()

let delegateListenerInstalled = false
let parentOpenListenerInstalled = false
let playerUiActionListenerInstalled = false

function createHandlerId(): string {
	return crypto.randomUUID()
}

export function useModalElectron() {
	const engine = useContextEngine()
	const { activeBluePrint, setActiveBluePrint } = useQuickBuild()

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

	const closeModal = useCallback(() => {
		void window.electronAPI.closeModalElectron()
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

		let playerUiEditorState: PlayerUiEditorState | undefined
		if (componentKey === 'PlayerUiEditorModalBody') {
			const sessionDeps = consumePendingPlayerUiEditorSession(handlerId)
			if (sessionDeps) {
				registerPlayerUiEditorSession(handlerId, sessionDeps)
				playerUiEditorState = buildPlayerUiEditorState(sessionDeps)
			}
		}

		const request: ModalElectronOpenRequest = {
			size,
			title,
			handlerId,
			componentKey,
			props: prepareModalElectronProps(componentKey, props),
			callbackKeys,
			...(playerUiEditorState ? { playerUiEditorState } : {}),
			...buildEngineSnapshot(componentKey, engineRef.current),
		}

		if (componentKey === 'BluePrintModalBody') {
			pendingDelegates.set(handlerId, async (data) => {
				const current = bluePrintDepsRef.current
				if (!current) return null
				return runBluePrintModalDelegate(data, current)
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
		await window.electronAPI.openModalElectron(ipcPayload)
	}, [closeModal, setActiveBluePrint])

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

	return { openModal, closeModal }
}
