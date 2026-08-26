//! Utilidades de plataforma Windows para overlay (position-tracker, Z-order).

pub type TrackerOffset =
    std::sync::Arc<(std::sync::atomic::AtomicI32, std::sync::atomic::AtomicI32)>;

// ---------------------------------------------------------------------------
// Windows: WinEventHook + respaldo de polling (sin IPC)
// ---------------------------------------------------------------------------
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

pub use win32_tracker::start_position_tracker;

/// Ventana owned del editor: sin botón en la barra de tareas ni en Alt+Tab.
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

/// Devuelve el foco del teclado a la ventana Electron (padre del overlay).
pub fn focus_overlay_parent_window(parent_id: u64) {
    if parent_id == 0 {
        return;
    }
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
