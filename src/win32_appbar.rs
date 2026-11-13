use crate::task_engine::DockSide;
use gpui::{Bounds, Pixels, point, px, size};

/// Custom Win32 callback message ID used for `SHAppBarMessage` notifications.
/// Matches the original TaskConveyor implementation (`1124`).
pub const MY_APPBAR_MSG_ID: u32 = 1124;

/// `ABE_LEFT` constant from `shellapi.h`.
pub const ABE_LEFT_EDGE: u32 = 0;

/// `ABE_RIGHT` constant from `shellapi.h`.
pub const ABE_RIGHT_EDGE: u32 = 2;

/// `ABN_POSCHANGED` notification code sent via `wParam` when taskbar or monitor geometry changes.
pub const ABN_POSCHANGED_NOTIF: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PhysicalRect {
    pub fn width(self) -> i32 {
        self.right - self.left
    }

    pub fn height(self) -> i32 {
        self.bottom - self.top
    }
}

pub fn edge_for_dock_side(side: DockSide) -> u32 {
    match side {
        DockSide::Left => ABE_LEFT_EDGE,
        DockSide::Right => ABE_RIGHT_EDGE,
    }
}

pub fn dip_bounds_to_physical_rect(bounds: Bounds<Pixels>, scale_factor: f32) -> PhysicalRect {
    let scale = if scale_factor > 0.0 { scale_factor } else { 1.0 };
    let left = (f32::from(bounds.origin.x) * scale).round() as i32;
    let top = (f32::from(bounds.origin.y) * scale).round() as i32;
    let width = (f32::from(bounds.size.width) * scale).round() as i32;
    let height = (f32::from(bounds.size.height) * scale).round() as i32;
    PhysicalRect {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

pub fn physical_rect_to_dip_bounds(rect: PhysicalRect, scale_factor: f32) -> Bounds<Pixels> {
    let scale = if scale_factor > 0.0 { scale_factor } else { 1.0 };
    Bounds::new(
        point(
            px(rect.left as f32 / scale),
            px(rect.top as f32 / scale),
        ),
        size(
            px(rect.width().max(0) as f32 / scale),
            px(rect.height().max(0) as f32 / scale),
        ),
    )
}

pub fn adjust_appbar_edge_width(
    mut rect: PhysicalRect,
    side: DockSide,
    physical_width: i32,
) -> PhysicalRect {
    match side {
        DockSide::Left => {
            rect.right = rect.left + physical_width;
        }
        DockSide::Right => {
            rect.left = rect.right - physical_width;
        }
    }
    rect
}

/// Insets representing the invisible resizing / drop shadow borders that Windows 10/11
/// applies to `WS_THICKFRAME` windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameInsets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// Expands a target physical client/content rect outward by the frame insets so that
/// `SetWindowPos` places the window's visible client area exactly at `rect`.
pub fn compensate_frame_insets(rect: PhysicalRect, insets: FrameInsets) -> PhysicalRect {
    PhysicalRect {
        left: rect.left - insets.left,
        top: rect.top - insets.top,
        right: rect.right + insets.right,
        bottom: rect.bottom + insets.bottom,
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use super::*;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::mem::{size_of, zeroed};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
    use windows_sys::Win32::Foundation::{
        BOOL, HMODULE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
    };
    use windows_sys::Win32::UI::Shell::{
        ABE_LEFT, ABE_RIGHT, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS, ABN_POSCHANGED,
        APPBARDATA, DefSubclassProc, RemoveWindowSubclass, SHAppBarMessage, SetWindowSubclass,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClientRect, GetWindowRect, HWND_NOTOPMOST, HWND_TOPMOST, IsIconic, SW_RESTORE,
        SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetForegroundWindow, SetWindowPos, ShowWindow,
    };

    type DwmGetWindowAttributeFn = unsafe extern "system" fn(
        HWND,
        u32,
        *mut std::ffi::c_void,
        u32,
    ) -> i32;

    const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;

    unsafe extern "system" {
        fn GetModuleHandleA(lp_module_name: *const u8) -> HMODULE;
        fn LoadLibraryA(lp_lib_file_name: *const u8) -> HMODULE;
        fn GetProcAddress(
            h_module: HMODULE,
            lp_proc_name: *const u8,
        ) -> Option<unsafe extern "system" fn() -> isize>;
        fn ClientToScreen(hwnd: HWND, lp_point: *mut POINT) -> BOOL;
    }

    pub unsafe fn get_window_frame_insets(hwnd: HWND, scale_factor: f32) -> FrameInsets {
        if hwnd.is_null() {
            return FrameInsets::default();
        }

        let mut win_rect: RECT = zeroed();
        if GetWindowRect(hwnd, &mut win_rect) == 0 {
            let default_border =
                (7.0 * if scale_factor > 0.0 { scale_factor } else { 1.0 }).round() as i32;
            return FrameInsets {
                left: default_border,
                top: 0,
                right: default_border,
                bottom: default_border,
            };
        }

        // 1. Try DwmGetWindowAttribute with DWMWA_EXTENDED_FRAME_BOUNDS.
        let mut h_dwm = GetModuleHandleA(b"dwmapi.dll\0".as_ptr());
        if h_dwm.is_null() {
            h_dwm = LoadLibraryA(b"dwmapi.dll\0".as_ptr());
        }
        if !h_dwm.is_null() {
            if let Some(proc) = GetProcAddress(h_dwm, b"DwmGetWindowAttribute\0".as_ptr()) {
                let dwm_get_attr: DwmGetWindowAttributeFn = std::mem::transmute(proc);
                let mut frame_bounds: RECT = zeroed();
                let hr = dwm_get_attr(
                    hwnd,
                    DWMWA_EXTENDED_FRAME_BOUNDS,
                    &mut frame_bounds as *mut _ as *mut std::ffi::c_void,
                    size_of::<RECT>() as u32,
                );
                if hr == 0
                    && frame_bounds.right > frame_bounds.left
                    && frame_bounds.bottom > frame_bounds.top
                {
                    let left = (frame_bounds.left - win_rect.left).max(0);
                    let top = (frame_bounds.top - win_rect.top).max(0);
                    let right = (win_rect.right - frame_bounds.right).max(0);
                    let bottom = (win_rect.bottom - frame_bounds.bottom).max(0);
                    if left > 0 || right > 0 || bottom > 0 || top > 0 {
                        return FrameInsets {
                            left,
                            top,
                            right,
                            bottom,
                        };
                    }
                }
            }
        }

        // 2. Try ClientToScreen combined with GetClientRect.
        let mut client_rect: RECT = zeroed();
        if GetClientRect(hwnd, &mut client_rect) != 0 {
            let win_width = win_rect.right - win_rect.left;
            let win_height = win_rect.bottom - win_rect.top;
            let client_width = client_rect.right - client_rect.left;
            let client_height = client_rect.bottom - client_rect.top;

            let mut client_origin = POINT { x: 0, y: 0 };
            if ClientToScreen(hwnd, &mut client_origin) != 0 {
                let left = (client_origin.x - win_rect.left).max(0);
                let top = (client_origin.y - win_rect.top).max(0);
                let right = (win_width - client_width - left).max(0);
                let bottom = (win_height - client_height - top).max(0);
                if left > 0 || right > 0 || bottom > 0 || top > 0 {
                    return FrameInsets {
                        left,
                        top,
                        right,
                        bottom,
                    };
                }
            }

            let width_diff = (win_width - client_width).max(0);
            let height_diff = (win_height - client_height).max(0);
            if width_diff > 0 || height_diff > 0 {
                let left = width_diff / 2;
                let right = width_diff - left;
                return FrameInsets {
                    left,
                    top: 0,
                    right,
                    bottom: height_diff,
                };
            }
        }

        // 3. Fallback to standard 7px scaled for DPI.
        let default_border =
            (7.0 * if scale_factor > 0.0 { scale_factor } else { 1.0 }).round() as i32;
        FrameInsets {
            left: default_border,
            top: 0,
            right: default_border,
            bottom: default_border,
        }
    }

    const APPBAR_SUBCLASS_ID: usize = 0x5443_4142; // "TCAB"

    static ACTIVE_APPBAR_HWND: AtomicIsize = AtomicIsize::new(0);
    static ACTIVE_DOCK_EDGE: AtomicU32 = AtomicU32::new(ABE_LEFT);
    static ACTIVE_PHYSICAL_WIDTH: AtomicIsize = AtomicIsize::new(400);
    static IS_REENTERING_APPBAR: AtomicBool = AtomicBool::new(false);
    static POSCHANGED_PENDING: AtomicBool = AtomicBool::new(false);
    static COMMITTED_APPBAR_RECT: Mutex<Option<PhysicalRect>> = Mutex::new(None);

    pub fn extract_hwnd(window: &gpui::Window) -> Option<HWND> {
        let handle = HasWindowHandle::window_handle(window).ok()?;
        match handle.as_raw() {
            RawWindowHandle::Win32(win32) => Some(win32.hwnd.get() as HWND),
            _ => None,
        }
    }

    unsafe extern "system" fn appbar_subclass_proc(
        hwnd: HWND,
        u_msg: u32,
        w_param: WPARAM,
        l_param: LPARAM,
        _u_id_subclass: usize,
        _dw_ref_data: usize,
    ) -> LRESULT {
        if u_msg == MY_APPBAR_MSG_ID && w_param == ABN_POSCHANGED as usize {
            // Only flag pending if this notification was not triggered synchronously
            // by our own ABM_SETPOS call.
            if !IS_REENTERING_APPBAR.load(Ordering::SeqCst) {
                POSCHANGED_PENDING.store(true, Ordering::SeqCst);
            }
        }
        unsafe { DefSubclassProc(hwnd, u_msg, w_param, l_param) }
    }

    pub fn take_poschanged_pending() -> bool {
        POSCHANGED_PENDING.swap(false, Ordering::SeqCst)
    }

    pub fn dock_appbar(
        window: &gpui::Window,
        side: DockSide,
        target_dip_bounds: Bounds<Pixels>,
    ) -> Option<Bounds<Pixels>> {
        if IS_REENTERING_APPBAR.swap(true, Ordering::SeqCst) {
            return None;
        }
        let result = dock_appbar_inner(window, side, target_dip_bounds);
        IS_REENTERING_APPBAR.store(false, Ordering::SeqCst);
        result
    }

    fn dock_appbar_inner(
        window: &gpui::Window,
        side: DockSide,
        target_dip_bounds: Bounds<Pixels>,
    ) -> Option<Bounds<Pixels>> {
        let hwnd = extract_hwnd(window)?;
        let scale = window.scale_factor();
        let initial_rect = dip_bounds_to_physical_rect(target_dip_bounds, scale);
        let physical_width = initial_rect.width().max(1);
        let u_edge = match side {
            DockSide::Left => ABE_LEFT,
            DockSide::Right => ABE_RIGHT,
        };

        ACTIVE_DOCK_EDGE.store(u_edge, Ordering::SeqCst);
        ACTIVE_PHYSICAL_WIDTH.store(physical_width as isize, Ordering::SeqCst);

        unsafe {
            let prev_hwnd = ACTIVE_APPBAR_HWND.load(Ordering::SeqCst);
            if prev_hwnd != hwnd as isize {
                if prev_hwnd != 0 {
                    remove_appbar_by_hwnd(prev_hwnd as HWND);
                }
                if let Ok(mut committed) = COMMITTED_APPBAR_RECT.lock() {
                    *committed = None;
                }
                let mut new_abd: APPBARDATA = zeroed();
                new_abd.cbSize = size_of::<APPBARDATA>() as u32;
                new_abd.hWnd = hwnd;
                new_abd.uCallbackMessage = MY_APPBAR_MSG_ID;
                SHAppBarMessage(ABM_NEW, &mut new_abd);
                let _ = SetWindowSubclass(
                    hwnd,
                    Some(appbar_subclass_proc),
                    APPBAR_SUBCLASS_ID,
                    0,
                );
                ACTIVE_APPBAR_HWND.store(hwnd as isize, Ordering::SeqCst);
            }

            let mut abd: APPBARDATA = zeroed();
            abd.cbSize = size_of::<APPBARDATA>() as u32;
            abd.hWnd = hwnd;
            abd.uEdge = u_edge;
            abd.rc = RECT {
                left: initial_rect.left,
                top: initial_rect.top,
                right: initial_rect.right,
                bottom: initial_rect.bottom,
            };

            SHAppBarMessage(ABM_QUERYPOS, &mut abd);

            let queried = PhysicalRect {
                left: abd.rc.left,
                top: abd.rc.top,
                right: abd.rc.right,
                bottom: abd.rc.bottom,
            };
            let adjusted = adjust_appbar_edge_width(queried, side, physical_width);

            let insets = get_window_frame_insets(hwnd, scale);
            let window_rect = compensate_frame_insets(adjusted, insets);

            // If the query position matches what was already committed for this window,
            // we do not call ABM_SETPOS again. Calling ABM_SETPOS needlessly broadcasts
            // ABN_POSCHANGED to all registered appbars (including ourselves).
            if let Ok(committed) = COMMITTED_APPBAR_RECT.lock() {
                if *committed == Some(adjusted) && prev_hwnd == hwnd as isize {
                    SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
                        window_rect.left,
                        window_rect.top,
                        window_rect.width(),
                        window_rect.height(),
                        SWP_SHOWWINDOW,
                    );
                    return Some(physical_rect_to_dip_bounds(adjusted, scale));
                }
            }

            abd.rc = RECT {
                left: adjusted.left,
                top: adjusted.top,
                right: adjusted.right,
                bottom: adjusted.bottom,
            };

            SHAppBarMessage(ABM_SETPOS, &mut abd);

            let final_rect = PhysicalRect {
                left: abd.rc.left,
                top: abd.rc.top,
                right: abd.rc.right,
                bottom: abd.rc.bottom,
            };

            if let Ok(mut committed) = COMMITTED_APPBAR_RECT.lock() {
                *committed = Some(final_rect);
            }

            let final_window_rect = compensate_frame_insets(final_rect, insets);

            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                final_window_rect.left,
                final_window_rect.top,
                final_window_rect.width(),
                final_window_rect.height(),
                SWP_SHOWWINDOW,
            );

            Some(physical_rect_to_dip_bounds(final_rect, scale))
        }
    }

    pub unsafe fn remove_appbar_by_hwnd(hwnd: HWND) {
        if hwnd.is_null() {
            return;
        }
        if let Ok(mut committed) = COMMITTED_APPBAR_RECT.lock() {
            *committed = None;
        }
        unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(appbar_subclass_proc), APPBAR_SUBCLASS_ID);
            let mut abd: APPBARDATA = zeroed();
            abd.cbSize = size_of::<APPBARDATA>() as u32;
            abd.hWnd = hwnd;
            SHAppBarMessage(ABM_REMOVE, &mut abd);
        }
    }

    pub fn remove_appbar(window: &gpui::Window) {
        if let Some(hwnd) = extract_hwnd(window) {
            unsafe {
                remove_appbar_by_hwnd(hwnd);
            }
            let _ = ACTIVE_APPBAR_HWND.compare_exchange(
                hwnd as isize,
                0,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
        } else {
            cleanup_appbar();
        }
    }

    pub fn cleanup_appbar() {
        if let Ok(mut committed) = COMMITTED_APPBAR_RECT.lock() {
            *committed = None;
        }
        let hwnd = ACTIVE_APPBAR_HWND.swap(0, Ordering::SeqCst);
        if hwnd != 0 {
            unsafe {
                remove_appbar_by_hwnd(hwnd as HWND);
            }
        }
    }

    pub fn set_window_bounds_and_topmost(
        window: &gpui::Window,
        bounds_dip: Bounds<Pixels>,
        always_on_top: bool,
    ) {
        if let Some(hwnd) = extract_hwnd(window) {
            let scale = window.scale_factor();
            let rect = dip_bounds_to_physical_rect(bounds_dip, scale);
            let insets = unsafe { get_window_frame_insets(hwnd, scale) };
            let window_rect = compensate_frame_insets(rect, insets);
            let insert_after = if always_on_top {
                HWND_TOPMOST
            } else {
                HWND_NOTOPMOST
            };
            unsafe {
                SetWindowPos(
                    hwnd,
                    insert_after,
                    window_rect.left,
                    window_rect.top,
                    window_rect.width(),
                    window_rect.height(),
                    SWP_SHOWWINDOW,
                );
            }
        }
    }

    pub fn set_window_topmost(window: &gpui::Window, always_on_top: bool) {
        if let Some(hwnd) = extract_hwnd(window) {
            let insert_after = if always_on_top {
                HWND_TOPMOST
            } else {
                HWND_NOTOPMOST
            };
            unsafe {
                SetWindowPos(
                    hwnd,
                    insert_after,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                );
            }
        }
    }

    pub fn restore_and_focus_window(window: &gpui::Window) {
        if let Some(hwnd) = extract_hwnd(window) {
            unsafe {
                if IsIconic(hwnd) != 0 {
                    ShowWindow(hwnd, SW_RESTORE);
                } else {
                    ShowWindow(hwnd, SW_RESTORE);
                }
                SetForegroundWindow(hwnd);
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub use imp::*;

#[cfg(not(target_os = "windows"))]
pub fn cleanup_appbar() {}

#[cfg(not(target_os = "windows"))]
pub fn take_poschanged_pending() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dip_to_physical_and_back_with_hidpi_scale() {
        let dip = Bounds::new(point(px(100.0), px(50.0)), size(px(400.0), px(900.0)));
        let phys = dip_bounds_to_physical_rect(dip, 1.5);
        assert_eq!(
            phys,
            PhysicalRect {
                left: 150,
                top: 75,
                right: 750,
                bottom: 1425,
            }
        );
        assert_eq!(phys.width(), 600);
        assert_eq!(phys.height(), 1350);

        let roundtrip = physical_rect_to_dip_bounds(phys, 1.5);
        assert_eq!(roundtrip, dip);
    }

    #[test]
    fn test_adjust_appbar_edge_width_left_and_right() {
        let queried_left = PhysicalRect {
            left: 0,
            top: 0,
            right: 500,
            bottom: 1040,
        };
        let adjusted_left = adjust_appbar_edge_width(queried_left, DockSide::Left, 400);
        assert_eq!(
            adjusted_left,
            PhysicalRect {
                left: 0,
                top: 0,
                right: 400,
                bottom: 1040,
            }
        );

        let queried_right = PhysicalRect {
            left: 1400,
            top: 0,
            right: 1920,
            bottom: 1040,
        };
        let adjusted_right = adjust_appbar_edge_width(queried_right, DockSide::Right, 400);
        assert_eq!(
            adjusted_right,
            PhysicalRect {
                left: 1520,
                top: 0,
                right: 1920,
                bottom: 1040,
            }
        );
    }

    #[test]
    fn test_edge_for_dock_side() {
        assert_eq!(edge_for_dock_side(DockSide::Left), ABE_LEFT_EDGE);
        assert_eq!(edge_for_dock_side(DockSide::Right), ABE_RIGHT_EDGE);
    }

    #[test]
    fn test_compensate_frame_insets_left_dock_eliminates_7px_gap() {
        // Reserved AppBar screen rect on 100% scale display (1920x1080)
        let final_rect = PhysicalRect {
            left: 0,
            top: 0,
            right: 400,
            bottom: 1080,
        };
        // Standard Windows 10/11 invisible frame resize border insets (7px left, right, bottom)
        let insets = FrameInsets {
            left: 7,
            top: 0,
            right: 7,
            bottom: 7,
        };
        let window_rect = compensate_frame_insets(final_rect, insets);
        assert_eq!(
            window_rect,
            PhysicalRect {
                left: -7,
                top: 0,
                right: 407,
                bottom: 1087,
            }
        );
        assert_eq!(window_rect.width(), 414);
        assert_eq!(window_rect.height(), 1087);

        // When the window frame is placed at `window_rect`, the resulting visible
        // client area is offset by `insets.left`, exactly matching `final_rect`.
        let visible_left = window_rect.left + insets.left;
        let visible_right = window_rect.right - insets.right;
        let visible_top = window_rect.top + insets.top;
        let visible_bottom = window_rect.bottom - insets.bottom;
        assert_eq!(visible_left, 0);
        assert_eq!(visible_top, 0);
        assert_eq!(visible_right, 400);
        assert_eq!(visible_bottom, 1080);
    }

    #[test]
    fn test_compensate_frame_insets_right_dock_eliminates_7px_overflow() {
        // Reserved AppBar screen rect on 100% scale display (1920x1080)
        let final_rect = PhysicalRect {
            left: 1520,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let insets = FrameInsets {
            left: 7,
            top: 0,
            right: 7,
            bottom: 7,
        };
        let window_rect = compensate_frame_insets(final_rect, insets);
        assert_eq!(
            window_rect,
            PhysicalRect {
                left: 1513,
                top: 0,
                right: 1927,
                bottom: 1087,
            }
        );
        assert_eq!(window_rect.width(), 414);
        assert_eq!(window_rect.height(), 1087);

        // Visible client area ends flush with the right screen edge (1920),
        // preventing the 7px off-screen spill and 7px gap on the left.
        let visible_left = window_rect.left + insets.left;
        let visible_right = window_rect.right - insets.right;
        assert_eq!(visible_left, 1520);
        assert_eq!(visible_right, 1920);
        assert_eq!(visible_right - visible_left, 400);
    }

    #[test]
    fn test_compensate_frame_insets_hidpi_scaling() {
        // HiDPI 150% scale display (e.g. 11px border)
        let final_rect = PhysicalRect {
            left: 0,
            top: 0,
            right: 600,
            bottom: 1620,
        };
        let insets = FrameInsets {
            left: 11,
            top: 0,
            right: 11,
            bottom: 11,
        };
        let window_rect = compensate_frame_insets(final_rect, insets);
        assert_eq!(
            window_rect,
            PhysicalRect {
                left: -11,
                top: 0,
                right: 611,
                bottom: 1631,
            }
        );
        assert_eq!(window_rect.left + insets.left, 0);
        assert_eq!(window_rect.right - insets.right, 600);
    }

    #[test]
    fn test_compensate_frame_insets_zero_insets() {
        let final_rect = PhysicalRect {
            left: 100,
            top: 50,
            right: 500,
            bottom: 800,
        };
        let insets = FrameInsets::default();
        assert_eq!(compensate_frame_insets(final_rect, insets), final_rect);
    }
}
