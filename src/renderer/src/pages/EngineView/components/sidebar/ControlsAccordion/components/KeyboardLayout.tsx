import type { ControlScript } from '@hooks'
import { KeyBtn } from './KeyBtn'

interface KeyboardLayoutProps {
  bindings: Record<string, ControlScript>
  onOpenScriptEditor: (k: string) => void
}

export function KeyboardLayout({ bindings, onOpenScriptEditor }: KeyboardLayoutProps) {
  const B = (k: string, wide?: boolean) => (
    <KeyBtn key={k} controlKey={k} bindings={bindings} onOpenScriptEditor={onOpenScriptEditor} wide={wide} />
  )

  return (
    <div className="row g-3">
      {/* Sección teclado */}
      <div className="col-8">
        <div className="ckb-section">
          <span className="ckb-section-title">Teclado</span>
          <hr className="ckb-section-hr" />
          <div className="ckb-keyboard-keys">
            <div className="ckb-row">
              {['1','2','3','4','5','6','7','8','9','0'].map(k => B(k))}
            </div>
            <div className="ckb-row">
              {['Q','W','E','R'].map(k => B(k))}
            </div>
            <div className="ckb-row">
              {['A','S','D','F'].map(k => B(k))}
            </div>
            <div className="ckb-row">
              {['Z','X','C'].map(k => B(k))}
            </div>
            <div className="ckb-row">
              {B('SHIFT')}
              {B('CTRL')}
              {B('ALT')}
              {B('SPACE', true)}
            </div>
          </div>
        </div>
      </div>
      {/* Sección mouse */}
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
