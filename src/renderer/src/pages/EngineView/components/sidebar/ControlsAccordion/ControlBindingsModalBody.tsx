import { Joystick, Keyboard, Pencil, Trash } from 'react-bootstrap-icons';

import AppTooltip from '../../../../../components/AppTooltip';
import type { ControlDeviceMode, ControlScript } from '../../../../../hooks/useControlBindings';

interface ControlBindingsModalBodyProps {
  mode: ControlDeviceMode
  characterLabel: string
  bindings: Record<string, ControlScript>
  onOpenScriptEditor: (controlKey: string) => void
  onRemoveBinding: (controlKey: string) => void
}

const KEYBOARD_KEYS = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0', 'W', 'A', 'S', 'D', 'SPACE', 'CTRL', 'SHIFT', 'MOUSE_LEFT', 'MOUSE_RIGHT', 'MOUSE_MIDDLE']
const GAMEPAD_KEYS = ['A', 'B', 'X', 'Y', 'LB', 'RB', 'LT', 'RT', 'START', 'BACK', 'D-UP', 'D-DOWN', 'D-LEFT', 'D-RIGHT', 'L3', 'R3']

export function ControlBindingsModalBody({
  mode,
  characterLabel,
  bindings,
  onOpenScriptEditor,
  onRemoveBinding,
}: ControlBindingsModalBodyProps) {
  const keys = mode === 'keyboard_mouse' ? KEYBOARD_KEYS : GAMEPAD_KEYS
  const hasBindings = Object.keys(bindings).length > 0

  return (
    <div className="d-flex flex-column gap-3">
      <div className="d-flex align-items-center justify-content-between flex-wrap gap-2">
        <div className="text-light small">
          Personaje seleccionado: <b>{characterLabel}</b>
        </div>
        <span className="badge bg-secondary">
          {Object.keys(bindings).length} asignaciones
        </span>
      </div>

      <div className="alert alert-info py-2 mb-0 small">
        Haz <b>doble click</b> sobre cualquier tecla/boton para abrir la modal de script y asignarlo.
      </div>

      <div className="control-grid-placeholder">
        {keys.map((controlKey) => {
          const assigned = bindings[controlKey]
          return (
            <AppTooltip
              key={controlKey}
              content={assigned ? `${controlKey}: ${assigned.name}` : `${controlKey}: sin script`}
              place="top"
            >
              <button
                type="button"
                className={`control-key-btn ${assigned ? 'control-key-btn--assigned' : ''}`}
                onDoubleClick={() => onOpenScriptEditor(controlKey)}
              >
                <span className="control-key-btn__key">{controlKey}</span>
                {assigned && <span className="control-key-btn__dot" />}
              </button>
            </AppTooltip>
          )
        })}
      </div>

      {!hasBindings && (
        <div className="text-secondary small fst-italic">
          No hay scripts asignados todavia para este modo.
        </div>
      )}

      {hasBindings && (
        <div className="d-flex flex-column gap-2">
          {Object.entries(bindings).map(([controlKey, script]) => (
            <div
              key={controlKey}
              className="d-flex align-items-center gap-2 p-2 rounded border border-secondary bg-dark"
            >
              {mode === 'keyboard_mouse' ? <Keyboard size={14} className="text-info" /> : <Joystick size={14} className="text-info" />}
              <span className="badge bg-dark border border-secondary">{controlKey}</span>
              <span className="text-light small text-truncate flex-fill">{script.name}</span>

              <button
                className="btn btn-sm btn-outline-primary p-1 lh-1"
                onClick={() => onOpenScriptEditor(controlKey)}
                type="button"
              >
                <Pencil size={12} />
              </button>
              <button
                className="btn btn-sm btn-outline-danger p-1 lh-1"
                onClick={() => onRemoveBinding(controlKey)}
                type="button"
              >
                <Trash size={12} />
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

export default ControlBindingsModalBody
