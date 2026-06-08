/// Estado de una tecla virtual en Windows (async + keyboard state).
#[derive(Clone, Copy, Debug, Default)]
pub struct VkOsProbe {
    pub async_down: bool,
    /// Bit 0 de GetAsyncKeyState: pulsada desde la última consulta de esa tecla.
    pub async_toggle: bool,
    pub kbd_down: bool,
}

#[cfg(target_os = "windows")]
pub(crate) fn probe_vk_os(vk: u8) -> VkOsProbe {
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, GetKeyboardState};
        let async_val = GetAsyncKeyState(i32::from(vk)) as u16;
        let mut kbd = [0u8; 256];
        let kbd_down = GetKeyboardState(&mut kbd)
            .is_ok()
            .then(|| kbd[vk as usize] & 0x80 != 0)
            .unwrap_or(false);
        VkOsProbe {
            async_down: async_val & 0x8000 != 0,
            async_toggle: async_val & 0x0001 != 0,
            kbd_down,
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn probe_vk_os(_vk: u8) -> VkOsProbe {
    VkOsProbe::default()
}

/// Q física (rotación izquierda del ghost muro/trigger). Lectura global vía OS.
#[cfg(target_os = "windows")]
pub(crate) fn query_key_q_held_os() -> bool {
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
        (GetAsyncKeyState(0x51) as u16 & 0x8000) != 0
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn query_key_q_held_os() -> bool {
    query_x11_keycode_held(24)
}

/// E física (rotación derecha del ghost muro/trigger). Lectura global vía OS.
#[cfg(target_os = "windows")]
pub(crate) fn query_key_e_held_os() -> bool {
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
        (GetAsyncKeyState(0x45) as u16 & 0x8000) != 0
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn query_key_e_held_os() -> bool {
    query_x11_keycode_held(26)
}

#[cfg(target_os = "linux")]
fn query_x11_keycode_held(keycode: u8) -> bool {
    unsafe {
        let display = x11::xlib::XOpenDisplay(std::ptr::null());
        if display.is_null() {
            return false;
        }
        let mut keys = [0u8; 32];
        x11::xlib::XQueryKeymap(display, keys.as_mut_ptr() as *mut i8);
        x11::xlib::XCloseDisplay(display);
        ((keys[(keycode as usize) / 8] >> ((keycode as usize) % 8)) & 1) != 0
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub(crate) fn query_key_q_held_os() -> bool {
    false
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub(crate) fn query_key_e_held_os() -> bool {
    false
}

