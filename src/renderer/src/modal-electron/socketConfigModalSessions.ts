import type { EngineContextValue } from '@engine'
import type { SocketConfigModalAction, SocketConfigModalState, SocketConfigTab } from './socketConfigModalTypes'
import type { SocketsModalState } from './socketsModalTypes'
import type { SocketLinksModalState } from './socketLinksModalTypes'
import {
	entityCanHaveSockets,
	listSocketAttachmentsForHost,
	resolveEntityEditorName,
	resolveSocketLinksModalHost,
	socketAttachChildCandidates,
} from '../utils/entity3dEditorSync'

let socketBonePickSeq = 0
let socketBonePickResult: SocketsModalState['socketBonePicked'] = null

export function recordSocketBonePick(entityId: number, boneName: string): void {
	socketBonePickSeq += 1
	socketBonePickResult = { entityId, boneName, seq: socketBonePickSeq }
}

export const activeSocketConfigModalHandlerRef = { current: null as string | null }

export interface SocketConfigModalSessionDeps {
	getEngine: () => EngineContextValue
	closeModal: () => void
	pushPatch: (handlerId: string, state: SocketConfigModalState) => void
}

interface Session {
	handlerId: string
	deps: SocketConfigModalSessionDeps
	activeTab: SocketConfigTab
	targetEntityId: number | null
	createLastInvalidPick: boolean
	hostEntityId: number | null
	pickPhase: SocketLinksModalState['pickPhase']
	pendingSocketName: string | null
	linksLastInvalidPick: 'host' | 'child' | null
}

const sessions = new Map<string, Session>()

export function resolveSocketsModalTarget(engine: EngineContextValue): number | null {
	const id = engine.selectedEntity?.id
	if (id == null) return null
	const meta = engine.entityMetaRef.current[id]
	if (!entityCanHaveSockets(id, meta, engine.editorCameraEntityIdRef.current)) {
		return null
	}
	return id
}

function disableBonePick(session: Session): void {
	if (session.targetEntityId == null) return
	session.deps.getEngine().send({
		cmd: 'set_socket_bone_pick_mode',
		entity_id: session.targetEntityId,
		active: false,
	})
}

function buildCreateState(engine: EngineContextValue, session: Session): SocketsModalState {
	const { targetEntityId, createLastInvalidPick } = session
	if (targetEntityId == null) {
		return {
			entityId: null,
			entityName: '',
			sockets: [],
			socketBonePicked: socketBonePickResult,
			statusMessage: createLastInvalidPick ? 'invalid_entity' : 'awaiting_entity',
		}
	}

	const meta = engine.entityMetaRef.current[targetEntityId]
	const entityName = resolveEntityEditorName(
		targetEntityId,
		engine.entityMetaRef.current,
		engine.selectedEntity,
	)

	return {
		entityId: targetEntityId,
		entityName,
		sockets: meta?.sockets ?? [],
		socketBonePicked: socketBonePickResult,
		statusMessage: null,
	}
}

function buildLinksState(engine: EngineContextValue, session: Session): SocketLinksModalState {
	if (session.hostEntityId == null) {
		return {
			hostEntityId: null,
			hostEntityName: '',
			sockets: [],
			attachments: [],
			pickPhase: 'host',
			pendingSocketName: null,
			statusMessage:
				session.linksLastInvalidPick === 'host' ? 'invalid_host' : 'awaiting_host',
		}
	}

	const hostEntityId = session.hostEntityId
	const meta = engine.entityMetaRef.current[hostEntityId]

	return {
		hostEntityId,
		hostEntityName: resolveEntityEditorName(
			hostEntityId,
			engine.entityMetaRef.current,
			engine.selectedEntity,
		),
		sockets: meta?.sockets ?? [],
		attachments: listSocketAttachmentsForHost(
			hostEntityId,
			engine.entityMetaRef.current,
			engine.selectedEntity,
		),
		pickPhase: session.pickPhase,
		pendingSocketName: session.pendingSocketName,
		statusMessage: session.linksLastInvalidPick === 'child' ? 'invalid_child' : null,
	}
}

export function buildSocketConfigModalState(
	engine: EngineContextValue,
	session: Pick<
		Session,
		| 'activeTab'
		| 'targetEntityId'
		| 'createLastInvalidPick'
		| 'hostEntityId'
		| 'pickPhase'
		| 'pendingSocketName'
		| 'linksLastInvalidPick'
	>,
): SocketConfigModalState {
	return {
		activeTab: session.activeTab,
		create: buildCreateState(engine, session as Session),
		links: buildLinksState(engine, session as Session),
	}
}

export function registerSocketConfigModalSession(
	handlerId: string,
	deps: SocketConfigModalSessionDeps,
): SocketConfigModalState {
	const engine = deps.getEngine()
	const targetEntityId = resolveSocketsModalTarget(engine)
	const hostEntityId = resolveSocketLinksModalHost(engine) ?? targetEntityId

	sessions.set(handlerId, {
		handlerId,
		deps,
		activeTab: 'create',
		targetEntityId,
		createLastInvalidPick: false,
		hostEntityId,
		pickPhase: hostEntityId == null ? 'host' : 'idle',
		pendingSocketName: null,
		linksLastInvalidPick: null,
	})
	activeSocketConfigModalHandlerRef.current = handlerId
	return buildSocketConfigModalState(engine, sessions.get(handlerId)!)
}

export function unregisterSocketConfigModalSession(handlerId: string): void {
	const session = sessions.get(handlerId)
	if (session) {
		disableBonePick(session)
	}
	sessions.delete(handlerId)
	if (activeSocketConfigModalHandlerRef.current === handlerId) {
		activeSocketConfigModalHandlerRef.current = null
	}
}

function syncTabEntities(session: Session, tab: SocketConfigTab): void {
	const engine = session.deps.getEngine()
	if (tab === 'links' && session.hostEntityId == null && session.targetEntityId != null) {
		const meta = engine.entityMetaRef.current[session.targetEntityId]
		if ((meta?.sockets?.length ?? 0) > 0) {
			session.hostEntityId = session.targetEntityId
			session.pickPhase = 'idle'
			session.linksLastInvalidPick = null
		}
	}
	if (tab === 'create' && session.targetEntityId == null && session.hostEntityId != null) {
		const meta = engine.entityMetaRef.current[session.hostEntityId]
		if (entityCanHaveSockets(session.hostEntityId, meta, engine.editorCameraEntityIdRef.current)) {
			session.targetEntityId = session.hostEntityId
			session.createLastInvalidPick = false
		}
	}
}

export function tryAssignSocketConfigModalEntity(handlerId: string, entityId: number): void {
	const session = sessions.get(handlerId)
	if (!session) return
	const engine = session.deps.getEngine()
	const meta = engine.entityMetaRef.current[entityId]

	if (session.activeTab === 'create') {
		if (!entityCanHaveSockets(entityId, meta, engine.editorCameraEntityIdRef.current)) {
			session.createLastInvalidPick = true
			session.targetEntityId = null
		} else {
			session.createLastInvalidPick = false
			session.targetEntityId = entityId
		}
		pushSocketConfigModalPatch(handlerId)
		return
	}

	if (session.hostEntityId == null) {
		if ((meta?.sockets?.length ?? 0) === 0) {
			session.linksLastInvalidPick = 'host'
			session.pickPhase = 'host'
		} else {
			session.hostEntityId = entityId
			session.pickPhase = 'idle'
			session.pendingSocketName = null
			session.linksLastInvalidPick = null
			engine.send({ cmd: 'list_entity_sockets', entity_id: entityId })
		}
		pushSocketConfigModalPatch(handlerId)
		return
	}

	if (session.pickPhase !== 'child' || !session.pendingSocketName) {
		return
	}

	const childCandidates = socketAttachChildCandidates(
		[entityId, session.hostEntityId],
		session.hostEntityId,
		engine.entityMetaRef.current,
	)
	if (!childCandidates.includes(entityId)) {
		session.linksLastInvalidPick = 'child'
		pushSocketConfigModalPatch(handlerId)
		return
	}

	engine.send({
		cmd: 'attach_to_socket',
		child_ids: [entityId],
		host_id: session.hostEntityId,
		socket_name: session.pendingSocketName,
	})
	session.pickPhase = 'idle'
	session.pendingSocketName = null
	session.linksLastInvalidPick = null
	pushSocketConfigModalPatch(handlerId)
}

export function pushSocketConfigModalPatch(handlerId: string): void {
	const session = sessions.get(handlerId)
	if (!session) return
	session.deps.pushPatch(
		handlerId,
		buildSocketConfigModalState(session.deps.getEngine(), session),
	)
}

export async function runSocketConfigModalAction(
	handlerId: string,
	action: SocketConfigModalAction,
): Promise<void> {
	const session = sessions.get(handlerId)
	if (!session) return
	const engine = session.deps.getEngine()

	switch (action.action) {
		case 'close': {
			unregisterSocketConfigModalSession(handlerId)
			session.deps.closeModal()
			return
		}
		case 'setTab': {
			if (session.activeTab === 'create' && action.tab !== 'create') {
				disableBonePick(session)
			}
			session.activeTab = action.tab
			syncTabEntities(session, action.tab)
			pushSocketConfigModalPatch(handlerId)
			return
		}
		case 'focusEntityPick': {
			session.createLastInvalidPick = false
			session.targetEntityId = null
			pushSocketConfigModalPatch(handlerId)
			return
		}
		case 'focusHostPick': {
			session.hostEntityId = null
			session.pickPhase = 'host'
			session.pendingSocketName = null
			session.linksLastInvalidPick = null
			pushSocketConfigModalPatch(handlerId)
			return
		}
		case 'create': {
			const entityId = session.targetEntityId
			if (entityId == null && action.payload.action !== 'close') return

			switch (action.payload.action) {
				case 'upsertSocket':
					engine.send({
						cmd: 'upsert_entity_socket',
						entity_id: entityId!,
						name: action.payload.socket.name,
						bone_name: action.payload.socket.bone_name,
						local_position: action.payload.socket.local_position,
						local_rotation: action.payload.socket.local_rotation,
					})
					return
				case 'removeSocket':
					engine.send({
						cmd: 'remove_entity_socket',
						entity_id: entityId!,
						name: action.payload.name,
					})
					return
				case 'requestSockets':
					engine.send({ cmd: 'list_entity_sockets', entity_id: entityId! })
					return
				case 'setSocketBonePickMode':
					engine.send({
						cmd: 'set_socket_bone_pick_mode',
						entity_id: entityId!,
						active: action.payload.active,
					})
					return
			}
			return
		}
		case 'links': {
			switch (action.payload.action) {
				case 'requestSockets': {
					if (session.hostEntityId == null) return
					engine.send({ cmd: 'list_entity_sockets', entity_id: session.hostEntityId })
					return
				}
				case 'startLink': {
					if (session.hostEntityId == null) return
					session.pendingSocketName = action.payload.socketName
					session.pickPhase = 'child'
					session.linksLastInvalidPick = null
					pushSocketConfigModalPatch(handlerId)
					return
				}
				case 'cancelPick': {
					session.pickPhase = 'idle'
					session.pendingSocketName = null
					session.linksLastInvalidPick = null
					pushSocketConfigModalPatch(handlerId)
					return
				}
				case 'detach': {
					engine.send({ cmd: 'detach_from_socket', child_id: action.payload.childId })
					pushSocketConfigModalPatch(handlerId)
					return
				}
			}
			return
		}
	}
}
