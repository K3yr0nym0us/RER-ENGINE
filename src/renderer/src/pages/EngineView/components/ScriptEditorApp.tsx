
import React, { useState, useRef, useEffect, useMemo, Suspense } from 'react';
import { FileEarmarkCode } from 'react-bootstrap-icons';
import { RHAI_MONACO_EDITOR_OPTIONS, RHAI_MONACO_LANGUAGE, registerRhaiMonacoLanguage } from '../../../editor/rhaiMonaco';
import { getDefaultEntityScript } from '../../../editor/rhaiScriptTemplates';
import { useLanguage } from '../../../context/LanguageContext';
import { useTraslate } from '@hooks';
// Dynamic import for monaco-editor (for code splitting)
const MonacoEditor = React.lazy(() => import('@monaco-editor/react'));

interface ScriptEditorInstance {
  focus: () => void;
  setValue: (value: string) => void;
}

interface ScriptEditorBridge {
  getScriptEditorInit: () => Promise<{ name: string; source: string } | null>
  saveScriptEditor: (data: { name: string; source: string }) => void | Promise<void>
  cancelScriptEditor: () => void | Promise<void>
}

function scriptEditorBridge(): ScriptEditorBridge {
  return window.electronAPI as unknown as ScriptEditorBridge
}

/**
 * Ventana secundaria del editor de scripts Rhai.
 * Se renderiza cuando la URL contiene `?mode=script-editor`.
 * Comunica el resultado al proceso main vía IPC (saveScriptEditor / cancelScriptEditor).
 */
export function ScriptEditorApp() {
  const { locale } = useLanguage()
  const { t } = useTraslate()
  const defaultScript = useMemo(() => getDefaultEntityScript(locale), [locale])
  const [isEditing, setIsEditing] = useState(false)
  const [name, setName]           = useState('')
  const sourceRef                 = useRef<string>(defaultScript)
  const editorRef                 = useRef<ScriptEditorInstance | null>(null)

  // Pedir datos iniciales al proceso main via IPC (evita el problema de
  // additionalArguments que corrompe JSON con comillas en Windows)
  useEffect(() => {
    void scriptEditorBridge().getScriptEditorInit().then(
      (data: { name: string; source: string } | null) => {
        if (!data) return
        setIsEditing(true)
        setName(data.name)
        sourceRef.current = data.source
        // Si Monaco ya montó, actualizar el valor directamente en el editor
        editorRef.current?.setValue(data.source)
      },
    )
  }, [])

  const handleMount = (editor: ScriptEditorInstance) => {
    editorRef.current = editor
    editor.focus()
  }

  const handleSave = () => {
    if (!name.trim()) return
    void scriptEditorBridge().saveScriptEditor({
      name:   name.trim(),
      source: sourceRef.current,
    })
  }

  const handleCancel = () => {
    void scriptEditorBridge().cancelScriptEditor()
  }

  return (
    <div
      className="d-flex flex-column p-3 gap-2"
      style={{ height: '100vh', background: '#0d0d1a', color: '#fff' }}
    >
      {/* Cabecera */}
      <div className="d-flex align-items-center gap-2 mb-1">
        <FileEarmarkCode size={18} className="text-warning" />
        <span className="fw-semibold" style={{ fontSize: '0.95rem' }}>
          {isEditing ? t('Edit Rhai script') : t('New Rhai script')}
        </span>
      </div>

      {/* Nombre del script */}
      <input
        type="text"
        placeholder={t('Script name placeholder')}
        value={name}
        onChange={(e) => setName(e.target.value)}
        className="form-control form-control-sm bg-dark text-light border-secondary"
        onKeyDown={(e) => { if (e.key === 'Enter') handleSave() }}
      />

      {/* Editor Monaco */}
      <div className="flex-fill rounded overflow-hidden border border-secondary" style={{ minHeight: 0 }}>
        <Suspense fallback={<div>{t('Loading editor...')}</div>}>
          <MonacoEditor
          height="100%"
          language={RHAI_MONACO_LANGUAGE}
          defaultValue={defaultScript}
          theme="vs-dark"
          beforeMount={registerRhaiMonacoLanguage}
          onChange={(val) => { sourceRef.current = val ?? '' }}
          onMount={handleMount}
          options={RHAI_MONACO_EDITOR_OPTIONS}
          />
        </Suspense>
      </div>

      {/* Acciones */}
      <div className="d-flex gap-2 justify-content-end">
        <button
          className="btn btn-sm btn-outline-secondary"
          onClick={handleCancel}
        >
          {t('Cancel')}
        </button>
        <button
          className="btn btn-sm btn-success"
          disabled={!name.trim()}
          onClick={handleSave}
        >
          {t('Save script')}
        </button>
      </div>
    </div>
  )
}

export default ScriptEditorApp
