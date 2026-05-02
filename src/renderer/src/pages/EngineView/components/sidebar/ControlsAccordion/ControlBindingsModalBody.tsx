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

export const KEYBOARD_KEYS = ['1','2','3','4','5','6','7','8','9','0','W','A','S','D','SPACE','CTRL','ALT','SHIFT','MOUSE_LEFT','MOUSE_RIGHT','MOUSE_MIDDLE']
export const GAMEPAD_KEYS = ['A','B','X','Y','LB','RB','LT','RT','START','BACK','D-UP','D-DOWN','D-LEFT','D-RIGHT','L3','R3']

const KEY_LABELS: Record<string, string> = {
  MOUSE_LEFT: 'M.IZQ',
  MOUSE_RIGHT: 'M.DER',
  MOUSE_MIDDLE: 'M.MED',
}

interface KeyBtnProps {
  controlKey: string
  bindings: Record<string, ControlScript>
  onOpenScriptEditor: (k: string) => void
  wide?: boolean
}

function KeyBtn({ controlKey, bindings, onOpenScriptEditor, wide }: KeyBtnProps) {
  const assigned = bindings[controlKey]
  const label = KEY_LABELS[controlKey] ?? controlKey
  return (
    <AppTooltip
      content={assigned ? `${controlKey}: ${assigned.name}` : `${controlKey}: sin script`}
      place="top"
    >
      <button
        type="button"
        className={`control-key-btn${assigned ? ' control-key-btn--assigned' : ''}${wide ? ' control-key-btn--wide' : ''}`}
        onDoubleClick={() => onOpenScriptEditor(controlKey)}
      >
        <span className="control-key-btn__key">{label}</span>
        {assigned && <span className="control-key-btn__dot" />}
      </button>
    </AppTooltip>
  )
}

interface LayoutProps {
  bindings: Record<string, ControlScript>
  onOpenScriptEditor: (k: string) => void
}

function KeyboardLayout({ bindings, onOpenScriptEditor }: LayoutProps) {
  const B = (k: string, wide?: boolean) => (
    <KeyBtn key={k} controlKey={k} bindings={bindings} onOpenScriptEditor={onOpenScriptEditor} wide={wide} />
  )
  return (
    <div className="row g-3">
      {/* Seccion teclado */}
      <div className="col-8">
        <div className="ckb-section">
          <span className="ckb-section-title">Teclado</span>
          <hr className="ckb-section-hr" />
          <div className="ckb-keyboard-keys">
            <div className="ckb-row">
              {['1','2','3','4','5','6','7','8','9','0'].map(k => B(k))}
            </div>
            <div className="ckb-row">
              {['W','A','S','D'].map(k => B(k))}
            </div>
            <div className="ckb-row">
              {B('CTRL')}
              {B('ALT')}
              {B('SHIFT')}
              {B('SPACE', true)}
            </div>
          </div>
        </div>
      </div>
      {/* Seccion mouse */}
      <div className="col-4">
        <div className="ckb-section">
          <span className="ckb-section-title">Mouse</span>
          <hr className="ckb-section-hr" />
          <div className="ckb-mouse-keys">
            {['MOUSE_LEFT','MOUSE_RIGHT','MOUSE_MIDDLE'].map(k => B(k))}
          </div>
        </div>
      </div>
    </div>
  )
}

function GamepadLayout({ bindings, onOpenScriptEditor }: LayoutProps) {
  const B = (k: string) => (
    <KeyBtn key={k} controlKey={k} bindings={bindings} onOpenScriptEditor={onOpenScriptEditor} />
  )
  return (
    <div className="ckb-gamepad-layout">
      {/* Fila gatillos y bumpers */}
      <div className="ckb-row ckb-row--spread">
        <div className="ckb-row">{B('LT')}{B('LB')}</div>
        <div className="ckb-row">{B('RB')}{B('RT')}</div>
      </div>
      {/* Fila principal: dpad | centro | botones de cara */}
      <div className="ckb-row ckb-row--spread ckb-row--vcenter">
        {/* D-pad */}
        <div className="ckb-dpad">
          <div className="ckb-row ckb-row--center">{B('D-UP')}</div>
          <div className="ckb-row">{B('D-LEFT')}{B('D-RIGHT')}</div>
          <div className="ckb-row ckb-row--center">{B('D-DOWN')}</div>
        </div>
        {/* Centro */}
        <div className="ckb-row ckb-row--center ckb-gamepad-center">
          {B('BACK')}{B('L3')}{B('R3')}{B('START')}
        </div>
        {/* Botones de cara */}
        <div className="ckb-face-btns">
          <div className="ckb-row ckb-row--center">{B('Y')}</div>
          <div className="ckb-row">{B('X')}{B('B')}</div>
          <div className="ckb-row ckb-row--center">{B('A')}</div>
        </div>
      </div>
    </div>
  )
}

export function ControlBindingsModalBody({
  mode,
  characterLabel,
  bindings,
  onOpenScriptEditor,
  onRemoveBinding,
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

      <div className="alert alert-info py-2 mb-0 small">
        Haz <b>doble click</b> sobre cualquier tecla/boton para abrir la modal de script y asignarlo.
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

      {hasBindings && (
        <div className="d-flex flex-column gap-2">
          {Object.entries(bindings).map(([controlKey, script]) => (
            <div
              key={controlKey}
              className="d-flex align-items-center gap-2 p-2 rounded border border-secondary bg-dark"
            >
              {mode === 'keyboard_mouse' ? <Keyboard size={14} className="text-info" /> : <Joystick size={14} className="text-info" />}
              <span className="badge bg-dark border border-secondary flex-shrink-0">{controlKey}</span>
              <AppTooltip content={script.name} place="top">
                <span className="text-light small text-truncate flex-fill">{script.name}</span>
              </AppTooltip>
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
