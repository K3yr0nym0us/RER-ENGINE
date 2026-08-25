//! BVH binario CPU (RTIOW libro 3) sobre triángulos en espacio mundo.

use glam::Vec3;

pub const MAX_RT_TRIANGLES: usize = 65_536;
#[allow(dead_code)]
pub const MAX_RT_BVH_NODES: usize = MAX_RT_TRIANGLES * 2;

const SAH_BINS: usize = 10;
const SAH_TRAVERSAL_COST: f32 = 0.125;
const SAH_INTERSECTION_COST: f32 = 1.0;

#[derive(Clone, Copy, Debug)]
pub struct RtTriangle {
    pub v0: Vec3,
    pub v1: Vec3,
    pub v2: Vec3,
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    pub uv2: [f32; 2],
    pub instance_slot: u32,
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RtTriangleGpu {
    pub v0: [f32; 4],
    pub v1: [f32; 4],
    pub v2: [f32; 4],
    pub uv0: [f32; 4],
    pub uv1: [f32; 4],
    pub uv2: [f32; 4],
}

impl From<RtTriangle> for RtTriangleGpu {
    fn from(t: RtTriangle) -> Self {
        Self {
            v0: [t.v0.x, t.v0.y, t.v0.z, f32::from_bits(t.instance_slot)],
            v1: [t.v1.x, t.v1.y, t.v1.z, 0.0],
            v2: [t.v2.x, t.v2.y, t.v2.z, 0.0],
            uv0: [t.uv0[0], t.uv0[1], 0.0, 0.0],
            uv1: [t.uv1[0], t.uv1[1], 0.0, 0.0],
            uv2: [t.uv2[0], t.uv2[1], 0.0, 0.0],
        }
    }
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BvhNodeGpu {
    pub bbox_min: [f32; 4],
    pub bbox_max: [f32; 4],
    pub left_or_tri_offset: u32,
    pub right_or_tri_count: u32,
    pub flags: u32,
    pub _pad: u32,
}

struct BuildNode {
    bbox_min: Vec3,
    bbox_max: Vec3,
    left: u32,
    right: u32,
    tri_start: u32,
    tri_count: u32,
    is_leaf: bool,
}

fn tri_bbox(t: RtTriangle) -> (Vec3, Vec3) {
    let mn = t.v0.min(t.v1).min(t.v2);
    let mx = t.v0.max(t.v1).max(t.v2);
    (mn, mx)
}

fn bbox_centroid(min: Vec3, max: Vec3) -> Vec3 {
    (min + max) * 0.5
}

fn bbox_surface_area(min: Vec3, max: Vec3) -> f32 {
    let e = max - min;
    2.0 * (e.x * e.y + e.x * e.z + e.y * e.z)
}

fn tri_centroid_axis(t: RtTriangle, axis: usize) -> f32 {
    let (mn, mx) = tri_bbox(t);
    bbox_centroid(mn, mx)[axis]
}

struct SahSplit {
    axis: usize,
    split_pos: f32,
    left_count: usize,
}

fn find_sah_split(
    triangles: &[RtTriangle],
    tri_order: &[usize],
    bb_min: Vec3,
    bb_max: Vec3,
) -> Option<SahSplit> {
    let n = tri_order.len();
    if n <= 2 {
        return None;
    }

    let parent_area = bbox_surface_area(bb_min, bb_max);
    if parent_area <= 0.0 {
        return None;
    }

    let mut best_cost = SAH_INTERSECTION_COST * n as f32;
    let mut best: Option<SahSplit> = None;

    for axis in 0..3 {
        let axis_min = bb_min[axis];
        let axis_max = bb_max[axis];
        let extent = axis_max - axis_min;
        if extent <= 1e-8 {
            continue;
        }

        let mut bin_counts = [0usize; SAH_BINS];
        let mut bin_bounds_min = [Vec3::splat(f32::INFINITY); SAH_BINS];
        let mut bin_bounds_max = [Vec3::splat(f32::NEG_INFINITY); SAH_BINS];

        for &ti in tri_order {
            let c = tri_centroid_axis(triangles[ti], axis);
            let mut bin = ((c - axis_min) / extent * SAH_BINS as f32) as usize;
            if bin >= SAH_BINS {
                bin = SAH_BINS - 1;
            }
            bin_counts[bin] += 1;
            let (mn, mx) = tri_bbox(triangles[ti]);
            bin_bounds_min[bin] = bin_bounds_min[bin].min(mn);
            bin_bounds_max[bin] = bin_bounds_max[bin].max(mx);
        }

        let mut left_count_prefix = [0usize; SAH_BINS];
        let mut left_min_prefix = [Vec3::splat(f32::INFINITY); SAH_BINS];
        let mut left_max_prefix = [Vec3::splat(f32::NEG_INFINITY); SAH_BINS];
        for i in 0..SAH_BINS {
            if i > 0 {
                left_count_prefix[i] = left_count_prefix[i - 1] + bin_counts[i - 1];
                left_min_prefix[i] = left_min_prefix[i - 1].min(bin_bounds_min[i - 1]);
                left_max_prefix[i] = left_max_prefix[i - 1].max(bin_bounds_max[i - 1]);
            }
            if bin_counts[i] > 0 {
                left_min_prefix[i] = left_min_prefix[i].min(bin_bounds_min[i]);
                left_max_prefix[i] = left_max_prefix[i].max(bin_bounds_max[i]);
            }
        }

        let mut right_count_suffix = [0usize; SAH_BINS + 1];
        let mut right_min_suffix = [Vec3::splat(f32::INFINITY); SAH_BINS + 1];
        let mut right_max_suffix = [Vec3::splat(f32::NEG_INFINITY); SAH_BINS + 1];
        for i in (0..SAH_BINS).rev() {
            right_count_suffix[i] = right_count_suffix[i + 1] + bin_counts[i];
            if bin_counts[i] > 0 {
                right_min_suffix[i] = right_min_suffix[i + 1]
                    .min(bin_bounds_min[i])
                    .min(bin_bounds_min[i]);
                right_max_suffix[i] = right_max_suffix[i + 1]
                    .max(bin_bounds_max[i])
                    .max(bin_bounds_max[i]);
            } else {
                right_min_suffix[i] = right_min_suffix[i + 1];
                right_max_suffix[i] = right_max_suffix[i + 1];
            }
        }

        for split_bin in 1..SAH_BINS {
            let left_count = left_count_prefix[split_bin];
            let right_count = right_count_suffix[split_bin];
            if left_count == 0 || right_count == 0 {
                continue;
            }

            let left_area =
                bbox_surface_area(left_min_prefix[split_bin], left_max_prefix[split_bin]);
            let right_area =
                bbox_surface_area(right_min_suffix[split_bin], right_max_suffix[split_bin]);
            let cost = SAH_TRAVERSAL_COST
                + SAH_INTERSECTION_COST
                    * (left_area * left_count as f32 + right_area * right_count as f32)
                    / parent_area;

            if cost < best_cost {
                best_cost = cost;
                let split_pos = axis_min + (split_bin as f32 / SAH_BINS as f32) * extent;
                best = Some(SahSplit {
                    axis,
                    split_pos,
                    left_count,
                });
            }
        }
    }

    best
}

fn partition_tri_order(
    triangles: &[RtTriangle],
    tri_order: &mut [usize],
    axis: usize,
    split_pos: f32,
    left_count: usize,
) {
    tri_order.sort_by(|&a, &b| {
        let ca = tri_centroid_axis(triangles[a], axis);
        let cb = tri_centroid_axis(triangles[b], axis);
        ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mid = left_count.min(tri_order.len()).max(1);
    if mid >= tri_order.len() {
        return;
    }
    let _ = split_pos;
}

/// Construye BVH SAH sobre `triangles` (se reordena internamente).
pub fn build_bvh(mut triangles: Vec<RtTriangle>) -> (Vec<BvhNodeGpu>, Vec<RtTriangleGpu>) {
    if triangles.is_empty() {
        return (Vec::new(), Vec::new());
    }
    if triangles.len() > MAX_RT_TRIANGLES {
        triangles.truncate(MAX_RT_TRIANGLES);
    }

    let mut nodes: Vec<BuildNode> = Vec::new();
    let mut tri_order: Vec<usize> = (0..triangles.len()).collect();

    fn build_recursive(
        nodes: &mut Vec<BuildNode>,
        triangles: &[RtTriangle],
        tri_order: &mut [usize],
    ) -> u32 {
        let mut bb_min = Vec3::splat(f32::INFINITY);
        let mut bb_max = Vec3::splat(f32::NEG_INFINITY);
        for &ti in tri_order.iter() {
            let (mn, mx) = tri_bbox(triangles[ti]);
            bb_min = bb_min.min(mn);
            bb_max = bb_max.max(mx);
        }

        let node_idx = nodes.len() as u32;
        if tri_order.len() <= 2 {
            nodes.push(BuildNode {
                bbox_min: bb_min,
                bbox_max: bb_max,
                left: 0,
                right: 0,
                tri_start: tri_order[0] as u32,
                tri_count: tri_order.len() as u32,
                is_leaf: true,
            });
            return node_idx;
        }

        let split = find_sah_split(triangles, tri_order, bb_min, bb_max).unwrap_or_else(|| {
            let extent = bb_max - bb_min;
            let axis = if extent.x >= extent.y && extent.x >= extent.z {
                0
            } else if extent.y >= extent.z {
                1
            } else {
                2
            };
            tri_order.sort_by(|&a, &b| {
                let ca = tri_centroid_axis(triangles[a], axis);
                let cb = tri_centroid_axis(triangles[b], axis);
                ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
            });
            SahSplit {
                axis,
                split_pos: 0.0,
                left_count: tri_order.len() / 2,
            }
        });

        partition_tri_order(
            triangles,
            tri_order,
            split.axis,
            split.split_pos,
            split.left_count,
        );

        let mid = split.left_count.min(tri_order.len() - 1).max(1);
        let (left_slice, right_slice) = tri_order.split_at_mut(mid);
        let left_child = build_recursive(nodes, triangles, left_slice);
        let right_child = build_recursive(nodes, triangles, right_slice);

        nodes.push(BuildNode {
            bbox_min: bb_min,
            bbox_max: bb_max,
            left: left_child,
            right: right_child,
            tri_start: 0,
            tri_count: 0,
            is_leaf: false,
        });
        node_idx
    }

    let _root = build_recursive(&mut nodes, &triangles, &mut tri_order);

    let gpu_nodes: Vec<BvhNodeGpu> = nodes
        .iter()
        .map(|n| {
            if n.is_leaf {
                BvhNodeGpu {
                    bbox_min: [n.bbox_min.x, n.bbox_min.y, n.bbox_min.z, 0.0],
                    bbox_max: [n.bbox_max.x, n.bbox_max.y, n.bbox_max.z, 0.0],
                    left_or_tri_offset: n.tri_start,
                    right_or_tri_count: n.tri_count,
                    flags: 1,
                    _pad: 0,
                }
            } else {
                BvhNodeGpu {
                    bbox_min: [n.bbox_min.x, n.bbox_min.y, n.bbox_min.z, 0.0],
                    bbox_max: [n.bbox_max.x, n.bbox_max.y, n.bbox_max.z, 0.0],
                    left_or_tri_offset: n.left,
                    right_or_tri_count: n.right,
                    flags: 0,
                    _pad: 0,
                }
            }
        })
        .collect();

    let gpu_tris: Vec<RtTriangleGpu> = triangles.into_iter().map(RtTriangleGpu::from).collect();
    (gpu_nodes, gpu_tris)
}
