import { Puzzle } from 'react-bootstrap-icons'

import { AppTooltip } from '@components'
import { useModal } from '@modal'
import { useTraslate } from '@hooks'
import { PluginsModalBody } from './PluginsModalBody'

export function PluginsButton() {
  const { t } = useTraslate()
  const { openModal } = useModal()

  const openPlugins = () => {
    openModal({
      title: t('Plugins'),
      body: <PluginsModalBody />,
      size: 'lg',
    })
  }

  return (
    <AppTooltip content={t('Browse optional plugins')} place="top">
      <button
        type="button"
        className="btn btn-outline-secondary btn-sm w-100 d-flex align-items-center justify-content-center gap-2 fw-semibold"
        onClick={openPlugins}
        data-plugin-target="plugins-button"
      >
        <Puzzle size={16} />
        <span>{t('Plugins')}</span>
      </button>
    </AppTooltip>
  )
}

export default PluginsButton
