//! Readback GPU de estadísticas SSR para diagnóstico en consola.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use wgpu::{Device, Queue, TextureView};

/// ~1.2 s a 60 FPS; evita inundar la consola.
const SSR_DEBUG_LOG_INTERVAL: u32 = 72;
const SSR_STATS_ALPHA_THRESHOLD: f32 = 0.05;
const SSR_STATS_STRIDE: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct SsrStatsGpu {
    skip_depth: u32,
    skip_rough: u32,
    skip_specular: u32,
    eligible: u32,
    miss_trace: u32,
    miss_vis: u32,
    screen_hits: u32,
    visible_hits: u32,
    sum_alpha: u32,
    sum_refl_lum: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SsrStatsUniforms {
    refl_resolution: [f32; 2],
    gbuffer_scale: f32,
    max_roughness: f32,
    alpha_threshold: f32,
    stride: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct SsrDebugLogSnapshot {
    pub frame_index: u32,
    pub tier: &'static str,
    pub viewport_w: u32,
    pub viewport_h: u32,
    pub refl_w: u32,
    pub refl_h: u32,
    pub screen_fraction: f32,
    pub gbuffer_scale: f32,
    pub coarse_resolution: f32,
    pub coarse_max_iters: u32,
    pub binary_steps: u32,
    pub step_m: f32,
    pub max_distance_m: f32,
    pub max_roughness: f32,
    pub temporal_blend: f32,
    pub ssr_ms: f32,
    pub temporal_ms: f32,
    pub composite_ms: f32,
}

pub struct SsrStatsReadback {
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
    stats_buffer: wgpu::Buffer,
    staging_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    pending: bool,
    snapshot: Option<SsrDebugLogSnapshot>,
    frames_until_sample: u32,
}

impl SsrStatsReadback {
    pub fn new(device: &Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ssr-stats"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ssr_stats.wgsl").into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ssr-stats-bgl"),
            entries: &[
                bgl_uniform(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                bgl_texture(2),
                bgl_texture(3),
                bgl_texture(4),
                bgl_texture(5),
                bgl_texture(6),
                bgl_texture(7),
            ],
        });

        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ssr-stats-pl"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("ssr-stats-pipeline"),
            layout: Some(&pl),
            module: &shader,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let stats_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ssr-stats-gpu"),
            contents: bytemuck::bytes_of(&SsrStatsGpu::default()),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
        });

        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ssr-stats-staging"),
            size: std::mem::size_of::<SsrStatsGpu>() as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ssr-stats-uniforms"),
            contents: bytemuck::bytes_of(&SsrStatsUniforms {
                refl_resolution: [1.0, 1.0],
                gbuffer_scale: 1.0,
                max_roughness: 0.7,
                alpha_threshold: 0.05,
                stride: 4,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            pipeline,
            bgl,
            stats_buffer,
            staging_buffer,
            uniform_buffer,
            pending: false,
            snapshot: None,
            frames_until_sample: 0,
        }
    }

    /// Fuerza muestreo+log en el próximo frame (p. ej. al activar vista ssr_debug).
    pub fn arm_immediate(&mut self) {
        self.frames_until_sample = 0;
    }

    /// Devuelve true cuando toca encolar stats este frame (~1 vez cada `SSR_DEBUG_LOG_INTERVAL`).
    pub fn tick_and_want_sample(&mut self) -> bool {
        if self.frames_until_sample > 0 {
            self.frames_until_sample -= 1;
            return false;
        }
        self.frames_until_sample = SSR_DEBUG_LOG_INTERVAL;
        true
    }

    #[allow(clippy::too_many_arguments)]
    pub fn queue_sample(
        &mut self,
        queue: &Queue,
        encoder: &mut wgpu::CommandEncoder,
        device: &Device,
        depth_view: &TextureView,
        surface_view: &TextureView,
        direct_view: &TextureView,
        base_color_view: &TextureView,
        reflection_view: &TextureView,
        hit_uv_view: &TextureView,
        refl_w: u32,
        refl_h: u32,
        snapshot: SsrDebugLogSnapshot,
    ) {
        let zero = SsrStatsGpu::default();
        queue.write_buffer(&self.stats_buffer, 0, bytemuck::bytes_of(&zero));

        let uniforms = SsrStatsUniforms {
            refl_resolution: [refl_w.max(1) as f32, refl_h.max(1) as f32],
            gbuffer_scale: snapshot.gbuffer_scale,
            max_roughness: snapshot.max_roughness,
            alpha_threshold: SSR_STATS_ALPHA_THRESHOLD,
            stride: SSR_STATS_STRIDE,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ssr-stats-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.stats_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(surface_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(reflection_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(hit_uv_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(direct_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(base_color_view),
                },
            ],
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ssr-stats-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let dx = (refl_w + 7) / 8;
            let dy = (refl_h + 7) / 8;
            pass.dispatch_workgroups(dx, dy, 1);
        }

        encoder.copy_buffer_to_buffer(
            &self.stats_buffer,
            0,
            &self.staging_buffer,
            0,
            std::mem::size_of::<SsrStatsGpu>() as u64,
        );

        self.pending = true;
        self.snapshot = Some(snapshot);
    }

    pub fn finish_and_log(&mut self, device: &Device) {
        if !self.pending {
            return;
        }
        self.pending = false;

        let snapshot = match self.snapshot.take() {
            Some(s) => s,
            None => return,
        };

        let slice = self.staging_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        if rx.recv().ok().and_then(|r| r.ok()).is_none() {
            log::warn!("[reflexiones][ssr] readback stats: falló map_async");
            self.staging_buffer.unmap();
            return;
        }

        let data = slice.get_mapped_range();
        let stats = *bytemuck::from_bytes::<SsrStatsGpu>(&data);
        drop(data);
        self.staging_buffer.unmap();

        log_ssr_debug_report(&snapshot, &stats);
    }
}

fn pct(part: u32, total: u32) -> f32 {
    if total == 0 {
        0.0
    } else {
        part as f32 / total as f32 * 100.0
    }
}

fn log_ssr_debug_report(snapshot: &SsrDebugLogSnapshot, stats: &SsrStatsGpu) {
    let sample_w = snapshot.refl_w.div_ceil(SSR_STATS_STRIDE);
    let sample_h = snapshot.refl_h.div_ceil(SSR_STATS_STRIDE);
    let sampled_px = sample_w * sample_h;

    let visible_rate = pct(stats.visible_hits, stats.eligible);
    let screen_rate = pct(stats.screen_hits, stats.eligible);
    let miss_trace_rate = pct(stats.miss_trace, stats.eligible);
    let miss_vis_rate = pct(stats.miss_vis, stats.eligible);

    let avg_alpha = if stats.visible_hits > 0 {
        stats.sum_alpha as f32 / stats.visible_hits as f32 / 10000.0
    } else {
        0.0
    };
    let avg_refl_lum = if stats.visible_hits > 0 {
        stats.sum_refl_lum as f32 / stats.visible_hits as f32 / 10000.0
    } else {
        0.0
    };

    log::info!(
        "[reflexiones][ssr] frame={} tier={} | muestra pass SSR pre-temporal cada {}f (~{:.1}s@60Hz)",
        snapshot.frame_index,
        snapshot.tier,
        SSR_DEBUG_LOG_INTERVAL,
        SSR_DEBUG_LOG_INTERVAL as f32 / 60.0,
    );
    log::info!(
        "[reflexiones][ssr]   resolución: viewport {}x{} pass {}x{} (frac={:.2}) gbuf_scale={:.2} \
         píxeles_muestreados≈{} (stride={})",
        snapshot.viewport_w,
        snapshot.viewport_h,
        snapshot.refl_w,
        snapshot.refl_h,
        snapshot.screen_fraction,
        snapshot.gbuffer_scale,
        sampled_px,
        SSR_STATS_STRIDE,
    );
    log::info!(
        "[reflexiones][ssr]   filtros G-buffer: sin_depth={} rugosidad_alta={} specular_cero={}",
        stats.skip_depth,
        stats.skip_rough,
        stats.skip_specular,
    );
    log::info!(
        "[reflexiones][ssr]   traza elegibles={} | miss_marcha={} ({:.1}%) impacto_uv={} ({:.1}%) \
         rechazo_vis={} ({:.1}%) visibles_composite={} ({:.1}%)",
        stats.eligible,
        stats.miss_trace,
        miss_trace_rate,
        stats.screen_hits,
        screen_rate,
        stats.miss_vis,
        miss_vis_rate,
        stats.visible_hits,
        visible_rate,
    );
    log::info!(
        "[reflexiones][ssr]   calidad visibles: alpha_media={:.3} lum_refl_rgb_media={:.4}",
        avg_alpha,
        avg_refl_lum,
    );
    log::info!(
        "[reflexiones][ssr]   params Lettier: coarse_res={:.2} coarse_iters≤{} refine={} grosor={:.3}m \
         dist_max={:.0}m rough≤{:.2} temporal_blend={:.2}",
        snapshot.coarse_resolution,
        snapshot.coarse_max_iters,
        snapshot.binary_steps,
        snapshot.step_m,
        snapshot.max_distance_m,
        snapshot.max_roughness,
        snapshot.temporal_blend,
    );
    log::info!(
        "[reflexiones][ssr]   timing ms: ssr={:.2} temporal={:.2} composite={:.2}",
        snapshot.ssr_ms,
        snapshot.temporal_ms,
        snapshot.composite_ms,
    );

    for hint in diagnose_ssr(stats, miss_trace_rate, miss_vis_rate, visible_rate, avg_refl_lum) {
        log::info!("[reflexiones][ssr]   → {hint}");
    }
}

fn diagnose_ssr(
    stats: &SsrStatsGpu,
    miss_trace_rate: f32,
    miss_vis_rate: f32,
    visible_rate: f32,
    avg_refl_lum: f32,
) -> Vec<&'static str> {
    let mut out = Vec::new();

    if stats.eligible == 0 {
        if stats.skip_specular > stats.skip_rough {
            out.push(
                "ningún píxel trazable: specular≈0 (albedo oscuro × rugosidad alta); prueba materiales más brillantes o menos roughness",
            );
        } else if stats.skip_rough > 0 {
            out.push(
                "ningún píxel trazable: rugosidad > max_roughness del tier; baja roughness en SurfacePbr o sube el tier",
            );
        } else {
            out.push("ningún píxel trazable en la muestra; revisa escena o cámara apuntando al vacío");
        }
        return out;
    }

    if visible_rate < 5.0 {
        if miss_trace_rate >= 85.0 {
            out.push(
                "miss_marcha alto: muchos rayos salen del frustum (cámara alta sobre suelo) o la línea en pantalla supera coarse_iters del tier",
            );
        }
        if visible_rate > 0.0 && avg_refl_lum < 0.15 {
            out.push(
                "lum_refl baja (~0.1): specular×color tenue (suelo rugoso); en Final puede ser casi invisible — usa ssr_debug (blanco=hit)",
            );
        }
        if stats.screen_hits > 0 && miss_vis_rate >= 1.0 {
            out.push(
                "parte de impactos UV anulados por visibilidad Lettier (grazing, grosor o distancia)",
            );
        }
        if visible_rate > 0.0 {
            out.push(
                "hay hits UV; si Final no los muestra, prueba cámara baja hacia el suelo o sube tier (más coarse_iters)",
            );
        }
    } else {
        out.push("traza SSR activa; si Final no se ve, revisa ángulo de cámara o fuerza en composite");
    }

    out
}

fn bgl_uniform(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn bgl_texture(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}
