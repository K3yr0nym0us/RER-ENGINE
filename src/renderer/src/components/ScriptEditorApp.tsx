import { useState, useRef, useEffect } from 'react'
import Editor, { type OnMount } from '@monaco-editor/react'
import { FileEarmarkCode } from 'react-bootstrap-icons'
import type * as Monaco from 'monaco-editor'

const DEFAULT_SCRIPT = `-- Escribe tu script Lua aquí
local script = {}

function script.on_start(self)
end

function script.update(self, dt)
end

function script.on_stop(self)
end

return script`

/**
 * Ventana secundaria del editor de scripts Lua.
 * Se renderiza cuando la URL contiene `?mode=script-editor`.
 * Comunica el resultado al proceso main vía IPC (saveScriptEditor / cancelScriptEditor).
 */
export function ScriptEditorApp() {
  const [isEditing, setIsEditing] = useState(false)
  const [name, setName]           = useState('')
  const sourceRef                 = useRef<string>(DEFAULT_SCRIPT)
  const editorRef                 = useRef<Monaco.editor.IStandaloneCodeEditor | null>(null)

  // Pedir datos iniciales al proceso main via IPC (evita el problema de
  // additionalArguments que corrompe JSON con comillas en Windows)
  useEffect(() => {
    void (window as any).electronAPI.getScriptEditorInit().then(
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

  const handleMount: OnMount = (editor) => {
    editorRef.current = editor
    editor.focus()
  }

  const handleSave = () => {
    if (!name.trim()) return
    void (window as any).electronAPI.saveScriptEditor({
      name:   name.trim(),
      source: sourceRef.current,
    })
  }

  const handleCancel = () => {
    void (window as any).electronAPI.cancelScriptEditor()
  }

  return (
    <div
      className="d-flex flex-column p-3 gap-2"
      style={{ height: '100vh', background: '#0d0d1a', color: '#fff' }}
    >
      {/* Cabecera */}
      <div className="d-flex align-items-center gap-2 mb-1">
        <FileEarmarkCode size={18} className="text-warning" />
        <span className="fw-semibold" style={{ fontSize: '0.95rem' }}>{isEditing ? 'Editar Script Lua' : 'Nuevo Script Lua'}</span>
      </div>

      {/* Nombre del script */}
      <input
        type="text"
        placeholder="Nombre del script (ej: enemigo_movimiento)..."
        value={name}
        onChange={(e) => setName(e.target.value)}
        className="form-control form-control-sm bg-dark text-light border-secondary"
        autoFocus
        onKeyDown={(e) => { if (e.key === 'Enter') handleSave() }}
      />

      {/* Editor Monaco */}
      <div className="flex-fill rounded overflow-hidden border border-secondary" style={{ minHeight: 0 }}>
        <Editor
          height="100%"
          defaultLanguage="lua"
          defaultValue={DEFAULT_SCRIPT}
          theme="vs-dark"
          onChange={(val) => { sourceRef.current = val ?? '' }}
          onMount={handleMount}
          options={{
            fontSize:          13,
            minimap:           { enabled: false },
            scrollBeyondLastLine: false,
            wordWrap:          'on',
            tabSize:           2,
            insertSpaces:      true,
            automaticLayout:   true,
            lineNumbersMinChars: 3,
            padding:           { top: 8 },
          }}
        />
      </div>

      {/* Acciones */}
      <div className="d-flex gap-2 justify-content-end">
        <button
          className="btn btn-sm btn-outline-secondary"
          onClick={handleCancel}
        >
          Cancelar
        </button>
        <button
          className="btn btn-sm btn-success"
          disabled={!name.trim()}
          onClick={handleSave}
        >
          Guardar script
        </button>
      </div>
    </div>
  )
}

export default ScriptEditorApp
