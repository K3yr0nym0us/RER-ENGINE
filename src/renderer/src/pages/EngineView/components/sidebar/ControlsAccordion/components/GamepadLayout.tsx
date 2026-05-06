import type { ControlScript } from '@hooks'
import { KeyBtn } from './KeyBtn'

interface GamepadLayoutProps {
  bindings: Record<string, ControlScript>
  onOpenScriptEditor: (k: string) => void
}

export function GamepadLayout({ bindings, onOpenScriptEditor }: GamepadLayoutProps) {
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
