//! Geometría de la ventana del motor dentro del editor egui.
//!
//! El motor se lanza en modo overlay con el HWND de esta ventana como padre.
//! El rectángulo central (entre sidebars, bajo la barra superior y sobre la
//! consola) se calcula con las mismas constantes que `ui::frame`.

use winit::window::Window;

use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineViewport {
    pub parent_hwnd: u64,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub rel_x: i32,
    pub rel_y: i32,
}

/// Rectángulo central en coordenadas físicas de pantalla.
pub fn central_viewport(window: &Window) -> Option<EngineViewport> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::{HWND, POINT};
        use windows::Win32::Graphics::Gdi::ClientToScreen;

        let parent_hwnd = window_hwnd(window)?;
        let scale = window.scale_factor().max(0.1);
        let inner = window.inner_size();
        if inner.width == 0 || inner.height == 0 {
            return None;
        }

        let client_origin = unsafe {
            let mut point = POINT { x: 0, y: 0 };
            if !ClientToScreen(HWND(parent_hwnd as *mut _), &mut point).as_bool() {
                return None;
            }
            point
        };

        let sidebar = theme::SIDEBAR_WIDTH as f64 * scale;
        let top = theme::TOP_BAR_HEIGHT as f64 * scale;
        let console = (inner.height as f64 * 0.25).max(120.0 * scale);
        let width = (inner.width as f64 - sidebar * 2.0).round().max(1.0) as u32;
        let height = (inner.height as f64 - top - console).round().max(1.0) as u32;
        let rel_x = sidebar.round() as i32;
        let rel_y = top.round() as i32;

        Some(EngineViewport {
            parent_hwnd: parent_hwnd as u64,
            x: client_origin.x + rel_x,
            y: client_origin.y + rel_y,
            width,
            height,
            rel_x,
            rel_y,
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        None
    }
}

impl EngineViewport {
    pub fn engine_bounds(self) -> crate::cli::EngineBounds {
        crate::cli::EngineBounds {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
            rel_x: self.rel_x,
            rel_y: self.rel_y,
        }
    }

    /// Comando para reposicionar el overlay tras un resize del editor.
    pub fn set_bounds_command(self) -> serde_json::Value {
        serde_json::json!({
            "cmd": "set_bounds",
            "x": self.x,
            "y": self.y,
            "width": self.width,
            "height": self.height,
            "offset_x": self.rel_x,
            "offset_y": self.rel_y,
        })
    }
}

#[cfg(target_os = "windows")]
fn window_hwnd(window: &Window) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}
