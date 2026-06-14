import type { ComponentType } from 'react'

/**
 * REGISTRO DE MODALES ELECTRON
 * =============================
 * Todas las ventanas abiertas con `useModal()` / `openModal({ body: <MiModal /> })`
 * se renderizan en una ventana Electron **hija**, no en el DOM del editor.
 *
 * Si olvidas registrar un modal aquí verás:
 *   "Componente modal no soportado: NombreDelComponente"
 *
 * Checklist al añadir un modal nuevo → docs/MODAL_ELECTRON.yaml
 *
 * Excepciones con host dedicado (no van en este mapa):
 *   - BluePrintModalBody      → BluePrintModalElectronHost
 *   - PlayerUiEditorModalBody → PlayerUiEditorElectronHost
 */

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
	SwitchSceneConfirmBody,
	UnsavedSceneBlockedBody,
} from '../pages/EngineView/hooks/sceneManagerModalBodies'
import { ModalConfirmBody } from './ModalConfirmBody'
import { ProjectSaveBlockingModalBody } from './ProjectSaveBlockingModalBody'
import { VisualScriptingModalBody } from '../visualScripting/components/VisualScriptingModalBody'
import { SceneScriptEditorModalBody } from '../visualScripting/components/SceneScriptEditorModalBody'
import { EntityPropertiesModalBody } from './EntityPropertiesElectronHost'
import { PluginsModalBody } from '../plugins/PluginsModalBody'
import { wireModalCallbacksForHost } from './modalElectronCallbacks'

export type ModalElectronHostProps = ModalElectronOpenRequest & {
	onClose: () => void
	onComplete: (result: unknown) => void
}

/** Props dinámicas ensambladas en el host modal (IPC + callbacks). */
export type RegistryEntry = ComponentType<Record<string, unknown>>

/** Registra un cuerpo modal con props tipadas en el mapa del host. */
function modalEntry(component: unknown): RegistryEntry {
	return component as RegistryEntry
}

export const MODAL_ELECTRON_REGISTRY: Record<string, RegistryEntry> = {
	ModalSelectFont: modalEntry(ModalSelectFont),
	ModalSelectHudImage: modalEntry(ModalSelectHudImage),
	ModalSetNameUi: modalEntry(ModalSetNameUi),
	ModalSetNameSound: modalEntry(ModalSetNameSound),
	ModalSetNameFont: modalEntry(ModalSetNameFont),
	ModalSetNameBackground: modalEntry(ModalSetNameBackground),
	ModalSetNameHudImage: modalEntry(ModalSetNameHudImage),
	ModalSetNameSprite: modalEntry(ModalSetNameSprite),
	ModalSetNameModel: modalEntry(ModalSetNameModel),
	ModalAddUiButton: modalEntry(ModalAddUiButton),
	ModalAddUiElementPlaceholder: modalEntry(ModalAddUiElementPlaceholder),
	ModalSelectUiElement: modalEntry(ModalSelectUiElement),
	ScriptEditorModalBody: modalEntry(ScriptEditorModalBody),
	SpritePreviewModalBody: modalEntry(SpritePreviewModalBody),
	CreateEntityFromSpriteModalBody: modalEntry(CreateEntityFromSpriteModalBody),
	CreateEntityFromModelModalBody: modalEntry(CreateEntityFromModelModalBody),
	ControlBindingsModalBody: modalEntry(ControlBindingsModalBody),
	UserGuide: modalEntry(UserGuide),
	CreateSceneModalBody: modalEntry(CreateSceneModalBody),
	SceneRenameModalBody: modalEntry(SceneRenameModalBody),
	DeleteConfirmBody: modalEntry(DeleteConfirmBody),
	SwitchSceneConfirmBody: modalEntry(SwitchSceneConfirmBody),
	DeleteBlockedBody: modalEntry(DeleteBlockedBody),
	UnsavedSceneBlockedBody: modalEntry(UnsavedSceneBlockedBody),
	ModalConfirmBody: modalEntry(ModalConfirmBody),
	ProjectSaveBlockingModalBody: modalEntry(ProjectSaveBlockingModalBody),
	VisualScriptingModalBody: modalEntry(VisualScriptingModalBody),
	SceneScriptEditorModalBody: modalEntry(SceneScriptEditorModalBody),
	EntityPropertiesModalBody: modalEntry(EntityPropertiesModalBody),
	PluginsModalBody: modalEntry(PluginsModalBody),
}

export function buildModalElectronHostProps(
	payload: ModalElectronOpenRequest,
	onClose: () => void,
): Record<string, unknown> {
	return wireModalCallbacksForHost(payload, onClose)
}
