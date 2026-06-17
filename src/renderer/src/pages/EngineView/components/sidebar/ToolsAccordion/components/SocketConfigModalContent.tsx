import { useEffect, useState } from 'react'

import { Nav, Tab } from 'react-bootstrap'
import { Diagram3, Link45deg } from 'react-bootstrap-icons'

import type { SocketConfigModalAction, SocketConfigModalState } from '../../../../../../modal-electron/socketConfigModalTypes'
import { SocketsModalContent } from './SocketsModalContent'
import { SocketLinksModalContent } from './SocketLinksModalContent'
import { useTraslate } from '@hooks'

export interface SocketConfigModalContentProps {
	state: SocketConfigModalState
	onAction: (action: SocketConfigModalAction) => void
}

type ConfigTab = SocketConfigModalState['activeTab']

export function SocketConfigModalContent({ state, onAction }: SocketConfigModalContentProps) {
	const { t } = useTraslate()
	const [activeTab, setActiveTab] = useState<ConfigTab>(state.activeTab)

	useEffect(() => {
		setActiveTab(state.activeTab)
	}, [state.activeTab])

	const handleTabSelect = (tab: string | null) => {
		if (tab !== 'create' && tab !== 'links') return
		setActiveTab(tab)
		onAction({ action: 'setTab', tab })
	}

	return (
		<div>
			<Nav variant="tabs" className="mb-3 border-secondary">
				<Nav.Item>
					<Nav.Link
						eventKey="create"
						active={activeTab === 'create'}
						onClick={() => handleTabSelect('create')}
						className="text-light"
					>
						<Diagram3 className="me-1" />
						{t('Create sockets')}
					</Nav.Link>
				</Nav.Item>
				<Nav.Item>
					<Nav.Link
						eventKey="links"
						active={activeTab === 'links'}
						onClick={() => handleTabSelect('links')}
						className="text-light"
					>
						<Link45deg className="me-1" />
						{t('Link sockets')}
					</Nav.Link>
				</Nav.Item>
			</Nav>

			<Tab.Content>
				{activeTab === 'create' && (
					<SocketsModalContent
						state={state.create}
						onAction={(payload) => onAction({ action: 'create', payload })}
						onRequestEntityPick={() => onAction({ action: 'focusEntityPick' })}
					/>
				)}
				{activeTab === 'links' && (
					<SocketLinksModalContent
						state={state.links}
						onAction={(payload) => onAction({ action: 'links', payload })}
						onRequestHostPick={() => onAction({ action: 'focusHostPick' })}
					/>
				)}
			</Tab.Content>
		</div>
	)
}
