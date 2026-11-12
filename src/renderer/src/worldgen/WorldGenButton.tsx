import { Globe } from 'react-bootstrap-icons'

import { AppTooltip } from '@components'
import { useModal } from '@modal'
import { useTraslate } from '@hooks'
import { WorldGenPanelBody } from './WorldGenPanel'

/** Botón de la barra lateral que abre el panel del generador de mundos. */
export function WorldGenButton() {
  const { t } = useTraslate()
  const { openModal } = useModal()

  const openPanel = () => {
    openModal({
      title: t('World Generator'),
      body: <WorldGenPanelBody />,
      size: 'md',
    })
  }

  return (
    <AppTooltip content={t('Procedural world generation')} place="top">
      <button
        type="button"
        className="btn btn-outline-secondary btn-sm w-100 d-flex align-items-center justify-content-center gap-2 fw-semibold"
        onClick={openPanel}
        data-plugin-target="worldgen-button"
      >
        <Globe size={16} />
        <span>{t('World Gen')}</span>
      </button>
    </AppTooltip>
  )
}

export default WorldGenButton
