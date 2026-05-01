import { useEffect, useMemo, useState } from 'react';
import { Files, Pencil, PlusLg, Trash } from 'react-bootstrap-icons';
import Nav from 'react-bootstrap/Nav';

import type { ProjectSaveData, SavedEntity, SavedScene, SavedWorldConfig } from '@shared-types';
import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { setSceneProjectState } from '../sceneStateStore';

interface SceneTab {
  id: number;
  name: string;
}

interface Props {
  initialSave?: ProjectSaveData | null;
  projectType?: string;
}

const DEFAULT_WORLD: SavedWorldConfig = {
  worldWidth: 100,
  worldHeight: 50,
  gridVisible: true,
  gridCellSize: 1,
};

export function SceneTabsBar({ initialSave, projectType }: Props) {
  const {
    worldConfig,
    backgroundPath,
    scenarioEntities,
    characterEntities,
    colliderEntities,
    loadedSpritesInfo,
    entityTransformsRef,
    entityMetaRef,
    pendingRestoresRef,
    playerEntityIdRef,
    camera2dRef,
    send,
    removeScenario,
    removeCharacter,
    removeCollider,
    setWorldSize,
    setGridVisible,
    setGridCellSize,
    setBackground,
    loadSprite,
    removeSprite,
  } = useContextEngine();

  const { openModal, closeModal } = useModal();

  const initialSceneState = useMemo(() => {
    const save = initialSave;
    if (save?.scenes && save.scenes.length > 0) {
      const tabs = save.scenes.map((scene) => ({ id: scene.id, name: scene.name }));
      const dataById: Record<number, SavedScene> = {};
      for (const scene of save.scenes) {
        dataById[scene.id] = scene;
      }
      const active = save.activeSceneId && dataById[save.activeSceneId]
        ? save.activeSceneId
        : save.scenes[0].id;
      return { tabs, dataById, activeSceneId: active };
    }

    const legacyScene: SavedScene = {
      id: 1,
      name: 'Escena 1',
      world: save?.world ?? DEFAULT_WORLD,
      backgroundPath: save?.backgroundPath ?? null,
      entities: save?.entities ?? [],
      playerTransform: save?.playerTransform ?? null,
      camera2d: save?.camera2d ?? null,
      sprites: save?.sprites ?? [],
    };
    return {
      tabs: [{ id: 1, name: 'Escena 1' }],
      dataById: { 1: legacyScene },
      activeSceneId: 1,
    };
  }, [initialSave]);

  const [scenes, setScenes] = useState<SceneTab[]>(initialSceneState.tabs);
  const [sceneDataById, setSceneDataById] = useState<Record<number, SavedScene>>(initialSceneState.dataById);
  const [activeSceneId, setActiveSceneId] = useState(initialSceneState.activeSceneId);

  const buildCurrentSceneSnapshot = (id: number, name: string): SavedScene => {
    const transforms = entityTransformsRef.current;
    const meta = entityMetaRef.current;
    const playerId = playerEntityIdRef.current;
    const DEFAULT_POS: [number, number, number] = [0, 0, 0];
    const DEFAULT_ROT: [number, number, number, number] = [0, 0, 0, 1];
    const DEFAULT_SCL: [number, number, number] = [1, 1, 1];

    const entities: SavedEntity[] = Object.entries(meta).reduce<SavedEntity[]>((acc, [idStr, m]) => {
      if (m.kind === 'character' && m.path === '[Player]' && Number(idStr) === playerId) return acc;
      const entityId = Number(idStr);
      acc.push({
        id: entityId,
        name: m.name,
        kind: m.kind,
        path: m.path,
        position: transforms[entityId]?.position ?? DEFAULT_POS,
        rotation: transforms[entityId]?.rotation ?? DEFAULT_ROT,
        scale: transforms[entityId]?.scale ?? DEFAULT_SCL,
        physics_enabled: m.physicsEnabled,
        physics_type: m.physicsType,
        points: m.points,
        animations: m.animations,
        scripts: m.scripts,
      });
      return acc;
    }, []);

    const sprites = Array.from(loadedSpritesInfo.entries()).map(([path, info]) => ({ name: info.name, path }));

    const playerTransform = playerId !== null
      ? {
          position: transforms[playerId]?.position ?? DEFAULT_POS,
          scale: transforms[playerId]?.scale ?? DEFAULT_SCL,
        }
      : null;

    return {
      id,
      name,
      world: { ...worldConfig },
      backgroundPath: backgroundPath ?? null,
      entities,
      playerTransform,
      camera2d: camera2dRef.current,
      sprites,
    };
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

    setWorldSize(scene.world.worldWidth, scene.world.worldHeight);
    setGridVisible(scene.world.gridVisible);
    setGridCellSize(scene.world.gridCellSize);

    if (scene.camera2d) {
      send({ cmd: 'set_camera2d', x: scene.camera2d.x, y: scene.camera2d.y, half_h: scene.camera2d.halfH });
      camera2dRef.current = scene.camera2d;
    }

    if (projectType === '2D') {
      setBackground(scene.backgroundPath);
    }

    for (const sprite of scene.sprites ?? []) {
      loadSprite(sprite.path, sprite.name);
    }

    pendingRestoresRef.current.clear();

    for (const entity of scene.entities) {
      const transform = {
        position: entity.position,
        rotation: entity.rotation,
        scale: entity.scale,
      };

      if (entity.kind === 'collider' && entity.points) {
        send({ cmd: 'create_collider_from_points', points: entity.points });
        continue;
      }

      const queue = pendingRestoresRef.current.get(entity.path) ?? [];
      pendingRestoresRef.current.set(entity.path, queue);
      queue.push({
        transform,
        name: entity.name,
        physicsEnabled: entity.physics_enabled ?? false,
        physicsType: entity.physics_type ?? 'static',
        animations: entity.animations,
        scripts: entity.scripts,
      });

      if (entity.kind === 'scenario') send({ cmd: 'load_scenario', path: entity.path });
      if (entity.kind === 'character') send({ cmd: 'load_character', path: entity.path });
      if (entity.kind === 'model') send({ cmd: 'load_model', path: entity.path });
    }

    if (scene.playerTransform && playerEntityIdRef.current !== null) {
      send({
        cmd: 'set_transform',
        id: playerEntityIdRef.current,
        position: scene.playerTransform.position,
        scale: scene.playerTransform.scale,
      });
      entityTransformsRef.current[playerEntityIdRef.current] = {
        position: scene.playerTransform.position,
        rotation: [0, 0, 0, 1],
        scale: scene.playerTransform.scale,
      };
    }
  };

  const getNextSceneId = () => {
    if (scenes.length === 0) return 1;
    return Math.max(...scenes.map((scene) => scene.id)) + 1;
  };

  const createScene = (name: string) => {
    const trimmed = name.trim();
    if (!trimmed) return;

    const current = scenes.find((scene) => scene.id === activeSceneId);
    if (current) {
      const snapshot = buildCurrentSceneSnapshot(current.id, current.name);
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

    clearCurrentSceneInEngine();
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

  const duplicateScene = (sceneId: number) => {
    const sourceTab = scenes.find((scene) => scene.id === sceneId);
    if (!sourceTab) return;
    const sourceData = sceneId === activeSceneId
      ? buildCurrentSceneSnapshot(sourceTab.id, sourceTab.name)
      : sceneDataById[sceneId];
    if (!sourceData) return;

    const nextId = getNextSceneId();
    const nextName = `${sourceTab.name} (copia)`;
    const duplicatedScene: SavedScene = {
      ...sourceData,
      id: nextId,
      name: nextName,
      entities: sourceData.entities.map((entity) => ({ ...entity })),
      sprites: (sourceData.sprites ?? []).map((sprite) => ({ ...sprite })),
      world: { ...sourceData.world },
      camera2d: sourceData.camera2d ? { ...sourceData.camera2d } : null,
      playerTransform: sourceData.playerTransform
        ? { position: [...sourceData.playerTransform.position] as [number, number, number], scale: [...sourceData.playerTransform.scale] as [number, number, number] }
        : null,
    };

    const current = scenes.find((scene) => scene.id === activeSceneId);
    if (current) {
      const snapshot = buildCurrentSceneSnapshot(current.id, current.name);
      setSceneDataById((prev) => ({ ...prev, [current.id]: snapshot, [nextId]: duplicatedScene }));
    } else {
      setSceneDataById((prev) => ({ ...prev, [nextId]: duplicatedScene }));
    }

    clearCurrentSceneInEngine();
    loadSceneIntoEngine(duplicatedScene);

    setScenes((prev) => [...prev, { id: nextId, name: nextName }]);
    setActiveSceneId(nextId);
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
      clearCurrentSceneInEngine();
      loadSceneIntoEngine(targetData);
    }
    setActiveSceneId(nextActive.id);
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
    const nextId = getNextSceneId();
    let draftName = `Escena ${nextId}`;

    openModal({
      title: 'Crear nueva escena',
      body: (
        <div className="d-flex flex-column gap-3">
          <div>
            <label htmlFor="scene-name-create" className="form-label mb-1">Nombre de la escena</label>
            <input
              id="scene-name-create"
              type="text"
              defaultValue={draftName}
              className="form-control"
              onChange={(event) => {
                draftName = event.target.value;
              }}
            />
          </div>

          <div className="d-flex justify-content-end gap-2">
            <button className="btn btn-secondary" onClick={closeModal} type="button">Cancelar</button>
            <button
              className="btn btn-success"
              onClick={() => {
                createScene(draftName);
                closeModal();
              }}
              type="button"
            >
              Crear escena
            </button>
          </div>
        </div>
      ),
    });
  };

  const openRenameSceneModal = (scene: SceneTab) => {
    let draftName = scene.name;

    openModal({
      title: `Editar ${scene.name}`,
      body: (
        <div className="d-flex flex-column gap-3">
          <div>
            <label htmlFor="scene-name-rename" className="form-label mb-1">Nombre de la escena</label>
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
              Guardar nombre
            </button>
            <button className="btn btn-secondary" onClick={closeModal} type="button">Cancelar</button>
          </div>
        </div>
      ),
    });
  };

  const openDeleteSceneModal = (scene: SceneTab) => {
    if (scenes.length <= 1) {
      openModal({
        title: `No se puede eliminar ${scene.name}`,
        body: (
          <div className="d-flex flex-column gap-2">
            <p className="mb-0">No puedes eliminar esta escena porque es la única del proyecto.</p>
            <small className="text-secondary">Debe existir al menos una escena para mantener el editor en un estado válido.</small>
          </div>
        ),
      });
      return;
    }

    openModal({
      title: `Eliminar ${scene.name}`,
      body: (
        <div className="d-flex flex-column gap-3">
          <p className="mb-0">Esta acción eliminará la escena seleccionada.</p>
          <div className="d-flex justify-content-end gap-2">
            <button className="btn btn-secondary" onClick={closeModal} type="button">Cancelar</button>
            <button
              className="btn btn-danger"
              onClick={() => {
                deleteScene(scene.id);
                closeModal();
              }}
              type="button"
            >
              Eliminar
            </button>
          </div>
        </div>
      ),
    });
  };

  const openDuplicateSceneModal = (scene: SceneTab) => {
    openModal({
      title: `Duplicar ${scene.name}`,
      body: (
        <div className="d-flex flex-column gap-3">
          <p className="mb-0">Se creará una copia completa de esta escena.</p>
          <small className="text-secondary">
            La copia incluirá todos los elementos de la escena: escenarios, personajes, colisionadores y el resto de su configuración visual y lógica.
          </small>
          <div className="d-flex justify-content-end gap-2">
            <button className="btn btn-secondary" onClick={closeModal} type="button">Cancelar</button>
            <button
              className="btn btn-success"
              onClick={() => {
                duplicateScene(scene.id);
                closeModal();
              }}
              type="button"
            >
              Duplicar escena
            </button>
          </div>
        </div>
      ),
    });
  };

  return (
    <div className="scene-tabs-bar px-1 d-flex align-items-center gap-2 overflow-auto">
      <div className="scene-tabs-nav-wrap flex-grow-1 overflow-auto">
        <Nav
          variant="tabs"
          activeKey={`${activeSceneId}`}
          className="scene-tabs-nav flex-nowrap"
          onSelect={(eventKey) => {
            if (!eventKey) return;
            if (eventKey === '__new_scene') {
              openCreateSceneModal();
              return;
            }
            const nextId = Number(eventKey);
            if (Number.isNaN(nextId) || nextId === activeSceneId) return;

            const current = scenes.find((scene) => scene.id === activeSceneId);
            const target = scenes.find((scene) => scene.id === nextId);
            if (!target) return;

            const currentSnapshot = current
              ? buildCurrentSceneSnapshot(current.id, current.name)
              : null;

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

            if (currentSnapshot) {
              setSceneDataById((prev) => ({ ...prev, [current.id]: currentSnapshot }));
            }

            clearCurrentSceneInEngine();
            loadSceneIntoEngine(targetSnapshot);
            setActiveSceneId(nextId);
          }}
        >
          {scenes.map((scene) => (
            <Nav.Item key={scene.id} className="scene-nav-item">
              <Nav.Link eventKey={`${scene.id}`} className="scene-nav-link d-flex align-items-center gap-2">
                <span className="scene-nav-link__name">{scene.name}</span>
                <span className="scene-nav-actions">
                  <button
                    className="scene-nav-action text-primary"
                    type="button"
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      openRenameSceneModal(scene);
                    }}
                    aria-label={`Editar ${scene.name}`}
                  >
                    <Pencil size={14} />
                  </button>

                  <button
                    className="scene-nav-action text-info"
                    type="button"
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      openDuplicateSceneModal(scene);
                    }}
                    aria-label={`Duplicar ${scene.name}`}
                  >
                    <Files size={14} />
                  </button>

                  <button
                    className="scene-nav-action scene-nav-action--danger text-danger"
                    type="button"
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      openDeleteSceneModal(scene);
                    }}
                    aria-label={`Eliminar ${scene.name}`}
                  >
                    <Trash size={14} />
                  </button>
                </span>
              </Nav.Link>
            </Nav.Item>
          ))}

          <Nav.Item className="scene-nav-item scene-nav-item--create">
            <Nav.Link eventKey="__new_scene" className="scene-nav-create d-flex align-items-center justify-content-center gap-1">
              <PlusLg size={12} />
            </Nav.Link>
          </Nav.Item>
        </Nav>
      </div>
    </div>
  );
}

export default SceneTabsBar;
