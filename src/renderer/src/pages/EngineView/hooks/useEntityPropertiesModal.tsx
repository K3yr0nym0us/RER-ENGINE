import { createElement, useCallback, useEffect, useRef } from 'react'

import { useContextEngine } from '@engine'
import { useModal } from '@modal'
import { useTraslate } from '@hooks'
import type { EngineEvent } from '@shared-types'
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
	const pendingOpenEntityIdRef = useRef<number | null>(null)

	const openPropertiesModal = useCallback((entityId?: number) => {
		if (activeSocketConfigModalHandlerRef.current) {
			return
		}
		if (modalOpenRef.current) {
			return
		}
		const targetId = entityId ?? pendingOpenEntityIdRef.current ?? selectedEntity?.id ?? null
		if (targetId == null) {
			return
		}
		if (selectedEntity?.id !== targetId) {
			pendingOpenEntityIdRef.current = targetId
			return
		}
		pendingOpenEntityIdRef.current = null
		const multiSelectedKey = multiSelectedIds.join(',')
		const selectionKey =
			multiSelectedIds.length > 1
				? `multi:${multiSelectedKey}`
				: `single:${targetId}`
		lastSelectionKeyRef.current = selectionKey
		openModalRef.current({
			title: t('Entity properties'),
			size: 'sm',
			body: createElement(EntityPropertiesModalBody),
		})
		modalOpenRef.current = true
		send({ cmd: 'query_entity_animation_play_state', entity_id: targetId } as never)
	}, [multiSelectedIds, selectedEntity, send, t])

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
		const onEngineEvent = (event: EngineEvent) => {
			if (event.event !== 'entity_properties_open') return
			const entityId = typeof event.id === 'number' ? event.id : undefined
			openPropertiesModal(entityId)
		}
		window.engine.on(onEngineEvent)
		return () => {
			window.engine.off(onEngineEvent)
		}
	}, [openPropertiesModal])

	useEffect(() => {
		if (pendingOpenEntityIdRef.current == null) return
		if (selectedEntity?.id !== pendingOpenEntityIdRef.current) return
		openPropertiesModal()
	}, [selectedEntity, openPropertiesModal])

	useEffect(() => {
		const multiSelectedKey = multiSelectedIds.join(',')
		const selectionKey =
			multiSelectedIds.length > 1
				? `multi:${multiSelectedKey}`
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

		if (!modalOpenRef.current) {
			return
		}

		if (lastSelectionKeyRef.current !== selectionKey) {
			lastSelectionKeyRef.current = selectionKey
			if (activeEntityPropertiesHandlerRef.current) {
				if (selectedEntity?.id != null) {
					send({ cmd: 'query_entity_animation_play_state', entity_id: selectedEntity.id } as never)
				}
				pushEntityPropertiesPatch(activeEntityPropertiesHandlerRef.current)
			}
		}
	}, [selectedEntity, multiSelectedIds, send])

	const selectedPositionKey = selectedEntity?.position?.join(',')
	const selectedRotationKey = selectedEntity?.rotation?.join(',')
	const selectedScaleKey = selectedEntity?.scale?.join(',')
	const selectedPhysicsEnabled = selectedEntity?.physicsEnabled
	const selectedPhysicsType = selectedEntity?.physicsType

	useEffect(() => {
		if (!modalOpenRef.current || !activeEntityPropertiesHandlerRef.current) return
		pushEntityPropertiesPatch(activeEntityPropertiesHandlerRef.current)
	}, [
		selectedPositionKey,
		selectedRotationKey,
		selectedScaleKey,
		selectedPhysicsEnabled,
		selectedPhysicsType,
	])
}
