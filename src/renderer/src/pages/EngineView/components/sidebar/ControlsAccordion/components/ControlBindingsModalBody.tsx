import { InfoCircleFill } from 'react-bootstrap-icons';

import { GamepadLayout } from './GamepadLayout';
import { KeyboardLayout } from './KeyboardLayout';

import type { ControlDeviceMode, ControlScript } from '@hooks';

interface ControlBindingsModalBodyProps {
  mode: ControlDeviceMode
  characterLabel: string
  bindings: Record<string, ControlScript>
  onOpenScriptEditor: (controlKey: string) => void
}

export function ControlBindingsModalBody({
  mode,
  characterLabel,
  bindings,
  onOpenScriptEditor,
}: ControlBindingsModalBodyProps) {
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

      {mode === 'keyboard_mouse'
        ? <KeyboardLayout bindings={bindings} onOpenScriptEditor={onOpenScriptEditor} />
        : <GamepadLayout bindings={bindings} onOpenScriptEditor={onOpenScriptEditor} />
      }

      {!hasBindings && (
        <div className="text-secondary small fst-italic">
          No hay scripts asignados todavia para este modo.
        </div>
      )}

      <div className="border-top border-secondary pt-2 text-info fw-bold small d-flex align-items-center gap-2">
        <InfoCircleFill size={14} className="flex-shrink-0" />
        <span>Haz <b>doble click</b> sobre cualquier tecla/boton para abrir la modal de script y asignarlo.</span>
      </div>
    </div>
  )
}

export default ControlBindingsModalBody
