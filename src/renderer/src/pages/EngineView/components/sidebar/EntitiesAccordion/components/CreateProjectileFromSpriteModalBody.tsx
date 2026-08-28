import { useState } from 'react';

import type { SpriteInfo } from '@shared-types';
import { useTraslate } from '@hooks';
import { useModalClose } from '../../../../../../modal-electron/useModalClose';

export interface CreateProjectileFromSpriteConfirmPayload {
	spritePath: string;
}

interface CreateProjectileFromSpriteModalBodyProps {
	sprites?: SpriteInfo[];
	/** Registrado en el padre vía modal Electron (la ventana hijo no tiene EngineProvider). */
	onConfirm?: (payload: CreateProjectileFromSpriteConfirmPayload) => void;
}

export function CreateProjectileFromSpriteModalBody({
	sprites = [],
	onConfirm,
}: CreateProjectileFromSpriteModalBodyProps) {
	const { t } = useTraslate();
	const closeModal = useModalClose();
	const [selectedSpritePath, setSelectedSpritePath] = useState('');
	const [loading, setLoading] = useState(false);

	const spriteName = (path: string) => path.split('/').pop() ?? path;

	const handleCreate = () => {
		if (!selectedSpritePath || loading) return;
		setLoading(true);
		onConfirm?.({ spritePath: selectedSpritePath });
		closeModal();
	};

	if (sprites.length === 0) {
		return (
			<div className="alert alert-warning mb-0">
				<p className="mb-2">
					{t('No preloaded sprites. Load sprites first in the Sprites accordion.')}
				</p>
				<button type="button" className="btn btn-secondary btn-sm" onClick={closeModal}>
					{t('Close')}
				</button>
			</div>
		);
	}

	return (
		<div>
			<div className="mb-3">
				<label className="form-label" htmlFor="create-projectile-sprite-select">
					{t('Select a sprite')}
				</label>
				<select
					id="create-projectile-sprite-select"
					className="form-select"
					value={selectedSpritePath}
					onChange={(e) => setSelectedSpritePath(e.target.value)}
				>
					<option value="">{t('-- Choose a sprite --')}</option>
					{sprites.map((s) => (
						<option key={s.path} value={s.path}>
							{s.name || spriteName(s.path)} ({s.width}x{s.height})
						</option>
					))}
				</select>
			</div>
			<div className="d-flex gap-2 justify-content-end mt-3">
				<button type="button" className="btn btn-secondary btn-sm" onClick={closeModal}>
					{t('Cancel')}
				</button>
				<button
					type="button"
					className="btn btn-primary btn-sm"
					disabled={!selectedSpritePath || loading}
					onClick={handleCreate}
				>
					{t('Create projectile')}
				</button>
			</div>
		</div>
	);
}

CreateProjectileFromSpriteModalBody.displayName = 'CreateProjectileFromSpriteModalBody';
