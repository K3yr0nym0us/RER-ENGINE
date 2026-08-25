import type { ControlScript } from '@hooks'
import { KeyBtn } from './KeyBtn'

interface GamepadLayoutProps {
  bindings: Record<string, ControlScript>
  onOpenScriptEditor: (k: string) => void
}

export function GamepadLayout({ bindings, onOpenScriptEditor }: GamepadLayoutProps) {
  const renderKey = (k: string) => (
    <KeyBtn key={k} controlKey={k} bindings={bindings} onOpenScriptEditor={onOpenScriptEditor} />
  )

  return (
    <div className="ckb-gamepad-layout">
      {/* Fila gatillos y bumpers */}
      <div className="ckb-row ckb-row--spread">
        <div className="ckb-row">{renderKey('LT')}{renderKey('LB')}</div>
        <div className="ckb-row">{renderKey('RB')}{renderKey('RT')}</div>
      </div>
      {/* Fila principal: dpad | centro | botones de cara */}
      <div className="ckb-row ckb-row--spread ckb-row--vcenter">
        {/* D-pad */}
        <div className="ckb-dpad">
          <div className="ckb-row ckb-row--center">{renderKey('D-UP')}</div>
          <div className="ckb-row">{renderKey('D-LEFT')}{renderKey('D-RIGHT')}</div>
          <div className="ckb-row ckb-row--center">{renderKey('D-DOWN')}</div>
        </div>
        {/* Centro */}
        <div className="ckb-row ckb-row--center ckb-gamepad-center">
          {renderKey('BACK')}{renderKey('L3')}{renderKey('R3')}{renderKey('START')}
        </div>
        {/* Botones de cara */}
        <div className="ckb-face-btns">
          <div className="ckb-row ckb-row--center">{renderKey('Y')}</div>
          <div className="ckb-row">{renderKey('X')}{renderKey('B')}</div>
          <div className="ckb-row ckb-row--center">{renderKey('A')}</div>
        </div>
      </div>
    </div>
  )
}
