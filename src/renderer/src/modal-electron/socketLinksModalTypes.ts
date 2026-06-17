import type { EntitySocket3D } from '@shared-types'

import type { SocketAttachmentLink } from '../utils/entity3dEditorSync'

export type SocketLinksPickPhase = 'host' | 'child' | 'idle'

export interface SocketLinksModalState {
	hostEntityId: number | null
	hostEntityName: string
	sockets: EntitySocket3D[]
	attachments: SocketAttachmentLink[]
	pickPhase: SocketLinksPickPhase
	pendingSocketName: string | null
	statusMessage: 'awaiting_host' | 'invalid_host' | 'invalid_child' | null
}

export type SocketLinksModalAction =
	| { action: 'close' }
	| { action: 'requestSockets' }
	| { action: 'startLink'; socketName: string }
	| { action: 'detach'; childId: number }
	| { action: 'cancelPick' }
