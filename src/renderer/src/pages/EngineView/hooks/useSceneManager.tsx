import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';

import type { GameStyle, ProjectSaveData, SavedScene, SavedWorldConfig } from '@shared-types';
import {
  DEFAULT_GRAVITY_MAGNITUDE,
  DEFAULT_LIGHT_AMBIENT,
  DEFAULT_LIGHT_INTENSITY,
  DEFAULT_SHADOW_DARKNESS,
} from '@shared-types';
import { isEditorBoxPath, isGroundPath, isPlayerPath, isSunPath } from '@shared-types';
import { buildActiveSceneSnapshotFromEngine } from '../../../defaults/buildProjectSaveFromEngine';
import { requestEngineDefaultSceneName } from '../../../defaults/requestEngineDefaultSceneName';
import { ensurePlayCharacterOnLoad } from '../../../defaults/playCharacterSceneRestore';
import { setSceneCommandForSavedProject } from '../../../defaults/projectSceneLoad';
import { buildImportSceneCommand, resolveEntityTransform } from '../../../context/useContextEngine/hooks/buildImportSceneCommand';
import {
  beginSceneBurstLoad,
  beginSceneImportLoading,
  needsSceneBurstLoad,
  trackSceneBurstCollider,
  tryEndSceneBurstLoad,
} from '../../../context/useContextEngine/hooks/sceneImportOverlay';
import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';
import { setSceneProjectState } from '../sceneStateStore';

export interface SceneTab {
  id: number;
  name: string;
}

interface SceneManagerContextValue {
  scenes: SceneTab[];
  activeSceneId: number;
  loadScene: (sceneId: number) => Promise<void>;
  openCreateSceneModal: () => void;
  openRenameSceneModal: (scene: SceneTab) => void;
  openDeleteSceneModal: (scene: SceneTab) => void;
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

function buildInitialSceneState(initialSave?: ProjectSaveData | null) {
  const save = initialSave;
  if (save?.scenes && save.scenes.length > 0) {
    const tabs = save.scenes.map((scene) => ({ id: scene.id, name: scene.name }));
    const dataById: Record<number, SavedScene> = {};
    for (const scene of save.scenes) {
      dataById[scene.id] = {
        ...scene,
        world: { ...DEFAULT_WORLD, ...(scene.world ?? {}) },
      };
    }
    const active = save.activeSceneId && dataById[save.activeSceneId]
      ? save.activeSceneId
      : save.scenes[0].id;
    return { tabs, dataById, activeSceneId: active };
  }

  const legacyScene: SavedScene = {
    id: 1,
    name: '',
    world: { ...DEFAULT_WORLD, ...(save?.world ?? {}) },
    backgroundPath: save?.backgroundPath ?? null,
    entities: save?.entities ?? [],
    playerTransform: save?.playerTransform ?? null,
    camera2d: save?.camera2d ?? null,
    sprites: save?.sprites ?? [],
  };
  return {
    tabs: [{ id: 1, name: '' }],
    dataById: { 1: legacyScene },
    activeSceneId: 1,
  };
}

export function SceneManagerProvider({
  children,
  initialSave,
  projectType,
  gameStyle,
}: {
  children: ReactNode;
  initialSave?: ProjectSaveData | null;
  projectType?: string;
  gameStyle?: GameStyle;
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
    pendingPlayCharacterViewRef,
    playCharacterViewRef,
    mainPlayerHandled,
    playerEntityIdRef,
    camera2dRef,
    pendingImportSceneRef,
    sceneImportInProgressRef,
    modelReplaceInProgressRef,
    sceneBurstLoadInProgressRef,
    sceneBurstAwaitingPlayerViewRef,
    sceneBurstPendingColliderCountRef,
    reportBounds,
    dispatch,
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
    loadModelAsset,
    removeSprite,
    blueprints,
  } = useContextEngine();

  const { openModal, closeModal } = useModal();

  const initialSceneState = useMemo(
    () => buildInitialSceneState(initialSave),
    [initialSave],
  );

  const [scenes, setScenes] = useState<SceneTab[]>(initialSceneState.tabs);
  const [sceneDataById, setSceneDataById] = useState<Record<number, SavedScene>>(initialSceneState.dataById);
  const [activeSceneId, setActiveSceneId] = useState(initialSceneState.activeSceneId);

  const pendingSceneNameIds = useMemo(
    () => scenes.filter((tab) => !tab.name.trim()).map((tab) => tab.id).join(','),
    [scenes],
  );

  useEffect(() => {
    if (!engineReady || !pendingSceneNameIds) return;

    const ids = pendingSceneNameIds.split(',').map(Number).filter((id) => !Number.isNaN(id));
    if (ids.length === 0) return;

    let cancelled = false;
    void (async () => {
      try {
        const resolved = await Promise.all(
          ids.map(async (id) => ({
            id,
            name: await requestEngineDefaultSceneName(id),
          })),
        );
        if (cancelled) return;

        setScenes((prev) => prev.map((tab) => {
          const match = resolved.find((entry) => entry.id === tab.id);
          return match ? { ...tab, name: match.name } : tab;
        }));
        setSceneDataById((prev) => {
          const next = { ...prev };
          for (const entry of resolved) {
            if (next[entry.id]) {
              next[entry.id] = { ...next[entry.id], name: entry.name };
            }
          }
          return next;
        });
      } catch (err) {
        console.error('[scenes] no se pudo obtener nombre por defecto del motor:', err);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [engineReady, pendingSceneNameIds]);

  const captureActiveSceneSnapshot = async (id: number, name: string): Promise<SavedScene> =>
    buildActiveSceneSnapshotFromEngine(id, name, entityMetaRef.current);

  const clearEngineBeforeSceneLoad = () => {
    if (projectType === '2D') return;
    clearCurrentSceneInEngine();
  };

  const clearCurrentSceneInEngine = () => {
    for (const scenario of scenarioEntities) {
      removeScenario(scenario.id);
    }

    for (const character of characterEntities) {
      if (character.path === '[Player]') continue;
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
    if (gameStyle === 'first-person' && projectType === '3D' && scene.playerTransform) {
      pendingPlayCharacterViewRef.current = scene.playerTransform;
      playCharacterViewRef.current = scene.playerTransform;
    } else {
      pendingPlayCharacterViewRef.current = null;
      playCharacterViewRef.current = null;
    }
    if (projectType !== '2D') {
      send({ cmd: 'set_scene', scene: setSceneCommandForSavedProject(projectType) });
    }

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
      setDirectionalLight({
        ambient: scene.world.lightAmbient ?? DEFAULT_LIGHT_AMBIENT,
        intensity: scene.world.lightIntensity ?? DEFAULT_LIGHT_INTENSITY,
        shadowDarkness: scene.world.shadowDarkness ?? DEFAULT_SHADOW_DARKNESS,
      });

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

    for (const model of scene.models ?? []) {
      loadModelAsset(model.path, model.name);
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

    const burstLoad = needsSceneBurstLoad(projectType, gameStyle, scene);
    if (burstLoad) {
      sceneBurstAwaitingPlayerViewRef.current = false;
      sceneBurstPendingColliderCountRef.current = 0;
      beginSceneBurstLoad(dispatch, sceneBurstLoadInProgressRef);
    }

    for (const entity of scene.entities) {
      const transform = resolveEntityTransform(entity, blueprints);

      if (entity.kind === 'collider' && entity.points) {
        if (burstLoad) {
          trackSceneBurstCollider({ sceneBurstPendingColliderCountRef });
        }
        send({ cmd: 'create_collider_from_points', points: entity.points, track_undo: false });
        continue;
      }

      if (entity.kind === 'execution_area' && entity.points) {
        const eaQueue = pendingRestoresRef.current.get('[ExecutionArea]') ?? [];
        pendingRestoresRef.current.set('[ExecutionArea]', eaQueue);
        eaQueue.push({
          transform,
          name: entity.name,
          physicsEnabled: entity.physics_enabled ?? false,
          physicsType: entity.physics_type ?? 'static',
          scripts: entity.scripts,
          controlBindings: entity.control_bindings,
        });
        send({ cmd: 'create_execution_area_from_points', points: entity.points, track_undo: false });
        continue;
      }

      if (entity.kind === 'directional_light' || isSunPath(entity.path)) {
        const sunQueue = pendingRestoresRef.current.get('[Sun]') ?? [];
        pendingRestoresRef.current.set('[Sun]', sunQueue);
        sunQueue.push({
          transform,
          name: entity.name,
          physicsEnabled: false,
          physicsType: 'static',
          scripts: entity.scripts,
          controlBindings: entity.control_bindings,
        });
        send({
          cmd: 'spawn_sun',
          name: entity.name ?? '',
          position: entity.position,
          scale: entity.scale,
        });
        continue;
      }

      if (entity.kind === 'model' && isGroundPath(entity.path)) {
        const groundQueue = pendingRestoresRef.current.get('[Ground]') ?? [];
        pendingRestoresRef.current.set('[Ground]', groundQueue);
        groundQueue.push({
          transform,
          name: entity.name,
          physicsEnabled: false,
          physicsType: 'static',
          scripts: entity.scripts,
          controlBindings: entity.control_bindings,
        });
        send({
          cmd: 'spawn_ground',
          position: entity.position,
          scale: entity.scale,
        });
        continue;
      }

      if (entity.kind === 'model' && isEditorBoxPath(entity.path)) {
        const boxQueue = pendingRestoresRef.current.get('[EditorBox]') ?? [];
        pendingRestoresRef.current.set('[EditorBox]', boxQueue);
        boxQueue.push({
          transform,
          name: entity.name,
          physicsEnabled: entity.physics_enabled ?? false,
          physicsType: entity.physics_type ?? 'static',
          scripts: entity.scripts,
          controlBindings: entity.control_bindings,
          blueprintId: entity.blueprint_id,
        });
        send({
          cmd: 'spawn_editor_box',
          name: entity.name ?? '',
          position: entity.position,
          scale: entity.scale,
        });
        continue;
      }

      if (entity.kind === 'character' && isPlayerPath(entity.path)) {
        const playerQueue = pendingRestoresRef.current.get('[Player]') ?? [];
        pendingRestoresRef.current.set('[Player]', playerQueue);
        playerQueue.push({
          transform,
          name: entity.name,
          physicsEnabled: true,
          physicsType: 'dynamic',
          scripts: entity.scripts,
          controlBindings: entity.control_bindings,
          visualModelPath: entity.visual_model_path ?? scene.playerTransform?.visual_model_path,
        });
        send({ cmd: 'load_character', path: entity.path });
        continue;
      }

      const bp = entity.blueprint_id
        ? (blueprints ?? []).find((b) => b.id === entity.blueprint_id) ?? null
        : null;
      const pendingRestore = {
        transform,
        name: entity.name,
        physicsEnabled: bp?.physics_enabled ?? entity.physics_enabled ?? false,
        physicsType: bp?.physics_type ?? entity.physics_type ?? 'static',
        animations: bp?.animations ?? entity.animations,
        scripts: bp?.scripts ?? entity.scripts,
        controlBindings: bp?.control_bindings ?? entity.control_bindings,
        blueprintId: entity.blueprint_id,
        entityCategory: entity.entity_category,
        visualModelPath: entity.visual_model_path,
      };
      const queue = pendingRestoresRef.current.get(entity.path) ?? [];
      pendingRestoresRef.current.set(entity.path, queue);
      queue.push(pendingRestore);

      if (entity.kind === 'scenario') send({ cmd: 'load_scenario', path: entity.path });
      if (entity.kind === 'character') send({ cmd: 'load_character', path: entity.path });
      if (entity.kind === 'model' && entity.path && !isEditorBoxPath(entity.path)) {
        pendingModelLoadQueueRef.current.push({
          modelPath: entity.path,
          pending: pendingRestore,
        });
        send({ cmd: 'load_model', path: entity.path });
      }
    }

    if (gameStyle === 'first-person' && projectType === '3D') {
      ensurePlayCharacterOnLoad(scene, pendingRestoresRef, send);
    }

    if (burstLoad) {
      setTimeout(() => {
        tryEndSceneBurstLoad(
          dispatch,
          sceneBurstLoadInProgressRef,
          {
            pendingRestoresRef,
            pendingModelLoadQueueRef,
            pendingPlayCharacterViewRef,
            mainPlayerHandled,
            sceneBurstAwaitingPlayerViewRef,
            sceneBurstPendingColliderCountRef,
          },
          sceneImportInProgressRef,
          modelReplaceInProgressRef,
          reportBounds,
        );
      }, 0);
    }
  };

  const getNextSceneId = () => {
    if (scenes.length === 0) return 1;
    return Math.max(...scenes.map((scene) => scene.id)) + 1;
  };

  const createScene = async (name: string) => {
    const trimmed = name.trim();
    if (!trimmed) return;

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
      playerTransform: null,
      camera2d: camera2dRef.current,
      sprites: [],
    };

    clearEngineBeforeSceneLoad();
    loadSceneIntoEngine(emptyScene);

    setScenes((prev) => [...prev, nextScene]);
    setSceneDataById((prev) => ({ ...prev, [nextId]: emptyScene }));
    setActiveSceneId(nextId);
  };

  const renameScene = (sceneId: number, nextName: string) => {
    const trimmed = nextName.trim();
    if (!trimmed) return;
    setScenes((prev) => prev.map((scene) => (scene.id === sceneId ? { ...scene, name: trimmed } : scene)));
  };

  const deleteScene = (sceneId: number) => {
    if (scenes.length <= 1) return;

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
    if (Number.isNaN(nextId) || nextId === activeSceneId) return;

    const current = scenes.find((scene) => scene.id === activeSceneId);
    const target = scenes.find((scene) => scene.id === nextId);
    if (!target) return;

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
      playerTransform: null,
      camera2d: camera2dRef.current,
      sprites: [],
    };

    if (current && currentSnapshot) {
      setSceneDataById((prev) => ({ ...prev, [current.id]: currentSnapshot }));
    }

    clearEngineBeforeSceneLoad();
    loadSceneIntoEngine(targetSnapshot);
    setActiveSceneId(nextId);
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
            playerTransform: null,
            camera2d: camera2dRef.current,
            sprites: [],
          };
        }
        return { ...scene, id: tab.id, name: tab.name };
      });

    setSceneProjectState({ scenes: orderedScenes, activeSceneId });
  }, [activeSceneId, camera2dRef, sceneDataById, scenes, worldConfig]);

  const openCreateSceneModal = () => {
    void (async () => {
      const nextId = getNextSceneId();
      let draftName = '';
      try {
        draftName = await requestEngineDefaultSceneName(nextId);
      } catch (err) {
        console.error('[scenes] no se pudo obtener nombre por defecto del motor:', err);
        return;
      }

      openModal({
        title: t('Create new scene'),
        body: (
          <CreateSceneModalBody
            defaultName={draftName}
            onCancel={closeModal}
            onCreate={(name) => {
              void createScene(name);
              closeModal();
            }}
            t={t}
          />
        ),
      });
    })();
  };

  const openRenameSceneModal = (scene: SceneTab) => {
    let draftName = scene.name;

    openModal({
      title: `${t('Edit')} ${scene.name}`,
      body: (
        <div className="d-flex flex-column gap-3">
          <div>
            <label htmlFor="scene-name-rename" className="form-label mb-1">{t('Scene name')}</label>
            <input
              id="scene-name-rename"
              type="text"
              defaultValue={scene.name}
              className="form-control"
              onChange={(event) => {
                draftName = event.target.value;
              }}
            />
          </div>

          <div className="d-flex gap-2 flex-wrap">
            <button
              className="btn btn-success"
              onClick={() => {
                renameScene(scene.id, draftName);
                closeModal();
              }}
              type="button"
            >
              {t('Save name')}
            </button>
            <button className="btn btn-secondary" onClick={closeModal} type="button">{t('Cancel')}</button>
          </div>
        </div>
      ),
    });
  };

  const openDeleteSceneModal = (scene: SceneTab) => {
    if (scenes.length <= 1) {
      openModal({
        title: `${t('Cannot delete')} ${scene.name}`,
        body: <DeleteBlockedBody t={t} />,
      });
      return;
    }

    openModal({
      title: `${t('Delete')} ${scene.name}`,
      body: (
        <DeleteConfirmBody
          onCancel={closeModal}
          onConfirm={() => {
            deleteScene(scene.id);
            closeModal();
          }}
          t={t}
        />
      ),
    });
  };

  const value: SceneManagerContextValue = {
    scenes,
    activeSceneId,
    loadScene,
    openCreateSceneModal,
    openRenameSceneModal,
    openDeleteSceneModal,
  };

  return (
    <SceneManagerContext.Provider value={value}>
      {children}
    </SceneManagerContext.Provider>
  );
}

function CreateSceneModalBody({
  defaultName,
  onCancel,
  onCreate,
  t,
}: {
  defaultName: string;
  onCancel: () => void;
  onCreate: (name: string) => void;
  t: (key: string) => string;
}) {
  let draftName = defaultName;

  return (
    <CreateSceneModalBodyContent
      defaultName={defaultName}
      onCancel={onCancel}
      onCreate={() => onCreate(draftName)}
      onDraftChange={(value) => {
        draftName = value;
      }}
      t={t}
    />
  );
}

function CreateSceneModalBodyContent({
  defaultName,
  onCancel,
  onCreate,
  onDraftChange,
  t,
}: {
  defaultName: string;
  onCancel: () => void;
  onCreate: () => void;
  onDraftChange: (value: string) => void;
  t: (key: string) => string;
}) {
  return (
    <div className="d-flex flex-column gap-3">
      <CreateSceneModalBodyFields
        defaultName={defaultName}
        onDraftChange={onDraftChange}
        t={t}
      />
      <div className="d-flex justify-content-end gap-2">
        <button className="btn btn-secondary" onClick={onCancel} type="button">{t('Cancel')}</button>
        <button className="btn btn-success" onClick={onCreate} type="button">{t('Create scene')}</button>
      </div>
    </div>
  );
}

function CreateSceneModalBodyFields({
  defaultName,
  onDraftChange,
  t,
}: {
  defaultName: string;
  onDraftChange: (value: string) => void;
  t: (key: string) => string;
}) {
  return (
    <div>
      <label htmlFor="scene-name-create" className="form-label mb-1">{t('Scene name')}</label>
      <input
        id="scene-name-create"
        type="text"
        defaultValue={defaultName}
        className="form-control"
        onChange={(event) => onDraftChange(event.target.value)}
      />
    </div>
  );
}

function DeleteBlockedBody({ t }: { t: (key: string) => string }) {
  return (
    <div className="d-flex flex-column gap-2">
      <p className="mb-0">{t('You cannot delete this scene because it is the only one in the project.')}</p>
      <small className="text-secondary">{t('There must be at least one scene to keep the editor in a valid state.')}</small>
    </div>
  );
}

function DeleteConfirmBody({
  onCancel,
  onConfirm,
  t,
}: {
  onCancel: () => void;
  onConfirm: () => void;
  t: (key: string) => string;
}) {
  return (
    <div className="d-flex flex-column gap-3">
      <p className="mb-0">{t('This action will delete the selected scene.')}</p>
      <div className="d-flex justify-content-end gap-2">
        <button className="btn btn-secondary" onClick={onCancel} type="button">{t('Cancel')}</button>
        <button className="btn btn-danger" onClick={onConfirm} type="button">{t('Delete')}</button>
      </div>
    </div>
  );
}

export function useSceneManager(): SceneManagerContextValue {
  const ctx = useContext(SceneManagerContext);
  if (!ctx) {
    throw new Error('useSceneManager must be used within SceneManagerProvider');
  }
  return ctx;
}
