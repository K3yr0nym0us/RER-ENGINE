import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent } from 'react';
import { Collapse } from 'react-bootstrap';
import { ChevronDown, ChevronUp, Image as ImageIcon, LockFill, Trash, Unlock } from 'react-bootstrap-icons';

import { AppTooltip } from '@components';
import { useTraslate } from '@hooks';
import type { PlayerUiObjectEntry } from '../../../../../../context/useContextEngine/types';
import {
	DEFAULT_OBJECT_FILL_COLOR,
	fillColorToHex,
	fillColorToTransparencyPercent,
	hexAndTransparencyToFillColor,
	type FillColorRgba,
	type PlayerUiObjectStyleCommitOptions,
} from './playerUiObjectStyle';

interface ObjectElementAccordionRowProps {
	item: PlayerUiObjectEntry;
	engineReady: boolean;
	lockTooltip: string;
	unlockTooltip: string;
	zIndexLabel: string;
	deleteTooltip: string;
	onToggleLock: () => void;
	onZIndexCommit: (value: number) => void;
	onDelete: () => void;
	onAssignTexture: () => void;
	onStyleChange: (fillColor: FillColorRgba, options?: PlayerUiObjectStyleCommitOptions) => void;
}

export default function ObjectElementAccordionRow({
	item,
	engineReady,
	lockTooltip,
	unlockTooltip,
	zIndexLabel,
	deleteTooltip,
	onToggleLock,
	onZIndexCommit,
	onDelete,
	onAssignTexture,
	onStyleChange,
}: ObjectElementAccordionRowProps) {
	const { t } = useTraslate();
	const label = `${t('Object')} #${item.id}`;
	const fill = item.fillColor ?? DEFAULT_OBJECT_FILL_COLOR;

	const [expanded, setExpanded] = useState(false);
	const [zDraft, setZDraft] = useState(String(item.zIndex));
	const [colorHex, setColorHex] = useState(() => fillColorToHex(fill));
	const [transparency, setTransparency] = useState(() => fillColorToTransparencyPercent(fill));
	const styleDraggingRef = useRef(false);
	const liveUndoPushedRef = useRef(false);
	const pendingLiveFillRef = useRef<FillColorRgba | null>(null);
	const liveStyleRafRef = useRef<number | null>(null);

	useEffect(() => {
		setZDraft(String(item.zIndex));
	}, [item.zIndex]);

	useEffect(() => {
		if (styleDraggingRef.current) return;
		const nextFill = item.fillColor ?? DEFAULT_OBJECT_FILL_COLOR;
		setColorHex(fillColorToHex(nextFill));
		setTransparency(fillColorToTransparencyPercent(nextFill));
	}, [item.fillColor]);

	useEffect(() => {
		return () => {
			if (liveStyleRafRef.current != null) {
				cancelAnimationFrame(liveStyleRafRef.current);
			}
		};
	}, []);

	const commitStyle = (
		hex: string,
		transparencyPercent: number,
		options?: PlayerUiObjectStyleCommitOptions,
	) => {
		onStyleChange(hexAndTransparencyToFillColor(hex, transparencyPercent), options);
	};

	const flushLiveStyle = () => {
		liveStyleRafRef.current = null;
		const fill = pendingLiveFillRef.current;
		if (!fill) return;
		pendingLiveFillRef.current = null;
		onStyleChange(fill, {
			live: true,
			skip_undo: liveUndoPushedRef.current,
		});
		liveUndoPushedRef.current = true;
	};

	const scheduleLiveStyle = (hex: string, transparencyPercent: number) => {
		pendingLiveFillRef.current = hexAndTransparencyToFillColor(hex, transparencyPercent);
		if (liveStyleRafRef.current != null) return;
		liveStyleRafRef.current = requestAnimationFrame(flushLiveStyle);
	};

	const beginStyleDrag = () => {
		styleDraggingRef.current = true;
		liveUndoPushedRef.current = false;
	};

	const endStyleDrag = (hex: string, transparencyPercent: number) => {
		if (liveStyleRafRef.current != null) {
			cancelAnimationFrame(liveStyleRafRef.current);
			liveStyleRafRef.current = null;
		}
		const fill =
			pendingLiveFillRef.current ?? hexAndTransparencyToFillColor(hex, transparencyPercent);
		pendingLiveFillRef.current = null;
		styleDraggingRef.current = false;
		onStyleChange(fill, { live: false, skip_undo: true });
	};
	const commitZ = () => {
		const parsed = Number.parseInt(zDraft, 10);
		const next = Number.isFinite(parsed) ? parsed : 0;
		setZDraft(String(next));
		if (next !== item.zIndex) onZIndexCommit(next);
	};

	const stopToggle = (event: MouseEvent | KeyboardEvent) => {
		event.stopPropagation();
	};

	return (
		<div className="border border-secondary rounded bg-dark overflow-hidden mb-2">
			<div className="d-flex align-items-center gap-1 p-2 pt-1 pb-1">
				<AppTooltip content={label} place="top">
					<span className="small text-light flex-fill text-truncate">{label}</span>
				</AppTooltip>
				<AppTooltip content={item.locked ? unlockTooltip : lockTooltip} place="top">
					<span
						role="button"
						tabIndex={0}
						className={item.locked ? 'text-warning' : 'text-secondary'}
						style={{ cursor: engineReady ? 'pointer' : 'not-allowed' }}
						onClick={() => {
							if (engineReady) onToggleLock();
						}}
						onKeyDown={(e) => {
							if (engineReady && (e.key === 'Enter' || e.key === ' ')) onToggleLock();
						}}
					>
						{item.locked ? <LockFill /> : <Unlock />}
					</span>
				</AppTooltip>
				<AppTooltip content={zIndexLabel} place="top">
					<input
						type="number"
						className="form-control form-control-sm bg-dark text-light border-secondary py-0 px-1"
						style={{ width: '3.1rem' }}
						title={zIndexLabel}
						aria-label={zIndexLabel}
						disabled={!engineReady}
						value={zDraft}
						onClick={stopToggle}
						onKeyDown={(e) => e.stopPropagation()}
						onChange={(e) => setZDraft(e.target.value)}
						onBlur={commitZ}
						onKeyUp={(e) => {
							if (e.key === 'Enter') {
								commitZ();
								(e.target as HTMLInputElement).blur();
							}
						}}
					/>
				</AppTooltip>
				<AppTooltip content={deleteTooltip} place="top">
					<span
						role="button"
						tabIndex={0}
						className="text-danger me-3"
						style={{ cursor: 'pointer' }}
						onClick={onDelete}
						onKeyDown={(e) => {
							if (e.key === 'Enter' || e.key === ' ') onDelete();
						}}
					>
						<Trash />
					</span>
				</AppTooltip>
				<button
					type="button"
					className="btn btn-link btn-sm text-secondary p-0 flex-shrink-0"
					aria-expanded={expanded}
					aria-label={expanded ? t('Collapse') : t('Expand')}
					onClick={() => setExpanded((open) => !open)}
				>
					{expanded ? <ChevronUp /> : <ChevronDown />}
				</button>
			</div>
			<Collapse in={expanded}>
				<div className="py-2 px-2 border-top border-secondary">
					<button
						className="btn btn-outline-info btn-sm w-100 mb-2 d-flex align-items-center justify-content-center gap-2"
						type="button"
						disabled={!engineReady}
						onClick={onAssignTexture}
					>
						<ImageIcon aria-hidden />
						<span>{item.textureName?.trim() ? t('Change texture') : t('Assign texture')}</span>
					</button>
					{item.textureName?.trim() ? (
						<p className="small text-secondary mb-2 text-truncate">{item.textureName}</p>
					) : null}
					<div className="mb-2">
						<label className="form-label small text-secondary mb-1">{t('Fill color')}</label>
						<div className="d-flex gap-2 align-items-center">
							<input
								type="color"
								className="form-control form-control-color flex-shrink-0"
								style={{ width: 40, height: 32, padding: 2 }}
								disabled={!engineReady}
								value={colorHex}
								onPointerDown={beginStyleDrag}
								onPointerUp={() => endStyleDrag(colorHex, transparency)}
								onPointerCancel={() => endStyleDrag(colorHex, transparency)}
								onBlur={() => {
									if (liveUndoPushedRef.current) {
										endStyleDrag(colorHex, transparency);
									}
								}}
								onChange={(e) => {
									const next = e.target.value;
									setColorHex(next);
									scheduleLiveStyle(next, transparency);
								}}
							/>
							<input
								type="text"
								className="form-control form-control-sm font-monospace"
								disabled={!engineReady}
								value={colorHex}
								onChange={(e) => {
									const next = e.target.value;
									setColorHex(next);
									if (/^#[0-9a-fA-F]{6}$/.test(next.trim())) {
										commitStyle(next.trim(), transparency);
									}
								}}
							/>
						</div>
					</div>
					<div className="mb-0">
						<div className="d-flex justify-content-between align-items-center mb-1">
							<label className="form-label small text-secondary mb-0">{t('Transparency')}</label>
							<span className="small text-light">{transparency}%</span>
						</div>
						<input
							type="range"
							className="form-range"
							min={0}
							max={100}
							step={1}
							disabled={!engineReady}
							value={transparency}
							onPointerDown={beginStyleDrag}
							onPointerUp={() => endStyleDrag(colorHex, transparency)}
							onPointerCancel={() => endStyleDrag(colorHex, transparency)}
							onBlur={() => {
								if (liveUndoPushedRef.current) {
									endStyleDrag(colorHex, transparency);
								}
							}}
							onChange={(e) => {
								const next = Number(e.target.value);
								setTransparency(next);
								scheduleLiveStyle(colorHex, next);
							}}
						/>
					</div>
				</div>
			</Collapse>
		</div>
	);
}
