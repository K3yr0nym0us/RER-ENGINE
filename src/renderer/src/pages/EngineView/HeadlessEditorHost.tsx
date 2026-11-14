import { createElement, useEffect, useRef } from 'react';

import { EngineProvider, useContextEngine } from '@engine';
import { ModalProvider, useModal } from '@modal';
import { useAutoSave, useControlBindings, useTraslate } from '@hooks';
import { ModalConfirmBody } from '../../modal-electron/ModalConfirmBody';
import { ExitConfirmBody } from '../../modal-electron/ExitConfirmBody';
import { SocketConfigModalBody } from '../../modal-electron/SocketConfigModalElectronHost';
import { EntityPropertiesModalBody } from '../../modal-electron/EntityPropertiesElectronHost';
import { usePlayerUiEditorModal } from '../../modal-electron/usePlayerUiEditorModal';
import { PluginsModalBody } from '../../plugins/PluginsModalBody';
import { WorldGenModalBody } from '../../plugins/worldgen/WorldGenModalBody';
import { UserGuide } from './components/sidebar/UserGuide';
import { BluePrintModalBody } from './components/sidebar/ToolsAccordion/components/BluePrintModalBody';
import { useQuickBuild } from '../../context/QuickBuildContext';
import {
  blueprintPlacementPhysics,
  blueprintUsesModel3D,
  buildBlueprintPlacementMeta,
  isModel3DPath,
  resolveBlueprintModelPath,
  resolveEngineModelPath,
} from '../../utils/blueprintModelPath';
import ModalSetNameUi from './components/sidebar/UIAccordion/components/ModalSetNameUi';
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
      <QuickBuildBridge />
      <UiScreensSync />
      <LegacyWindowListener />
    </SceneManagerProvider>
  )
}

/** Conecta el blueprint activo de React con el motor nativo (ghost + colocación). */
function QuickBuildBridge() {
  const { activeBluePrint } = useQuickBuild()
  const engine = useContextEngine()
  const modelsRef = useRef(engine.models)
  modelsRef.current = engine.models
  const sendRef = useRef(engine.send)
  sendRef.current = engine.send
  const projectTypeRef = useRef(engine.projectType)
  projectTypeRef.current = engine.projectType
  const hadActiveRef = useRef(false)

  useEffect(() => {
    window.electronAPI.syncQuickBuildState({
      active: Boolean(activeBluePrint),
      name: activeBluePrint?.name,
    })
  }, [activeBluePrint])

  useEffect(() => {
    if (!engine.engineReady) return
    if (activeBluePrint) {
      hadActiveRef.current = true
      const is3D = projectTypeRef.current === '3D'
      const models = modelsRef.current
      const modelPath = resolveBlueprintModelPath(activeBluePrint)
      const enginePath = resolveEngineModelPath(modelPath, models)
      const firstFrame = activeBluePrint.animations?.[0]?.frames?.[0]
      const previewSrcRect =
        !is3D
        && firstFrame?.src_x != null
        && firstFrame?.src_y != null
        && firstFrame?.src_w != null
        && firstFrame?.src_h != null
          ? ([firstFrame.src_x, firstFrame.src_y, firstFrame.src_w, firstFrame.src_h] as [
              number,
              number,
              number,
              number,
            ])
          : undefined

      if (is3D) {
        const placementMeta = buildBlueprintPlacementMeta(activeBluePrint, models)
        const { physicsEnabled, physicsType } = blueprintPlacementPhysics(activeBluePrint, models)
        sendRef.current({
          cmd: 'set_active_tool',
          tool: 'quick_build_place',
          preview_path:
            blueprintUsesModel3D(activeBluePrint) && isModel3DPath(modelPath)
              ? enginePath
              : modelPath,
          preview_kind: 'model',
          preview_scale: activeBluePrint.scale,
          preview_rotation: activeBluePrint.rotation ?? [0, 0, 0, 1],
          preview_name: activeBluePrint.name,
          preview_entity_category: placementMeta.category,
          preview_physics_enabled: physicsEnabled,
          preview_physics_type: physicsType,
          preview_blueprint: placementMeta,
          preview_blueprint_id: activeBluePrint.id,
          preview_src_rect: previewSrcRect,
        } as never)
      } else {
        sendRef.current({
          cmd: 'set_active_tool',
          tool: 'quick_build_place',
          preview_path: firstFrame?.path ?? activeBluePrint.path,
          preview_kind: activeBluePrint.kind === 'scenario' ? 'scenario' : 'character',
          preview_scale: activeBluePrint.scale,
          preview_blueprint_id: activeBluePrint.id,
          preview_src_rect: previewSrcRect,
        } as never)
      }
    } else if (hadActiveRef.current) {
      hadActiveRef.current = false
      sendRef.current({ cmd: 'set_active_tool', tool: '' } as never)
    }
  }, [activeBluePrint, engine.engineReady])

  return null
}

function UiScreensSync() {
  const { playerUiScreens, menuUiScreens } = useContextEngine()
  const lastSentRef = useRef('')

  useEffect(() => {
    const screens = {
      player: playerUiScreens.map((screen) => ({
        id: screen.id,
        name: screen.name,
        active: Boolean(screen.active),
      })),
      menu: menuUiScreens.map((screen) => ({
        id: screen.id,
        name: screen.name,
        active: Boolean(screen.active),
      })),
    }
    const serialized = JSON.stringify(screens)
    if (serialized === lastSentRef.current) return
    lastSentRef.current = serialized
    window.electronAPI.syncUiScreens(screens)
  }, [playerUiScreens, menuUiScreens])

  return null
}

function LegacyWindowListener() {
  const engine = useContextEngine()
  const { openModal } = useModal()
  const { t } = useTraslate()
  const { openBindingsModal } = useControlBindings()
  const playerUiEditor = usePlayerUiEditorModal('player')
  const { activeBluePrint, setActiveBluePrint } = useQuickBuild()
  const playerUiEditorRef = useRef(playerUiEditor)
  playerUiEditorRef.current = playerUiEditor
  const {
    openCreateSceneModal,
    openDeleteSceneModal,
    openSceneScriptEditor,
    openVisualScriptingModal,
    scenes,
  } = useSceneManager()

  // Mantiene el ciclo de vida de propiedades (parches de selecciÃ³n, cierre).
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
        case 'exit_confirm':
          openModal({
            title: t('Exit RER-ENGINE'),
            size: 'sm',
            body: createElement(ExitConfirmBody),
          })
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
        case 'plugins':
          openModal({
            title: t('Plugins'),
            size: 'lg',
            body: createElement(PluginsModalBody),
          })
          break
        case 'worldgen':
          openModal({
            title: t('World Generator'),
            size: 'lg',
            body: createElement(WorldGenModalBody),
          })
          break
        case 'user_guide':
          openModal({
            title: t('User guide'),
            size: 'lg',
            body: createElement(UserGuide),
          })
          break
        case 'control_bindings':
          openBindingsModal(payload.mode === 'gamepad' ? 'gamepad' : 'keyboard_mouse')
          break
        case 'ui_create_screen': {
          const scope = payload.scope === 'menu' ? 'menu' : 'player'
          const list = scope === 'player' ? engine.playerUiScreens : engine.menuUiScreens
          const id = engine.addUiScreen(
            scope,
            `${scope === 'player' ? 'HUD' : 'UI Menu'} ${list.length + 1}`,
          )
          if (scope === 'player' && id) playerUiEditorRef.current.openEditor(id)
          break
        }
        case 'ui_edit_screen': {
          const scope = payload.scope === 'menu' ? 'menu' : 'player'
          const screenId = String(payload.screen_id ?? '')
          if (!screenId) break
          if (scope === 'player') {
            playerUiEditorRef.current.openEditor(screenId)
          } else {
            openModal({
              title: t('Edit UI'),
              size: 'sm',
              body: createElement(ModalSetNameUi, {
                defaultName:
                  engine.menuUiScreens.find((screen) => screen.id === screenId)?.name ?? '',
                onConfirm: (name: string) => engine.renameUiScreen('menu', screenId, name),
              }),
            })
          }
          break
        }
        case 'ui_delete_screen': {
          const scope = payload.scope === 'menu' ? 'menu' : 'player'
          const screenId = String(payload.screen_id ?? '')
          const screenName = String(payload.name ?? '')
          if (!screenId) break
          openModal({
            title: t('Confirm deletion'),
            size: 'sm',
            body: createElement(ModalConfirmBody, {
              buttonSize: 'sm',
              messageSpec: {
                lines: [
                  {
                    parts: [
                      { type: 'text', value: `${t('Are you sure you want to delete the UI screen')} ` },
                      { type: 'bold', value: screenName },
                    ],
                  },
                ],
              },
              onConfirm: () => engine.removeUiScreen(scope, screenId),
            }),
          })
          break
        }
        case 'ui_set_active_screen': {
          const screenId = payload.screen_id ? String(payload.screen_id) : null
          engine.setActivePlayerUiScreen(screenId)
          break
        }
        case 'quick_build': {
          if (payload.action === 'toggle' && activeBluePrint) {
            setActiveBluePrint(null)
          } else if (!activeBluePrint) {
            openModal({
              title: t('Construction'),
              size: 'lg',
              body: createElement(BluePrintModalBody),
            })
          }
          break
        }
        default:
          break
      }
    })
  }, [
    activeBluePrint,
    engine,
    openBindingsModal,
    openCreateSceneModal,
    openDeleteSceneModal,
    openModal,
    openSceneScriptEditor,
    openVisualScriptingModal,
    scenes,
    setActiveBluePrint,
    t,
  ])

  return null
}
