import { createElement, useCallback, useEffect, useRef } from 'react'

import type { UiScreenScope } from '@engine'
import { useContextEngine } from '@engine'
import { useModal } from '@modal'
import { useTraslate } from '@hooks'
import { usePlayerUiObjectDrawing } from '@hooks'
import { PlayerUiEditorModalBody } from './PlayerUiEditorModalBody'
import { setPendingPlayerUiEditorSession } from './playerUiEditorBridge'
import {
	activePlayerUiHandlerRef,
	pushPlayerUiEditorPatch,
	type PlayerUiEditorSessionDeps,
} from './playerUiEditorSessions'

/** Abre el editor Player UI en modal Electron (solo scope `player`). */
export function usePlayerUiEditorModal(scope: UiScreenScope) {
	const { t } = useTraslate()
	const engine = useContextEngine()
	const engineRef = useRef(engine)
	engineRef.current = engine
	const { openModal } = useModal()
	const objectDraw = usePlayerUiObjectDrawing(engine.send, engine.playerUiObjectDrawEndTick)

	const pushPatch = useCallback(
		(handlerId: string, state: import('./playerUiEditorTypes').PlayerUiEditorState) => {
			window.electronAPI.patchModalElectron({ handlerId, playerUiEditorState: state })
		},
		[],
	)

	useEffect(() => {
		if (scope !== 'player') return
		const handlerId = activePlayerUiHandlerRef.current
		if (!handlerId) return
		pushPlayerUiEditorPatch(handlerId)
	}, [
		scope,
		engine.editingUiElements,
		engine.playerUiObjectDrawEndTick,
		engine.engineReady,
		engine.playerUiScreens,
		objectDraw.isActive,
	])

	const openEditor = useCallback(
		(screenId: string) => {
			if (scope !== 'player') return
			const screen = engineRef.current.playerUiScreens.find((s) => s.id === screenId)
			if (!screen) return

			setPendingPlayerUiEditorSession((handlerId) => {
				const deps: PlayerUiEditorSessionDeps = {
					scope: 'player',
					screenId,
					getEngine: () => engineRef.current,
					openModal,
					closeModal: () => void window.electronAPI.closeModalElectron(),
					objectDrawStart: objectDraw.start,
					objectDrawCancel: objectDraw.cancel,
					objectDrawActive: () => objectDraw.isActiveRef.current,
					pushPatch,
					onEnd: () => {
						activePlayerUiHandlerRef.current = null
					},
					t,
				}
				activePlayerUiHandlerRef.current = handlerId
				return deps
			})

			void openModal({
				title: `${t('Edit UI')}: ${screen.name}`,
				size: 'sm',
				body: createElement(PlayerUiEditorModalBody, { scope: 'player', screenId }),
			})
		},
		[scope, engine, openModal, objectDraw, pushPatch, t],
	)

	return { openEditor }
}
