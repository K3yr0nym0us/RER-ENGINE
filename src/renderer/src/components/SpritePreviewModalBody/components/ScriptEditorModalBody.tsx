
import React, { useState, useRef, useMemo, Suspense } from 'react';
import { RHAI_MONACO_EDITOR_OPTIONS, RHAI_MONACO_LANGUAGE, registerRhaiMonacoLanguage } from '../../../editor/rhaiMonaco';
import { getDefaultEntityScript } from '../../../editor/rhaiScriptTemplates';
import { useLanguage } from '../../../context/LanguageContext';
import { modalTallContentHeightPx } from '../../../modal-electron/modalElectronLayout';
import { useTraslate } from '@hooks';
// Dynamic import for monaco-editor (for code splitting)
const MonacoEditor = React.lazy(() => import('@monaco-editor/react'));

interface ScriptEditorInstance {
  focus: () => void;
}

interface ScriptEditorModalBodyProps {
  initialData?: { name?: string; source?: string }
  onSave:       (data: { name: string; source: string }) => void
  onCancel:     () => void
}

/**
 * Cuerpo del editor de scripts Rhai para usar dentro de ModalProvider.
 * Contiene el input de nombre, el editor Monaco y los botones de acción.
 */
export function ScriptEditorModalBody({ initialData, onSave, onCancel }: ScriptEditorModalBodyProps) {
  const { locale } = useLanguage()
  const { t } = useTraslate()
  const defaultScript = useMemo(() => getDefaultEntityScript(locale), [locale])
  const [name, setName] = useState(initialData?.name ?? '')
  const sourceRef        = useRef<string>(initialData?.source ?? defaultScript)
  const editorRef        = useRef<ScriptEditorInstance | null>(null)

  const handleMount = (editor: ScriptEditorInstance) => {
    editorRef.current = editor
    editor.focus()
  }

  const handleSave = () => {
    if (!name.trim()) return
    onSave({ name: name.trim(), source: sourceRef.current })
  }

  return (
    <div
      className="d-flex flex-column gap-2"
      style={{ height: modalTallContentHeightPx(), minHeight: modalTallContentHeightPx() }}
    >

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
            defaultValue={initialData?.source ?? defaultScript}
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
        <button className="btn btn-sm btn-outline-secondary" onClick={onCancel}>
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

export default ScriptEditorModalBody
