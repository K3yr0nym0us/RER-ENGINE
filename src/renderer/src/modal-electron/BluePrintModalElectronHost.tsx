import { useCallback, useEffect, useState } from 'react'

import type { BluePrintEntry, ModalElectronOpenRequest } from '@shared-types'
import { BluePrintModalContent } from '../pages/EngineView/components/sidebar/ToolsAccordion/components/BluePrintModalBody'
import type { BluePrintModalDelegateAction } from '../pages/EngineView/components/sidebar/ToolsAccordion/components/bluePrintModalActions'

interface BluePrintModalElectronHostProps {
	payload: ModalElectronOpenRequest
}

export function BluePrintModalElectronHost({ payload }: BluePrintModalElectronHostProps) {
	const [blueprints, setBlueprints] = useState<BluePrintEntry[]>(
		(payload.blueprints as BluePrintEntry[] | undefined) ?? [],
	)
	const [linkedCounts, setLinkedCounts] = useState<Record<string, number>>(
		(payload.linkedEntityCounts as Record<string, number> | undefined) ?? {},
	)

	// La ventana modal se reutiliza (hide, no destroy): sincronizar lista al cada open/render.
	useEffect(() => {
		setBlueprints((payload.blueprints as BluePrintEntry[] | undefined) ?? [])
		setLinkedCounts(
			(payload.linkedEntityCounts as Record<string, number> | undefined) ?? {},
		)
	}, [payload.handlerId, payload.blueprints, payload.linkedEntityCounts])

	const delegate = useCallback(
		async (data: BluePrintModalDelegateAction) => {
			const result = await window.electronAPI.delegateModalElectron({
				handlerId: payload.handlerId,
				...data,
			})
			if (result?.blueprints) {
				setBlueprints(result.blueprints as BluePrintEntry[])
			}
		},
		[payload.handlerId],
	)

	const completeSelect = (bp: BluePrintEntry) => {
		window.electronAPI.completeModalElectron(payload.handlerId, bp, 'onSelect')
	}

	return (
		<BluePrintModalContent
			blueprints={blueprints}
			getLinkedEntityCount={(bpId) => linkedCounts[bpId] ?? 0}
			onSelect={(bp) => completeSelect(bp)}
			onDeleteWithEntities={(bp) => void delegate({ action: 'deleteWithEntities', blueprint: bp })}
			onDeleteKeepEntities={(bp) => void delegate({ action: 'deleteKeepEntities', blueprint: bp })}
		/>
	)
}
