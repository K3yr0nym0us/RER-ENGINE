import { useScripting } from '../../../hooks/useScripting'
import { ScriptingAccordion } from './ScriptingAccordion'

/**
 * Panel de scripting Lua para la entidad seleccionada.
 * Delega la UI al componente ScriptingAccordion y la lógica al hook useScripting.
 */
export function ScriptingPanel() {
  const { scripts, openEditor, editScript, removeScript } = useScripting()

  return (
    <ScriptingAccordion
      scripts={scripts}
      onNew={openEditor}
      onEdit={editScript}
      onRemove={removeScript}
    />
  )
}

export default ScriptingPanel
