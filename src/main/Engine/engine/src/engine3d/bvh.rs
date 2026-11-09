//! BVH binario en CPU: respaldo por software del trazado de reflejos cuando el
//! adaptador no expone `VK_KHR_ray_tracing_pipeline`, o cuando el switch RT está
//! apagado pero el tier de reflejos sigue activo.
//!
//! Los triángulos llegan ya en espacio mundo y etiquetados con el slot de material
//! (índice en `RtMaterialGpu`). El layout GPU debe coincidir con `shaders/bvh/trace.comp`:
//!
//! ```text
//! BvhNode  = 48 B (bbox_min.xyz + pad, bbox_max.xyz + pad, left/tri_offset, right/tri_count, flags, pad)
//! Tri      = 96 B (v0, v1, v2, uv0, uv1, uv2; v0.w guarda el slot como bits de f32)
//! Material = 48 B (albedo, pbr, probe)
//! ```

use glam::Vec3;

/// Cota de triángulos que se aceptan en el BVH software. Por encima se trunca con un
/// aviso: el fallback es para GPUs sin RT, donde un BVH mayor costaría más que el frame.
pub const MAX_RT_TRIANGLES: usize = 262_144;

/// Triángulos por hoja. Mantiene la pila del shader (`stack[32]`) holgada.
const LEAF_TRIANGLES: usize = 4;

/// Pieza skinned lista para el trazado por hardware: el BLAS se construye desde el buffer de
/// posiciones locales ya poseadas de `State::rt_skinned_vertices`, y la instancia aplica el
/// `transform` de la entidad.
///
/// `material_slot` es el índice en la lista de `RtMaterialGpu` y tiene que coincidir con el orden
/// de `build_skinned_trace`: por eso ambos recorridos usan la lista ordenada de
/// `collect_skinned_trace_targets`, no el orden de iteración de un `HashMap`.
#[derive(Clone, Copy, Debug)]
pub struct SkinnedRtInstance {
    /// Índice en `State::skinned_gpu_meshes` (único por entidad y pieza).
    pub gpu_mesh_idx: usize,
    /// Transform 3x4 fila-major que espera el TLAS.
    pub transform: [f32; 12],
    /// Posiciones locales poseídas de esta pieza (stride 12 B).
    pub vertex_count: u32,
    /// Índice dentro de `State::rt_skinned_materials`.
    pub material_slot: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct RtTriangle {
    pub v0: Vec3,
    pub v1: Vec3,
    pub v2: Vec3,
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    pub uv2: [f32; 2],
    /// Índice en `RtMaterialGpu`; el shader lo lee de `v0.w`.
    pub material_slot: u32,
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
            v0: [t.v0.x, t.v0.y, t.v0.z, f32::from_bits(t.material_slot)],
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
    /// 0 = nodo interno, 1 = hoja.
    pub flags: u32,
    pub _pad: u32,
}

/// Material por instancia. Coincide con `struct Material` de `trace.comp` y `primary.rchit`.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RtMaterialGpu {
    /// rgb = albedo; w = bits de flags (bit 0 = dieléctrico).
    pub albedo: [f32; 4],
    /// x = roughness, y = metallic, z = ior, w = tex_idx.
    pub pbr: [f32; 4],
    /// x = probe layer (-1 = automático).
    pub probe: [f32; 4],
}

/// Resultado listo para subir a la GPU.
#[derive(Clone, Debug, Default)]
pub struct RtSceneBlob {
    pub nodes: Vec<u8>,
    pub node_count: u32,
    pub tris: Vec<u8>,
    pub tri_count: u32,
    pub materials: Vec<u8>,
    pub material_count: u32,
}

impl RtSceneBlob {
    pub fn from_parts(
        nodes: &[BvhNodeGpu],
        tris: &[RtTriangleGpu],
        materials: &[RtMaterialGpu],
    ) -> Self {
        Self {
            nodes: bytemuck::cast_slice(nodes).to_vec(),
            node_count: nodes.len() as u32,
            tris: bytemuck::cast_slice(tris).to_vec(),
            tri_count: tris.len() as u32,
            materials: bytemuck::cast_slice(materials).to_vec(),
            material_count: materials.len() as u32,
        }
    }
}

#[derive(Clone, Copy)]
struct BuildNode {
    bbox_min: Vec3,
    bbox_max: Vec3,
    left: u32,
    right: u32,
    tri_start: u32,
    tri_count: u32,
    leaf: bool,
}

fn bbox_of(triangles: &[RtTriangle], order: &[usize]) -> (Vec3, Vec3) {
    let mut bb_min = Vec3::splat(f32::INFINITY);
    let mut bb_max = Vec3::splat(f32::NEG_INFINITY);
    for &ti in order {
        let t = triangles[ti];
        let mn = t.v0.min(t.v1).min(t.v2);
        let mx = t.v0.max(t.v1).max(t.v2);
        bb_min = bb_min.min(mn);
        bb_max = bb_max.max(mx);
    }
    (bb_min, bb_max)
}

fn longest_axis(min: Vec3, max: Vec3) -> usize {
    let extent = max - min;
    if extent.x >= extent.y && extent.x >= extent.z {
        0
    } else if extent.y >= extent.z {
        1
    } else {
        2
    }
}

/// Construye un BVH binario balanceado por la mediana del centroide.
///
/// Devuelve nodos y triángulos reordenados de forma que cada hoja referencia un rango
/// contiguo `[left_or_tri_offset, left_or_tri_offset + right_or_tri_count)`.
pub fn build_bvh(mut triangles: Vec<RtTriangle>) -> (Vec<BvhNodeGpu>, Vec<RtTriangleGpu>) {
    if triangles.is_empty() {
        let node = BvhNodeGpu {
            flags: 1,
            ..BvhNodeGpu::default()
        };
        return (vec![node], Vec::new());
    }
    if triangles.len() > MAX_RT_TRIANGLES {
        log::warn!(
            "[RT] BVH software truncado: {} > {MAX_RT_TRIANGLES} triángulos",
            triangles.len()
        );
        triangles.truncate(MAX_RT_TRIANGLES);
    }

    let centroids: Vec<Vec3> = triangles
        .iter()
        .map(|t| (t.v0.min(t.v1).min(t.v2) + t.v0.max(t.v1).max(t.v2)) * 0.5)
        .collect();

    let mut nodes: Vec<BuildNode> = Vec::with_capacity(triangles.len().saturating_mul(2));
    let mut ordered: Vec<RtTriangle> = Vec::with_capacity(triangles.len());
    let mut order: Vec<usize> = (0..triangles.len()).collect();

    fn recurse(
        nodes: &mut Vec<BuildNode>,
        ordered: &mut Vec<RtTriangle>,
        triangles: &[RtTriangle],
        centroids: &[Vec3],
        order: &mut [usize],
    ) -> u32 {
        let node_index = nodes.len() as u32;
        let (bb_min, bb_max) = bbox_of(triangles, order);
        nodes.push(BuildNode {
            bbox_min: bb_min,
            bbox_max: bb_max,
            left: 0,
            right: 0,
            tri_start: 0,
            tri_count: 0,
            leaf: false,
        });

        if order.len() <= LEAF_TRIANGLES {
            let tri_start = ordered.len() as u32;
            for &ti in order.iter() {
                ordered.push(triangles[ti]);
            }
            nodes[node_index as usize] = BuildNode {
                bbox_min: bb_min,
                bbox_max: bb_max,
                left: 0,
                right: 0,
                tri_start,
                tri_count: order.len() as u32,
                leaf: true,
            };
            return node_index;
        }

        let axis = longest_axis(bb_min, bb_max);
        let mid = order.len() / 2;
        order.select_nth_unstable_by(mid, |&a, &b| {
            centroids[a][axis]
                .partial_cmp(&centroids[b][axis])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let (left_order, right_order) = order.split_at_mut(mid);
        let left = recurse(nodes, ordered, triangles, centroids, left_order);
        let right = recurse(nodes, ordered, triangles, centroids, right_order);

        nodes[node_index as usize].left = left;
        nodes[node_index as usize].right = right;
        node_index
    }

    let _root = recurse(&mut nodes, &mut ordered, &triangles, &centroids, &mut order);

    let gpu_nodes: Vec<BvhNodeGpu> = nodes
        .iter()
        .map(|n| BvhNodeGpu {
            bbox_min: [n.bbox_min.x, n.bbox_min.y, n.bbox_min.z, 0.0],
            bbox_max: [n.bbox_max.x, n.bbox_max.y, n.bbox_max.z, 0.0],
            left_or_tri_offset: if n.leaf { n.tri_start } else { n.left },
            right_or_tri_count: if n.leaf { n.tri_count } else { n.right },
            flags: u32::from(n.leaf),
            _pad: 0,
        })
        .collect();

    let gpu_tris: Vec<RtTriangleGpu> = ordered.into_iter().map(RtTriangleGpu::from).collect();
    (gpu_nodes, gpu_tris)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri(x: f32, slot: u32) -> RtTriangle {
        RtTriangle {
            v0: Vec3::new(x, 0.0, 0.0),
            v1: Vec3::new(x + 1.0, 0.0, 0.0),
            v2: Vec3::new(x, 1.0, 0.0),
            uv0: [0.0, 0.0],
            uv1: [1.0, 0.0],
            uv2: [0.0, 1.0],
            material_slot: slot,
        }
    }

    #[test]
    fn empty_build_has_a_leaf_root() {
        let (nodes, tris) = build_bvh(Vec::new());
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].flags, 1);
        assert!(tris.is_empty());
    }

    #[test]
    fn single_triangle_is_a_leaf() {
        let (nodes, tris) = build_bvh(vec![tri(0.0, 7)]);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].flags, 1);
        assert_eq!(nodes[0].left_or_tri_offset, 0);
        assert_eq!(nodes[0].right_or_tri_count, 1);
        assert_eq!(tris.len(), 1);
        assert_eq!(tris[0].v0[3].to_bits(), 7);
    }

    #[test]
    fn leaves_cover_every_triangle_exactly_once() {
        let triangles: Vec<RtTriangle> = (0..100).map(|i| tri(i as f32, i)).collect();
        let (nodes, tris) = build_bvh(triangles);
        assert_eq!(tris.len(), 100);

        let mut covered = vec![0u32; tris.len()];
        for node in nodes.iter().filter(|n| n.flags == 1) {
            let start = node.left_or_tri_offset as usize;
            let count = node.right_or_tri_count as usize;
            assert!(
                start + count <= tris.len(),
                "rango de hoja fuera de límites"
            );
            for slot in &mut covered[start..start + count] {
                *slot += 1;
            }
        }
        assert!(
            covered.iter().all(|&n| n == 1),
            "cada triángulo debe aparecer exactamente en una hoja"
        );
    }

    #[test]
    fn internal_nodes_point_to_children_and_bounds_contain_geometry() {
        let triangles: Vec<RtTriangle> = (0..64).map(|i| tri(i as f32 * 0.1, 0)).collect();
        let (nodes, _) = build_bvh(triangles);
        for (index, node) in nodes.iter().enumerate() {
            if node.flags == 1 {
                continue;
            }
            let left = node.left_or_tri_offset as usize;
            let right = node.right_or_tri_count as usize;
            assert!(left != index && right != index);
            assert!(left < nodes.len() && right < nodes.len());
            let child_left = &nodes[left];
            let child_right = &nodes[right];
            for child in [child_left, child_right] {
                for axis in 0..3 {
                    assert!(node.bbox_min[axis] <= child.bbox_min[axis]);
                    assert!(node.bbox_max[axis] >= child.bbox_max[axis]);
                }
            }
        }
    }

    #[test]
    fn gpu_layout_matches_shader_strides() {
        assert_eq!(std::mem::size_of::<BvhNodeGpu>(), 48);
        assert_eq!(std::mem::size_of::<RtTriangleGpu>(), 96);
        assert_eq!(std::mem::size_of::<RtMaterialGpu>(), 48);
        assert_eq!(std::mem::align_of::<BvhNodeGpu>(), 16);
        assert_eq!(std::mem::align_of::<RtTriangleGpu>(), 16);
        assert_eq!(std::mem::align_of::<RtMaterialGpu>(), 16);
    }

    #[test]
    fn tree_depth_fits_shader_stack() {
        // 65 536 triángulos → 16 384 hojas → profundidad 14; el shader reserva 32.
        let triangles: Vec<RtTriangle> = (0..65_536).map(|i| tri(i as f32, 0)).collect();
        let (nodes, _) = build_bvh(triangles);
        let mut depth = vec![0u32; nodes.len()];
        depth[0] = 1;
        let mut max_depth = 1;
        for (index, node) in nodes.iter().enumerate() {
            if node.flags == 1 {
                continue;
            }
            let d = depth[index] + 1;
            max_depth = max_depth.max(d);
            depth[node.left_or_tri_offset as usize] = d;
            depth[node.right_or_tri_count as usize] = d;
        }
        assert!(
            max_depth < 31,
            "profundidad {max_depth} no cabe en stack[32]"
        );
    }

    #[test]
    fn blob_serializes_expected_sizes() {
        let (nodes, tris) = build_bvh(vec![tri(0.0, 2), tri(4.0, 3)]);
        let materials = [RtMaterialGpu::default(); 2];
        let blob = RtSceneBlob::from_parts(&nodes, &tris, &materials);
        assert_eq!(blob.node_count as usize, nodes.len());
        assert_eq!(blob.nodes.len(), nodes.len() * 48);
        assert_eq!(blob.tri_count as usize, tris.len());
        assert_eq!(blob.tris.len(), tris.len() * 96);
        assert_eq!(blob.materials.len(), 2 * 48);
    }
}
