import { useState, useEffect, useRef, useCallback } from 'react';

import { useContextEngine } from '@engine';
import { useLanguage } from '@context';
import type { GameStyle, ProjectType } from '@shared-types';
import { DEFAULT_3D_CAMERA_MODE } from '@shared-types';
import { getSceneProjectState } from '../pages/EngineView/sceneStateStore';
import { blueprintToSave } from '../utils/blueprintModelPath';

interface UseAutoSaveOptions {
  projectType?: ProjectType
  gameStyle?: GameStyle
  initialSavePath?: string | null
  initialExtractDir?: string | null
}

export interface UseAutoSaveReturn {
  autoSaveEnabled: boolean
  toggleAutoSave: () => void
  hasSavedOnce: boolean
  setHasSavedOnce: (v: boolean) => void
  handleSave: () => void
  savingProject: boolean
}

/**
 * Guardar/Auto delegan en el editor Rust (dueño del motor y del empaquetado
 * `.save`). React solo aporta el estado de editor que no vive en el motor.
 */
export function useAutoSave({
  projectType = '2D' as ProjectType,
  gameStyle,
  initialSavePath = null,
}: UseAutoSaveOptions = {}): UseAutoSaveReturn {
  const {
    entityMetaRef,
    blueprints,
    sounds,
    fonts,
    models,
    hudImages,
    backgrounds,
    playerUiScreens,
    menuUiScreens,
  } = useContextEngine();
  const { locale } = useLanguage();
  const [hasSavedOnce, setHasSavedOnce] = useState(Boolean(initialSavePath));
  const [autoSaveEnabled, setAutoSaveEnabled] = useState(false);

  useEffect(() => {
    if (initialSavePath) {
      setHasSavedOnce(true);
    }
  }, [initialSavePath]);

  /**
   * Estado que el editor Rust necesita para guardar sin Electron: escenas
   * inactivas, blueprints, `entityMeta`, pantallas UI e idioma.
   */
  const buildProjectState = useCallback(() => {
    const sceneState = getSceneProjectState();
    const defaultStyle = projectType === '3D' ? DEFAULT_3D_CAMERA_MODE : 'top-down';
    return {
      projectType,
      gameStyle: gameStyle ?? defaultStyle,
      locale,
      scenes: sceneState?.scenes ?? [],
      activeSceneId: sceneState?.activeSceneId ?? 1,
      // 3D normaliza a `Blueprint3D`; 2D guarda el shape de editor tal cual
      // (`kind`/`path`/`scale` son obligatorios en el loader 2D).
      blueprints: projectType === '3D' ? blueprints.map(blueprintToSave) : blueprints,
      entityMeta: entityMetaRef.current,
      sounds,
      fonts,
      hudImages,
      models,
      backgrounds,
      playerUiScreens,
      menuUiScreens,
    };
  }, [
    projectType,
    gameStyle,
    entityMetaRef,
    blueprints,
    sounds,
    fonts,
    hudImages,
    models,
    backgrounds,
    locale,
    playerUiScreens,
    menuUiScreens,
  ]);

  const projectStateBuilderRef = useRef<() => unknown>(() => ({}));

  useEffect(() => {
    projectStateBuilderRef.current = buildProjectState;
  }, [buildProjectState]);

  useEffect(() => {
    return window.electronAPI.onProjectStateRequest((id) => {
      window.electronAPI.respondProjectState(id, projectStateBuilderRef.current());
    });
  }, []);

  useEffect(() => {
    const offSaved = window.electronAPI.onProjectSaved(() => {
      setHasSavedOnce(true);
    });
    const offFailed = window.electronAPI.onProjectSaveFailed((message) => {
      console.error('[save] editor Rust:', message);
    });
    return () => {
      offSaved();
      offFailed();
    };
  }, []);

  useEffect(() => {
    if (!hasSavedOnce && autoSaveEnabled) {
      setAutoSaveEnabled(false);
      window.engine.send({ cmd: 'set_autosave', enabled: false } as never);
    }
  }, [hasSavedOnce, autoSaveEnabled]);

  useEffect(() => {
    return () => {
      window.engine.send({ cmd: 'set_autosave', enabled: false } as never);
    };
  }, []);

  const handleSave = useCallback(() => {
    window.electronAPI.requestProjectSave();
  }, []);

  const toggleAutoSave = useCallback(() => {
    if (!hasSavedOnce) return;
    setAutoSaveEnabled((prev) => {
      const next = !prev;
      window.engine.send({ cmd: 'set_autosave', enabled: next } as never);
      return next;
    });
  }, [hasSavedOnce]);

  return {
    autoSaveEnabled,
    toggleAutoSave,
    hasSavedOnce,
    setHasSavedOnce,
    handleSave,
    savingProject: false,
  };
}
