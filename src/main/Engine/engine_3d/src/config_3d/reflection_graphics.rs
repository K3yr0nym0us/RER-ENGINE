//! Nivel global de reflejos (Off / Low / Medium / High) y presets internos.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ReflectionTier {
    #[default]
    Off,
    Low,
    Medium,
    High,
    Ultra,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReflectionDebugView {
    Final,
    Normals,
    Depth,
    SsrHits,
    ReflectionMask,
    /// Solo el buffer `ambient`: en el metal ES la contribución del cubemap (entorno). Aísla
    /// si el grano viene del cubemap (sin SSR/RT encima).
    Cubemap,
    /// Solo el color del reflejo SSR+RT (`t_reflection.rgb`). Aísla si el grano viene de SSR/RT.
    ReflectionColor,
    /// Rugosidad cruda del G-buffer (`surface.g`: blanco=1 mate, negro=0 espejo).
    Roughness,
    /// Metallic crudo del G-buffer (`direct.a`: blanco=1 metal, negro=0 dieléctrico).
    Metallic,
    /// Posición mundo reconstruida desde depth (patrón RGB).
    ReconWorld,
    /// NDC OpenGL usado en inv_view_proj (z ya convertido de Vulkan).
    ReconNdc,
    /// Posición en espacio vista (matriz view del frame).
    ReconView,
    /// UV reproyectada world→clip→uv; canal B = error |Δuv|.
    ReprojectUv,
    /// RGB = view_dir mundo (superficie → cámara).
    SsrViewVector,
    /// RGB = refl_dir mundo tras reflect(incident, normal).
    SsrReflectionVector,
    /// R = recorrido UV (px), G = progreso march, B = hit.
    SsrRaymarchPath,
    /// |Δdepth| rayo vs buffer en el hit (escala ×10).
    SsrHitDepthDelta,
    /// UV final del impacto SSR (RG).
    SsrHitUv,
    /// Color lit_scene en hit UV sin blur (ambient+direct).
    SsrHitColorRaw,
    /// Color lit_scene en hit UV con blur 7×7 (como SSR).
    SsrHitColorBlurred,
    /// Salida SSR sin blur (re-ejecuta SSR con ssr_blur_enabled=0).
    SsrNoBlur,
    /// RG = hit_uv world-space, BA = hit_uv screen-space (march en UV/proyección).
    SsrHitUvWorldScreenPair,
    /// R=|Δu| G=|Δv| B=|Δuv|×50 entre hit world-space y screen-space.
    SsrHitUvWorldScreenDelta,
    /// Mitad izq.: hit UV world (RG); mitad der.: hit UV screen (RG).
    SsrHitUvWorldScreenSplit,
    /// Escena post-composite (misma mezcla que Final, sin TAA escena).
    SsrFinalComposite,
    /// Albedo del G-buffer (`base_color` Rgba8Unorm) para validar tinte RTIOW.
    BaseColor,
    /// Máscara `refl_trace_strength` (F0 unificado + rugosidad).
    TraceStrength,
}

impl ReflectionDebugView {
    pub fn from_wire(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "final" | "" => Some(Self::Final),
            "normals" | "normal" => Some(Self::Normals),
            "depth" => Some(Self::Depth),
            "ssr_hits" | "ssrhits" | "hits" => Some(Self::SsrHits),
            "reflection_mask" | "mask" => Some(Self::ReflectionMask),
            "cubemap" | "cube" | "env" => Some(Self::Cubemap),
            "reflection_color" | "refl_color" | "ssr_rt" => Some(Self::ReflectionColor),
            "roughness" | "rough" => Some(Self::Roughness),
            "metallic" | "metal" => Some(Self::Metallic),
            "recon_world" | "world_pos" => Some(Self::ReconWorld),
            "recon_ndc" | "ndc" => Some(Self::ReconNdc),
            "recon_view" | "view_pos" => Some(Self::ReconView),
            "reproject_uv" | "reproj_uv" => Some(Self::ReprojectUv),
            "ssr_view_vector" | "view_vector" => Some(Self::SsrViewVector),
            "ssr_reflection_vector" | "reflection_vector" | "refl_vector" => {
                Some(Self::SsrReflectionVector)
            }
            "ssr_raymarch_path" | "raymarch_path" | "ray_path" => Some(Self::SsrRaymarchPath),
            "ssr_hit_depth_delta" | "hit_depth_delta" | "depth_delta" => {
                Some(Self::SsrHitDepthDelta)
            }
            "ssr_hit_uv" | "hit_uv" => Some(Self::SsrHitUv),
            "ssr_hit_color_raw" | "hit_color_raw" => Some(Self::SsrHitColorRaw),
            "ssr_hit_color_blurred" | "hit_color_blurred" => Some(Self::SsrHitColorBlurred),
            "ssr_no_blur" | "no_blur" => Some(Self::SsrNoBlur),
            "ssr_hit_color_uv" | "hit_color_uv" | "hit_uv_color_uv" => {
                Some(Self::SsrHitUvWorldScreenPair)
            }
            "ssr_hit_color_uv_delta" | "hit_color_uv_delta" | "color_uv_delta" => {
                Some(Self::SsrHitUvWorldScreenDelta)
            }
            "ssr_hit_uv_world_screen" | "hit_uv_world_screen" | "world_screen_pair" => {
                Some(Self::SsrHitUvWorldScreenPair)
            }
            "ssr_hit_uv_world_screen_delta" | "hit_uv_world_screen_delta" | "world_screen_delta" => {
                Some(Self::SsrHitUvWorldScreenDelta)
            }
            "ssr_hit_uv_world_screen_split" | "hit_uv_world_screen_split" | "world_screen_split" => {
                Some(Self::SsrHitUvWorldScreenSplit)
            }
            "ssr_final_composite" | "final_composite" | "composite" => {
                Some(Self::SsrFinalComposite)
            }
            "base_color" | "albedo" => Some(Self::BaseColor),
            "trace_strength" | "refl_strength" => Some(Self::TraceStrength),
            _ => None,
        }
    }

    pub fn wire(self) -> &'static str {
        match self {
            Self::Final => "final",
            Self::Normals => "normals",
            Self::Depth => "depth",
            Self::SsrHits => "ssr_hits",
            Self::ReflectionMask => "reflection_mask",
            Self::Cubemap => "cubemap",
            Self::ReflectionColor => "reflection_color",
            Self::Roughness => "roughness",
            Self::Metallic => "metallic",
            Self::ReconWorld => "recon_world",
            Self::ReconNdc => "recon_ndc",
            Self::ReconView => "recon_view",
            Self::ReprojectUv => "reproject_uv",
            Self::SsrViewVector => "ssr_view_vector",
            Self::SsrReflectionVector => "ssr_reflection_vector",
            Self::SsrRaymarchPath => "ssr_raymarch_path",
            Self::SsrHitDepthDelta => "ssr_hit_depth_delta",
            Self::SsrHitUv => "ssr_hit_uv",
            Self::SsrHitColorRaw => "ssr_hit_color_raw",
            Self::SsrHitColorBlurred => "ssr_hit_color_blurred",
            Self::SsrNoBlur => "ssr_no_blur",
            Self::SsrHitUvWorldScreenPair => "ssr_hit_uv_world_screen",
            Self::SsrHitUvWorldScreenDelta => "ssr_hit_uv_world_screen_delta",
            Self::SsrHitUvWorldScreenSplit => "ssr_hit_uv_world_screen_split",
            Self::SsrFinalComposite => "ssr_final_composite",
            Self::BaseColor => "base_color",
            Self::TraceStrength => "trace_strength",
        }
    }

    pub fn shader_index(self) -> u32 {
        match self {
            Self::Final => 0,
            Self::Normals => 1,
            Self::Depth => 2,
            Self::SsrHits => 3,
            Self::ReflectionMask => 4,
            Self::Cubemap => 5,
            Self::ReflectionColor => 6,
            Self::Roughness => 7,
            Self::Metallic => 8,
            Self::ReconWorld => 9,
            Self::ReconNdc => 10,
            Self::ReconView => 11,
            Self::ReprojectUv => 12,
            Self::SsrViewVector => 13,
            Self::SsrReflectionVector => 14,
            Self::SsrRaymarchPath => 15,
            Self::SsrHitDepthDelta => 16,
            Self::SsrHitUv => 17,
            Self::SsrHitColorRaw => 18,
            Self::SsrHitColorBlurred => 19,
            Self::SsrNoBlur => 20,
            Self::BaseColor => 21,
            Self::TraceStrength => 22,
            Self::SsrHitUvWorldScreenPair => 23,
            Self::SsrHitUvWorldScreenDelta => 24,
            Self::SsrHitUvWorldScreenSplit => 25,
            Self::SsrFinalComposite => 26,
        }
    }
}

/// Preset derivado del tier; no se persiste en `.save`.
#[derive(Clone, Copy, Debug)]
pub struct ReflectionSettings {
    pub tier: ReflectionTier,
    pub max_steps: u32,
    pub max_distance_m: f32,
    pub temporal_blend: f32,
    pub max_roughness_to_trace: f32,
    pub rt_enabled: bool,
    pub rt_static_only: bool,
    /// Peso del color RT sobre SSR donde la máscara SSR es baja (0–1).
    pub rt_blend: f32,
}

impl ReflectionTier {
    pub fn from_wire(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "off" | "disabled" | "desactivado" | "none" => Some(Self::Off),
            "low" | "bajo" => Some(Self::Low),
            "medium" | "medio" => Some(Self::Medium),
            "high" | "alto" => Some(Self::High),
            "ultra" => Some(Self::Ultra),
            _ => None,
        }
    }

    pub fn wire(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Ultra => "ultra",
        }
    }

    /// Resolución por cara del cubemap de probes según el tier (px). Off conserva el mínimo
    /// (no captura, pero la textura existe). Low 128, Medium 256, High 512, Ultra 1024.
    pub fn cubemap_face_size(self) -> u32 {
        use crate::reflections::probe_env::PROBE_FACE_SIZE;
        match self {
            Self::Off => PROBE_FACE_SIZE,
            Self::Low => PROBE_FACE_SIZE,
            Self::Medium => 256,
            Self::High => 512,
            Self::Ultra => 1024,
        }
    }
}

impl ReflectionSettings {
    pub fn from_tier(tier: ReflectionTier, rt_available: bool) -> Self {
        match tier {
            ReflectionTier::Off => Self {
                tier,
                max_steps: 0,
                max_distance_m: 0.0,
                temporal_blend: 0.0,
                max_roughness_to_trace: 1.0,
                rt_enabled: false,
                rt_static_only: true,
                rt_blend: 0.85,
            },
            // Presets de reflejos (bloque "Reflections" del preset de gráficos). El difuminado
            // glossy por rugosidad en SSR/RT permite trazar hasta `0.7` sin grano; por encima
            // del límite la reflexión la cubre el cubemap de entorno (siempre presente, así
            // hasta la esfera más rugosa refleja algo tenue). Ultra sube el límite a `0.8`.
            ReflectionTier::Low => Self {
                tier,
                max_steps: 16,
                max_distance_m: 8.0,
                temporal_blend: 0.0,
                max_roughness_to_trace: 0.70,
                rt_enabled: false,
                rt_static_only: true,
                rt_blend: 0.85,
            },
            ReflectionTier::Medium => Self {
                tier,
                max_steps: 32,
                max_distance_m: 20.0,
                temporal_blend: 0.35,
                max_roughness_to_trace: 0.70,
                rt_enabled: false,
                rt_static_only: true,
                rt_blend: 0.85,
            },
            ReflectionTier::High => Self {
                tier,
                max_steps: 48,
                max_distance_m: 40.0,
                temporal_blend: 0.42,
                max_roughness_to_trace: 0.70,
                rt_enabled: rt_available,
                rt_static_only: true,
                rt_blend: 0.85,
            },
            ReflectionTier::Ultra => Self {
                tier,
                max_steps: 64,
                max_distance_m: 80.0,
                temporal_blend: 0.45,
                max_roughness_to_trace: 0.80,
                rt_enabled: rt_available,
                rt_static_only: false,
                rt_blend: 0.85,
            },
        }
    }

    pub fn active(self) -> bool {
        self.tier != ReflectionTier::Off
    }
}

pub const DEFAULT_REFLECTION_TIER: ReflectionTier = ReflectionTier::Off;

#[derive(Clone, Copy, Debug, Default)]
pub struct ReflectionProfilerMs {
    pub ssr_ms: f32,
    pub temporal_ms: f32,
    pub rt_ms: f32,
    pub composite_ms: f32,
}
