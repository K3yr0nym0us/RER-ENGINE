import { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';

import type {
  EditorSceneListItem,
  EngineEvent,
  Entity3D,
  GameStyle,
  SavedScene,
  SavedWorldConfig,
  VisualGraphDocument,
} from '@shared-types';
import {
  DEFAULT_GRAVITY_MAGNITUDE,
  DEFAULT_LIGHT_AMBIENT,
  DEFAULT_LIGHT_INTENSITY,
  DEFAULT_SHADOW_DARKNESS,
} from '@shared-types';
import { isEditorBoxPath, isEditorCameraPath, isGroundPath, isPlayerPath, isSunPath } from '@shared-types';
import { is3dModelFileEntity } from '../../../utils/blueprintModelPath';
import {
  entity3dPendingRestore,
  entity3dSpawnPath,
  entity3dTransform,
  playViewFromPlayerAndCamera,
} from '../../../utils/entity3dEditorSync';
import { buildActiveSceneSnapshotFromEngine } from '../../../defaults/buildProjectSaveFromEngine';
import { defaultSceneName } from '../../../defaults/defaultSceneName';
import { ensurePlayCharacterOnLoad } from '../../../defaults/playCharacterSceneRestore';
import { buildImportSceneCommand, syncEditorStateFromSavedScene } from '../../../context/useContextEngine/hooks/buildImportSceneCommand';
import {
  beginSceneBurstLoad,
  beginSceneImportLoading,
  beginFpSceneBaselineLogging,
  beginSceneWorldCleanup,
  endSceneImportLoading,
  needsSceneBurstLoad,
  scheduleEndSceneWorldCleanup,
  trackSceneBurstCollider,
  trackSceneBurstOp,
  trackSceneBurstModelPreloads,
  collectUncachedBurstModelPaths,
  countCachedBurstModelPreloads,
  kickCachedBurstModelSpawns,
  tryEndSceneBurstLoad,
} from '../../../context/useContextEngine/hooks/sceneImportOverlay';
import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';
import { getSceneVisualGraph, getSceneScriptRhai, setSceneProjectState } from '../sceneStateStore';
import { reloadActiveSceneVisualScript, saveSceneScriptRhai, saveSceneVisualGraph } from '../../../visualScripting/sceneVisualScript';
import { VisualScriptingModalBody } from '../../../visualScripting/components/VisualScriptingModalBody';
import { resolveSceneEntitiesForVisualScript } from '../../../visualScripting/resolveSceneEntities';
import { SceneScriptEditorModalBody } from '../../../visualScripting/components/SceneScriptEditorModalBody';
import {
	CreateSceneModalBody,
	DeleteBlockedBody,
	DeleteConfirmBody,
	SceneRenameModalBody,
	SwitchSceneConfirmBody,
	UnsavedSceneBlockedBody,
} from './sceneManagerModalBodies';

export interface SceneTab {
  id: number;
  name: string;
  /** Escena activa con pila undo no vacía (solo 3D FP, motor). */
  dirty?: boolean;
}

interface SceneManagerContextValue {
  scenes: SceneTab[];
  activeSceneId: number;
  scenesListLoading: boolean;
  switchingToSceneId: number | null;
  sceneActionsDisabled: boolean;
  openSwitchSceneModal: (scene: SceneTab) => void;
  openCreateSceneModal: () => void;
  openRenameSceneModal: (scene: SceneTab) => void;
  openDeleteSceneModal: (scene: SceneTab) => void;
  openVisualScriptingModal: (sceneId?: number) => void;
  openSceneScriptEditor: (sceneId?: number) => void;
  persistSceneVisualGraph: (
    sceneId: number,
    graph: VisualGraphDocument,
  ) => { ok: boolean; errors: string[] };
}

const DEFAULT_WORLD: SavedWorldConfig = {
  worldWidth: 100,
  worldHeight: 50,
  worldDepth: 100,
  gridVisible: true,
  gridCellSize: 1,
  gravity: DEFAULT_GRAVITY_MAGNITUDE,
  targetFps: 60,
  lightAmbient: DEFAULT_LIGHT_AMBIENT,
  lightIntensity: DEFAULT_LIGHT_INTENSITY,
  shadowDarkness: DEFAULT_SHADOW_DARKNESS,
};

const SceneManagerContext = createContext<SceneManagerContextValue | null>(null);

/** Estado inicial vacío; lista de escenas y escena activa llegan por `project_loaded_*` del motor. */
function buildInitialSceneState() {
  return {
    tabs: [] as SceneTab[],
    dataById: {} as Record<number, SavedScene>,
    activeSceneId: 1,
  };
}

function motorManagesScenes(projectType?: string, gameStyle?: GameStyle): boolean {
  return projectType === '3D' && gameStyle === 'first-person';
}

function tabsFromMotorItems(items: EditorSceneListItem[]): SceneTab[] {
  return items.map((s) => ({
    id: s.id,
    name: s.name,
    dirty: s.dirty === true,
  }));
}

export function SceneManagerProvider({
  children,
  initialSavePath,
  initialExtractDir,
  projectType,
  gameStyle,
  onSaveProject,
}: {
  children: ReactNode;
  initialSavePath?: string | null;
  initialExtractDir?: string | null;
  projectType?: string;
  gameStyle?: GameStyle;
  onSaveProject?: () => void | Promise<void>;
}) {
  const { t } = useTraslate();
  const {
    engineReady,
    worldConfig,
    backgroundPath,
    scenarioEntities,
    characterEntities,
    colliderEntities,
    executionAreaEntities,
    loadedSpritesInfo,
    entityTransformsRef,
    entityMetaRef,
    pendingRestoresRef,
    pendingModelLoadQueueRef,
    pendingBurstSpawnRestoreRef,
    pendingPlayCharacterViewRef,
    playCharacterViewRef,
    mainPlayerHandled,
    playerEntityIdRef,
    playerRemoved,
    editorCameraEntityIdRef,
    camera2dRef,
    pendingImportSceneRef,
    sceneImportInProgressRef,
    modelReplaceInProgressRef,
    sceneBurstLoadInProgressRef,
    sceneBurstPendingColliderCountRef,
    sceneBurstPendingOpsRef,
    sceneWorldCleanupRef,
    fpSceneBaselineLogRef,
    reportBounds,
    dispatch,
    sceneImportLoading,
    send,
    removeScenario,
    removeCharacter,
    removeCollider,
    removeExecutionArea,
    setWorldSize,
    setGridVisible,
    setGridCellSize,
    setTargetFps,
    setDirectionalLight,
    setBackground,
    loadSprite,
    removeSprite,
    blueprints,
    projectLoaded2dSeq,
    projectLoaded2dMetaRef,
    projectLoaded3dSeq,
    projectLoaded3dMetaRef,
  } = useContextEngine();

  const { openModal, closeModal } = useModal();
  const useMotorScenes = motorManagesScenes(projectType, gameStyle);
  const openModalRef = useRef(openModal);
  const onSaveProjectRef = useRef(onSaveProject);
  const tRef = useRef(t);
  openModalRef.current = openModal;
  onSaveProjectRef.current = onSaveProject;
  tRef.current = t;

  const initialSceneState = useMemo(() => buildInitialSceneState(), []);

  const [scenes, setScenes] = useState<SceneTab[]>(initialSceneState.tabs);
  const [sceneDataById, setSceneDataById] = useState<Record<number, SavedScene>>(initialSceneState.dataById);
  const [activeSceneId, setActiveSceneId] = useState(initialSceneState.activeSceneId);
  const [switchingToSceneId, setSwitchingToSceneId] = useState<number | null>(null);
  const scenesListLoading = sceneImportLoading && switchingToSceneId === null;
  const sceneActionsDisabled = switchingToSceneId !== null || scenesListLoading;
  const pendingScenesTabsRef = useRef<SceneTab[] | null>(null);
  const scenesListLoadingRef = useRef(scenesListLoading);
  scenesListLoadingRef.current = scenesListLoading;
  const lastProjectLoaded2dSeq = useRef(0);
  const lastProjectLoaded3dSeq = useRef(0);

  const applySceneTabs = (tabs: SceneTab[]) => {
    if (scenesListLoadingRef.current) {
      pendingScenesTabsRef.current = tabs;
      return;
    }
    setScenes(tabs);
  };

  useEffect(() => {
    if (!scenesListLoading) return;
    setScenes([]);
  }, [scenesListLoading]);

  useEffect(() => {
    if (sceneImportLoading) return;

    const pending = pendingScenesTabsRef.current;
    if (pending != null && pending.length > 0) {
      setScenes(pending);
      pendingScenesTabsRef.current = null;
      return;
    }

    if (projectType === '2D') {
      const meta = projectLoaded2dMetaRef.current;
      if (!meta) return;
      const tabs = meta.scenes?.length
        ? meta.scenes
        : [{ id: meta.activeSceneId, name: meta.sceneName }];
      setScenes(tabs.map((tab) => ({ id: tab.id, name: tab.name })));
      return;
    }

    if (projectType === '3D') {
      const meta = projectLoaded3dMetaRef.current;
      if (!meta) return;
      const tabs = meta.scenes?.length
        ? meta.scenes
        : [{ id: meta.activeSceneId, name: meta.sceneName }];
      setScenes(tabs.map((tab) => ({ id: tab.id, name: tab.name })));
    }
  }, [
    sceneImportLoading,
    projectType,
    projectLoaded2dMetaRef,
    projectLoaded3dMetaRef,
    projectLoaded2dSeq,
    projectLoaded3dSeq,
  ]);

  useEffect(() => {
    if (!sceneImportLoading && switchingToSceneId != null) {
      setSwitchingToSceneId(null);
    }
  }, [sceneImportLoading, switchingToSceneId]);

  useEffect(() => {
    if (!useMotorScenes) return;

    const onEngineEvent = (event: EngineEvent) => {
      if (
        event.event === 'editor_scene_created'
        || event.event === 'editor_scene_switched'
        || event.event === 'editor_scenes_updated'
      ) {
        const items = (event.scenes as EditorSceneListItem[] | undefined) ?? [];
        const activeId = event.active_scene_id as number | undefined;
        if (items.length > 0) {
          const tabs = tabsFromMotorItems(items);
          if (scenesListLoadingRef.current) {
            pendingScenesTabsRef.current = tabs;
          } else {
            setScenes(tabs);
          }
        }
        if (activeId != null && !Number.isNaN(activeId)) {
          setActiveSceneId(activeId);
        }
        if (event.event === 'editor_scene_switched') {
          setSwitchingToSceneId(null);
          if (activeId != null) {
            reloadActiveSceneVisualScript(activeId);
          }
        }
      }
      if (event.event === 'editor_scene_switch_blocked') {
        setSwitchingToSceneId(null);
        if (sceneImportInProgressRef.current) {
          endSceneImportLoading(
            dispatch,
            sceneImportInProgressRef,
            pendingImportSceneRef,
            sceneBurstLoadInProgressRef,
            modelReplaceInProgressRef,
            reportBounds,
          );
        }
        const reason = event.reason as string | undefined;
        if (reason !== 'unsaved_changes') return;
        openModalRef.current({
          title: tRef.current('Unsaved scene'),
          size: 'sm',
          body: (
            <UnsavedSceneBlockedBody
              onSave={() => {
                void onSaveProjectRef.current?.();
              }}
            />
          ),
        });
      }
    };

    window.engine.on(onEngineEvent);
    return () => {
      window.engine.off(onEngineEvent);
    };
  }, [useMotorScenes]);

  useEffect(() => {
    if (projectType !== '2D' || projectLoaded2dSeq === 0) return;
    if (projectLoaded2dSeq === lastProjectLoaded2dSeq.current) return;
    lastProjectLoaded2dSeq.current = projectLoaded2dSeq;

    const meta = projectLoaded2dMetaRef.current;
    if (!meta) return;

    const tabs = meta.scenes?.length
      ? meta.scenes
      : [{ id: meta.activeSceneId, name: meta.sceneName }];
    applySceneTabs(tabs.map((tab) => ({ id: tab.id, name: tab.name })));
    setActiveSceneId(meta.activeSceneId);

    const dataById: Record<number, SavedScene> = {};
    for (const tab of tabs) {
      const isActive = tab.id === meta.activeSceneId;
      dataById[tab.id] = {
        id: tab.id,
        name: tab.name,
        world: { ...DEFAULT_WORLD, ...meta.world },
        backgroundPath: isActive ? meta.backgroundPath : null,
        entities: [],
        player: null,
    config_camera: null,
    config_editor_camera: null,
        camera2d: isActive ? meta.camera2d : null,
        sprites: isActive ? meta.sprites : [],
      };
    }
    setSceneDataById(dataById);
  }, [projectType, projectLoaded2dSeq, projectLoaded2dMetaRef, scenesListLoading]);

  useEffect(() => {
    if (projectType !== '3D' || projectLoaded3dSeq === 0) return;
    if (projectLoaded3dSeq === lastProjectLoaded3dSeq.current) return;
    const isInitialProjectOpen = lastProjectLoaded3dSeq.current === 0;
    lastProjectLoaded3dSeq.current = projectLoaded3dSeq;

    // 3D FP: el motor manda la lista con editor_scene_*; solo hidratar React en la primera apertura.
    if (useMotorScenes && !isInitialProjectOpen) return;

    const meta = projectLoaded3dMetaRef.current;
    if (!meta) return;

    const tabs = meta.scenes?.length
      ? meta.scenes
      : [{ id: meta.activeSceneId, name: meta.sceneName }];
    applySceneTabs(tabs.map((tab) => ({ id: tab.id, name: tab.name })));
    setActiveSceneId(meta.activeSceneId);

    const dataById: Record<number, SavedScene> = {};
    for (const tab of tabs) {
      const isActive = tab.id === meta.activeSceneId;
      dataById[tab.id] = {
        id: tab.id,
        name: tab.name,
        world: { ...DEFAULT_WORLD, ...meta.world },
        backgroundPath: null,
        entities: [],
        player: isActive ? (meta.player ?? null) : null,
        config_camera: isActive ? (meta.config_camera ?? null) : null,
        config_editor_camera: isActive ? (meta.config_editor_camera ?? null) : null,
        camera2d: null,
        sprites: [],
        models: isActive ? meta.models : [],
      };
    }
    setSceneDataById(dataById);
  }, [projectType, projectLoaded3dSeq, projectLoaded3dMetaRef, useMotorScenes, scenesListLoading]);

  const captureActiveSceneSnapshot = async (id: number, name: string): Promise<SavedScene> =>
    buildActiveSceneSnapshotFromEngine(id, name, entityMetaRef.current);

  const clearEngineBeforeSceneLoad = () => {
    if (projectType === '2D') return;
    beginSceneWorldCleanup(sceneWorldCleanupRef);
    clearCurrentSceneInEngine();
    scheduleEndSceneWorldCleanup(sceneWorldCleanupRef);
  };

  const clearCurrentSceneInEngine = () => {
    const playerId = playerEntityIdRef.current;
    if (playerId != null) {
      removeCharacter(playerId);
    }

    for (const scenario of scenarioEntities) {
      removeScenario(scenario.id);
    }

    for (const character of characterEntities) {
      if (character.id === playerId) continue;
      removeCharacter(character.id);
    }

    for (const collider of colliderEntities) {
      removeCollider(collider.id);
    }

    for (const executionArea of executionAreaEntities) {
      removeExecutionArea(executionArea.id);
    }

    for (const [idStr, meta] of Object.entries(entityMetaRef.current)) {
      if (meta.kind !== 'model') continue;
      const id = Number(idStr);
      send({ cmd: 'remove_entity', id });
      delete entityMetaRef.current[id];
      delete entityTransformsRef.current[id];
    }

    for (const [path] of loadedSpritesInfo.entries()) {
      removeSprite(path);
    }
  };

  const loadSceneIntoEngine = (scene: SavedScene) => {
    send({ cmd: 'set_preview_playing', playing: false });
    mainPlayerHandled.current = false;
    playerEntityIdRef.current = null;
    if (
      gameStyle === 'first-person' &&
      projectType === '3D' &&
      scene.player &&
      scene.config_camera
    ) {
      const view = playViewFromPlayerAndCamera(scene.player, scene.config_camera);
      pendingPlayCharacterViewRef.current = view;
      playCharacterViewRef.current = view;
    } else {
      pendingPlayCharacterViewRef.current = null;
      playCharacterViewRef.current = null;
    }

    const burstLoad =
      projectType !== '2D' && needsSceneBurstLoad(projectType, gameStyle, scene);
    const deferDirectionalLight =
      burstLoad && (scene.entities?.length ?? 0) > 0;

    if (projectType === '2D') {
      dispatch({
        type: 'SET_WORLD_CONFIG',
        payload: {
          ...scene.world,
          gravity: scene.world.gravity ?? DEFAULT_GRAVITY_MAGNITUDE,
        },
      });
      camera2dRef.current = scene.camera2d ?? { x: 0, y: 0, halfH: 3.5 };
    } else {
      setWorldSize(
        scene.world.worldWidth,
        scene.world.worldHeight,
        scene.world.worldDepth ?? DEFAULT_WORLD.worldDepth,
      );
      setGridVisible(scene.world.gridVisible);
      setGridCellSize(scene.world.gridCellSize);
      setTargetFps(Number.isFinite(scene.world?.targetFps) ? scene.world.targetFps : DEFAULT_WORLD.targetFps);
      if (!deferDirectionalLight) {
        setDirectionalLight({
          ambient: scene.world.lightAmbient ?? DEFAULT_LIGHT_AMBIENT,
          intensity: scene.world.lightIntensity ?? DEFAULT_LIGHT_INTENSITY,
          shadowDarkness: scene.world.shadowDarkness ?? DEFAULT_SHADOW_DARKNESS,
        });
      }

      if (scene.camera2d) {
        send({ cmd: 'set_camera2d', x: scene.camera2d.x, y: scene.camera2d.y, half_h: scene.camera2d.halfH });
        camera2dRef.current = scene.camera2d;
      }
    }

    if (projectType === '2D') {
      dispatch({ type: 'SET_BACKGROUND', payload: scene.backgroundPath });
    } else if (scene.backgroundPath != null) {
      setBackground(scene.backgroundPath);
    }

    if (projectType !== '2D') {
      for (const sprite of scene.sprites ?? []) {
        loadSprite(sprite.path, sprite.name);
      }
    }

    if (projectType === '2D') {
      pendingRestoresRef.current.clear();
      pendingModelLoadQueueRef.current = [];
      pendingImportSceneRef.current = scene;
      beginSceneImportLoading(dispatch, sceneImportInProgressRef);
      send(buildImportSceneCommand(scene, blueprints) as never);
      return;
    }

    pendingRestoresRef.current.clear();
    pendingModelLoadQueueRef.current = [];

    if (
      projectType === '3D'
      && gameStyle === 'first-person'
      && (scene.entities?.length ?? 0) === 0
    ) {
      beginFpSceneBaselineLogging(fpSceneBaselineLogRef);
    }

    if (burstLoad) {
      sceneBurstPendingColliderCountRef.current = 0;
      beginSceneBurstLoad(dispatch, sceneBurstLoadInProgressRef, {
        sceneBurstPendingOpsRef,
        pendingBurstSpawnRestoreRef,
      });
    }

    if (scene.models?.length) {
      dispatch({ type: 'SET_MODELS', payload: scene.models });
    }

    for (const entity of scene.entities) {
      const spawnPath = entity3dSpawnPath(entity);
      const transform = entity3dTransform(entity);
      const pendingRestore = entity3dPendingRestore(entity, blueprints);

      if (entity.category === 'player' || isPlayerPath(spawnPath)) {
        continue;
      }

      if (isEditorCameraPath(spawnPath)) {
        continue;
      }

      if (entity.category === 'sun' || isSunPath(spawnPath)) {
        const sunQueue = pendingRestoresRef.current.get('[Sun]') ?? [];
        pendingRestoresRef.current.set('[Sun]', sunQueue);
        sunQueue.push({
          transform,
          name: entity.name,
          physicsEnabled: false,
          physicsType: 'static',
          scripts: entity.scripts,
          controlBindings: entity.controls,
        });
        if (burstLoad) trackSceneBurstOp({ sceneBurstPendingOpsRef });
        send({
          cmd: 'spawn_sun',
          name: entity.name ?? '',
          position: entity.position,
          scale: entity.scale,
        });
        continue;
      }

      if (isGroundPath(spawnPath)) {
        const groundQueue = pendingRestoresRef.current.get('[Ground]') ?? [];
        pendingRestoresRef.current.set('[Ground]', groundQueue);
        groundQueue.push({
          transform,
          name: entity.name,
          physicsEnabled: false,
          physicsType: 'static',
          scripts: entity.scripts,
          controlBindings: entity.controls,
        });
        if (burstLoad) trackSceneBurstOp({ sceneBurstPendingOpsRef });
        send({
          cmd: 'spawn_ground',
          position: entity.position,
          scale: entity.scale,
        });
        continue;
      }

      if (isEditorBoxPath(spawnPath)) {
        const boxQueue = pendingRestoresRef.current.get('[EditorBox]') ?? [];
        pendingRestoresRef.current.set('[EditorBox]', boxQueue);
        boxQueue.push({
          transform,
          name: entity.name,
          physicsEnabled: pendingRestore.physicsEnabled,
          physicsType: pendingRestore.physicsType,
          scripts: entity.scripts,
          controlBindings: entity.controls,
          blueprintId: entity.blueprint_id,
        });
        if (burstLoad) trackSceneBurstOp({ sceneBurstPendingOpsRef });
        send({
          cmd: 'spawn_editor_box',
          name: entity.name ?? '',
          position: entity.position,
          scale: entity.scale,
        });
        continue;
      }

      if (!is3dModelFileEntity(projectType, { path: spawnPath })) {
        continue;
      }

      const modelPath = entity.model;
      const queue = pendingRestoresRef.current.get(modelPath) ?? [];
      pendingRestoresRef.current.set(modelPath, queue);
      queue.push(pendingRestore);

      pendingModelLoadQueueRef.current.push({
        modelPath,
        pending: pendingRestore,
      });
      if (!burstLoad) {
        send({
          cmd: 'load_model',
          path: modelPath,
          single_instance: true,
          ...(pendingRestore.entityCategory ? { entity_category: pendingRestore.entityCategory } : {}),
        });
      }
    }

    if (burstLoad && pendingModelLoadQueueRef.current.length > 0) {
      const preloadedPaths = (scene.models ?? []).map((model) => model.path);
      const queuedPaths = pendingModelLoadQueueRef.current.map((item) => item.modelPath);
      const cachedPreloadCount = countCachedBurstModelPreloads(scene.models, queuedPaths);
      if (cachedPreloadCount > 0) {
        trackSceneBurstModelPreloads({ sceneBurstPendingOpsRef }, cachedPreloadCount);
        kickCachedBurstModelSpawns(
          scene.models,
          pendingModelLoadQueueRef.current,
          (cmd) => send(cmd as never),
          { sceneBurstPendingOpsRef, pendingBurstSpawnRestoreRef },
        );
      }
      const extraPaths = collectUncachedBurstModelPaths(queuedPaths, preloadedPaths);
      if (extraPaths.size > 0) {
        trackSceneBurstModelPreloads({ sceneBurstPendingOpsRef }, extraPaths.size);
      }
    }

    if (gameStyle === 'first-person' && projectType === '3D') {
      ensurePlayCharacterOnLoad(scene, pendingRestoresRef, send, {
        onBurstOp: burstLoad ? () => trackSceneBurstOp({ sceneBurstPendingOpsRef }) : undefined,
      });
    }

    if (deferDirectionalLight) {
      setDirectionalLight({
        ambient: scene.world.lightAmbient ?? DEFAULT_LIGHT_AMBIENT,
        intensity: scene.world.lightIntensity ?? DEFAULT_LIGHT_INTENSITY,
        shadowDarkness: scene.world.shadowDarkness ?? DEFAULT_SHADOW_DARKNESS,
      });
    }

    if (burstLoad) {
      setTimeout(() => {
        tryEndSceneBurstLoad(
          dispatch,
          sceneBurstLoadInProgressRef,
          {
            pendingRestoresRef,
            pendingModelLoadQueueRef,
            pendingBurstSpawnRestoreRef,
            pendingPlayCharacterViewRef,
            mainPlayerHandled,
            playerEntityIdRef,
            sceneBurstPendingColliderCountRef,
            sceneBurstPendingOpsRef,
          },
          sceneImportInProgressRef,
          modelReplaceInProgressRef,
          reportBounds,
        );
      }, 0);
      return;
    }

    if (projectType !== '2D') {
      queueMicrotask(() => {
        syncEditorStateFromSavedScene(
          scene,
          {
            entityMetaRef,
            entityTransformsRef,
            camera2dRef,
            playerEntityIdRef,
            editorCameraEntityIdRef,
            mainPlayerHandled,
            playerRemoved,
          },
          dispatch,
          blueprints,
        );
        endSceneImportLoading(
          dispatch,
          sceneImportInProgressRef,
          pendingImportSceneRef,
          sceneBurstLoadInProgressRef,
          modelReplaceInProgressRef,
          reportBounds,
        );
        dispatch({ type: 'SYNC_PLAY_CHARACTER_VIEW' });
      });
    }
  };

  const getNextSceneId = () => {
    if (scenes.length === 0) return 1;
    return Math.max(...scenes.map((scene) => scene.id)) + 1;
  };

  const createScene = async (name: string) => {
    const trimmed = name.trim();
    if (!trimmed) return;

    if (useMotorScenes) {
      send({ cmd: 'create_editor_scene', name: trimmed } as never);
      return;
    }

    const current = scenes.find((scene) => scene.id === activeSceneId);
    if (current) {
      const snapshot = await captureActiveSceneSnapshot(current.id, current.name);
      setSceneDataById((prev) => ({ ...prev, [current.id]: snapshot }));
    }

    const nextId = getNextSceneId();
    const nextScene: SceneTab = { id: nextId, name: trimmed };
    const emptyScene: SavedScene = {
      id: nextId,
      name: trimmed,
      world: { ...worldConfig },
      backgroundPath: null,
      entities: [],
      player: null,
    config_camera: null,
    config_editor_camera: null,
      camera2d: camera2dRef.current,
      sprites: [],
    };

    setScenes((prev) => [...prev, nextScene]);
    setSceneDataById((prev) => ({ ...prev, [nextId]: emptyScene }));
  };

  const renameScene = (sceneId: number, nextName: string) => {
    const trimmed = nextName.trim();
    if (!trimmed) return;
    setScenes((prev) => prev.map((scene) => (scene.id === sceneId ? { ...scene, name: trimmed } : scene)));
  };

  const deleteScene = (sceneId: number) => {
    if (scenes.length <= 1) return;

    if (useMotorScenes) {
      send({ cmd: 'delete_editor_scene', scene_id: sceneId } as never);
      return;
    }

    const remaining = scenes.filter((scene) => scene.id !== sceneId);
    setScenes(remaining);
    setSceneDataById((prev) => {
      const next = { ...prev };
      delete next[sceneId];
      return next;
    });

    if (activeSceneId !== sceneId) return;

    const nextActive = remaining[0];
    if (!nextActive) return;

    const targetData = sceneDataById[nextActive.id];
    if (targetData) {
      clearEngineBeforeSceneLoad();
      loadSceneIntoEngine(targetData);
    }
    setActiveSceneId(nextActive.id);
  };

  const loadScene = async (nextId: number) => {
    if (Number.isNaN(nextId) || nextId === activeSceneId || sceneActionsDisabled) return;

    setSwitchingToSceneId(nextId);

    if (useMotorScenes) {
      beginSceneImportLoading(dispatch, sceneImportInProgressRef);
      send({ cmd: 'switch_editor_scene', scene_id: nextId } as never);
      return;
    }

    const current = scenes.find((scene) => scene.id === activeSceneId);
    const target = scenes.find((scene) => scene.id === nextId);
    if (!target) {
      setSwitchingToSceneId(null);
      return;
    }

    const currentSnapshot = current
      ? await captureActiveSceneSnapshot(current.id, current.name)
      : null;

    if (currentSnapshot) {
      if (!currentSnapshot.backgroundPath && backgroundPath) {
        currentSnapshot.backgroundPath = backgroundPath;
      }
      if (!currentSnapshot.camera2d && camera2dRef.current) {
        currentSnapshot.camera2d = camera2dRef.current;
      }
    }

    const targetSnapshot = sceneDataById[nextId] ?? {
      id: target.id,
      name: target.name,
      world: { ...worldConfig },
      backgroundPath: null,
      entities: [],
      player: null,
    config_camera: null,
    config_editor_camera: null,
      camera2d: camera2dRef.current,
      sprites: [],
    };

    if (current && currentSnapshot) {
      setSceneDataById((prev) => ({ ...prev, [current.id]: currentSnapshot }));
    }

    try {
      clearEngineBeforeSceneLoad();
      loadSceneIntoEngine(targetSnapshot);
      setActiveSceneId(nextId);
    } finally {
      setSwitchingToSceneId(null);
    }
  };

  useEffect(() => {
    const orderedScenes: SavedScene[] = scenes
      .map((tab) => {
        const scene = sceneDataById[tab.id];
        if (!scene) {
          return {
            id: tab.id,
            name: tab.name,
            world: { ...worldConfig },
            backgroundPath: null,
            entities: [],
            player: null,
    config_camera: null,
    config_editor_camera: null,
            camera2d: camera2dRef.current,
            sprites: [],
          };
        }
        return { ...scene, id: tab.id, name: tab.name };
      });

    setSceneProjectState({ scenes: orderedScenes, activeSceneId });
  }, [activeSceneId, camera2dRef, sceneDataById, scenes, worldConfig]);

  useEffect(() => {
    reloadActiveSceneVisualScript(activeSceneId);
  }, [activeSceneId]);

  useEffect(() => {
    if (!initialExtractDir?.trim()) return;
    let cancelled = false;
    void window.electronAPI.readProjectManifest().then((data) => {
      if (cancelled || !data || !('version' in data)) return;
      const scenesFromSave = data.scenes;
      if (!scenesFromSave?.length) return;
      const tabs = scenesFromSave.map((s) => ({ id: s.id, name: s.name }));
      applySceneTabs(tabs);
      const activeId = data.activeSceneId ?? tabs[0]?.id ?? 1;
      setActiveSceneId(activeId);
      const byId: Record<number, SavedScene> = {};
      for (const scene of scenesFromSave) {
        byId[scene.id] = scene;
      }
      setSceneDataById(byId);
      reloadActiveSceneVisualScript(activeId);
    });
    return () => {
      cancelled = true;
    };
  }, [initialExtractDir]);

  const persistSceneVisualGraph = (
    sceneId: number,
    graph: VisualGraphDocument,
  ): { ok: boolean; errors: string[] } => {
    const result = saveSceneVisualGraph(sceneId, graph);
    if (!result.ok) return { ok: false, errors: result.errors };
    setSceneDataById((prev) => {
      const existing = prev[sceneId];
      const tab = scenes.find((s) => s.id === sceneId);
      const base: SavedScene = existing ?? {
        id: sceneId,
        name: tab?.name ?? defaultSceneName(sceneId),
        world: { ...worldConfig },
        backgroundPath: null,
        entities: [],
        player: null,
        config_camera: null,
        config_editor_camera: null,
        camera2d: camera2dRef.current,
        sprites: [],
      };
      return {
        ...prev,
        [sceneId]: {
          ...base,
          visualGraph: graph,
          visualScriptRhai: result.rhaiSource,
        },
      };
    });
    return { ok: true, errors: [] };
  };

  const openVisualScriptingModal = (sceneId?: number) => {
    const id = sceneId ?? activeSceneId;
    const sceneName = scenes.find((s) => s.id === id)?.name;
    const sceneData = sceneDataById[id];
    const initialGraph = getSceneVisualGraph(id) ?? sceneData?.visualGraph;
    const sceneEntities = resolveSceneEntitiesForVisualScript({
      savedEntities: sceneData?.entities,
      savedPlayer: sceneData?.player,
      entityMeta: entityMetaRef.current,
      entityTransforms: entityTransformsRef.current,
    });
    openModal({
      title: t('Scene logic'),
      size: 'xl',
      body: (
        <VisualScriptingModalBody
          context="scene"
          sceneId={id}
          sceneName={sceneName}
          sceneEntities={sceneEntities}
          blueprints={blueprints}
          initialGraph={initialGraph}
          onSave={(graph) => {
            const saveResult = persistSceneVisualGraph(id, graph);
            if (!saveResult.ok) {
              return { ok: false, errors: saveResult.errors };
            }
            closeModal();
            return { ok: true };
          }}
          onCancel={closeModal}
        />
      ),
    });
  };

  const openSceneScriptEditor = (sceneId?: number) => {
    const id = sceneId ?? activeSceneId;
    const sceneName = scenes.find((s) => s.id === id)?.name;
    const initialSource =
      getSceneScriptRhai(id) ?? sceneDataById[id]?.sceneScriptRhai ?? '';
    openModal({
      title: sceneName ? `${t('Scene script')}: ${sceneName}` : t('Scene script'),
      size: 'lg',
      body: (
        <SceneScriptEditorModalBody
          initialSource={initialSource}
          onSave={(source) => {
            saveSceneScriptRhai(id, source);
            setSceneDataById((prev) => {
              const existing = prev[id];
              const tab = scenes.find((s) => s.id === id);
              const base: SavedScene = existing ?? {
                id,
                name: tab?.name ?? defaultSceneName(id),
                world: { ...worldConfig },
                backgroundPath: null,
                entities: [],
                player: null,
                config_camera: null,
                config_editor_camera: null,
                camera2d: camera2dRef.current,
                sprites: [],
              };
              return {
                ...prev,
                [id]: { ...base, sceneScriptRhai: source },
              };
            });
            closeModal();
          }}
          onCancel={closeModal}
        />
      ),
    });
  };

  const openUnsavedSceneBlockedModal = () => {
    openModal({
      title: t('Unsaved scene'),
      size: 'sm',
      body: (
        <UnsavedSceneBlockedBody
          onSave={() => {
            void onSaveProjectRef.current?.();
          }}
        />
      ),
    });
  };

  const activeSceneHasUnsavedChanges = (): boolean => {
    if (!useMotorScenes) return false;
    return scenes.find((s) => s.id === activeSceneId)?.dirty === true;
  };

  const openSwitchSceneModal = (scene: SceneTab) => {
    if (sceneActionsDisabled || scene.id === activeSceneId) return;
    if (activeSceneHasUnsavedChanges()) {
      openUnsavedSceneBlockedModal();
      return;
    }
    openModal({
      title: t('Load scene'),
      size: 'sm',
      body: (
        <SwitchSceneConfirmBody
          sceneName={scene.name}
          onConfirm={() => {
            void loadScene(scene.id);
          }}
        />
      ),
    });
  };

  const openCreateSceneModal = () => {
    if (sceneActionsDisabled) return;
    const nextId = getNextSceneId();
    openModal({
      title: t('Create new scene'),
      size: 'sm',
      body: (
        <CreateSceneModalBody
          defaultName={defaultSceneName(nextId)}
          onCreate={(name) => {
            void createScene(name);
          }}
        />
      ),
    });
  };

  const openRenameSceneModal = (scene: SceneTab) => {
    if (sceneActionsDisabled) return;
    openModal({
      title: `${t('Edit')} ${scene.name}`,
      body: (
        <SceneRenameModalBody
          defaultName={scene.name}
          onRename={(name) => renameScene(scene.id, name)}
        />
      ),
    });
  };

  const openDeleteSceneModal = (scene: SceneTab) => {
    if (sceneActionsDisabled) return;
    if (scenes.length <= 1) {
      openModal({
        title: `${t('Cannot delete')} ${scene.name}`,
        body: <DeleteBlockedBody />,
      });
      return;
    }

    openModal({
      title: `${t('Delete')} ${scene.name}`,
      body: (
        <DeleteConfirmBody
          onConfirm={() => deleteScene(scene.id)}
        />
      ),
    });
  };

  const value: SceneManagerContextValue = {
    scenes,
    activeSceneId,
    scenesListLoading,
    switchingToSceneId,
    sceneActionsDisabled,
    openSwitchSceneModal,
    openCreateSceneModal,
    openRenameSceneModal,
    openDeleteSceneModal,
    openVisualScriptingModal,
    openSceneScriptEditor,
    persistSceneVisualGraph,
  };

  return (
    <SceneManagerContext.Provider value={value}>
      {children}
    </SceneManagerContext.Provider>
  );
}

export function useSceneManager(): SceneManagerContextValue {
  const ctx = useContext(SceneManagerContext);
  if (!ctx) {
    throw new Error('useSceneManager must be used within SceneManagerProvider');
  }
  return ctx;
}
