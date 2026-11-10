//! Utilidades de plataforma Windows compartidas por los motores.

/// Icono de la aplicación (mismo PNG que usa Electron) incrustado en el binario.
///
/// Incrustarlo evita depender de rutas relativas distintas en desarrollo y en
/// el paquete de Electron.
const APP_ICON_PNG: &[u8] = include_bytes!("../../../../../src/resources/RER-ENGINE-LOGO.png");

/// Icono listo para `WindowAttributes::with_window_icon`.
pub fn app_window_icon() -> Option<winit::window::Icon> {
    let image = image::load_from_memory(APP_ICON_PNG).ok()?;
    let resized = image.resize_exact(256, 256, image::imageops::FilterType::Lanczos3);
    let rgba = resized.to_rgba8();
    let (width, height) = rgba.dimensions();
    winit::window::Icon::from_rgba(rgba.into_raw(), width, height).ok()
}

pub fn query_ctrl_held_os() -> bool {
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_LCONTROL, VK_RCONTROL,
        };
        let left = (GetAsyncKeyState(VK_LCONTROL.0 as i32) as u16 & 0x8000) != 0;
        let right = (GetAsyncKeyState(VK_RCONTROL.0 as i32) as u16 & 0x8000) != 0;
        left || right
    }
}

pub fn query_shift_held_os() -> bool {
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LSHIFT, VK_RSHIFT};
        let left = (GetAsyncKeyState(VK_LSHIFT.0 as i32) as u16 & 0x8000) != 0;
        let right = (GetAsyncKeyState(VK_RSHIFT.0 as i32) as u16 & 0x8000) != 0;
        left || right
    }
}

/// Restringe el cursor al rectángulo físico indicado (coordenadas de pantalla).
#[cfg(target_os = "windows")]
pub fn clip_cursor_to_rect(x: i32, y: i32, width: u32, height: u32) {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::ClipCursor;
    let rect = RECT {
        left: x,
        top: y,
        right: x.saturating_add(width as i32),
        bottom: y.saturating_add(height as i32),
    };
    unsafe {
        let _ = ClipCursor(Some(&rect as *const _));
    }
}

#[cfg(not(target_os = "windows"))]
pub fn clip_cursor_to_rect(_x: i32, _y: i32, _width: u32, _height: u32) {}

/// Libera la restricción de cursor impuesta con [`clip_cursor_to_rect`].
pub fn release_cursor_clip() {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::ClipCursor;
        let _ = ClipCursor(None);
    }
}
