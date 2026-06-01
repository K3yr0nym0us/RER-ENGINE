import { useRef } from 'react';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';

interface ModalSetNameUiProps {
	defaultName: string;
	onConfirm: (name: string) => void;
}

export default function ModalSetNameUi({ defaultName, onConfirm }: ModalSetNameUiProps) {
	const { t } = useTraslate();
	const { closeModal } = useModal();
	const nameRef = useRef<HTMLInputElement>(null);

	const handleConfirm = () => {
		const name = nameRef.current?.value.trim() || defaultName.trim();
		if (!name) return;
		onConfirm(name);
		closeModal();
	};

	return (
		<div>
			<label className="form-label text-light small mb-1" htmlFor="ui-screen-name-input">
				{t('UI name')}
			</label>
			<input
				id="ui-screen-name-input"
				className="form-control mb-3"
				type="text"
				defaultValue={defaultName}
				ref={nameRef}
				placeholder={t('UI name')}
				onKeyDown={(e) => {
					if (e.key === 'Enter') handleConfirm();
				}}
			/>
			<div className="d-flex gap-2 justify-content-end">
				<button className="btn btn-secondary btn-sm" type="button" onClick={closeModal}>
					{t('Cancel')}
				</button>
				<button className="btn btn-primary btn-sm" type="button" onClick={handleConfirm}>
					{t('Confirm')}
				</button>
			</div>
		</div>
	);
}
