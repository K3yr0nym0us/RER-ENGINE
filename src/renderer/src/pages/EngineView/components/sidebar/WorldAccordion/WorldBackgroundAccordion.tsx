import { useEffect, useState } from 'react'
import { Accordion } from 'react-bootstrap'
import { CheckLg, Image } from 'react-bootstrap-icons'

import { AppTooltip } from '@components'
import { useContextEngine } from '@engine'
import { useModal } from '@modal'
import { ModalConfirmBody } from '../../../../../modal-electron/ModalConfirmBody'
import { useTraslate } from '@hooks'

export default function WorldBackgroundAccordion() {
	const { t } = useTraslate()
	const { engineReady, backgroundPath, backgrounds, setBackground } = useContextEngine()
	const { openModal } = useModal()
	const [selectedBg, setSelectedBg] = useState(backgroundPath ?? '')

	useEffect(() => {
		setSelectedBg(backgroundPath ?? '')
	}, [backgroundPath])

	const handleApplyBackground = () => {
		if (!selectedBg) return
		const selectedBackground = backgrounds.find((bg) => bg.path === selectedBg)
		openModal({
			title: t('Apply Background'),
			size: 'sm',
			body: (
				<ModalConfirmBody
					confirmVariant="primary"
					confirmLabel={t('Apply')}
					message={
						<>
							<p className="mb-2">{t('Apply selected background to current scene?')}</p>
							<p className="mb-0">
								<strong>{selectedBackground?.name ?? selectedBg}</strong>
							</p>
						</>
					}
					onConfirm={() => setBackground(selectedBg)}
				/>
			),
		})
	}

	return (
		<Accordion.Item eventKey="world-background">
			<Accordion.Header>
				<Image className="me-2" />
				{t('World background')}
			</Accordion.Header>
			<Accordion.Body className="py-2 px-2">
				<div className="d-flex gap-1">
					<select
						className="form-select form-select-sm bg-dark text-light border-secondary flex-fill"
						value={selectedBg}
						disabled={!engineReady || backgrounds.length === 0}
						onChange={(e) => setSelectedBg(e.target.value)}
					>
						{backgrounds.length === 0 && <option value="">{t('No backgrounds loaded')}</option>}
						{backgrounds.length > 0 && <option value="">{t('— Select background —')}</option>}
						{backgrounds.map((bg) => (
							<option key={bg.path} value={bg.path}>
								{bg.name}
							</option>
						))}
					</select>
					<AppTooltip content={t('Apply background')} place="top">
						<button
							className="btn btn-sm btn-outline-info"
							disabled={!engineReady || !selectedBg}
							onClick={handleApplyBackground}
						>
							<CheckLg />
						</button>
					</AppTooltip>
				</div>
			</Accordion.Body>
		</Accordion.Item>
	)
}
