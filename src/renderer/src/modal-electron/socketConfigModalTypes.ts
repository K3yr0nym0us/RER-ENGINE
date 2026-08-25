import type { SocketsModalAction, SocketsModalState } from './socketsModalTypes'
import type { SocketLinksModalAction, SocketLinksModalState } from './socketLinksModalTypes'

export type SocketConfigTab = 'create' | 'links'

export interface SocketConfigModalState {
	activeTab: SocketConfigTab
	create: SocketsModalState
	links: SocketLinksModalState
}

export type SocketConfigModalAction =
	| { action: 'close' }
	| { action: 'setTab'; tab: SocketConfigTab }
	| { action: 'focusEntityPick' }
	| { action: 'focusHostPick' }
	| { action: 'create'; payload: SocketsModalAction }
	| { action: 'links'; payload: SocketLinksModalAction }
