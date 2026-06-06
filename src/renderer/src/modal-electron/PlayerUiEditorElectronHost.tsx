import { useCallback, useEffect, useState } from 'react'

import type { ModalElectronOpenRequest } from '@shared-types'
import { PlayerUiEditorPanel } from '../pages/EngineView/components/sidebar/UIAccordion/components/PlayerUiEditorPanel'
import type { PlayerUiEditorAction, PlayerUiEditorState } from './playerUiEditorTypes'

interface PlayerUiEditorElectronHostProps {
	payload: ModalElectronOpenRequest
}

export function PlayerUiEditorElectronHost({ payload }: PlayerUiEditorElectronHostProps) {
	const [state, setState] = useState<PlayerUiEditorState>(
		(payload.playerUiEditorState as PlayerUiEditorState | undefined) ?? {
			screenId: '',
			screenName: '',
			elements: [],
			engineReady: false,
			objectDrawActive: false,
		},
	)

	useEffect(() => {
		const initial = payload.playerUiEditorState as PlayerUiEditorState | undefined
		if (initial) setState(initial)
	}, [payload.handlerId, payload.playerUiEditorState])

	useEffect(() => {
		const remove = window.electronAPI.onModalElectronPatch((data) => {
			if (data.handlerId !== payload.handlerId) return
			if (data.playerUiEditorState) {
				setState(data.playerUiEditorState as PlayerUiEditorState)
			}
		})
		return remove
	}, [payload.handlerId])

	// Al montar, pedir al renderer principal el estado actual del motor (mismo contexto vía IPC).
	useEffect(() => {
		let cancelled = false
		void window.electronAPI.fetchPlayerUiEditorState(payload.handlerId).then((next) => {
			if (cancelled || !next) return
			setState(next as PlayerUiEditorState)
		})
		return () => {
			cancelled = true
		}
	}, [payload.handlerId])

	const delegate = useCallback(
		(action: PlayerUiEditorAction) => {
			void window.electronAPI.playerUiEditorAction(payload.handlerId, action)
		},
		[payload.handlerId],
	)

	return (
		<PlayerUiEditorPanel
			state={state}
			onRename={(name) => delegate({ action: 'rename', name })}
			onAddText={() => delegate({ action: 'addText' })}
			onAddImage={() => delegate({ action: 'addImage' })}
			onAddObject={() => delegate({ action: 'objectDrawStart' })}
			onCancelObjectDraw={() => delegate({ action: 'objectDrawCancel' })}
			onRemoveText={(id, label) => delegate({ action: 'removeText', id, label })}
			onRemoveImage={(id, label) => delegate({ action: 'removeImage', id, label })}
			onRemoveObject={(id, label) => delegate({ action: 'removeObject', id, label })}
			onSetElementProps={(kind, id, props) =>
				delegate({ action: 'setElementProps', kind, id, props })
			}
			onSave={() => delegate({ action: 'save' })}
			onCancel={() => delegate({ action: 'cancel' })}
		/>
	)
}
