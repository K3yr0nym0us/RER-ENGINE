//! Utilidades de plataforma para overlay (position-tracker, Z-order).

#[cfg(any(target_os = "windows", target_os = "linux"))]
pub type TrackerOffset =
    std::sync::Arc<(std::sync::atomic::AtomicI32, std::sync::atomic::AtomicI32)>;

// ---------------------------------------------------------------------------
// Windows: WinEventHook + respaldo de polling (sin IPC)
// ---------------------------------------------------------------------------
#[cfg(target_os = "windows")]
mod win32_tracker {
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, OnceLock};

    use windows::Win32::Foundation::{HWND, POINT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, EVENT_OBJECT_LOCATIONCHANGE, HWND_TOP, IsWindow, MSG, PM_REMOVE,
        PeekMessageW, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos, TranslateMessage,
        WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
    };

    use super::TrackerOffset;

    #[inline]
    fn hwnd_from_isize(v: isize) -> HWND {
        HWND(v as *mut _)
    }

    struct TrackerState {
        engine_hwnd: isize,
        parent_hwnd: isize,
        offset: TrackerOffset,
    }

    static TRACKER_STATE: OnceLock<Arc<TrackerState>> = OnceLock::new();

    fn sync_engine_position(state: &TrackerState) {
        unsafe {
            let engine = hwnd_from_isize(state.engine_hwnd);
            let parent = hwnd_from_isize(state.parent_hwnd);
            let mut pt = POINT { x: 0, y: 0 };
            if !ClientToScreen(parent, &mut pt).as_bool() {
                return;
            }
            let off_x = state.offset.0.load(Ordering::Relaxed);
            let off_y = state.offset.1.load(Ordering::Relaxed);
            let _ = SetWindowPos(
                engine,
                Some(HWND_TOP),
                pt.x + off_x,
                pt.y + off_y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    unsafe extern "system" fn win_event_proc(
        _hook: HWINEVENTHOOK,
        event: u32,
        hwnd: HWND,
        _id_object: i32,
        _id_child: i32,
        _id_thread: u32,
        _dwms_event_time: u32,
    ) {
        if event != EVENT_OBJECT_LOCATIONCHANGE {
            return;
        }
        let Some(state) = TRACKER_STATE.get() else {
            return;
        };
        let parent = hwnd_from_isize(state.parent_hwnd);
        let engine = hwnd_from_isize(state.engine_hwnd);
        if hwnd != parent && hwnd != engine {
            return;
        }
        sync_engine_position(state);
    }

    pub fn start_position_tracker(engine_hwnd: isize, parent_hwnd: isize, offset: TrackerOffset) {
        let state = Arc::new(TrackerState {
            engine_hwnd,
            parent_hwnd,
            offset,
        });
        let _ = TRACKER_STATE.set(Arc::clone(&state));

        std::thread::Builder::new()
            .name("position-tracker".into())
            .spawn(move || {
                let hook = unsafe {
                    SetWinEventHook(
                        EVENT_OBJECT_LOCATIONCHANGE,
                        EVENT_OBJECT_LOCATIONCHANGE,
                        None,
                        Some(win_event_proc),
                        0,
                        0,
                        WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                    )
                };

                sync_engine_position(&state);

                loop {
                    unsafe {
                        let mut msg = MSG::default();
                        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).into() {
                            let _ = TranslateMessage(&msg);
                            let _ = DispatchMessageW(&msg);
                        }
                        if !IsWindow(Some(hwnd_from_isize(state.parent_hwnd))).as_bool() {
                            if !hook.is_invalid() {
                                let _ = UnhookWinEvent(hook);
                            }
                            break;
                        }
                    }
                    sync_engine_position(&state);
                    // Respaldo mínimo por si el hook no dispara en algún frame del arrastre.
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            })
            .expect("No se pudo crear el hilo position-tracker");
    }
}

#[cfg(target_os = "windows")]
pub use win32_tracker::start_position_tracker;

/// Ventana owned del editor: sin botón en la barra de tareas ni en Alt+Tab.
#[cfg(target_os = "windows")]
pub fn setup_overlay_win32(engine_hwnd: isize, parent_hwnd: isize, offset: TrackerOffset) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWLP_HWNDPARENT, GetWindowLongPtrW, HWND_TOP, SWP_FRAMECHANGED, SWP_NOMOVE,
        SWP_NOSIZE, SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos, WS_EX_APPWINDOW,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    unsafe {
        let motor = HWND(engine_hwnd as *mut _);
        let parent = HWND(parent_hwnd as *mut _);
        SetWindowLongPtrW(motor, GWLP_HWNDPARENT, parent.0 as usize as isize);
        let ex = GetWindowLongPtrW(motor, GWL_EXSTYLE);
        let new_ex = (ex & !(WS_EX_APPWINDOW.0 as isize))
            | WS_EX_NOACTIVATE.0 as isize
            | WS_EX_TOOLWINDOW.0 as isize;
        SetWindowLongPtrW(motor, GWL_EXSTYLE, new_ex);
        let _ = SetWindowPos(
            motor,
            Some(HWND_TOP),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
        );
    }
    start_position_tracker(engine_hwnd, parent_hwnd, offset);
}

// ---------------------------------------------------------------------------
// Linux X11 position tracker + transient (Z-order)
// ---------------------------------------------------------------------------
#[cfg(target_os = "linux")]
pub fn setup_overlay_x11(engine_xid: u32, parent_xid: u32) {
    if parent_xid == 0 {
        return;
    }
    unsafe {
        let display = x11::xlib::XOpenDisplay(std::ptr::null());
        if display.is_null() {
            return;
        }
        x11::xlib::XSetTransientForHint(display, engine_xid, parent_xid);
        x11::xlib::XFlush(display);
        x11::xlib::XCloseDisplay(display);
    }
}

#[cfg(target_os = "linux")]
pub fn start_position_tracker(engine_xid: u32, parent_xid: u32, offset: TrackerOffset) {
    if parent_xid == 0 {
        return;
    }

    std::thread::Builder::new()
        .name("position-tracker".into())
        .spawn(move || unsafe {
            let display = x11::xlib::XOpenDisplay(std::ptr::null());
            if display.is_null() {
                return;
            }
            let root = x11::xlib::XDefaultRootWindow(display);
            x11::xlib::XSelectInput(display, parent_xid, x11::xlib::StructureNotifyMask);

            let mut sync = || {
                let mut parent_root_x: i32 = 0;
                let mut parent_root_y: i32 = 0;
                let mut child_return: x11::xlib::Window = 0;
                if x11::xlib::XTranslateCoordinates(
                    display,
                    parent_xid,
                    root,
                    0,
                    0,
                    &mut parent_root_x,
                    &mut parent_root_y,
                    &mut child_return,
                ) == 0
                {
                    return false;
                }
                let off_x = offset.0.load(std::sync::atomic::Ordering::Relaxed);
                let off_y = offset.1.load(std::sync::atomic::Ordering::Relaxed);
                let desired_x = parent_root_x + off_x;
                let desired_y = parent_root_y + off_y;

                let mut cur_x: i32 = 0;
                let mut cur_y: i32 = 0;
                if x11::xlib::XTranslateCoordinates(
                    display,
                    engine_xid,
                    root,
                    0,
                    0,
                    &mut cur_x,
                    &mut cur_y,
                    &mut child_return,
                ) == 0
                {
                    return false;
                }
                if cur_x != desired_x || cur_y != desired_y {
                    x11::xlib::XMoveWindow(display, engine_xid, desired_x, desired_y);
                    x11::xlib::XFlush(display);
                }
                true
            };

            sync();

            loop {
                while x11::xlib::XPending(display) > 0 {
                    let mut event: x11::xlib::XEvent = std::mem::zeroed();
                    x11::xlib::XNextEvent(display, &mut event);
                    if event.type_ == x11::xlib::ConfigureNotify {
                        sync();
                    }
                }
                if !sync() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(4));
            }
            x11::xlib::XCloseDisplay(display);
        })
        .expect("No se pudo crear el hilo position-tracker");
}

#[cfg(target_os = "linux")]
pub fn query_ctrl_held_os() -> bool {
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

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn query_ctrl_held_os() -> bool {
    false
}

#[cfg(target_os = "linux")]
pub fn query_shift_held_os() -> bool {
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
pub fn query_shift_held_os() -> bool {
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LSHIFT, VK_RSHIFT};
        let left = (GetAsyncKeyState(VK_LSHIFT.0 as i32) as u16 & 0x8000) != 0;
        let right = (GetAsyncKeyState(VK_RSHIFT.0 as i32) as u16 & 0x8000) != 0;
        left || right
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn query_shift_held_os() -> bool {
    false
}

/// Devuelve el foco del teclado a la ventana Electron (padre del overlay).
pub fn focus_overlay_parent_window(parent_id: u64) {
    if parent_id == 0 {
        return;
    }
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            ASFW_ANY, AllowSetForegroundWindow, SW_SHOW, SetForegroundWindow, ShowWindow,
        };
        unsafe {
            let _ = AllowSetForegroundWindow(ASFW_ANY);
            let hwnd = HWND(parent_id as *mut _);
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
    }
    #[cfg(target_os = "linux")]
    {
        unsafe {
            let display = x11::xlib::XOpenDisplay(std::ptr::null());
            if display.is_null() {
                return;
            }
            x11::xlib::XRaiseWindow(display, parent_id);
            x11::xlib::XSetInputFocus(
                display,
                parent_id,
                x11::xlib::RevertToParent,
                x11::xlib::CurrentTime,
            );
            x11::xlib::XFlush(display);
            x11::xlib::XCloseDisplay(display);
        }
    }
}
