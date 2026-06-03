import { Children, Fragment, isValidElement, type ReactNode } from 'react'

export type ModalConfirmMessagePart =
	| { type: 'text'; value: string }
	| { type: 'bold'; value: string }

export interface ModalConfirmMessageLine {
	className?: string
	parts: ModalConfirmMessagePart[]
}

/** Mensaje serializable para IPC (sustituye ReactNode en `message`). */
export interface ModalConfirmMessageSpec {
	lines?: ModalConfirmMessageLine[]
	/** Plantilla con icono y párrafos fijos (convertir a blueprint). */
	template?: 'convertBlueprint'
	entityName?: string
}

function partsFromNode(node: ReactNode): ModalConfirmMessagePart[] {
	if (node == null || typeof node === 'boolean') return []
	if (typeof node === 'string' || typeof node === 'number') {
		const value = String(node).trim()
		return value ? [{ type: 'text', value }] : []
	}
	if (Array.isArray(node)) {
		return node.flatMap((child) => partsFromNode(child))
	}
	if (!isValidElement(node)) return []

	const el = node as React.ReactElement<{ children?: ReactNode; className?: string }>
	const childType = el.type

	if (childType === 'strong' || childType === 'b') {
		const value = Children.toArray(el.props.children)
			.map((c) => (typeof c === 'string' || typeof c === 'number' ? String(c) : ''))
			.join('')
			.trim()
		return value ? [{ type: 'bold', value }] : []
	}

	return partsFromNode(el.props.children)
}

function lineFromNode(node: ReactNode): ModalConfirmMessageLine | null {
	if (node == null || typeof node === 'boolean') return null
	if (typeof node === 'string' || typeof node === 'number') {
		const value = String(node).trim()
		return value ? { parts: [{ type: 'text', value }] } : null
	}
	if (Array.isArray(node)) {
		const parts = node.flatMap((child) => partsFromNode(child))
		return parts.length > 0 ? { parts } : null
	}
	if (!isValidElement(node)) return null

	const el = node as React.ReactElement<{ children?: ReactNode; className?: string }>
	const parts = partsFromNode(el.props.children)
	if (parts.length === 0) return null

	return {
		className: typeof el.props.className === 'string' ? el.props.className : undefined,
		parts,
	}
}

/** Convierte el `message` React (main) a spec clonable para la ventana modal. */
export function extractModalConfirmMessageSpec(message: unknown): ModalConfirmMessageSpec | undefined {
	if (message == null) return undefined

	if (typeof message === 'string' || typeof message === 'number') {
		return { lines: [{ parts: [{ type: 'text', value: String(message) }] }] }
	}

	if (!isValidElement(message) && !Array.isArray(message)) return undefined

	let nodes: ReactNode[]
	if (isValidElement(message)) {
		const el = message as React.ReactElement<{ children?: ReactNode }>
		if (el.type === Fragment) {
			nodes = Children.toArray(el.props.children)
		} else if (el.type === 'p' || el.type === 'div' || el.type === 'span') {
			const line = lineFromNode(el)
			return line ? { lines: [line] } : undefined
		} else {
			nodes = [message]
		}
	} else {
		nodes = Children.toArray(message as ReactNode)
	}

	const lines: ModalConfirmMessageLine[] = []
	let inlineBuffer: ModalConfirmMessagePart[] = []
	const flushInline = () => {
		if (inlineBuffer.length > 0) {
			lines.push({ parts: [...inlineBuffer] })
			inlineBuffer = []
		}
	}

	for (const node of nodes) {
		if (isValidElement(node)) {
			const el = node as React.ReactElement<{ children?: ReactNode; className?: string }>
			const type = el.type
			if (type === 'p' || type === 'div') {
				flushInline()
				const line = lineFromNode(el)
				if (line) lines.push(line)
				continue
			}
		}
		inlineBuffer.push(...partsFromNode(node))
	}
	flushInline()

	if (lines.length === 0) {
		const parts = partsFromNode(message as ReactNode)
		if (parts.length > 0) return { lines: [{ parts }] }
		return undefined
	}

	return { lines }
}
