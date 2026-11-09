//! Culling de AABB contra el frustum de la cámara.

use super::*;

pub(crate) fn is_aabb_visible_3d(view_proj: &glam::Mat4, center: GlamVec3, half: GlamVec3) -> bool {
    let m = view_proj.to_cols_array_2d();
    let r0 = [m[0][0], m[1][0], m[2][0], m[3][0]];
    let r1 = [m[0][1], m[1][1], m[2][1], m[3][1]];
    let r2 = [m[0][2], m[1][2], m[2][2], m[3][2]];
    let r3 = [m[0][3], m[1][3], m[2][3], m[3][3]];

    // Vulkan: z en [0, 1]. Los planos laterales usan r3 ± r0/r1 y el
    // near/far son r2 y r3 - r2 (no r3 ± r2, que es la convención OpenGL).
    let planes: [[f32; 4]; 6] = [
        [r3[0] + r0[0], r3[1] + r0[1], r3[2] + r0[2], r3[3] + r0[3]],
        [r3[0] - r0[0], r3[1] - r0[1], r3[2] - r0[2], r3[3] - r0[3]],
        [r3[0] + r1[0], r3[1] + r1[1], r3[2] + r1[2], r3[3] + r1[3]],
        [r3[0] - r1[0], r3[1] - r1[1], r3[2] - r1[2], r3[3] - r1[3]],
        [r2[0], r2[1], r2[2], r2[3]],
        [r3[0] - r2[0], r3[1] - r2[1], r3[2] - r2[2], r3[3] - r2[3]],
    ];

    for plane in planes {
        let d = plane[0] * center.x + plane[1] * center.y + plane[2] * center.z + plane[3];
        if d < -half.x * plane[0].abs() - half.y * plane[1].abs() - half.z * plane[2].abs() {
            return false;
        }
    }
    true
}
