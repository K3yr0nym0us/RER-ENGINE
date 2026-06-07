import { useState, useEffect, useRef, useCallback } from 'react';

import { useContextEngine } from '@engine';
import { useLanguage } from '@context';
import type { GameStyle, ProjectType, ProjectSaveData } from '@shared-types';
import {
	buildProjectSaveFromEngineSnapshot,
	requestEngineSaveSnapshot,
} from '../defaults/buildProjectSaveFromEngine';

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
  handleSave: () => Promise<void>
}

export function useAutoSave({
  projectType = '2D' as ProjectType,
  gameStyle,
  initialSavePath = null,
  initialExtractDir = null,
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
  const lastSavePath = useRef<string | null>(initialSavePath);
  const autoSaveEnabledRef = useRef(false);
  const buildSaveDataRef = useRef<(() => Promise<ProjectSaveData | null>) | null>(null);
  const autoSaveListenerRegisteredRef = useRef(false);

  useEffect(() => {
    if (initialSavePath) {
      setHasSavedOnce(true);
    }
  }, [initialSavePath]);

  useEffect(() => {
    if (initialSavePath) {
      lastSavePath.current = initialSavePath;
    }
  }, [initialSavePath]);

  const buildSaveData = useCallback(async (): Promise<ProjectSaveData | null> => {
    try {
      const engineScene = await requestEngineSaveSnapshot();
      const defaultGameStyle = projectType === '3D' ? 'first-person' : 'top-down';
      return await buildProjectSaveFromEngineSnapshot(engineScene, {
        projectType,
        gameStyle: gameStyle ?? defaultGameStyle,
        locale,
        blueprints,
        sounds,
        fonts,
        hudImages,
        models,
        backgrounds,
        entityMeta: entityMetaRef.current,
        initialGameStyle: gameStyle,
        playerUiScreens,
        menuUiScreens,
      });
    } catch (err) {
      console.error('[save] export_save_snapshot falló:', err);
      return null;
    }
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

  useEffect(() => {
    autoSaveEnabledRef.current = autoSaveEnabled;
  }, [autoSaveEnabled]);

  useEffect(() => {
    buildSaveDataRef.current = buildSaveData;
  }, [buildSaveData]);

  useEffect(() => {
    if (!hasSavedOnce && autoSaveEnabled) {
      setAutoSaveEnabled(false);
      window.engine.send({ cmd: 'set_autosave', enabled: false } as never);
    }
  }, [hasSavedOnce, autoSaveEnabled]);

  useEffect(() => {
    if (autoSaveListenerRegisteredRef.current) return;
    autoSaveListenerRegisteredRef.current = true;

    window.electronAPI.onAutoSaveRequest(async (filePath: string) => {
      if (!autoSaveEnabledRef.current) return;
      const snapshotBuilder = buildSaveDataRef.current;
      if (!snapshotBuilder) return;
      const data = await snapshotBuilder();
      if (!data) return;
      const ok = await window.electronAPI.saveProjectSilent(filePath, data);
      if (ok && projectType === '3D') {
        const dir = await window.electronAPI.getProjectExtractDir();
        if (dir?.trim()) {
          window.engine.send({
            cmd: 'notify_project_saved',
            extract_dir: dir.trim(),
          } as never);
        }
      }
    });
  }, [projectType]);

  useEffect(() => {
    return () => {
      window.engine.send({ cmd: 'set_autosave', enabled: false } as never);
    };
  }, []);

  const notifyMotorProjectSaved = useCallback(async () => {
    if (projectType !== '3D') return;
    const dir =
      (await window.electronAPI.getProjectExtractDir())
      || initialExtractDir?.trim()
      || null;
    if (!dir) return;
    window.engine.send({
      cmd: 'notify_project_saved',
      extract_dir: dir,
    } as never);
  }, [projectType, initialExtractDir]);

  const handleSave = useCallback(async () => {
    const data = await buildSaveData();
    if (!data) return;

    const savedPath = await window.electronAPI.saveProject(data);
    if (savedPath) {
      lastSavePath.current = savedPath;
      setHasSavedOnce(true);
      await notifyMotorProjectSaved();
    }
  }, [buildSaveData, notifyMotorProjectSaved]);

  const setHasSavedOnceTrue = useCallback((v: boolean) => {
    setHasSavedOnce(v);
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
    setHasSavedOnce: setHasSavedOnceTrue,
    handleSave,
  };
}
