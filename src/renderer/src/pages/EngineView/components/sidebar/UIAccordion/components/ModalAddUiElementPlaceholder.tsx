import { Image, Square } from 'react-bootstrap-icons';
import { useModalClose } from '@hooks';
import { useTraslate } from '@hooks';
import type { UiElementKind } from './ModalSelectUiElement';

interface ModalAddUiElementPlaceholderProps {
	kind: Extract<UiElementKind, 'button' | 'image'>;
	onBack?: () => void;
}

export default function ModalAddUiElementPlaceholder({
	kind,
	onBack,
}: ModalAddUiElementPlaceholderProps) {
	const { t } = useTraslate();
	const closeModal = useModalClose();

	const isButton = kind === 'button';
	const title = isButton ? t('Button') : t('Image');
	const message = isButton
		? t('Button UI elements will be available in a future update.')
		: t('Image UI elements will be available in a future update.');

	return (
		<div>
			<div className="d-flex align-items-center gap-2 mb-3 text-light">
				{isButton ? <Square /> : <Image />}
				<span className="fw-semibold">{title}</span>
			</div>
			<p className="text-secondary small mb-3">{message}</p>
			<div className={`d-flex gap-2 ${onBack ? 'justify-content-between' : 'justify-content-end'}`}>
				{onBack && (
					<button className="btn btn-outline-secondary btn-sm" type="button" onClick={onBack}>
						{t('Back')}
					</button>
				)}
				<button className="btn btn-secondary btn-sm" type="button" onClick={closeModal}>
					{t('Close')}
				</button>
			</div>
		</div>
	);
}
