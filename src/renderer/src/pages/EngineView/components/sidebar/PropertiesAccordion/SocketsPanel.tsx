import { useEffect, useRef, useState } from 'react'

import { Plus, Trash } from 'react-bootstrap-icons'

import type { EntitySocket3D } from '@shared-types'
import { useTraslate } from '@hooks'

const IDENTITY_ROT: [number, number, number, number] = [0, 0, 0, 1]
const ZERO_POS: [number, number, number] = [0, 0, 0]

export interface SocketBonePickedResult {
	entityId: number
	boneName: string
	seq: number
}

export interface SocketsPanelProps {
	entityId: number
	sockets: EntitySocket3D[]
	socketBonePicked: SocketBonePickedResult | null
	onSaveSocket: (socket: EntitySocket3D) => void
	onRemoveSocket: (name: string) => void
	onSetBonePickMode: (active: boolean) => void
}

export function SocketsPanel({
	entityId,
	sockets,
	socketBonePicked,
	onSaveSocket,
	onRemoveSocket,
	onSetBonePickMode,
}: SocketsPanelProps) {
	const { t } = useTraslate()
	const [draft, setDraft] = useState<EntitySocket3D | null>(null)
	const [editingName, setEditingName] = useState<string | null>(null)
	const [bonePickActive, setBonePickActive] = useState(false)
	const onSetBonePickModeRef = useRef(onSetBonePickMode)
	onSetBonePickModeRef.current = onSetBonePickMode

	useEffect(() => {
		return () => {
			onSetBonePickModeRef.current(false)
		}
	}, [])

	useEffect(() => {
		setBonePickActive(false)
		onSetBonePickModeRef.current(false)
	}, [entityId])

	useEffect(() => {
		if (!socketBonePicked || socketBonePicked.entityId !== entityId) return
		setDraft((prev) => (prev ? { ...prev, bone_name: socketBonePicked.boneName } : prev))
		setBonePickActive(false)
		onSetBonePickModeRef.current(false)
	}, [socketBonePicked, entityId])

	const startAdd = () => {
		setEditingName(null)
		setDraft({
			name: '',
			bone_name: '',
			local_position: [...ZERO_POS],
			local_rotation: [...IDENTITY_ROT],
		})
	}

	const startEdit = (socket: EntitySocket3D) => {
		setEditingName(socket.name)
		setDraft({ ...socket })
	}

	const cancelDraft = () => {
		if (bonePickActive) {
			setBonePickActive(false)
			onSetBonePickMode(false)
		}
		setDraft(null)
		setEditingName(null)
	}

	const saveDraft = () => {
		if (!draft) return
		const name = draft.name.trim()
		const bone_name = draft.bone_name.trim()
		if (!name || !bone_name) return
		onSaveSocket({
			...draft,
			name,
			bone_name,
		})
		if (bonePickActive) {
			setBonePickActive(false)
			onSetBonePickMode(false)
		}
		setDraft(null)
		setEditingName(null)
	}

	const toggleBonePick = () => {
		const next = !bonePickActive
		setBonePickActive(next)
		onSetBonePickMode(next)
	}

	const renderForm = (form: EntitySocket3D, isNew: boolean) => (
		<div className="border border-secondary rounded p-2 mb-2" key={isNew ? '__new__' : form.name}>
			<div className="mb-2">
				<label className="form-label small mb-1">{t('Name')}</label>
				<input
					type="text"
					className="form-control form-control-sm"
					value={form.name}
					disabled={!isNew && editingName != null}
					onChange={(e) => setDraft((prev) => (prev ? { ...prev, name: e.target.value } : prev))}
				/>
			</div>
			<div className="mb-2">
				<label className="form-label small mb-1">{t('Bone')}</label>
				<input
					type="text"
					className="form-control form-control-sm mb-2"
					value={form.bone_name}
					readOnly
					placeholder={t('No bone selected')}
				/>
				<button
					type="button"
					className={`btn btn-sm w-100 ${bonePickActive ? 'btn-warning' : 'btn-outline-primary'}`}
					onClick={toggleBonePick}
				>
					{bonePickActive ? t('Cancel bone selection') : t('Select bone')}
				</button>
				{bonePickActive && (
					<p className="text-warning small mb-0 mt-2">
						{t('Click a bone in the viewport to assign it to this socket.')}
					</p>
				)}
			</div>
			<div className="d-flex gap-2">
				<button
					type="button"
					className="btn btn-sm btn-primary"
					onClick={saveDraft}
					disabled={!form.bone_name.trim()}
				>
					{t('Save')}
				</button>
				<button type="button" className="btn btn-sm btn-outline-secondary" onClick={cancelDraft}>
					{t('Cancel')}
				</button>
			</div>
		</div>
	)

	return (
		<div>
			<button type="button" className="btn btn-sm btn-outline-primary w-100 mb-3" onClick={startAdd}>
				<Plus className="me-1" />
				{t('Add Socket')}
			</button>

			{draft && editingName == null && renderForm(draft, true)}
			{draft && editingName != null && renderForm(draft, false)}

			{sockets.map((socket) =>
				editingName === socket.name && draft ? null : (
					<div
						key={socket.name}
						className="border border-secondary rounded p-2 mb-2 d-flex justify-content-between align-items-start gap-2"
					>
						<div className="small">
							<div className="fw-semibold">{socket.name}</div>
							<div className="text-secondary">{socket.bone_name}</div>
						</div>
						<div className="d-flex gap-1 flex-shrink-0">
							<button
								type="button"
								className="btn btn-sm btn-outline-secondary"
								onClick={() => startEdit(socket)}
							>
								{t('Edit')}
							</button>
							<button
								type="button"
								className="btn btn-sm btn-outline-danger"
								onClick={() => onRemoveSocket(socket.name)}
							>
								<Trash />
							</button>
						</div>
					</div>
				),
			)}
		</div>
	)
}
