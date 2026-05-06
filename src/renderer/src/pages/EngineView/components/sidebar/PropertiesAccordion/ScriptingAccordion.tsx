import { Accordion } from 'react-bootstrap';
import { FileEarmarkCode, Pencil, Plus, Trash } from 'react-bootstrap-icons';

import { AppTooltip } from '@components';
import type { ScriptEntry } from '@hooks';
import { useTraslate } from '@hooks';

interface ScriptingAccordionProps {
  scripts:  ScriptEntry[]
  onNew:    () => void
  onEdit:   (name: string) => void
  onRemove: (name: string) => void
}

export function ScriptingAccordion({ scripts, onNew, onEdit, onRemove }: ScriptingAccordionProps) {
  const { t } = useTraslate()
  return (
    <Accordion.Item eventKey="scripting">
      <Accordion.Header>Scripting (Lua)</Accordion.Header>
      <Accordion.Body className="py-2 px-2">

        <button
          className="btn btn-sm btn-outline-warning w-100 mb-2"
          onClick={onNew}
        >
          <Plus size={15} className="me-1" />
          {t('New Script')}
        </button>

        {scripts.length === 0 && (
          <div className="alert py-1 text-center mb-0" role="alert">
            {t('No scripts attached.')}
          </div>
        )}

        {scripts.map((s) => (
          <div
            key={s.name}
            className="d-flex align-items-center gap-2 mb-1 p-2 rounded border border-secondary bg-dark"
          >
            <FileEarmarkCode size={14} className="text-warning flex-shrink-0" />
            <AppTooltip content={s.source.slice(0, 160)} place="top">
              <span className="text-light small text-truncate flex-fill">
                {s.name}
              </span>
            </AppTooltip>
            <AppTooltip content={t('Edit script')} place="top">
              <button
                className="btn btn-sm btn-outline-primary p-1 lh-1"
                onClick={() => onEdit(s.name)}
              >
                <Pencil size={12} />
              </button>
            </AppTooltip>
            <AppTooltip content={t('Remove script')} place="top">
              <button
                className="btn btn-sm btn-outline-danger p-1 lh-1"
                onClick={() => onRemove(s.name)}
              >
                <Trash size={12} />
              </button>
            </AppTooltip>
          </div>
        ))}

      </Accordion.Body>
    </Accordion.Item>
  )
}

export default ScriptingAccordion
