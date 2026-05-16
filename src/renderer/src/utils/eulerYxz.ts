/**
 * Euler YXZ — misma convención que `glam::Quat::from_euler(EulerRot::YXZ, y, x, z)`.
 * Panel: rot[0]=eje X (pitch), rot[1]=eje Y (yaw), rot[2]=eje Z (roll).
 */

const RAD_TO_DEG = 180 / Math.PI;
const DEG_TO_RAD = Math.PI / 180;

/** Quaternion [x,y,z,w] → grados [pitchX, yawY, rollZ] en YXZ. */
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
	return [pitch * RAD_TO_DEG, yaw * RAD_TO_DEG, roll * RAD_TO_DEG];
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
