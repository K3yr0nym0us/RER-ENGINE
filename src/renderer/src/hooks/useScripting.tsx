import { useState, useEffect } from 'react';

import ScriptEditorModalBody from '../components/SpritePreviewModalBody/components/ScriptEditorModalBody';
import { VisualScriptingModalBody } from '../visualScripting/components/VisualScriptingModalBody';
import { createEmptyEntityVisualGraph, saveEntityVisualGraph } from '../visualScripting/entityVisualScript';
import { resolveSceneEntitiesForVisualScript } from '../visualScripting/resolveSceneEntities';

import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';
import { ModalConfirmBody } from '../modal-electron/ModalConfirmBody';

export interface ScriptEntry {
  name:   string
  source: string
}

export interface UseScriptingReturn {
  scripts:      ScriptEntry[]
  openEditor:   () => void
  openVisualScripting: () => void
  editScript:   (name: string) => void
  removeScript: (name: string) => void
}

/**
 * Gestiona la lista de scripts Rhai adjuntos a la entidad seleccionada.
 */
export function useScripting(): UseScriptingReturn {
  const {
    selectedEntity,
    send,
    entityMetaRef,
    entityTransformsRef,
    updateEntityScripts,
    updateEntityVisualGraph,
  } = useContextEngine()
  const { openModal, closeModal } = useModal()
  const { t } = useTraslate()
  const [scripts, setScripts] = useState<ScriptEntry[]>([])

  useEffect(() => {
    if (!selectedEntity) { setScripts([]); return }
    setScripts(entityMetaRef.current[selectedEntity.id]?.scripts ?? [])
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedEntity?.id])

  const handleSave = (currentScripts: ScriptEntry[], data: ScriptEntry, replacing?: string) => {
    if (!selectedEntity) return
    const next = replacing
      ? currentScripts.map((s) => s.name === replacing ? data : s)
      : [...currentScripts, data]
    setScripts(next)
    updateEntityScripts(selectedEntity.id, next)
    send({ cmd: 'load_script', id: selectedEntity.id, path: data.name, source: data.source })
    closeModal()
  }

  const openEditor = () => {
    if (!selectedEntity) return
    openModal({
      title: t('New Rhai script'),
      size:  'lg',
      body: (
        <ScriptEditorModalBody
          onSave={(data) => handleSave(scripts, data)}
          onCancel={closeModal}
        />
      ),
    })
  }

  const openVisualScripting = () => {
    if (!selectedEntity) return
    const entityId = selectedEntity.id
    const meta = entityMetaRef.current[entityId]
    const initialGraph = meta?.visualGraph ?? createEmptyEntityVisualGraph(entityId)
    const sceneEntities = resolveSceneEntitiesForVisualScript({
      entityMeta: entityMetaRef.current,
      entityTransforms: entityTransformsRef.current,
    })
    openModal({
      title: t('Entity logic'),
      size: 'xl',
      body: (
        <VisualScriptingModalBody
          context="entity"
          entityId={entityId}
          entityName={selectedEntity.name ?? meta?.name}
          sceneEntities={sceneEntities}
          initialGraph={initialGraph}
          onSave={(graph) => {
            const saveResult = saveEntityVisualGraph(entityId, graph)
            if (!saveResult.ok || !saveResult.rhaiSource) {
              return { ok: false, errors: saveResult.errors }
            }
            updateEntityVisualGraph(entityId, graph, saveResult.rhaiSource)
            closeModal()
            return { ok: true }
          }}
          onCancel={closeModal}
        />
      ),
    })
  }

  const editScript = (scriptName: string) => {
    if (!selectedEntity) return
    const existing = scripts.find((s) => s.name === scriptName)
    if (!existing) return
    openModal({
      title: `${t('Edit script')}: ${scriptName}`,
      size:  'lg',
      body: (
        <ScriptEditorModalBody
          initialData={existing}
          onSave={(data) => handleSave(scripts, data, scriptName)}
          onCancel={closeModal}
        />
      ),
    })
  }

  const removeScript = (name: string) => {
    if (!selectedEntity) return
    openModal({
      title: t('Confirm deletion'),
      size: 'sm',
      body: (
        <ModalConfirmBody
          message={
            <div className="text-center">
              <p>{t('Delete script confirm')} <strong>{name}</strong>?</p>
              <p className="text-danger small mb-0">{t('This action cannot be undone.')}</p>
            </div>
          }
          confirmLabel={t('Yes, delete')}
          cancelLabel={t('Cancel')}
          onConfirm={() => {
            const next = scripts.filter((s) => s.name !== name)
            setScripts(next)
            updateEntityScripts(selectedEntity.id, next)
            if (next.length === 0) {
              send({ cmd: 'unload_script', id: selectedEntity.id })
            }
          }}
        />
      ),
    })
  }

  return { scripts, openEditor, openVisualScripting, editScript, removeScript }
}
