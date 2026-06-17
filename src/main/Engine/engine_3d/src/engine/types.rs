use std::sync::Arc;
use std::time::{Duration, Instant};

use bytemuck::{Pod, Zeroable};

use crate::ecs::MeshComponent;
use crate::entity_save_meta::EntitySaveMeta;
use crate::ipc::{AnimScriptData, AnimationFrameData};

use super::DecodedAudio;

pub(crate) enum UndoAction {
    RestoreTransform {
        id: u32,
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    },
    RestoreTransforms {
        items: Vec<(u32, [f32; 3], [f32; 4], [f32; 3])>,
    },
    /// Undo de creación: deshacer = eliminar la entidad (snapshot para redo).
    RemoveEntity { snapshot: EntityUndoSnapshot },
    /// Redo tras Ctrl+Z en creación: volver a insertar con el mismo id.
    RestoreEntity { snapshot: EntityUndoSnapshot },
    /// Deshacer cambios en el HUD de la pantalla UI en edición (texto, botones, imágenes, objetos, dibujo).
    RestorePlayerUiHud {
        snapshot: crate::config_3d::player_ui::hud_undo::PlayerUiHudUndoSnapshot,
    },
    /// Deshacer creación de socket: eliminar el socket creado.
    RemoveEntitySocket {
        entity_id: u32,
        socket: crate::config_3d::entity_sockets::EntitySocketSnapshot,
    },
    /// Redo tras deshacer creación de socket.
    RestoreEntitySocket {
        entity_id: u32,
        socket: crate::config_3d::entity_sockets::EntitySocketSnapshot,
    },
    /// Deshacer vinculación a socket: restaurar attachment/transform previos.
    RestoreSocketAttachment {
        child_id: u32,
        previous_attachment: Option<crate::config_3d::entity_attachments::EntityAttachmentLocal>,
        previous_position: [f32; 3],
        previous_rotation: [f32; 4],
        previous_scale: [f32; 3],
        applied_attachment: crate::config_3d::entity_attachments::EntityAttachmentLocal,
    },
    RestoreBonePhysics {
        entity_id: u32,
        bone_name: String,
        before: Option<crate::config_3d::bone_physics::BonePhysicsMode>,
        after: Option<crate::config_3d::bone_physics::BonePhysicsMode>,
    },
}

#[derive(Clone)]
pub(crate) struct EntityUndoSnapshot {
    pub id:                 u32,
    pub name:               String,
    pub transform_position: [f32; 3],
    pub transform_rotation: [f32; 4],
    pub transform_scale:    [f32; 3],
    pub mesh:               MeshComponent,
    pub save_meta:          EntitySaveMeta,
    pub physics_enabled:    bool,
    pub physics_type:       String,
    pub physics_half:       [f32; 3],
    pub in_character_list:  bool,
    pub in_scenario_list:   bool,
}

#[derive(Clone)]
pub struct AnimationState {
    pub frames: Vec<AnimationFrameData>,
    pub fps: u32,
    pub loop_: bool,
    pub flip_horizontal: bool,
    pub audio_decoded: Option<Arc<DecodedAudio>>,
    pub logical_w: u32,
    pub logical_h: u32,
    pub scripts: Vec<AnimScriptData>,
    pub is_cancelable: bool,
}

pub struct ActiveAnimation {
    pub animation_name: String,
    pub current_frame: usize,
    pub last_frame_time: Instant,
    pub fps: u32,
    pub finished: bool,
}

pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub(crate) const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// 1024: ~4× menos trabajo que 2048 en el shadow pass; PCF/texel usan esta constante.
pub(crate) const SHADOW_MAP_SIZE: u32 = 1024;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct SceneUniforms {
    /// Proyección con jitter Halton (raster).
    pub(crate) view_proj: [[f32; 4]; 4],
    /// Proyección sin jitter (velocity / `prev_view_proj`).
    pub(crate) view_proj_stable: [[f32; 4]; 4],
    pub(crate) prev_view_proj: [[f32; 4]; 4],
    pub(crate) inv_view_proj: [[f32; 4]; 4],
    pub(crate) cam_pos: [f32; 4],
    /// xyz = hacia el sol, w = ambiente 0–1.
    pub(crate) light_dir: [f32; 4],
    /// rgb = color de luz; w > 0.5 activa sombras proyectadas.
    pub(crate) light_color: [f32; 4],
    pub(crate) light_view_proj: [[f32; 4]; 4],
    /// x = intensidad, z = 1/texel sombra, w = radio PCF (y sin uso en GPU).
    pub(crate) light_params: [f32; 4],
    /// xy = jitter subpíxel en espacio de proyección.
    pub(crate) jitter: [f32; 4],
    /// x = bias_min, y = bias_max, z = depth_const, w = depth_slope.
    pub(crate) shadow_bias: [f32; 4],
}

const INSTANCE_STRIDE: u64 = std::mem::size_of::<crate::mesh::InstanceData>() as u64;
/// Capacidad inicial por slot del pool (se amplía si hace falta).
const INSTANCE_POOL_MIN_BYTES: u64 = INSTANCE_STRIDE * 64;

/// Buffers de instancias reutilizables: `write_buffer` por frame, sin `create_buffer` cada vez.
pub(crate) struct InstanceBufferPool {
    buffers: Vec<wgpu::Buffer>,
    capacities: Vec<u64>,
}

impl InstanceBufferPool {
    pub fn new() -> Self {
        Self {
            buffers: Vec::new(),
            capacities: Vec::new(),
        }
    }

    /// Sube datos de cada batch al slot `i` y devuelve los buffers usados este frame.
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        batches: &[&[crate::mesh::InstanceData]],
    ) -> &[wgpu::Buffer] {
        let count = batches.len();
        while self.buffers.len() < count {
            let cap = INSTANCE_POOL_MIN_BYTES;
            self.buffers.push(create_instance_buffer(device, cap));
            self.capacities.push(cap);
        }
        for (i, instances) in batches.iter().enumerate() {
            let bytes = (instances.len() as u64).max(1) * INSTANCE_STRIDE;
            if self.capacities[i] < bytes {
                let cap = bytes.next_power_of_two().max(INSTANCE_POOL_MIN_BYTES);
                self.buffers[i] = create_instance_buffer(device, cap);
                self.capacities[i] = cap;
            }
            queue.write_buffer(&self.buffers[i], 0, bytemuck::cast_slice(instances));
        }
        &self.buffers[..count]
    }

    /// Igual que `upload` pero para `SkinnedInstanceData` (pipeline skinned).
    pub fn upload_skinned(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        batches: &[&[crate::mesh::SkinnedInstanceData]],
    ) -> &[wgpu::Buffer] {
        let stride = std::mem::size_of::<crate::mesh::SkinnedInstanceData>() as u64;
        let count = batches.len();
        while self.buffers.len() < count {
            let cap = stride * 64;
            self.buffers.push(create_instance_buffer(device, cap));
            self.capacities.push(cap);
        }
        for (i, instances) in batches.iter().enumerate() {
            let bytes = (instances.len() as u64).max(1) * stride;
            if self.capacities[i] < bytes {
                let cap = bytes.next_power_of_two().max(stride * 64);
                self.buffers[i] = create_instance_buffer(device, cap);
                self.capacities[i] = cap;
            }
            queue.write_buffer(&self.buffers[i], 0, bytemuck::cast_slice(instances));
        }
        &self.buffers[..count]
    }
}

fn create_instance_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("inst-buf-pool"),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
