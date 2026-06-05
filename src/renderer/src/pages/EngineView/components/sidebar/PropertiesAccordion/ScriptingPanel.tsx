import { useScripting } from '@hooks';
import { ScriptingAccordion } from './ScriptingAccordion';

/**
 * Panel de scripting Rhai para la entidad seleccionada.
 * Delega la UI al componente ScriptingAccordion y la lógica al hook useScripting.
 */
export function ScriptingPanel() {
  const { scripts, openEditor, openVisualScripting, editScript, removeScript } = useScripting()

  return (
    <ScriptingAccordion
      scripts={scripts}
      onNew={openEditor}
      onVisual={openVisualScripting}
      onEdit={editScript}
      onRemove={removeScript}
    />
  )
}

export default ScriptingPanel
