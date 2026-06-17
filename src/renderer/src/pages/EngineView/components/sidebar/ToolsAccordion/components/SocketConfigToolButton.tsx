import { createElement } from 'react'

import { Diagram3 } from 'react-bootstrap-icons'

import { AppTooltip } from '@components'
import { useContextEngine } from '@engine'
import { useTraslate } from '@hooks'
import { useModal } from '@modal'
import { SocketConfigModalBody } from '../../../../../../modal-electron/SocketConfigModalElectronHost'

interface Props {
	projectType: import('@shared-types').ProjectType
}

export function SocketConfigToolButton({ projectType }: Props) {
	const { t } = useTraslate()
	const { engineReady } = useContextEngine()
	const { openModal } = useModal()

	const is3D = projectType === '3D'
	if (!is3D) return null

	const handleOpen = () => {
		openModal({
			title: t('Socket configuration'),
			size: 'sm',
			body: createElement(SocketConfigModalBody),
		})
	}

	return (
		<AppTooltip
			content={
				<>
					{t('Create sockets and link objects to them.')}
					<br />
					{t('Click an entity in the viewport if none is selected yet.')}
				</>
			}
			place="bottom"
		>
			<button
				type="button"
				className="btn btn-sm btn-outline-primary mb-2 d-flex flex-column justify-content-center align-items-center"
				style={{ height: 64, width: 64 }}
				onClick={handleOpen}
				disabled={!engineReady}
			>
				<span style={{ fontSize: 9, lineHeight: 1.1 }}>{t('Socket')}</span>
				<Diagram3 className="my-1" size={20} />
				<span style={{ fontSize: 9, lineHeight: 1.1 }}>{t('Config')}</span>
			</button>
		</AppTooltip>
	)
}
