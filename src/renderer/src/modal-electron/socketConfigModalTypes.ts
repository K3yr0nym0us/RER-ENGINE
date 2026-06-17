import type { SocketsModalAction, SocketsModalState } from './socketsModalTypes'
import type { SocketLinksModalAction, SocketLinksModalState } from './socketLinksModalTypes'

export type SocketConfigTab = 'create' | 'links'

export interface SocketConfigModalState {
	activeTab: SocketConfigTab
	create: SocketsModalState
	links: SocketLinksModalState
}

type CreateAction = Exclude<SocketsModalAction, { action: 'close' }>
type LinksAction = Exclude<SocketLinksModalAction, { action: 'close' }>

export type SocketConfigModalAction =
	| { action: 'close' }
	| { action: 'setTab'; tab: SocketConfigTab }
	| { action: 'focusEntityPick' }
	| { action: 'focusHostPick' }
	| { action: 'create'; payload: CreateAction }
	| { action: 'links'; payload: LinksAction }
