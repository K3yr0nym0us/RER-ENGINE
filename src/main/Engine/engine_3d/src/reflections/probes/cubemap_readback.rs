//! Readback GPU del cubemap capturado (centro de cada cara, mip 0) para diagnóstico.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use glam::Vec3;
use wgpu;

use crate::ecs::EntityId;
use crate::reflections::probe_env::MAX_PROBES;

const FACES: usize = 6;
/// Alineación mínima de fila para `copy_texture_to_buffer` (Rgba8Unorm).
const ROW_BYTES: u32 = 256;

pub(crate) struct ProbeCubemapReadback {
    staging: wgpu::Buffer,
    pending: bool,
    /// (entidad, ranura cubemap) en el mismo orden que las copias encoladas.
    pending_probes: Vec<(EntityId, usize)>,
    copy_count: usize,
}

impl ProbeCubemapReadback {
    pub fn new(device: &wgpu::Device) -> Self {
        let max_copies = MAX_PROBES * FACES;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("probe-cubemap-readback"),
            size: max_copies as u64 * ROW_BYTES as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            staging,
            pending: false,
            pending_probes: Vec::new(),
            copy_count: 0,
        }
    }

    pub fn is_pending(&self) -> bool {
        self.pending
    }

    /// Encola copia 1×1 del centro de cada cara mip 0 por probe activo.
    pub fn queue(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        cube: &wgpu::Texture,
        face_size: u32,
        probe_list: &[(EntityId, Vec3, usize)],
    ) {
        self.pending_probes.clear();
        self.copy_count = 0;
        let cx = face_size / 2;
        let cy = face_size / 2;

        for &(entity_id, _, slot) in probe_list {
            self.pending_probes.push((entity_id, slot));
            for face in 0..FACES {
                let array_layer = (slot * FACES + face) as u32;
                let buffer_offset = (self.copy_count * ROW_BYTES as usize) as u64;
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo {
                        texture: cube,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: cx,
                            y: cy,
                            z: array_layer,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyBufferInfo {
                        buffer: &self.staging,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: buffer_offset,
                            bytes_per_row: Some(ROW_BYTES),
                            rows_per_image: Some(1),
                        },
                    },
                    wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                );
                self.copy_count += 1;
            }
        }
        self.pending = !probe_list.is_empty();
    }

    /// Tras `queue.submit`, hashea y registra en consola.
    pub fn finish_and_log(
        &mut self,
        device: &wgpu::Device,
        frame_id: u32,
        face_size: u32,
    ) -> bool {
        if !self.pending {
            return false;
        }
        self.pending = false;

        let slice = self.staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        if rx.recv().ok().and_then(|r| r.ok()).is_none() {
            log::warn!("[reflexiones][cubemap-capture] readback: falló map_async");
            self.staging.unmap();
            return false;
        }

        let data = slice.get_mapped_range();
        log::info!(
            "[reflexiones][cubemap-capture] frame={frame_id} face_size={face_size}px \
             — hash por ranura (6 caras, centro mip0); captura omite entidades [ReflectionProbe]"
        );

        let mut all_same = true;
        let mut first_hash: Option<u64> = None;
        let face_labels = ["+X", "-X", "+Y", "-Y", "+Z", "-Z"];

        for (probe_i, &(entity_id, slot)) in self.pending_probes.iter().enumerate() {
            let mut face_hashes = [0u64; FACES];
            let mut face_rgb = [[0u8; 3]; FACES];
            for face in 0..FACES {
                let copy_idx = probe_i * FACES + face;
                let offset = copy_idx * ROW_BYTES as usize;
                let px = &data[offset..offset + 4];
                face_rgb[face] = [px[0], px[1], px[2]];
                face_hashes[face] = hash_bytes(px);
            }
            let slot_hash = {
                let mut h = DefaultHasher::new();
                for fh in face_hashes {
                    fh.hash(&mut h);
                }
                h.finish()
            };
            if let Some(first) = first_hash {
                if slot_hash != first {
                    all_same = false;
                }
            } else {
                first_hash = Some(slot_hash);
            }
            log::info!(
                "[reflexiones][cubemap-capture] entidad={entity_id} ranura={slot} \
                 cubemap_hash=0x{slot_hash:016x} +X_rgb=({},{},{})",
                face_rgb[0][0],
                face_rgb[0][1],
                face_rgb[0][2],
            );
            for face in 0..FACES {
                log::info!(
                    "[reflexiones][cubemap-capture]   cara[{face}]={} hash=0x{:016x} rgb=({},{},{})",
                    face_labels[face],
                    face_hashes[face],
                    face_rgb[face][0],
                    face_rgb[face][1],
                    face_rgb[face][2],
                );
            }
        }

        if self.pending_probes.len() > 1 {
            if all_same {
                log::warn!(
                    "[reflexiones][cubemap-capture] todas las ranuras comparten el mismo hash \
                     — contenido idéntico en GPU o escena vacía/simetrica"
                );
            } else {
                log::info!(
                    "[reflexiones][cubemap-capture] hashes distintos entre ranuras — cubemaps difieren en GPU"
                );
            }
        }

        drop(data);
        self.staging.unmap();
        true
    }
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}
