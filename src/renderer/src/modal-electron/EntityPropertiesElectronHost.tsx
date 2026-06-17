import { useCallback, useEffect, useState } from 'react'

import type { ModalElectronOpenRequest } from '@shared-types'
import { EntityPropertiesModalContent } from '../pages/EngineView/components/sidebar/PropertiesAccordion/EntityPropertiesModalContent'
import type {
	EntityPropertiesAction,
	EntityPropertiesState,
} from './entityPropertiesTypes'

interface EntityPropertiesElectronHostProps {
	payload: ModalElectronOpenRequest
}

export function EntityPropertiesElectronHost({ payload }: EntityPropertiesElectronHostProps) {
	const [state, setState] = useState<EntityPropertiesState>(
		(payload.entityPropertiesState as EntityPropertiesState | undefined) ?? {
			projectType: '2D',
			selectedEntity: null,
			multiSelectedIds: [],
			multiSelectAlreadyMerged: false,
			isScenario: false,
			isCharacter: false,
			isEnvironment: false,
			isPlayer: false,
			isEditorCamera: false,
			isCollider: false,
			isExecutionArea: false,
			isFromBlueprint: false,
			linkedBlueprintName: null,
			scripts: [],
			animationPlayingIds: [],
			playingAnimationName: null,
		},
	)

	useEffect(() => {
		const initial = payload.entityPropertiesState as EntityPropertiesState | undefined
		if (initial) setState(initial)
	}, [payload.handlerId, payload.entityPropertiesState])

	useEffect(() => {
		const remove = window.electronAPI.onModalElectronPatch((data) => {
			if (data.handlerId !== payload.handlerId) return
			if (data.entityPropertiesState) {
				setState(data.entityPropertiesState as EntityPropertiesState)
			}
		})
		return remove
	}, [payload.handlerId])

	const delegate = useCallback(
		(action: EntityPropertiesAction) =>
			window.electronAPI.entityPropertiesAction(payload.handlerId, action),
		[payload.handlerId],
	)

	return <EntityPropertiesModalContent state={state} onAction={delegate} />
}

/** Cuerpo registrable para openModal (clave de componente). */
export function EntityPropertiesModalBody() {
	return null
}
