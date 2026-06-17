import { createElement, useEffect, useRef } from 'react'

import { useContextEngine } from '@engine'
import { useModal } from '@modal'
import { useTraslate } from '@hooks'
import {
	activeEntityPropertiesHandlerRef,
	pushEntityPropertiesPatch,
	unregisterEntityPropertiesSession,
} from '../../../modal-electron/entityPropertiesModalSessions'
import { activeSocketConfigModalHandlerRef } from '../../../modal-electron/socketConfigModalSessions'
import { EntityPropertiesModalBody } from '../../../modal-electron/EntityPropertiesElectronHost'

export function useEntityPropertiesModal(): void {
	const { selectedEntity, multiSelectedIds, send } = useContextEngine()
	const { openModal, closeModal } = useModal()
	const { t } = useTraslate()

	const openModalRef = useRef(openModal)
	const closeModalRef = useRef(closeModal)
	openModalRef.current = openModal
	closeModalRef.current = closeModal

	const modalOpenRef = useRef(false)
	const lastSelectionKeyRef = useRef('')
	const skipDeselectOnCloseRef = useRef(false)

	useEffect(() => {
		const removeClosed = window.electronAPI.onModalElectronClosed((data) => {
			if (data.componentKey !== 'EntityPropertiesModalBody') return
			modalOpenRef.current = false
			lastSelectionKeyRef.current = ''
			if (activeEntityPropertiesHandlerRef.current) {
				unregisterEntityPropertiesSession(activeEntityPropertiesHandlerRef.current)
			}
			if (!skipDeselectOnCloseRef.current) {
				send({ cmd: 'deselect_entity' })
			}
			skipDeselectOnCloseRef.current = false
		})
		return removeClosed
	}, [send])

	useEffect(() => {
		const selectionKey =
			multiSelectedIds.length > 1
				? `multi:${multiSelectedIds.join(',')}`
				: selectedEntity
					? `single:${selectedEntity.id}`
					: ''

		if (!selectionKey) {
			if (modalOpenRef.current) {
				skipDeselectOnCloseRef.current = true
				modalOpenRef.current = false
				lastSelectionKeyRef.current = ''
				closeModalRef.current()
			}
			return
		}

		if (activeSocketConfigModalHandlerRef.current) {
			return
		}

		if (modalOpenRef.current) {
			if (lastSelectionKeyRef.current !== selectionKey) {
				lastSelectionKeyRef.current = selectionKey
				if (activeEntityPropertiesHandlerRef.current) {
					if (selectedEntity?.id != null) {
						send({ cmd: 'query_entity_animation_play_state', entity_id: selectedEntity.id } as never)
					}
					pushEntityPropertiesPatch(activeEntityPropertiesHandlerRef.current)
				}
			}
			return
		}

		lastSelectionKeyRef.current = selectionKey

		openModalRef.current({
			title: t('Entity properties'),
			size: 'sm',
			body: createElement(EntityPropertiesModalBody),
		})
		modalOpenRef.current = true
		if (selectedEntity?.id != null) {
			send({ cmd: 'query_entity_animation_play_state', entity_id: selectedEntity.id } as never)
		}
	}, [selectedEntity?.id, selectedEntity?.name, multiSelectedIds.join(','), t, send])

	useEffect(() => {
		if (!modalOpenRef.current || !activeEntityPropertiesHandlerRef.current) return
		pushEntityPropertiesPatch(activeEntityPropertiesHandlerRef.current)
	}, [
		selectedEntity?.position?.join(','),
		selectedEntity?.rotation?.join(','),
		selectedEntity?.scale?.join(','),
		selectedEntity?.physicsEnabled,
		selectedEntity?.physicsType,
	])
}
