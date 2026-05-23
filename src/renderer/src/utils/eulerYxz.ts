/**
 * Euler YXZ — misma convención que `glam::Quat::from_euler(EulerRot::YXZ, y, x, z)`.
 * Panel: rot[0]=eje X (pitch), rot[1]=eje Y (yaw), rot[2]=eje Z (roll).
 */

const RAD_TO_DEG = 180 / Math.PI;
const DEG_TO_RAD = Math.PI / 180;

/** Quaternion [x,y,z,w] → grados [pitchX, yawY, rollZ] en YXZ. */
/** Normaliza un ángulo en grados al rango (-180, 180]. */
export function normalizeEulerDegrees(deg: number): number {
	let d = deg % 360;
	if (d > 180) d -= 360;
	if (d <= -180) d += 360;
	return d;
}

/** Delta más corto entre dos ángulos (p. ej. 350°→10° = +20°, no −340°). */
export function shortestDegDelta(prev: number, next: number, wrap: 360 | 180 = 360): number {
	let d = next - prev;
	const half = wrap / 2;
	if (d > half) d -= wrap;
	if (d < -half) d += wrap;
	return d;
}

/**
 * Extracción YXZ (Q = Y(y) · X(x) · Z(z)) — misma convención que `glam::EulerRot::YXZ`.
 * Los tres ejes usan `atan2` + `normalizeEulerDegrees`: rango circular (-180, 180] igual que
 * los sliders del panel Transform (X, Y y Z con la misma lógica).
 */
export function quatToEulerYxzDegrees(
	q: [number, number, number, number],
): [number, number, number] {
	const [qx, qy, qz, qw] = q;
	const sinx = 2 * (qw * qx - qy * qz);
	const cosx = 1 - 2 * (qx * qx + qy * qy);
	const pitch = Math.atan2(sinx, cosx);
	const siny = 2 * (qw * qy + qx * qz);
	const cosy = 1 - 2 * (qy * qy + qz * qz);
	const yaw = Math.atan2(siny, cosy);
	const sinz = 2 * (qw * qz + qx * qy);
	const cosz = 1 - 2 * (qz * qz + qx * qx);
	const roll = Math.atan2(sinz, cosz);
	return [
		normalizeEulerDegrees(pitch * RAD_TO_DEG),
		normalizeEulerDegrees(yaw * RAD_TO_DEG),
		normalizeEulerDegrees(roll * RAD_TO_DEG),
	];
}

export function quatNormalize(
	q: [number, number, number, number],
): [number, number, number, number] {
	const [x, y, z, w] = q;
	const len = Math.hypot(x, y, z, w);
	if (len < 1e-8) return [0, 0, 0, 1];
	return [x / len, y / len, z / len, w / len];
}

export function quatMultiply(
	a: [number, number, number, number],
	b: [number, number, number, number],
): [number, number, number, number] {
	const [ax, ay, az, aw] = a;
	const [bx, by, bz, bw] = b;
	return quatNormalize([
		aw * bx + ax * bw + ay * bz - az * by,
		aw * by - ax * bz + ay * bw + az * bx,
		aw * bz + ax * by - ay * bx + az * bw,
		aw * bw - ax * bx - ay * by - az * bz,
	]);
}

/** Gira el quaternion actual en su espacio local (post-multiply: q * axis). */
export function quatRotateLocalAxis(
	q: [number, number, number, number],
	axisIndex: 0 | 1 | 2,
	angleRad: number,
): [number, number, number, number] {
	const h = angleRad * 0.5;
	const s = Math.sin(h);
	const c = Math.cos(h);
	const dq: [number, number, number, number] =
		axisIndex === 0 ? [s, 0, 0, c] :
		axisIndex === 1 ? [0, s, 0, c] :
		[0, 0, s, c];
	return quatMultiply(q, dq);
}

/** Grados [pitchX, yawY, rollZ] → quaternion [x,y,z,w]. */
export function eulerYxzDegreesToQuat(
	pitchDeg: number,
	yawDeg: number,
	rollDeg: number,
): [number, number, number, number] {
	const pitch = pitchDeg * DEG_TO_RAD;
	const yaw = yawDeg * DEG_TO_RAD;
	const roll = rollDeg * DEG_TO_RAD;
	const cy = Math.cos(yaw / 2);
	const sy = Math.sin(yaw / 2);
	const cx = Math.cos(pitch / 2);
	const sx = Math.sin(pitch / 2);
	const cz = Math.cos(roll / 2);
	const sz = Math.sin(roll / 2);
	return [
		sx * cy * cz + cx * sy * sz,
		cx * sy * cz - sx * cy * sz,
		cx * cy * sz + sx * sy * cz,
		cx * cy * cz - sx * sy * sz,
	];
}
