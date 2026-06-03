import type { ComponentType } from 'react'

import type { ModalElectronOpenRequest } from '@shared-types'
import { ScriptEditorModalBody } from '../components/SpritePreviewModalBody/components/ScriptEditorModalBody'
import { SpritePreviewModalBody } from '@components'
import { UserGuide } from '../pages/EngineView/components/sidebar/UserGuide'
import { ControlBindingsModalBody } from '../pages/EngineView/components/sidebar/ControlsAccordion/components/ControlBindingsModalBody'
import { CreateEntityFromSpriteModalBody } from '../pages/EngineView/components/sidebar/EntitiesAccordion/components/CreateEntityFromSpriteModalBody'
import { CreateEntityFromModelModalBody } from '../pages/EngineView/components/sidebar/EntitiesAccordion/components/CreateEntityFromModelModalBody'
import ModalSetNameBackground from '../pages/EngineView/components/sidebar/ResourcesAccordion/BackgroundsAccordion/components/ModalSetNameBackground'
import ModalSetNameFont from '../pages/EngineView/components/sidebar/ResourcesAccordion/FontsAccordion/components/ModalSetNameFont'
import ModalSetNameHudImage from '../pages/EngineView/components/sidebar/ResourcesAccordion/ImagesAccordion/components/ModalSetNameHudImage'
import ModalSetNameModel from '../pages/EngineView/components/sidebar/ResourcesAccordion/ModelsAccordion/components/ModalSetNameModel'
import ModalSetNameSound from '../pages/EngineView/components/sidebar/ResourcesAccordion/SoundsAccordion/components/ModalSetNameSound'
import ModalSetNameSprite from '../pages/EngineView/components/sidebar/ResourcesAccordion/SpritesAccordion/components/ModalSetNameSprite'
import ModalSelectFont from '../pages/EngineView/components/sidebar/UIAccordion/components/ModalSelectFont'
import ModalSelectHudImage from '../pages/EngineView/components/sidebar/UIAccordion/components/ModalSelectHudImage'
import ModalSetNameUi from '../pages/EngineView/components/sidebar/UIAccordion/components/ModalSetNameUi'
import ModalAddUiButton from '../pages/EngineView/components/sidebar/UIAccordion/components/ModalAddUiButton'
import ModalAddUiElementPlaceholder from '../pages/EngineView/components/sidebar/UIAccordion/components/ModalAddUiElementPlaceholder'
import ModalSelectUiElement from '../pages/EngineView/components/sidebar/UIAccordion/components/ModalSelectUiElement'
import {
	CreateSceneModalBody,
	DeleteBlockedBody,
	DeleteConfirmBody,
	SceneRenameModalBody,
} from '../pages/EngineView/hooks/sceneManagerModalBodies'
import { ModalConfirmBody } from './ModalConfirmBody'
import { wireModalCallbacksForHost } from './modalElectronCallbacks'

export type ModalElectronHostProps = ModalElectronOpenRequest & {
	onClose: () => void
	onComplete: (result: unknown) => void
}

type RegistryEntry = ComponentType<Record<string, unknown>>

export const MODAL_ELECTRON_REGISTRY: Record<string, RegistryEntry> = {
	ModalSelectFont: ModalSelectFont as RegistryEntry,
	ModalSelectHudImage: ModalSelectHudImage as RegistryEntry,
	ModalSetNameUi: ModalSetNameUi as RegistryEntry,
	ModalSetNameSound: ModalSetNameSound as RegistryEntry,
	ModalSetNameFont: ModalSetNameFont as RegistryEntry,
	ModalSetNameBackground: ModalSetNameBackground as RegistryEntry,
	ModalSetNameHudImage: ModalSetNameHudImage as RegistryEntry,
	ModalSetNameSprite: ModalSetNameSprite as RegistryEntry,
	ModalSetNameModel: ModalSetNameModel as RegistryEntry,
	ModalAddUiButton: ModalAddUiButton as RegistryEntry,
	ModalAddUiElementPlaceholder: ModalAddUiElementPlaceholder as RegistryEntry,
	ModalSelectUiElement: ModalSelectUiElement as RegistryEntry,
	ScriptEditorModalBody: ScriptEditorModalBody as RegistryEntry,
	SpritePreviewModalBody: SpritePreviewModalBody as RegistryEntry,
	CreateEntityFromSpriteModalBody: CreateEntityFromSpriteModalBody as RegistryEntry,
	CreateEntityFromModelModalBody: CreateEntityFromModelModalBody as RegistryEntry,
	ControlBindingsModalBody: ControlBindingsModalBody as RegistryEntry,
	UserGuide: UserGuide as RegistryEntry,
	CreateSceneModalBody: CreateSceneModalBody as RegistryEntry,
	SceneRenameModalBody: SceneRenameModalBody as RegistryEntry,
	DeleteConfirmBody: DeleteConfirmBody as RegistryEntry,
	DeleteBlockedBody: DeleteBlockedBody as RegistryEntry,
	ModalConfirmBody: ModalConfirmBody as RegistryEntry,
}

export function buildModalElectronHostProps(
	payload: ModalElectronOpenRequest,
	onClose: () => void,
): Record<string, unknown> {
	return wireModalCallbacksForHost(payload, onClose)
}
