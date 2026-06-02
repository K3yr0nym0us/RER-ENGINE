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
