import { Type } from 'react-bootstrap-icons';
import { useModal } from '@modal';
import { useContextEngine } from '@engine';
import { useTraslate } from '@hooks';

interface ModalSelectFontProps {
	onSelect: (fontPath: string) => void;
	onBack?: () => void;
}

export default function ModalSelectFont({ onSelect, onBack }: ModalSelectFontProps) {
	const { t } = useTraslate();
	const { fonts } = useContextEngine();
	const { closeModal } = useModal();

	const footer = (
		<div className={`d-flex mt-3 gap-2 ${onBack ? 'justify-content-between' : 'justify-content-end'}`}>
			{onBack && (
				<button className="btn btn-outline-secondary btn-sm" type="button" onClick={onBack}>
					{t('Back')}
				</button>
			)}
			<button className="btn btn-secondary btn-sm" type="button" onClick={closeModal}>
				{t('Cancel')}
			</button>
		</div>
	);

	if (fonts.length === 0) {
		return (
			<div>
				<p className="text-secondary small mb-3">
					{t('No fonts loaded. Load a font in Resources first.')}
				</p>
				{footer}
			</div>
		);
	}

	return (
		<div>
			<p className="text-secondary small mb-2">{t('Select a font for the text box')}</p>
			<ul className="list-unstyled mb-0" style={{ maxHeight: 280, overflowY: 'auto' }}>
				{fonts.map((font) => (
					<li key={font.path} className="mb-1">
						<button
							type="button"
							className="btn btn-outline-secondary btn-sm w-100 text-start d-flex align-items-center gap-2"
							onClick={() => {
								onSelect(font.path);
								closeModal();
							}}
						>
							<Type className="flex-shrink-0" />
							<span className="text-truncate">{font.name}</span>
						</button>
					</li>
				))}
			</ul>
			{footer}
		</div>
	);
}
