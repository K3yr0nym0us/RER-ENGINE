import { useState, useEffect } from 'react';

import ScriptEditorModalBody from '../components/SpritePreviewModalBody/components/ScriptEditorModalBody';

import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import { ModalConfirmBody } from '../modal-electron/ModalConfirmBody';

export interface ScriptEntry {
  name:   string
  source: string
}

export interface UseScriptingReturn {
  scripts:      ScriptEntry[]
  openEditor:   () => void
  editScript:   (name: string) => void
  removeScript: (name: string) => void
}

/**
 * Gestiona la lista de scripts Lua adjuntos a la entidad seleccionada.
 */
export function useScripting(): UseScriptingReturn {
  const { selectedEntity, send, entityMetaRef, updateEntityScripts } = useContextEngine()
  const { openModal, closeModal } = useModal()
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
      title: 'Nuevo Script Lua',
      size:  'lg',
      body: (
        <ScriptEditorModalBody
          onSave={(data) => handleSave(scripts, data)}
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
      title: `Editar Script: ${scriptName}`,
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
      title: 'Confirmar eliminación',
      size: 'sm',
      body: (
        <ModalConfirmBody
          message={
            <div className="text-center">
              <p>¿Estás seguro de que deseas eliminar el script <strong>{name}</strong>?</p>
              <p className="text-danger small mb-0">Esta acción no se puede deshacer.</p>
            </div>
          }
          confirmLabel="Sí, eliminar"
          cancelLabel="Cancelar"
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

  return { scripts, openEditor, editScript, removeScript }
}
