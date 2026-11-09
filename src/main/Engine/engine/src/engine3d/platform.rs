/// Estado de una tecla virtual en Windows (async + keyboard state).
#[derive(Clone, Copy, Debug, Default)]
pub struct VkOsProbe {
    pub async_down: bool,
    /// Bit 0 de GetAsyncKeyState: pulsada desde la última consulta de esa tecla.
    pub async_toggle: bool,
    pub kbd_down: bool,
}

pub(crate) fn probe_vk_os(vk: u8) -> VkOsProbe {
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, GetKeyboardState};
        let async_val = GetAsyncKeyState(i32::from(vk)) as u16;
        let mut kbd = [0u8; 256];
        let kbd_down = if GetKeyboardState(&mut kbd).is_ok() {
            kbd[vk as usize] & 0x80 != 0
        } else {
            false
        };
        VkOsProbe {
            async_down: async_val & 0x8000 != 0,
            async_toggle: async_val & 0x0001 != 0,
            kbd_down,
        }
    }
}

/// Q física (rotación izquierda del ghost muro/trigger). Lectura global vía OS.
pub(crate) fn query_key_q_held_os() -> bool {
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
        (GetAsyncKeyState(0x51) as u16 & 0x8000) != 0
    }
}

/// E física (rotación derecha del ghost muro/trigger). Lectura global vía OS.
pub(crate) fn query_key_e_held_os() -> bool {
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
        (GetAsyncKeyState(0x45) as u16 & 0x8000) != 0
    }
}
