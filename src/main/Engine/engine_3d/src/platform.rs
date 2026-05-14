#[cfg(target_os = "windows")]
pub type TrackerOffset =
    std::sync::Arc<(std::sync::atomic::AtomicI32, std::sync::atomic::AtomicI32)>;

#[cfg(target_os = "windows")]
pub(crate) fn start_position_tracker(engine_hwnd: isize, parent_hwnd: isize, offset: TrackerOffset) {
    use std::sync::atomic::Ordering;

    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
    };

    let engine_hwnd = HWND(engine_hwnd);
    let parent_hwnd = HWND(parent_hwnd);

    std::thread::Builder::new()
        .name("position-tracker".into())
        .spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(8));
            unsafe {
                let mut pt = POINT { x: 0, y: 0 };
                if !ClientToScreen(parent_hwnd, &mut pt).as_bool() {
                    break;
                }
                let off_x = offset.0.load(Ordering::Relaxed);
                let off_y = offset.1.load(Ordering::Relaxed);
                let desired_x = pt.x + off_x;
                let desired_y = pt.y + off_y;

                let mut engine = RECT::default();
                if GetWindowRect(engine_hwnd, &mut engine).is_ok() {
                    if engine.left != desired_x || engine.top != desired_y {
                        let _ = SetWindowPos(
                            engine_hwnd,
                            HWND(0isize),
                            desired_x,
                            desired_y,
                            0,
                            0,
                            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                }
            }
        })
        .expect("No se pudo crear el hilo position-tracker");
}

#[cfg(target_os = "linux")]
pub(crate) fn query_ctrl_held_x11() -> bool {
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
pub(crate) fn query_ctrl_held_x11() -> bool {
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
pub(crate) fn query_ctrl_held_x11() -> bool {
    false
}
