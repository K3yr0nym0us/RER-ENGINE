import { AppTooltip } from '@components'
import type { ControlScript } from '@hooks'

const KEY_LABELS: Record<string, string> = {
  MOUSE_LEFT:   'MOUSE LEFT',
  MOUSE_RIGHT:  'MOUSE RIGHT',
  MOUSE_MIDDLE: 'MOUSE MIDDLE',
}

interface KeyBtnProps {
  controlKey: string
  bindings: Record<string, ControlScript>
  onOpenScriptEditor: (k: string) => void
  wide?: boolean
}

export function KeyBtn({ controlKey, bindings, onOpenScriptEditor, wide }: KeyBtnProps) {
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
