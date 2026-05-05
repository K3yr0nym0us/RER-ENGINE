import { Tools } from 'react-bootstrap-icons'

import AppTooltip from '../../../../../../components/AppTooltip'
import { BluePrintModalBody } from './BluePrintModalBody'
import { useContextEngine } from '@engine'
import { useModal } from '@modal'
import { useQuickBuild } from '../../../../../../context/QuickBuildContext'

export function QuickBuildToolButton() {
  const { engineReady } = useContextEngine()
  const { openModal } = useModal()
  const { activeBluePrint, setActiveBluePrint } = useQuickBuild()

  const handleOpenBluePrint = () => {
    openModal({
      title: 'Construcción',
      body: <BluePrintModalBody />,
      size: 'lg',
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
          ? <>Modo activo: <b>{activeBluePrint.name}</b><br />Click para desactivar</>
          : <>Construcción rápida.<br /><b>(basada en BluePrints)</b></>
      }
      place="left"
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
          {activeBluePrint ? 'Activo' : 'Construcción'}
        </span>
        <Tools className="my-1" size={20} />
        <span style={{ fontSize: 9, lineHeight: 1.1 }}>
          {activeBluePrint ? '⬡ ON' : 'Rápida'}
        </span>
      </button>
    </AppTooltip>
  )
}
