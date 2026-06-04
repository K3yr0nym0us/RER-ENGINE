import { Tools } from 'react-bootstrap-icons'

import { AppTooltip } from '@components'
import { BluePrintModalBody } from './BluePrintModalBody'
import { useContextEngine } from '@engine'
import { useQuickBuild } from '../../../../../../context/QuickBuildContext'
import { useTraslate } from '@hooks'
import { useModal } from '@modal'

export function QuickBuildToolButton() {
  const { t } = useTraslate()
  const { engineReady } = useContextEngine()
  const { openModal } = useModal()
  const { activeBluePrint, setActiveBluePrint } = useQuickBuild()

  const handleOpenBluePrint = () => {
    openModal({
      title: t('Construction'),
      size: 'lg',
      body: <BluePrintModalBody />,
    })
  }

  const handleToggleBluePrint = () => {
    if (activeBluePrint) {
      setActiveBluePrint(null)
      return
    }
    handleOpenBluePrint()
  }

  return (
    <AppTooltip
      content={
        activeBluePrint
          ? <>{t('Active mode:')} <b>{activeBluePrint.name}</b><br />{t('Click to deactivate')}</>
          : <>{t('Quick build.')}<br /><b>({t('based on BluePrints')})</b></>
      }
      place="bottom"
    >
      <button
        className={`btn btn-sm mb-2 d-flex flex-column justify-content-center align-items-center ${
          activeBluePrint ? 'btn-warning' : 'btn-outline-warning'
        }`}
        style={{ height: 64, width: 64 }}
        onClick={handleToggleBluePrint}
        disabled={!engineReady}
        aria-pressed={!!activeBluePrint}
      >
        <span style={{ fontSize: 9, lineHeight: 1.1 }}>
          {activeBluePrint ? t('Active') : t('Construction')}
        </span>
        <Tools className="my-1" size={20} />
        <span style={{ fontSize: 9, lineHeight: 1.1 }}>
          {activeBluePrint ? '? ON' : t('Quick')}
        </span>
      </button>
    </AppTooltip>
  )
}
