#[cfg(target_os = "linux")]
pub(crate) fn query_ctrl_held_os() -> bool {
    unsafe {
        let display = x11::xlib::XOpenDisplay(std::ptr::null());
        if display.is_null() {
            return false;
        }
        let mut keys = [0u8; 32];
        x11::xlib::XQueryKeymap(display, keys.as_mut_ptr() as *mut i8);
        x11::xlib::XCloseDisplay(display);
        let lctrl = (keys[37 / 8] >> (37 % 8)) & 1;
        let rctrl = (keys[105 / 8] >> (105 % 8)) & 1;
        lctrl != 0 || rctrl != 0
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn query_ctrl_held_os() -> bool {
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_LCONTROL, VK_RCONTROL,
        };
        let left = (GetAsyncKeyState(VK_LCONTROL.0 as i32) as u16 & 0x8000) != 0;
        let right = (GetAsyncKeyState(VK_RCONTROL.0 as i32) as u16 & 0x8000) != 0;
        left || right
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub(crate) fn query_ctrl_held_os() -> bool {
    false
}

#[cfg(target_os = "linux")]
pub(crate) fn query_shift_held_os() -> bool {
    unsafe {
        let display = x11::xlib::XOpenDisplay(std::ptr::null());
        if display.is_null() {
            return false;
        }
        let mut keys = [0u8; 32];
        x11::xlib::XQueryKeymap(display, keys.as_mut_ptr() as *mut i8);
        x11::xlib::XCloseDisplay(display);
        let lshift = (keys[50 / 8] >> (50 % 8)) & 1;
        let rshift = (keys[62 / 8] >> (62 % 8)) & 1;
        lshift != 0 || rshift != 0
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn query_shift_held_os() -> bool {
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_LSHIFT, VK_RSHIFT,
        };
        let left = (GetAsyncKeyState(VK_LSHIFT.0 as i32) as u16 & 0x8000) != 0;
        let right = (GetAsyncKeyState(VK_RSHIFT.0 as i32) as u16 & 0x8000) != 0;
        left || right
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub(crate) fn query_shift_held_os() -> bool {
    false
}

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

