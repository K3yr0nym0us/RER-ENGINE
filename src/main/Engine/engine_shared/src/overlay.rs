//! Configuración de ventana overlay (viewport alineado al editor sin reparent X11).

/// Modo de acoplamiento de la ventana del motor al editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAttachMode {
    /// Ventana decorada independiente (depuración).
    Standalone,
    /// Popup alineado por coordenadas de pantalla; permite Vulkan.
    Overlay,
    /// Ventana X11 hija (legacy): fuerza backend GL.
    #[allow(dead_code)]
    X11Child,
}

impl WindowAttachMode {
    /// `true` solo para ventanas hijas X11 donde Vulkan no puede presentar.
    pub fn force_gl_backend(self) -> bool {
        matches!(self, Self::X11Child)
    }
}

/// Bounds iniciales y handle del padre (Electron) para overlay.
#[derive(Debug, Clone)]
pub struct OverlayConfig {
    pub parent_id: u64,
    pub x:         i32,
    pub y:         i32,
    pub width:     u32,
    pub height:    u32,
    /// Offset físico del viewport dentro del área de contenido de Electron.
    pub rel_x:     i32,
    pub rel_y:     i32,
}

/// Parsea `--overlay` o `--embed` (alias legacy).
/// Formato: `<flag> <parent_id> <x> <y> <width> <height> [rel_x rel_y]`
pub fn parse_overlay_config() -> Option<OverlayConfig> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 7 {
        return None;
    }
    let flag = args[1].as_str();
    if flag != "--overlay" && flag != "--embed" {
        return None;
    }
    Some(OverlayConfig {
        parent_id: args[2].parse().ok()?,
        x:         args[3].parse().ok()?,
        y:         args[4].parse().ok()?,
        width:     args[5].parse().ok()?,
        height:    args[6].parse().ok()?,
        rel_x:     args.get(7).and_then(|a| a.parse().ok()).unwrap_or(0),
        rel_y:     args.get(8).and_then(|a| a.parse().ok()).unwrap_or(0),
    })
}

/// Orden de backends wgpu a probar (con fallback a GL si falla la surface).
pub fn wgpu_backend_attempts(mode: WindowAttachMode) -> Vec<wgpu::Backends> {
    if mode.force_gl_backend() {
        vec![wgpu::Backends::GL]
    } else {
        vec![wgpu::Backends::all(), wgpu::Backends::GL]
    }
}
