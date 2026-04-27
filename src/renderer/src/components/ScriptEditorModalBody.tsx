import { useState, useRef } from 'react'
import Editor, { type OnMount } from '@monaco-editor/react'
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

interface ScriptEditorModalBodyProps {
  initialData?: { name: string; source: string }
  onSave:       (data: { name: string; source: string }) => void
  onCancel:     () => void
}

/**
 * Cuerpo del editor de scripts Lua para usar dentro de ModalProvider.
 * Contiene el input de nombre, el editor Monaco y los botones de acción.
 */
export function ScriptEditorModalBody({ initialData, onSave, onCancel }: ScriptEditorModalBodyProps) {
  const [name, setName] = useState(initialData?.name ?? '')
  const sourceRef        = useRef<string>(initialData?.source ?? DEFAULT_SCRIPT)
  const editorRef        = useRef<Monaco.editor.IStandaloneCodeEditor | null>(null)

  const handleMount: OnMount = (editor) => {
    editorRef.current = editor
    editor.focus()
  }

  const handleSave = () => {
    if (!name.trim()) return
    onSave({ name: name.trim(), source: sourceRef.current })
  }

  return (
    <div className="d-flex flex-column gap-2" style={{ height: '60vh', minHeight: 320 }}>

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
          defaultValue={initialData?.source ?? DEFAULT_SCRIPT}
          theme="vs-dark"
          onChange={(val) => { sourceRef.current = val ?? '' }}
          onMount={handleMount}
          options={{
            fontSize:             13,
            minimap:              { enabled: false },
            scrollBeyondLastLine: false,
            wordWrap:             'on',
            tabSize:              2,
            insertSpaces:         true,
            automaticLayout:      true,
            lineNumbersMinChars:  3,
            padding:              { top: 8 },
          }}
        />
      </div>

      {/* Acciones */}
      <div className="d-flex gap-2 justify-content-end">
        <button className="btn btn-sm btn-outline-secondary" onClick={onCancel}>
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

export default ScriptEditorModalBody
