import { useCallback, useEffect, useState } from 'react';
import { Accordion } from 'react-bootstrap';
import { CameraVideo, CheckLg } from 'react-bootstrap-icons';

import { AppTooltip } from '@components';
import { useContextEngine } from '@engine';
import { useTraslate } from '@hooks';
import { FIRST_PERSON_PLAYER_BODY_SCALE, type GameStyle, type ProjectType } from '@shared-types';
import {
	applySavedFirstPersonView,
	FP_DEFAULT_FOV_Y,
	FP_DEFAULT_FRUSTUM_DISTANCE,
	FP_DEFAULT_YAW,
	FP_EDITOR_ORBIT_PITCH,
	syncFirstPersonViewRefFromPlayer,
} from '../../../../defaults/firstPersonSceneRestore';

const RAD_TO_DEG = 180 / Math.PI;
const DEG_TO_RAD = Math.PI / 180;
const INPUT_CLASS = 'form-control form-control-sm bg-dark text-light border-secondary w-100';
/** Misma anchura en filas de 3: basis 0 evita que la etiqueta corta (p. ej. FOV) encoja la columna. */
const FIELD_COL_STYLE: React.CSSProperties = { flex: '1 1 0', minWidth: 0 };

/** Máx. 1 decimal; entero si no hace falta la fracción. */
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

export function CameraAccordion({
	projectType = '2D',
	gameStyle,
}: {
	projectType?: ProjectType
	gameStyle?: GameStyle
}) {
	const { t } = useTraslate();
	const {
		engineReady,
		send,
		camera2dRef,
		firstPersonViewRef,
		entityTransformsRef,
		playerEntityIdRef,
		selectedEntity,
	} = useContextEngine();

	const is3d = projectType === '3D';
	const is3dFp = is3d && gameStyle === 'first-person';

	const [posX, setPosX] = useState('0');
	const [posY, setPosY] = useState('0');
	const [posZ, setPosZ] = useState('0');
	const [yawDeg, setYawDeg] = useState(formatCameraNum(FP_DEFAULT_YAW * RAD_TO_DEG));
	const [fovDeg, setFovDeg] = useState(formatCameraNum(FP_DEFAULT_FOV_Y * RAD_TO_DEG));
	const [frustumDist, setFrustumDist] = useState(formatCameraNum(FP_DEFAULT_FRUSTUM_DISTANCE));

	const [cam2dX, setCam2dX] = useState('0');
	const [cam2dY, setCam2dY] = useState('0');
	const [cam2dHalfH, setCam2dHalfH] = useState('10');

	const formatOnBlur = (raw: string, setter: (v: string) => void) => {
		const n = parseCameraNum(raw);
		if (!Number.isNaN(n)) setter(formatCameraNum(n));
	};

	const loadFromScene = useCallback(() => {
		if (is3dFp) {
			const playerId = playerEntityIdRef.current;
			if (playerId != null) {
				syncFirstPersonViewRefFromPlayer(firstPersonViewRef, playerId, entityTransformsRef);
			}
			const v = firstPersonViewRef.current;
			if (v) {
				setPosX(formatCameraNum(v.position[0]));
				setPosY(formatCameraNum(v.position[1]));
				setPosZ(formatCameraNum(v.position[2]));
				setYawDeg(formatCameraNum((v.yaw ?? FP_DEFAULT_YAW) * RAD_TO_DEG));
				setFovDeg(formatCameraNum((v.fov_y ?? FP_DEFAULT_FOV_Y) * RAD_TO_DEG));
				setFrustumDist(formatCameraNum(v.frustum_distance ?? FP_DEFAULT_FRUSTUM_DISTANCE));
			}
			return;
		}
		if (!is3d && camera2dRef.current) {
			const c = camera2dRef.current;
			setCam2dX(formatCameraNum(c.x));
			setCam2dY(formatCameraNum(c.y));
			setCam2dHalfH(formatCameraNum(c.halfH));
		}
	}, [is3dFp, is3d, camera2dRef, firstPersonViewRef, entityTransformsRef, playerEntityIdRef]);

	useEffect(() => {
		loadFromScene();
	}, [loadFromScene, selectedEntity?.id, engineReady]);

	const apply3dFp = () => {
		const playerId = playerEntityIdRef.current;
		if (playerId == null) return;
		const x = parseCameraNum(posX);
		const y = parseCameraNum(posY);
		const z = parseCameraNum(posZ);
		const yaw = parseCameraNum(yawDeg) * DEG_TO_RAD;
		const fov = parseCameraNum(fovDeg) * DEG_TO_RAD;
		const frustum = parseCameraNum(frustumDist);
		if ([x, y, z, yaw, fov, frustum].some((n) => Number.isNaN(n))) return;

		const prev = firstPersonViewRef.current;
		const view = {
			position: [x, y, z] as [number, number, number],
			scale: prev?.scale ?? FIRST_PERSON_PLAYER_BODY_SCALE,
			yaw,
			pitch: FP_EDITOR_ORBIT_PITCH,
			fov_y: fov,
			frustum_distance: frustum,
			...(prev?.visual_model_path ? { visual_model_path: prev.visual_model_path } : {}),
			...(prev?.control_bindings ? { control_bindings: prev.control_bindings } : {}),
		};
		firstPersonViewRef.current = view;
		applySavedFirstPersonView(view, playerId, entityTransformsRef, { editorOrbit: true });
	};

	const apply2d = () => {
		const x = parseCameraNum(cam2dX);
		const y = parseCameraNum(cam2dY);
		const halfH = parseCameraNum(cam2dHalfH);
		if ([x, y, halfH].some((n) => Number.isNaN(n))) return;
		send({ cmd: 'set_camera2d', x, y, half_h: halfH });
		camera2dRef.current = { x, y, halfH };
	};

	const handleApply = () => {
		if (is3dFp) apply3dFp();
		else if (!is3d) apply2d();
	};

	const handleKey = (e: React.KeyboardEvent) => {
		if (e.key === 'Enter') handleApply();
	};

	return (
		<Accordion.Item eventKey="camera">
			<Accordion.Header>
				<CameraVideo className="me-2" />
				{t('Camera')}
			</Accordion.Header>
			<Accordion.Body className="py-2 px-2">
				{is3dFp && (
					<>
						<p className="text-secondary small mb-1 fw-semibold">{t('Position (feet)')}</p>
						<div className="d-flex gap-1 mb-2">
							<div style={FIELD_COL_STYLE}>
								<label className="form-label small text-secondary mb-0" htmlFor="cam-pos-x">X</label>
								<input
									id="cam-pos-x"
									type="number"
									step="0.1"
									className={INPUT_CLASS}
									value={posX}
									disabled={!engineReady}
									onChange={(e) => setPosX(e.target.value)}
									onBlur={() => formatOnBlur(posX, setPosX)}
									onKeyDown={handleKey}
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
									disabled={!engineReady}
									onChange={(e) => setPosY(e.target.value)}
									onBlur={() => formatOnBlur(posY, setPosY)}
									onKeyDown={handleKey}
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
									disabled={!engineReady}
									onChange={(e) => setPosZ(e.target.value)}
									onBlur={() => formatOnBlur(posZ, setPosZ)}
									onKeyDown={handleKey}
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
									disabled={!engineReady}
									onChange={(e) => setYawDeg(e.target.value)}
									onBlur={() => formatOnBlur(yawDeg, setYawDeg)}
									onKeyDown={handleKey}
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
									disabled={!engineReady}
									onChange={(e) => setFovDeg(e.target.value)}
									onBlur={() => formatOnBlur(fovDeg, setFovDeg)}
									onKeyDown={handleKey}
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
									disabled={!engineReady}
									onChange={(e) => setFrustumDist(e.target.value)}
									onBlur={() => formatOnBlur(frustumDist, setFrustumDist)}
									onKeyDown={handleKey}
								/>
							</div>
						</div>
					</>
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
									onChange={(e) => setCam2dX(e.target.value)}
									onBlur={() => formatOnBlur(cam2dX, setCam2dX)}
									onKeyDown={handleKey}
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
									onChange={(e) => setCam2dY(e.target.value)}
									onBlur={() => formatOnBlur(cam2dY, setCam2dY)}
									onKeyDown={handleKey}
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
									onChange={(e) => setCam2dHalfH(e.target.value)}
									onBlur={() => formatOnBlur(cam2dHalfH, setCam2dHalfH)}
									onKeyDown={handleKey}
								/>
							</div>
						</div>
					</>
				)}
				{is3d && !is3dFp && (
					<p className="text-secondary small mb-0 fst-italic">
						{t('Camera settings are available in first-person 3D projects.')}
					</p>
				)}

				{(is3dFp || !is3d) && (
					<AppTooltip content={t('Apply camera')} place="top">
						<button
							type="button"
							className="btn btn-sm btn-outline-info w-100 d-flex align-items-center justify-content-center gap-1"
							disabled={!engineReady || (is3dFp && playerEntityIdRef.current == null)}
							onClick={handleApply}
						>
							<CheckLg />
							{t('Apply camera')}
						</button>
					</AppTooltip>
				)}
			</Accordion.Body>
		</Accordion.Item>
	);
}
