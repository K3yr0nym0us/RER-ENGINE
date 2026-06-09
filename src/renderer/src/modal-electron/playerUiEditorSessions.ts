import { createElement } from 'react'

import type { UiScreenScope, EngineContextValue } from '../context/useContextEngine/types'
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
	getEngine: () => EngineContextValue
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
	const engine = deps.getEngine()
	const screens = deps.scope === 'player' ? engine.playerUiScreens : engine.menuUiScreens
	const screen = screens.find((s) => s.id === deps.screenId)
	return {
		screenId: deps.screenId,
		screenName: screen?.name ?? '',
		elements: engine.editingUiElements,
		engineReady: engine.engineReady,
		objectDrawActive: deps.objectDrawActive(),
	}
}

export function pushPlayerUiEditorPatch(handlerId: string): void {
	const session = sessions.get(handlerId)
	if (!session) return
	session.deps.pushPatch(handlerId, buildPlayerUiEditorState(session.deps))
}

export function resolvePlayerUiEditorState(handlerId: string): PlayerUiEditorState | null {
	const session = sessions.get(handlerId)
	if (!session) return null
	return buildPlayerUiEditorState(session.deps)
}

/** La ventana modal es singleton: tras una submodal hay que reabrir el editor. */
function reopenPlayerUiEditorModal(oldHandlerId: string): void {
	const session = sessions.get(oldHandlerId)
	if (!session) return
	const { deps } = session
	unregisterPlayerUiEditorSession(oldHandlerId)

	const screen = deps.getEngine().playerUiScreens.find((s) => s.id === deps.screenId)
	setPendingPlayerUiEditorSession((handlerId) => {
		activePlayerUiHandlerRef.current = handlerId
		return { ...deps, pushPatch: deps.pushPatch }
	})

	void deps.openModal({
		title: `${deps.t('Edit HUD')}: ${screen?.name ?? ''}`,
		size: 'sm',
		body: createElement(PlayerUiEditorModalBody, { scope: 'player', screenId: deps.screenId }),
	})
}

/** Cierra la sesión del editor (save/cancel/X). `skipClose` si la ventana modal ya se cerró. */
export function finishPlayerUiEditorSession(handlerId: string, skipClose = false): void {
	const session = sessions.get(handlerId)
	if (!session) return
	const { deps } = session
	deps.objectDrawCancel()
	deps.getEngine().endUiScreenEdit()
	deps.onEnd()
	unregisterPlayerUiEditorSession(handlerId)
	if (!skipClose) {
		deps.closeModal()
	}
}

export async function runPlayerUiEditorAction(
	handlerId: string,
	req: PlayerUiEditorAction,
): Promise<void> {
	const session = sessions.get(handlerId)
	if (!session) return
	const { deps } = session
	const { getEngine, scope, screenId } = deps
	const engine = getEngine()

	switch (req.action) {
		case 'rename':
			engine.renameUiScreen(scope, screenId, req.name)
			pushPlayerUiEditorPatch(handlerId)
			break
		case 'setElementProps':
			engine.setPlayerUiHudElementProps(req.kind, req.id, req.props)
			pushPlayerUiEditorPatch(handlerId)
			break
		case 'setObjectStyle':
			engine.setPlayerUiObjectStyle(req.id, {
				fill_color: req.fill_color,
				live: req.live,
				skip_undo: req.skip_undo,
			});
			if (!req.live) {
				pushPlayerUiEditorPatch(handlerId);
			}
			break
		case 'assignObjectTexture':
			deps.openModal({
				title: deps.t('Assign texture'),
				body: createElement(ModalSelectHudImage, {
					hudImages: engine.hudImages ?? [],
					onSelect: (imagePath: string) => {
						engine.setPlayerUiObjectStyle(req.id, { texture_path: imagePath });
						reopenPlayerUiEditorModal(handlerId);
					},
				}),
			});
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
			finishPlayerUiEditorSession(handlerId)
			break
		default:
			break
	}
}
