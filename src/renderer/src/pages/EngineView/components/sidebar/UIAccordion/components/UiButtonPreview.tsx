import type { CSSProperties } from 'react';
import {
	type PlayerUiButtonConfig,
	buttonPreviewMetrics,
	buttonShapeBorderRadius,
	buttonShapeClipPath,
	clampPercent,
	hexToRgba,
} from './playerUiButtonModel';
import { useUiButtonTextureUrl } from './useUiButtonTextureUrl';

interface UiButtonPreviewProps {
	config: PlayerUiButtonConfig;
}

export default function UiButtonPreview({ config }: UiButtonPreviewProps) {
	const textureUrl = useUiButtonTextureUrl(config.texturePath);
	const { width, height } = buttonPreviewMetrics(config.type);
	const clipPath = buttonShapeClipPath(config.type);
	const borderRadius = buttonShapeBorderRadius(config.type, config.round);
	const bgAlpha = clampPercent(config.transparencyBackground) / 100;
	const textAlpha = clampPercent(config.transparencyText) / 100;

	const shell: CSSProperties = {
		width,
		height,
		border: `${config.borderWeight}px solid ${config.borderColor}`,
		borderRadius,
		clipPath,
		overflow: 'hidden',
		position: 'relative',
		display: 'flex',
		alignItems: 'center',
		justifyContent: 'center',
		boxSizing: 'border-box',
	};

	const fill: CSSProperties = {
		position: 'absolute',
		inset: 0,
		backgroundColor: hexToRgba(config.backgroundColor, config.transparencyBackground),
		backgroundImage: textureUrl ? `url(${textureUrl})` : undefined,
		backgroundSize: 'cover',
		backgroundPosition: 'center',
		opacity: textureUrl ? bgAlpha : 1,
		borderRadius,
		clipPath,
	};

	const label: CSSProperties = {
		position: 'relative',
		zIndex: 1,
		color: config.textColor,
		opacity: textAlpha,
		fontFamily: config.fontName ? `"${config.fontName}", sans-serif` : 'sans-serif',
		fontSize: '0.85rem',
		fontWeight: 600,
		textAlign: 'center',
		padding: '0 8px',
		lineHeight: 1.2,
		wordBreak: 'break-word',
		maxWidth: '100%',
		pointerEvents: 'none',
	};

	return (
		<div
			className="d-flex align-items-center justify-content-center rounded border border-secondary p-3"
			style={{ minHeight: 140, background: '#0f1419' }}
		>
			<div style={shell}>
				<div style={fill} aria-hidden />
				<span style={label}>{config.text || ' '}</span>
			</div>
		</div>
	);
}
