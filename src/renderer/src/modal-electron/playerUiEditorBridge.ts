import type { PlayerUiEditorSessionDeps } from './playerUiEditorSessions'

type SessionFactory = (handlerId: string) => PlayerUiEditorSessionDeps

let pendingFactory: SessionFactory | null = null

export function setPendingPlayerUiEditorSession(factory: SessionFactory | null): void {
	pendingFactory = factory
}

export function consumePendingPlayerUiEditorSession(handlerId: string): PlayerUiEditorSessionDeps | null {
	if (!pendingFactory) return null
	const deps = pendingFactory(handlerId)
	pendingFactory = null
	return deps
}
