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
    use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, IsWindow, PeekMessageW, SetWindowPos, TranslateMessage,
        EVENT_OBJECT_LOCATIONCHANGE, MSG, PM_REMOVE, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
        WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
    };

    use super::TrackerOffset;

    struct TrackerState {
        engine: HWND,
        parent: HWND,
        offset: TrackerOffset,
    }

    static TRACKER_STATE: OnceLock<Arc<TrackerState>> = OnceLock::new();

    fn sync_engine_position(state: &TrackerState) {
        unsafe {
            let mut pt = POINT { x: 0, y: 0 };
            if !ClientToScreen(state.parent, &mut pt).as_bool() {
                return;
            }
            let off_x = state.offset.0.load(Ordering::Relaxed);
            let off_y = state.offset.1.load(Ordering::Relaxed);
            let _ = SetWindowPos(
                state.engine,
                HWND(0isize),
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
        if hwnd != state.parent && hwnd != state.engine {
            return;
        }
        sync_engine_position(state);
    }

    pub fn start_position_tracker(engine_hwnd: isize, parent_hwnd: isize, offset: TrackerOffset) {
        let state = Arc::new(TrackerState {
            engine: HWND(engine_hwnd),
            parent: HWND(parent_hwnd),
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
                        while PeekMessageW(&mut msg, HWND(0isize), 0, 0, PM_REMOVE).into() {
                            let _ = TranslateMessage(&msg);
                            let _ = DispatchMessageW(&msg);
                        }
                        if !IsWindow(state.parent).as_bool() {
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
        .spawn(move || {
            unsafe {
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
            }
        })
        .expect("No se pudo crear el hilo position-tracker");
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
            AllowSetForegroundWindow, SetForegroundWindow, ShowWindow, ASFW_ANY, SW_SHOW,
        };
        unsafe {
            let _ = AllowSetForegroundWindow(ASFW_ANY);
            let hwnd = HWND(parent_id as isize);
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
            x11::xlib::XSetInputFocus(display, parent_id, x11::xlib::RevertToParent, x11::xlib::CurrentTime);
            x11::xlib::XFlush(display);
            x11::xlib::XCloseDisplay(display);
        }
    }
}
