import { createElement, useEffect, useRef } from 'react';

import { EngineProvider, useContextEngine } from '@engine';
import { ModalProvider, useModal } from '@modal';
import { useAutoSave, useTraslate } from '@hooks';
import { ModalConfirmBody } from '../../modal-electron/ModalConfirmBody';
import { SocketConfigModalBody } from '../../modal-electron/SocketConfigModalElectronHost';
import { EntityPropertiesModalBody } from '../../modal-electron/EntityPropertiesElectronHost';
import { QuickBuildProvider } from '../../context/QuickBuildContext';
import { PlaneToolProvider } from '../../context/PlaneToolContext';
import { SceneManagerProvider, useSceneManager } from './hooks/useSceneManager';
import { useEntityPropertiesModal } from './hooks/useEntityPropertiesModal';

import type { GameStyle, ProjectType } from '@shared-types';

interface HeadlessProject {
  projectType: ProjectType
  gameStyle?: GameStyle
  initialSavePath?: string | null
  initialExtractDir?: string | null
}

/**
 * Host invisible que mantiene el estado React necesario para las ventanas
 * legacy (RHAI, nodos, propiedades, sockets, etc.).
 *
 * No renderiza el editor: solo monta los providers y escucha peticiones del
 * editor Rust. Se ejecuta en la antigua ventana del selector, que Electron
 * oculta al arrancar el motor.
 */
export function HeadlessEditorHost(project: HeadlessProject) {
  const viewportRef = useRef<HTMLDivElement | null>(null)
  return (
    <EngineProvider
      viewportRef={viewportRef}
      projectType={project.projectType}
      gameStyle={project.gameStyle}
      initialSavePath={project.initialSavePath}
      initialExtractDir={project.initialExtractDir}
    >
      <QuickBuildProvider>
        <PlaneToolProvider>
          <ModalProvider>
            <HeadlessEditorHostInner {...project} />
          </ModalProvider>
        </PlaneToolProvider>
      </QuickBuildProvider>
    </EngineProvider>
  )
}

function HeadlessEditorHostInner({
  projectType,
  gameStyle,
  initialSavePath,
  initialExtractDir,
}: HeadlessProject) {
  const { handleSave } = useAutoSave({
    projectType,
    gameStyle,
    initialSavePath,
    initialExtractDir,
  })

  return (
    <SceneManagerProvider
      initialSavePath={initialSavePath}
      initialExtractDir={initialExtractDir}
      projectType={projectType}
      gameStyle={gameStyle}
      onSaveProject={handleSave}
    >
      <LegacyWindowListener />
    </SceneManagerProvider>
  )
}

function LegacyWindowListener() {
  const engine = useContextEngine()
  const { openModal } = useModal()
  const { t } = useTraslate()
  const {
    openCreateSceneModal,
    openDeleteSceneModal,
    openSceneScriptEditor,
    openVisualScriptingModal,
    scenes,
  } = useSceneManager()

  // Mantiene el ciclo de vida de propiedades (parches de selección, cierre).
  useEntityPropertiesModal()

  useEffect(() => {
    return window.electronAPI.onLegacyWindowOpen((request) => {
      const payload = (request.payload ?? {}) as Record<string, unknown>
      switch (request.kind) {
        case 'scene_script':
          openSceneScriptEditor(Number(payload.scene_id ?? 0) || undefined)
          break
        case 'scene_visual_script':
          openVisualScriptingModal(Number(payload.scene_id ?? 0) || undefined)
          break
        case 'create_scene':
          openCreateSceneModal()
          break
        case 'delete_scene': {
          const sceneId = Number(payload.scene_id ?? 0)
          const scene = scenes.find((entry) => entry.id === sceneId)
          if (scene) openDeleteSceneModal(scene)
          break
        }
        case 'remove_asset': {
          const assetKind = String(payload.asset_kind ?? '')
          const assetPath = String(payload.path ?? '')
          const assetName = String(payload.name ?? '')
          if (!assetKind || !assetPath) break
          openModal({
            title: t('Delete asset'),
            size: 'sm',
            body: createElement(ModalConfirmBody, {
              messageSpec: {
                lines: [
                  {
                    className: 'mb-2',
                    parts: [
                      { type: 'text', value: t('Are you sure you want to delete the asset') },
                      { type: 'bold', value: assetName },
                      { type: 'text', value: '?' },
                    ],
                  },
                  {
                    className: 'text-danger small mb-0',
                    parts: [{ type: 'text', value: t('This action cannot be undone.') }],
                  },
                ],
              },
              confirmLabel: t('Yes, Delete'),
              onConfirm: () => {
                if (assetKind === 'model') engine.removeModelAsset(assetPath)
                else if (assetKind === 'sprite') engine.removeSprite(assetPath)
                else if (assetKind === 'sound') engine.removeSound(assetPath)
                else if (assetKind === 'font') engine.removeFont(assetPath)
                else if (assetKind === 'hud_image') engine.removeHudImage(assetPath)
                else if (assetKind === 'background') engine.removeBackgroundFromLibrary(assetPath)
              },
            }),
          })
          break
        }
        case 'socket_config':
          openModal({
            title: t('Socket configuration'),
            size: 'sm',
            body: createElement(SocketConfigModalBody),
          })
          break
        case 'entity_properties':
          openModal({
            title: t('Entity properties'),
            size: 'sm',
            body: createElement(EntityPropertiesModalBody),
          })
          break
        default:
          break
      }
    })
  }, [
    engine,
    openCreateSceneModal,
    openDeleteSceneModal,
    openModal,
    openSceneScriptEditor,
    openVisualScriptingModal,
    scenes,
    t,
  ])

  return null
}
