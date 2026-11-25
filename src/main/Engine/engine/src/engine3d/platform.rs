/// Estado de una tecla virtual consultado mediante el sistema operativo.
#[derive(Clone, Copy, Debug, Default)]
pub struct VkOsProbe {
    pub async_down: bool,
    /// Bit de conmutación reportado por la última consulta de esa tecla.
    pub async_toggle: bool,
    pub kbd_down: bool,
}

#[cfg(target_os = "windows")]
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

#[cfg(not(target_os = "windows"))]
pub(crate) fn query_key_q_held_os() -> bool {
    false
}

/// E física (rotación derecha del ghost muro/trigger). Lectura global vía OS.
#[cfg(target_os = "windows")]
pub(crate) fn query_key_e_held_os() -> bool {
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
        (GetAsyncKeyState(0x45) as u16 & 0x8000) != 0
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn query_key_e_held_os() -> bool {
    false
}
