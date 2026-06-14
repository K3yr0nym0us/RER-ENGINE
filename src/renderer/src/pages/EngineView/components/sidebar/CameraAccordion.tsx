import { useCallback, useEffect, useRef, useState } from 'react';
import { Accordion } from 'react-bootstrap';
import { CameraVideo } from 'react-bootstrap-icons';

import { AppTooltip } from '@components';
import { useContextEngine } from '@engine';
import { useTraslate } from '@hooks';
import { get3DCameraModeOptions } from '../../../../constants/cameraModeOptions';
import {
	type EngineStartPayload,
	type GameStyle,
	type ProjectType,
	type SavedPlayerTransform,
	DEFAULT_3D_CAMERA_MODE,
} from '@shared-types';
import {
	applyPlayCharacterCameraPatch,
} from '../../../../defaults/playCharacterSceneRestore';

const RAD_TO_DEG = 180 / Math.PI;
const DEG_TO_RAD = Math.PI / 180;
const INPUT_CLASS = 'form-control form-control-sm bg-dark text-light border-secondary w-100';
const SELECT_CLASS = 'form-select form-select-sm bg-dark text-light border-secondary w-100';
const FIELD_COL_STYLE: React.CSSProperties = { flex: '1 1 0', minWidth: 0 };

/** Controles numéricos del acordeón (ojo/yaw/FOV) solo para modos de cámara implementados. */
function cameraModeHasEditorControls(mode: GameStyle): boolean {
	return mode === 'first-person';
}

function formatCameraNum(n: number): string {
	if (!Number.isFinite(n)) return '0';
	const rounded = Math.round(n * 10) / 10;
	return Number.isInteger(rounded) ? String(rounded) : rounded.toFixed(1);
}

function parseCameraNum(s: string): number {
	const n = parseFloat(s);
	if (Number.isNaN(n)) return NaN;
	return Math.round(n * 10) / 10;
}

function playCameraYawFromView(v: SavedPlayerTransform): number | null {
	const y = v.fps_camera_yaw ?? v.yaw;
	return y !== undefined ? y : null;
}

export function CameraAccordion({
	projectType = '2D',
	gameStyle = DEFAULT_3D_CAMERA_MODE,
	initialSavePath,
	initialExtractDir,
	onGameStyleChange,
}: {
	projectType?: ProjectType
	gameStyle?: GameStyle
	initialSavePath?: string | null
	initialExtractDir?: string | null
	onGameStyleChange?: (mode: GameStyle) => void
}) {
	const { t } = useTraslate();
	const {
		engineReady,
		send,
		camera2dRef,
		playCharacterViewRef,
		playerEntityIdRef,
		selectedEntity,
		playCharacterViewSyncSeq,
		previewPlaying,
	} = useContextEngine();

	const is3d = projectType === '3D';
	const showCameraControls = is3d && cameraModeHasEditorControls(gameStyle);
	const cameraModeOptions = get3DCameraModeOptions();

	const [posX, setPosX] = useState('0');
	const [posY, setPosY] = useState('0');
	const [posZ, setPosZ] = useState('0');
	const [yawDeg, setYawDeg] = useState('0');
	const [fovDeg, setFovDeg] = useState('0');
	const [frustumDist, setFrustumDist] = useState('0');

	const [cam2dX, setCam2dX] = useState('0');
	const [cam2dY, setCam2dY] = useState('0');
	const [cam2dHalfH, setCam2dHalfH] = useState('10');

	const skipSyncRef = useRef(false);
	const hasPlayer = playerEntityIdRef.current != null;

	const loadFromScene = useCallback(() => {
		if (showCameraControls) {
			const v = playCharacterViewRef.current;
			if (!v?.camera_eye_position) return;
			const eye = v.camera_eye_position;
			setPosX(formatCameraNum(eye[0]));
			setPosY(formatCameraNum(eye[1]));
			setPosZ(formatCameraNum(eye[2]));
			const yaw = playCameraYawFromView(v);
			if (yaw !== null) {
				setYawDeg(formatCameraNum(yaw * RAD_TO_DEG));
			}
			if (v.fov_y !== undefined) {
				setFovDeg(formatCameraNum(v.fov_y * RAD_TO_DEG));
			}
			if (v.frustum_distance !== undefined) {
				setFrustumDist(formatCameraNum(v.frustum_distance));
			}
			return;
		}
		if (!is3d && camera2dRef.current) {
			const c = camera2dRef.current;
			setCam2dX(formatCameraNum(c.x));
			setCam2dY(formatCameraNum(c.y));
			setCam2dHalfH(formatCameraNum(c.halfH));
		}
	}, [showCameraControls, is3d, camera2dRef, playCharacterViewRef]);

	useEffect(() => {
		if (skipSyncRef.current) return;
		loadFromScene();
	}, [loadFromScene, selectedEntity?.id, engineReady, playCharacterViewSyncSeq, previewPlaying, gameStyle]);

	const commitPosAxis = (axis: 0 | 1 | 2, raw: string) => {
		if (playerEntityIdRef.current == null) return;
		const parsed = parseFloat(raw);
		if (!Number.isFinite(parsed)) return;
		skipSyncRef.current = true;
		applyPlayCharacterCameraPatch({ positionAxis: { axis, value: parsed } });
	};

	const commitYaw = (raw: string) => {
		if (playerEntityIdRef.current == null) return;
		const parsed = parseCameraNum(raw);
		if (Number.isNaN(parsed)) return;
		skipSyncRef.current = true;
		applyPlayCharacterCameraPatch({ yaw: parsed * DEG_TO_RAD });
	};

	const commitFov = (raw: string) => {
		if (playerEntityIdRef.current == null) return;
		const parsed = parseCameraNum(raw);
		if (Number.isNaN(parsed)) return;
		skipSyncRef.current = true;
		applyPlayCharacterCameraPatch({ fov_y: parsed * DEG_TO_RAD });
	};

	const commitFrustum = (raw: string) => {
		if (playerEntityIdRef.current == null) return;
		const parsed = parseCameraNum(raw);
		if (Number.isNaN(parsed)) return;
		skipSyncRef.current = true;
		applyPlayCharacterCameraPatch({ frustum_distance: parsed });
	};

	const commit2d = (x: string, y: string, halfH: string) => {
		const px = parseCameraNum(x);
		const py = parseCameraNum(y);
		const ph = parseCameraNum(halfH);
		if ([px, py, ph].some((n) => Number.isNaN(n))) return;
		skipSyncRef.current = true;
		send({ cmd: 'set_camera2d', x: px, y: py, half_h: ph });
		camera2dRef.current = { x: px, y: py, halfH: ph };
	};

	const handleCameraModeChange = (nextMode: GameStyle) => {
		if (!is3d || nextMode === gameStyle) return;
		const option = cameraModeOptions.find((o) => o.type === nextMode);
		if (!option?.available) return;

		const payload: EngineStartPayload = {
			projectType: '3D',
			mode: nextMode,
			save_path: initialSavePath ?? false,
			...(initialExtractDir?.trim() ? { extract_dir: initialExtractDir.trim() } : {}),
		};
		window.electronAPI.setGameStyle(payload);
		onGameStyleChange?.(nextMode);
	};

	const finishEdit3d = (
		raw: string,
		setter: (v: string) => void,
		commit: (formatted: string) => void,
	) => {
		const n = parseCameraNum(raw);
		if (Number.isNaN(n)) {
			skipSyncRef.current = false;
			loadFromScene();
			return;
		}
		const formatted = formatCameraNum(n);
		setter(formatted);
		commit(formatted);
		skipSyncRef.current = false;
	};

	const finishEdit2d = (
		raw: string,
		setter: (v: string) => void,
		getAll: () => { x: string; y: string; halfH: string },
		key: 'x' | 'y' | 'halfH',
	) => {
		const n = parseCameraNum(raw);
		if (Number.isNaN(n)) {
			skipSyncRef.current = false;
			loadFromScene();
			return;
		}
		const formatted = formatCameraNum(n);
		setter(formatted);
		const fields = getAll();
		const next = { ...fields, [key]: formatted };
		commit2d(next.x, next.y, next.halfH);
		skipSyncRef.current = false;
	};

	const camDisabled = !engineReady || !hasPlayer;

	return (
		<Accordion.Item eventKey="camera">
			<Accordion.Header>
				<CameraVideo className="me-2" />
				{t('Camera')}
			</Accordion.Header>
			<Accordion.Body className="py-2 px-2">
				{is3d && (
					<>
						<p className="text-secondary small mb-1 fw-semibold">{t('Camera type')}</p>
						<select
							id="cam-mode-select"
							className={`${SELECT_CLASS} mb-2`}
							value={gameStyle}
							disabled={!engineReady}
							onChange={(e) => handleCameraModeChange(e.target.value as GameStyle)}
						>
							{cameraModeOptions.map((opt) => (
								<option key={opt.type} value={opt.type} disabled={!opt.available}>
									{t(opt.labelKey)}{!opt.available ? ` (${t('COMING SOON')})` : ''}
								</option>
							))}
						</select>
					</>
				)}
				{showCameraControls && (
					<>
						<p className="text-secondary small mb-1 fw-semibold">{t('Camera position')}</p>
						<div className="d-flex gap-1 mb-2">
							<div style={FIELD_COL_STYLE}>
								<label className="form-label small text-secondary mb-0" htmlFor="cam-pos-x">X</label>
								<input
									id="cam-pos-x"
									type="number"
									step="0.1"
									className={INPUT_CLASS}
									value={posX}
									disabled={camDisabled}
									onChange={(e) => {
										setPosX(e.target.value);
										commitPosAxis(0, e.target.value);
									}}
									onBlur={() => finishEdit3d(posX, setPosX, (f) => commitPosAxis(0, f))}
								/>
							</div>
							<div style={FIELD_COL_STYLE}>
								<label className="form-label small text-secondary mb-0" htmlFor="cam-pos-y">Y</label>
								<input
									id="cam-pos-y"
									type="number"
									step="0.1"
									className={INPUT_CLASS}
									value={posY}
									disabled={camDisabled}
									onChange={(e) => {
										setPosY(e.target.value);
										commitPosAxis(1, e.target.value);
									}}
									onBlur={() => finishEdit3d(posY, setPosY, (f) => commitPosAxis(1, f))}
								/>
							</div>
							<div style={FIELD_COL_STYLE}>
								<label className="form-label small text-secondary mb-0" htmlFor="cam-pos-z">Z</label>
								<input
									id="cam-pos-z"
									type="number"
									step="0.1"
									className={INPUT_CLASS}
									value={posZ}
									disabled={camDisabled}
									onChange={(e) => {
										setPosZ(e.target.value);
										commitPosAxis(2, e.target.value);
									}}
									onBlur={() => finishEdit3d(posZ, setPosZ, (f) => commitPosAxis(2, f))}
								/>
							</div>
						</div>
						<div className="d-flex gap-1 mb-2">
							<div style={FIELD_COL_STYLE}>
								<label className="form-label small text-secondary mb-0" htmlFor="cam-yaw">
									{t('Yaw (°)')}
								</label>
								<input
									id="cam-yaw"
									type="number"
									step="0.1"
									className={INPUT_CLASS}
									value={yawDeg}
									disabled={camDisabled}
									onChange={(e) => {
										setYawDeg(e.target.value);
										commitYaw(e.target.value);
									}}
									onBlur={() => finishEdit3d(yawDeg, setYawDeg, commitYaw)}
								/>
							</div>
							<div style={FIELD_COL_STYLE}>
								<AppTooltip content={t('Field of view (°)')} place="top">
									<label className="form-label small text-secondary mb-0" htmlFor="cam-fov">
										{t('FOV (°)')}
									</label>
								</AppTooltip>
								<input
									id="cam-fov"
								 type="number"
									step="0.1"
									min="10"
									max="120"
									className={INPUT_CLASS}
									value={fovDeg}
									disabled={camDisabled}
									onChange={(e) => {
										setFovDeg(e.target.value);
										commitFov(e.target.value);
									}}
									onBlur={() => finishEdit3d(fovDeg, setFovDeg, commitFov)}
								/>
							</div>
							<div style={FIELD_COL_STYLE}>
								<AppTooltip content={t('Frustum range (m)')} place="top">
									<label className="form-label small text-secondary mb-0" htmlFor="cam-frustum">
										{t('F. Range (m)')}
									</label>
								</AppTooltip>
								<input
									id="cam-frustum"
									type="number"
									step="0.1"
									min="0.5"
									className={INPUT_CLASS}
									value={frustumDist}
									disabled={camDisabled}
									onChange={(e) => {
										setFrustumDist(e.target.value);
										commitFrustum(e.target.value);
									}}
									onBlur={() => finishEdit3d(frustumDist, setFrustumDist, commitFrustum)}
								/>
							</div>
						</div>
					</>
				)}
				{is3d && !showCameraControls && (
					<p className="text-secondary small mb-0 fst-italic">
						{t('Detailed camera controls for this mode are not available yet.')}
					</p>
				)}
				{!is3d && (
					<>
						<p className="text-secondary small mb-1 fw-semibold">{t('Editor camera')}</p>
						<div className="d-flex gap-1 mb-2">
							<div style={FIELD_COL_STYLE}>
								<label className="form-label small text-secondary mb-0" htmlFor="cam2d-x">X</label>
								<input
									id="cam2d-x"
									type="number"
									step="0.1"
									className={INPUT_CLASS}
									value={cam2dX}
									disabled={!engineReady}
									onChange={(e) => {
										setCam2dX(e.target.value);
										commit2d(e.target.value, cam2dY, cam2dHalfH);
									}}
									onBlur={() => finishEdit2d(
										cam2dX,
										setCam2dX,
										() => ({ x: cam2dX, y: cam2dY, halfH: cam2dHalfH }),
										'x',
									)}
								/>
							</div>
							<div style={FIELD_COL_STYLE}>
								<label className="form-label small text-secondary mb-0" htmlFor="cam2d-y">Y</label>
								<input
									id="cam2d-y"
									type="number"
									step="0.1"
									className={INPUT_CLASS}
									value={cam2dY}
									disabled={!engineReady}
									onChange={(e) => {
										setCam2dY(e.target.value);
										commit2d(cam2dX, e.target.value, cam2dHalfH);
									}}
									onBlur={() => finishEdit2d(
										cam2dY,
										setCam2dY,
										() => ({ x: cam2dX, y: cam2dY, halfH: cam2dHalfH }),
										'y',
									)}
								/>
							</div>
							<div style={FIELD_COL_STYLE}>
								<label className="form-label small text-secondary mb-0" htmlFor="cam2d-zoom">
									{t('Zoom (halfH)')}
								</label>
								<input
									id="cam2d-zoom"
									type="number"
									step="0.1"
									min="1"
									className={INPUT_CLASS}
									value={cam2dHalfH}
									disabled={!engineReady}
									onChange={(e) => {
										setCam2dHalfH(e.target.value);
										commit2d(cam2dX, cam2dY, e.target.value);
									}}
									onBlur={() => finishEdit2d(
										cam2dHalfH,
										setCam2dHalfH,
										() => ({ x: cam2dX, y: cam2dY, halfH: cam2dHalfH }),
										'halfH',
									)}
								/>
							</div>
						</div>
					</>
				)}
			</Accordion.Body>
		</Accordion.Item>
	);
}
