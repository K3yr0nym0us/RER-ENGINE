import { GlobeAmericas } from 'react-bootstrap-icons'

import { AppTooltip } from '@components'
import { useLanguage } from '../context/LanguageContext'
import { useTraslate } from '@hooks'
import { openWorldGenPluginModal } from '../plugins/worldgen/WorldGenModalBody'

/** Botón del sidebar que abre la modal del World Generator. */
export function WorldGenButton() {
  const { t } = useTraslate()
  const { locale } = useLanguage()

  const openWorldGen = () => {
    openWorldGenPluginModal(t('World Generator'), locale)
  }

  return (
    <AppTooltip content={t('World Generator')} place="top">
      <button
        type="button"
        className="btn btn-outline-secondary btn-sm w-100 d-flex align-items-center justify-content-center gap-2 fw-semibold"
        onClick={openWorldGen}
        data-plugin-target="worldgen-button"
      >
        <GlobeAmericas size={16} />
        <span>{t('World Generator')}</span>
      </button>
    </AppTooltip>
  )
}

export default WorldGenButton
