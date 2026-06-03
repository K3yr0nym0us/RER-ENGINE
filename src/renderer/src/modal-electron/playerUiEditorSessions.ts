import { createElement } from 'react'

import type { UiScreenScope } from '@engine'
import type { EngineContextValue } from '@engine'
import { ModalConfirmBody } from './ModalConfirmBody'
import type { OpenModalElectronOptions } from '../hooks/useModalElectron'
import ModalSelectFont from '../pages/EngineView/components/sidebar/UIAccordion/components/ModalSelectFont'
import ModalSelectHudImage from '../pages/EngineView/components/sidebar/UIAccordion/components/ModalSelectHudImage'
import type { PlayerUiEditorAction, PlayerUiEditorState } from './playerUiEditorTypes'
import { setPendingPlayerUiEditorSession } from './playerUiEditorBridge'
import { PlayerUiEditorModalBody } from './PlayerUiEditorModalBody'

/** Handler activo del editor Player UI (renderer principal). */
export const activePlayerUiHandlerRef = { current: null as string | null }

export interface PlayerUiEditorSessionDeps {
	scope: UiScreenScope
	screenId: string
	engine: EngineContextValue
	openModal: (opts: OpenModalElectronOptions) => void
	closeModal: () => void
	objectDrawStart: () => void
	objectDrawCancel: () => void
	objectDrawActive: () => boolean
	pushPatch: (handlerId: string, state: PlayerUiEditorState) => void
	onEnd: () => void
	t: (key: string) => string
}

interface Session {
	handlerId: string
	deps: PlayerUiEditorSessionDeps
}

const sessions = new Map<string, Session>()

export function registerPlayerUiEditorSession(handlerId: string, deps: PlayerUiEditorSessionDeps): void {
	sessions.set(handlerId, { handlerId, deps })
}

export function unregisterPlayerUiEditorSession(handlerId: string): void {
	sessions.delete(handlerId)
}

export function buildPlayerUiEditorState(deps: PlayerUiEditorSessionDeps): PlayerUiEditorState {
	const screens = deps.scope === 'player' ? deps.engine.playerUiScreens : deps.engine.menuUiScreens
	const screen = screens.find((s) => s.id === deps.screenId)
	return {
		screenId: deps.screenId,
		screenName: screen?.name ?? '',
		elements: deps.engine.editingUiElements,
		engineReady: deps.engine.engineReady,
		objectDrawActive: deps.objectDrawActive(),
	}
}

export function pushPlayerUiEditorPatch(handlerId: string): void {
	const session = sessions.get(handlerId)
	if (!session) return
	session.deps.pushPatch(handlerId, buildPlayerUiEditorState(session.deps))
}

/** La ventana modal es singleton: tras una submodal hay que reabrir el editor. */
function reopenPlayerUiEditorModal(oldHandlerId: string): void {
	const session = sessions.get(oldHandlerId)
	if (!session) return
	const { deps } = session
	unregisterPlayerUiEditorSession(oldHandlerId)

	const screen = deps.engine.playerUiScreens.find((s) => s.id === deps.screenId)
	setPendingPlayerUiEditorSession((handlerId) => {
		activePlayerUiHandlerRef.current = handlerId
		return { ...deps, pushPatch: deps.pushPatch }
	})

	void deps.openModal({
		title: `${deps.t('Edit UI')}: ${screen?.name ?? ''}`,
		size: 'sm',
		body: createElement(PlayerUiEditorModalBody, { scope: 'player', screenId: deps.screenId }),
	})
}

export async function runPlayerUiEditorAction(
	handlerId: string,
	req: PlayerUiEditorAction,
): Promise<void> {
	const session = sessions.get(handlerId)
	if (!session) return
	const { deps } = session
	const { engine, scope, screenId } = deps

	switch (req.action) {
		case 'rename':
			engine.renameUiScreen(scope, screenId, req.name)
			pushPlayerUiEditorPatch(handlerId)
			break
		case 'setElementProps':
			engine.setPlayerUiHudElementProps(req.kind, req.id, req.props)
			pushPlayerUiEditorPatch(handlerId)
			break
		case 'addText':
			deps.openModal({
				title: deps.t('Add text'),
				size: 'sm',
				body: createElement(ModalSelectFont, {
					onSelect: (fontPath: string) => {
						engine.addPlayerUiTextBox(fontPath)
						reopenPlayerUiEditorModal(handlerId)
					},
				}),
			})
			break
		case 'addImage':
			deps.openModal({
				title: deps.t('Add image'),
				body: createElement(ModalSelectHudImage, {
					onSelect: (imagePath: string) => {
						engine.addPlayerUiImage(imagePath)
						reopenPlayerUiEditorModal(handlerId)
					},
				}),
			})
			break
		case 'objectDrawStart':
			deps.objectDrawStart()
			pushPlayerUiEditorPatch(handlerId)
			break
		case 'objectDrawCancel':
			deps.objectDrawCancel()
			pushPlayerUiEditorPatch(handlerId)
			break
		case 'removeText':
			deps.openModal({
				title: deps.t('Confirm deletion'),
				size: 'sm',
				body: createElement(ModalConfirmBody, {
					buttonSize: 'sm',
					messageSpec: {
						lines: [
							{
								parts: [
									{ type: 'text', value: `${deps.t('Are you sure you want to delete this text box')}? ` },
									{ type: 'bold', value: req.label },
								],
							},
						],
					},
					onConfirm: () => {
						engine.removePlayerUiTextBox(req.id)
						reopenPlayerUiEditorModal(handlerId)
					},
				}),
			})
			break
		case 'removeImage':
			deps.openModal({
				title: deps.t('Confirm deletion'),
				size: 'sm',
				body: createElement(ModalConfirmBody, {
					buttonSize: 'sm',
					messageSpec: {
						lines: [
							{
								parts: [
									{ type: 'text', value: `${deps.t('Are you sure you want to delete this element')}? ` },
									{ type: 'bold', value: req.label },
								],
							},
						],
					},
					onConfirm: () => {
						engine.removePlayerUiImage(req.id)
						reopenPlayerUiEditorModal(handlerId)
					},
				}),
			})
			break
		case 'removeObject':
			deps.openModal({
				title: deps.t('Confirm deletion'),
				size: 'sm',
				body: createElement(ModalConfirmBody, {
					buttonSize: 'sm',
					messageSpec: {
						lines: [
							{
								parts: [
									{ type: 'text', value: `${deps.t('Are you sure you want to delete this element')}? ` },
									{ type: 'bold', value: req.label },
								],
							},
						],
					},
					onConfirm: () => {
						engine.removePlayerUiObject(req.id)
						reopenPlayerUiEditorModal(handlerId)
					},
				}),
			})
			break
		case 'save':
		case 'cancel':
			deps.objectDrawCancel()
			engine.endUiScreenEdit()
			deps.onEnd()
			unregisterPlayerUiEditorSession(handlerId)
			deps.closeModal()
			break
		default:
			break
	}
}
