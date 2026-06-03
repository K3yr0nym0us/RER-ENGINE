import { useCallback, useState } from 'react';
import { Image as ImageIcon } from 'react-bootstrap-icons';
import { useModalClose } from '@hooks';
import { useContextEngine } from '@engine';
import { useTraslate } from '@hooks';
import UiButtonPreview from './UiButtonPreview';
import {
	DEFAULT_PLAYER_UI_BUTTON_CONFIG,
	UI_BUTTON_SHAPE_OPTIONS,
	type PlayerUiButtonConfig,
	type UiButtonShapeType,
	clampBorderWeight,
	clampPercent,
	clampRound,
	normalizeHexColor,
} from './playerUiButtonModel';

interface ModalAddUiButtonProps {
	onConfirm: (config: PlayerUiButtonConfig) => void;
	onBack?: () => void;
}

function ColorField({
	label,
	value,
	onChange,
}: {
	label: string;
	value: string;
	onChange: (hex: string) => void;
}) {
	const pickerValue = normalizeHexColor(value) ?? '#000000';

	return (
		<div className="mb-2">
			<label className="form-label small text-secondary mb-1">{label}</label>
			<div className="d-flex gap-2 align-items-center">
				<input
					type="color"
					className="form-control form-control-color flex-shrink-0"
					style={{ width: 40, height: 32, padding: 2 }}
					value={pickerValue}
					onChange={(e) => onChange(e.target.value)}
				/>
				<input
					type="text"
					className="form-control form-control-sm font-monospace"
					value={value}
					onChange={(e) => {
						const next = normalizeHexColor(e.target.value);
						if (next) onChange(next);
						else onChange(e.target.value);
					}}
				/>
			</div>
		</div>
	);
}

function RangeField({
	label,
	value,
	onChange,
}: {
	label: string;
	value: number;
	onChange: (n: number) => void;
}) {
	return (
		<div className="mb-2">
			<div className="d-flex justify-content-between align-items-center mb-1">
				<label className="form-label small text-secondary mb-0">{label}</label>
				<span className="small text-light">{Math.round(value)}%</span>
			</div>
			<input
				type="range"
				className="form-range"
				min={0}
				max={100}
				step={1}
				value={value}
				onChange={(e) => onChange(Number(e.target.value))}
			/>
		</div>
	);
}

const SHAPE_LABEL_KEYS: Record<UiButtonShapeType, string> = {
	square: 'Square',
	rectangle: 'Rectangle',
	diamond: 'Diamond',
	triangle: 'Triangle',
	circle: 'Circle',
};

export default function ModalAddUiButton({ onConfirm, onBack }: ModalAddUiButtonProps) {
	const { t } = useTraslate();
	const closeModal = useModalClose();
	const { fonts } = useContextEngine();
	const [config, setConfig] = useState<PlayerUiButtonConfig>(DEFAULT_PLAYER_UI_BUTTON_CONFIG);
	const [error, setError] = useState<string | null>(null);

	const patch = useCallback((partial: Partial<PlayerUiButtonConfig>) => {
		setConfig((prev) => ({ ...prev, ...partial }));
		setError(null);
	}, []);

	const pickTexture = async () => {
		const path = await window.electronAPI.openBackgroundDialog();
		if (path) patch({ texturePath: path });
	};

	const handleConfirm = () => {
		if (config.text.trim() && !config.fontPath) {
			setError(t('Select a font for the button text.'));
			return;
		}
		const bg = normalizeHexColor(config.backgroundColor);
		const tc = normalizeHexColor(config.textColor);
		const bc = normalizeHexColor(config.borderColor);
		if (!bg || !tc || !bc) {
			setError(t('Invalid color. Use hexadecimal format (#RRGGBB).'));
			return;
		}
		onConfirm({
			...config,
			backgroundColor: bg,
			textColor: tc,
			borderColor: bc,
			round: clampRound(config.round),
			transparencyBackground: clampPercent(config.transparencyBackground),
			transparencyText: clampPercent(config.transparencyText),
			borderWeight: clampBorderWeight(config.borderWeight),
		});
		closeModal();
	};

	return (
		<div>
			<div className="row g-3">
				<div className="col-12 col-lg-5">
					<p className="small text-secondary mb-2">{t('Preview')}</p>
					<UiButtonPreview config={config} />
				</div>
				<div
					className="col-12 col-lg-7"
					style={{ maxHeight: 420, overflowY: 'auto' }}
				>
					<div className="mb-2">
						<label className="form-label small text-secondary mb-1">{t('Type')}</label>
						<select
							className="form-select form-select-sm"
							value={config.type}
							onChange={(e) => patch({ type: e.target.value as UiButtonShapeType })}
						>
							{UI_BUTTON_SHAPE_OPTIONS.map((shape) => (
								<option key={shape} value={shape}>
									{t(SHAPE_LABEL_KEYS[shape])}
								</option>
							))}
						</select>
					</div>

					<div className="mb-2">
						<div className="d-flex justify-content-between align-items-center mb-1">
							<label className="form-label small text-secondary mb-0">{t('Round')}</label>
							<span className="small text-light">{clampRound(config.round)} px</span>
						</div>
						<input
							type="range"
							className="form-range"
							min={0}
							max={64}
							step={1}
							value={config.round}
							disabled={config.type === 'diamond' || config.type === 'triangle'}
							onChange={(e) => patch({ round: Number(e.target.value) })}
						/>
					</div>

					<ColorField
						label={t('Background color')}
						value={config.backgroundColor}
						onChange={(backgroundColor) => patch({ backgroundColor })}
					/>

					<div className="mb-2">
						<label className="form-label small text-secondary mb-1">{t('Texture')}</label>
						<div className="d-flex gap-2 flex-wrap">
							<button
								type="button"
								className="btn btn-outline-secondary btn-sm d-flex align-items-center gap-1"
								onClick={() => void pickTexture()}
							>
								<ImageIcon />
								{t('Choose image')}
							</button>
							{config.texturePath && (
								<button
									type="button"
									className="btn btn-outline-danger btn-sm"
									onClick={() => patch({ texturePath: null })}
								>
									{t('Remove texture')}
								</button>
							)}
						</div>
						{config.texturePath && (
							<p className="small text-secondary mb-0 mt-1 text-truncate">
								{config.texturePath.replace(/\\/g, '/').split('/').pop()}
							</p>
						)}
						<p className="small text-secondary mb-0 mt-1">
							{t('PNG or WebP recommended for efficient backgrounds.')}
						</p>
					</div>

					<RangeField
						label={t('Background transparency')}
						value={config.transparencyBackground}
						onChange={(transparencyBackground) => patch({ transparencyBackground })}
					/>

					<div className="mb-2">
						<label className="form-label small text-secondary mb-1">{t('Text')}</label>
						<input
							type="text"
							className="form-control form-control-sm"
							value={config.text}
							maxLength={128}
							onChange={(e) => patch({ text: e.target.value })}
						/>
					</div>

					<ColorField
						label={t('Text color')}
						value={config.textColor}
						onChange={(textColor) => patch({ textColor })}
					/>

					<RangeField
						label={t('Text transparency')}
						value={config.transparencyText}
						onChange={(transparencyText) => patch({ transparencyText })}
					/>

					<div className="mb-2">
						<label className="form-label small text-secondary mb-1">{t('Font')}</label>
						{fonts.length === 0 ? (
							<p className="small text-warning mb-0">
								{t('No fonts loaded. Load a font in Resources first.')}
							</p>
						) : (
							<select
								className="form-select form-select-sm"
								value={config.fontPath}
								onChange={(e) => {
									const font = fonts.find((f) => f.path === e.target.value);
									patch({
										fontPath: e.target.value,
										fontName: font?.name ?? '',
									});
								}}
							>
								<option value="">{t('— Select font —')}</option>
								{fonts.map((font) => (
									<option key={font.path} value={font.path}>
										{font.name}
									</option>
								))}
							</select>
						)}
					</div>

					<ColorField
						label={t('Border color')}
						value={config.borderColor}
						onChange={(borderColor) => patch({ borderColor })}
					/>

					<div className="mb-2">
						<div className="d-flex justify-content-between align-items-center mb-1">
							<label className="form-label small text-secondary mb-0">
								{t('Border weight')}
							</label>
							<span className="small text-light">{clampBorderWeight(config.borderWeight)} px</span>
						</div>
						<input
							type="range"
							className="form-range"
							min={0}
							max={24}
							step={1}
							value={config.borderWeight}
							onChange={(e) => patch({ borderWeight: Number(e.target.value) })}
						/>
					</div>
				</div>
			</div>

			{error && <p className="small text-danger mb-2 mt-2">{error}</p>}

			<div
				className={`d-flex gap-2 mt-3 ${onBack ? 'justify-content-between' : 'justify-content-end'}`}
			>
				{onBack && (
					<button className="btn btn-outline-secondary btn-sm" type="button" onClick={onBack}>
						{t('Back')}
					</button>
				)}
				<div className="d-flex gap-2">
					<button className="btn btn-secondary btn-sm" type="button" onClick={closeModal}>
						{t('Cancel')}
					</button>
					<button className="btn btn-primary btn-sm" type="button" onClick={handleConfirm}>
						{t('Add button')}
					</button>
				</div>
			</div>
		</div>
	);
}
