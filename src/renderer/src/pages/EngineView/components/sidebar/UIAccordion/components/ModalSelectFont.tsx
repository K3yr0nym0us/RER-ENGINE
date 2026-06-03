import { Type } from 'react-bootstrap-icons';
import { useModalClose } from '@hooks';
import { useContextEngine } from '@engine';
import { useTraslate } from '@hooks';
import type { FontInfo } from '@shared-types';

interface ModalSelectFontProps {
	onSelect: (fontPath: string) => void;
	onBack?: () => void;
	/** Lista inyectada (ventana modal Electron); si falta, usa el contexto del motor. */
	fonts?: FontInfo[];
	/** Cerrar modal; en Electron lo provee el host. */
	onClose?: () => void;
}

function ModalSelectFontInner({
	onSelect,
	onBack,
	fonts,
	onClose,
}: ModalSelectFontProps & { fonts: FontInfo[]; onClose: () => void }) {
	const { t } = useTraslate();

	const footer = (
		<div className={`d-flex mt-3 gap-2 ${onBack ? 'justify-content-between' : 'justify-content-end'}`}>
			{onBack && (
				<button className="btn btn-outline-secondary btn-sm" type="button" onClick={onBack}>
					{t('Back')}
				</button>
			)}
			<button className="btn btn-secondary btn-sm" type="button" onClick={onClose}>
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
								onClose();
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

function ModalSelectFontWithEngine(props: ModalSelectFontProps) {
	const { fonts } = useContextEngine();
	const closeModal = useModalClose();
	return (
		<ModalSelectFontInner
			{...props}
			fonts={props.fonts ?? fonts}
			onClose={props.onClose ?? closeModal}
		/>
	);
}

export default function ModalSelectFont(props: ModalSelectFontProps) {
	if (props.fonts) {
		const onClose = props.onClose ?? (() => {});
		return <ModalSelectFontInner {...props} fonts={props.fonts} onClose={onClose} />;
	}
	return <ModalSelectFontWithEngine {...props} />;
}
