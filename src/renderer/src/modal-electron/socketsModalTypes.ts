import type { EntitySocket3D } from '@shared-types'

export interface SocketBonePickedResult {
	entityId: number
	boneName: string
	seq: number
}

export interface SocketsModalState {
	entityId: number | null
	entityName: string
	sockets: EntitySocket3D[]
	socketBonePicked: SocketBonePickedResult | null
	/** Clave i18n cuando aún no hay entidad válida. */
	statusMessage: 'awaiting_entity' | 'invalid_entity' | null
}

export type SocketsModalAction =
	| { action: 'close' }
	| { action: 'upsertSocket'; socket: EntitySocket3D }
	| { action: 'removeSocket'; name: string }
	| { action: 'requestSockets' }
	| { action: 'setSocketBonePickMode'; active: boolean }
