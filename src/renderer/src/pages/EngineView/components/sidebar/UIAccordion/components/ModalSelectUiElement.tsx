import type { ReactNode } from 'react';
import { Image, Square, Type } from 'react-bootstrap-icons';
import { useModal } from '@modal';
import { useTraslate } from '@hooks';

export type UiElementKind = 'button' | 'text' | 'image';

interface ModalSelectUiElementProps {
	onSelectButton: () => void;
	onSelectText: () => void;
	onSelectImage: () => void;
}

export default function ModalSelectUiElement({
	onSelectButton,
	onSelectText,
	onSelectImage,
}: ModalSelectUiElementProps) {
	const { t } = useTraslate();
	const { closeModal } = useModal();

	const options: Array<{
		kind: UiElementKind;
		label: string;
		icon: ReactNode;
		onClick: () => void;
	}> = [
		{
			kind: 'button',
			label: t('Button'),
			icon: <Square className="flex-shrink-0" />,
			onClick: onSelectButton,
		},
		{
			kind: 'text',
			label: t('Text'),
			icon: <Type className="flex-shrink-0" />,
			onClick: onSelectText,
		},
		{
			kind: 'image',
			label: t('Image'),
			icon: <Image className="flex-shrink-0" />,
			onClick: onSelectImage,
		},
	];

	return (
		<div>
			<p className="text-secondary small mb-2">{t('Select the type of element to add')}</p>
			<ul className="list-unstyled mb-0 d-flex flex-column gap-2">
				{options.map((opt) => (
					<li key={opt.kind}>
						<button
							type="button"
							className="btn btn-outline-secondary btn-sm w-100 text-start d-flex align-items-center gap-2"
							onClick={opt.onClick}
						>
							{opt.icon}
							<span>{opt.label}</span>
						</button>
					</li>
				))}
			</ul>
			<div className="d-flex justify-content-end mt-3">
				<button className="btn btn-secondary btn-sm" type="button" onClick={closeModal}>
					{t('Cancel')}
				</button>
			</div>
		</div>
	);
}
