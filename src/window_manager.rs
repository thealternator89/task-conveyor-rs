use crate::config_store::ConfigStore;
use crate::task_engine::DockSide;
use crate::task_store::TaskStore;
use crate::views::main_sidebar::MainSidebarView;
use crate::views::spotlight::{SpotlightView, spotlight_window_options};
use crate::win32_appbar;
use gpui::{
    App, Bounds, Context, Entity, Pixels, Size, Window, WindowBounds, WindowHandle, WindowOptions,
    point, prelude::*, px, size,
};

pub const DOCK_WIDTH: f32 = 400.0;
pub const FLOAT_WIDTH: f32 = 400.0;
pub const FLOAT_HEIGHT: f32 = 600.0;

pub fn compute_dock_bounds(
    side: DockSide,
    work_area: Bounds<Pixels>,
    dock_width: Pixels,
) -> Bounds<Pixels> {
    let clamped_width = if dock_width > work_area.size.width {
        work_area.size.width
    } else {
        dock_width
    };
    let x = match side {
        DockSide::Left => work_area.origin.x,
        DockSide::Right => work_area.origin.x + work_area.size.width - clamped_width,
    };
    Bounds::new(
        point(x, work_area.origin.y),
        size(clamped_width, work_area.size.height),
    )
}

pub fn compute_float_bounds(work_area: Bounds<Pixels>, float_size: Size<Pixels>) -> Bounds<Pixels> {
    let origin = point(
        work_area.origin.x + (work_area.size.width - float_size.width) * 0.5,
        work_area.origin.y + (work_area.size.height - float_size.height) * 0.5,
    );
    Bounds::new(origin, float_size)
}

pub fn centered_sidebar_bounds(cx: &App) -> Bounds<Pixels> {
    let target_size = size(px(FLOAT_WIDTH), px(FLOAT_HEIGHT));
    if let Some(display) = cx.primary_display() {
        compute_float_bounds(display.visible_bounds(), target_size)
    } else {
        Bounds::centered(None, target_size, cx)
    }
}

#[allow(dead_code)]
fn display_bounds_for_window(window: &Window, cx: &App) -> Bounds<Pixels> {
    window
        .display(cx)
        .or_else(|| cx.primary_display())
        .map(|display| display.bounds())
        .unwrap_or_else(|| Bounds::new(point(px(0.0), px(0.0)), size(px(1920.0), px(1080.0))))
}

fn work_area_for_window(window: &Window, cx: &App) -> Bounds<Pixels> {
    window
        .display(cx)
        .or_else(|| cx.primary_display())
        .map(|display| display.visible_bounds())
        .unwrap_or_else(|| Bounds::new(point(px(0.0), px(0.0)), size(px(1920.0), px(1080.0))))
}

#[cfg(target_os = "macos")]
mod macos_window {
    use gpui::{Bounds, Pixels, Window};
    use objc2::msg_send;
    use objc2::runtime::{AnyObject, Bool};
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::ptr;

    const NS_NORMAL_WINDOW_LEVEL: isize = 0;
    const NS_FLOATING_WINDOW_LEVEL: isize = 3;

    unsafe fn ns_window_from_gpui(window: &Window) -> Option<*mut AnyObject> {
        let handle = HasWindowHandle::window_handle(window).ok()?;
        match handle.as_raw() {
            RawWindowHandle::AppKit(appkit) => {
                let ns_view = appkit.ns_view.as_ptr().cast::<AnyObject>();
                if ns_view.is_null() {
                    return None;
                }
                let ns_window: *mut AnyObject = unsafe { msg_send![ns_view, window] };
                if ns_window.is_null() {
                    None
                } else {
                    Some(ns_window)
                }
            }
            _ => None,
        }
    }

    pub fn apply_bounds_and_level(
        window: &mut Window,
        bounds: Bounds<Pixels>,
        always_on_top: bool,
        is_docked: bool,
    ) {
        window.resize(bounds.size);
        unsafe {
            let Some(ns_window) = ns_window_from_gpui(window) else {
                return;
            };
            let level = if always_on_top {
                NS_FLOATING_WINDOW_LEVEL
            } else {
                NS_NORMAL_WINDOW_LEVEL
            };
            let _: () = msg_send![ns_window, setLevel: level];
            let _: () = msg_send![ns_window, setMovable: Bool::new(!is_docked)];

            let screen: *mut AnyObject = msg_send![ns_window, screen];
            if !screen.is_null() {
                let screen_frame: NSRect = msg_send![screen, frame];
                let width = f32::from(bounds.size.width) as f64;
                let height = f32::from(bounds.size.height) as f64;
                let x = screen_frame.origin.x + f32::from(bounds.origin.x) as f64;
                let y = screen_frame.size.height
                    - (f32::from(bounds.origin.y) as f64 - screen_frame.origin.y)
                    - height;
                let frame = NSRect::new(NSPoint::new(x, y), NSSize::new(width, height));
                let _: () = msg_send![ns_window, setFrame: frame, display: Bool::YES];
            }
        }
    }

    pub fn apply_level(window: &Window, always_on_top: bool) {
        unsafe {
            let Some(ns_window) = ns_window_from_gpui(window) else {
                return;
            };
            let level = if always_on_top {
                NS_FLOATING_WINDOW_LEVEL
            } else {
                NS_NORMAL_WINDOW_LEVEL
            };
            let _: () = msg_send![ns_window, setLevel: level];
        }
    }

    pub fn restore_and_focus(window: &mut Window) {
        unsafe {
            if let Some(ns_window) = ns_window_from_gpui(window) {
                let is_miniaturized: Bool = msg_send![ns_window, isMiniaturized];
                if is_miniaturized.as_bool() {
                    let null_sender: *const AnyObject = ptr::null();
                    let _: () = msg_send![ns_window, deminiaturize: null_sender];
                }
                let null_sender: *const AnyObject = ptr::null();
                let _: () = msg_send![ns_window, makeKeyAndOrderFront: null_sender];
            }
        }
        window.activate_window();
    }
}

pub struct WindowManager {
    pub dock_side: Option<DockSide>,
    pub always_on_top: bool,
    pub is_docking: bool,
    pub appbar_registered: bool,
    pub main_window: Option<WindowHandle<MainSidebarView>>,
    pub spotlight_window: Option<WindowHandle<SpotlightView>>,
    config_store: Entity<ConfigStore>,
    task_store: Entity<TaskStore>,
}

impl WindowManager {
    pub fn new(config_store: Entity<ConfigStore>, task_store: Entity<TaskStore>) -> Self {
        Self {
            dock_side: None,
            always_on_top: false,
            is_docking: false,
            appbar_registered: false,
            main_window: None,
            spotlight_window: None,
            config_store,
            task_store,
        }
    }

    pub fn set_main_window(&mut self, handle: Option<WindowHandle<MainSidebarView>>) {
        self.main_window = handle;
    }

    pub fn dock_window(&mut self, side: DockSide, cx: &mut Context<Self>) {
        if self.is_docking {
            return;
        }
        self.is_docking = true;
        self.dock_side = Some(side);
        self.always_on_top = true;

        if let Some(main_handle) = self.main_window {
            let mut registered = self.appbar_registered;
            let _ = main_handle.update(cx, |_view, window, cx| {
                #[cfg(target_os = "windows")]
                {
                    let display_bounds = display_bounds_for_window(window, cx);
                    let target_bounds = compute_dock_bounds(side, display_bounds, px(DOCK_WIDTH));

                    if let Some(final_bounds) =
                        win32_appbar::dock_appbar(window, side, target_bounds)
                    {
                        window.resize(final_bounds.size);
                        registered = true;
                    } else {
                        let work_area = work_area_for_window(window, cx);
                        let fallback_bounds = compute_dock_bounds(side, work_area, px(DOCK_WIDTH));
                        win32_appbar::set_window_bounds_and_topmost(window, fallback_bounds, true);
                        window.resize(fallback_bounds.size);
                    }
                }

                #[cfg(target_os = "macos")]
                {
                    let work_area = work_area_for_window(window, cx);
                    let target_bounds = compute_dock_bounds(side, work_area, px(DOCK_WIDTH));
                    let _ = &mut registered;
                    macos_window::apply_bounds_and_level(window, target_bounds, true, true);
                }

                #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                {
                    let work_area = work_area_for_window(window, cx);
                    let target_bounds = compute_dock_bounds(side, work_area, px(DOCK_WIDTH));
                    let _ = &mut registered;
                    window.resize(target_bounds.size);
                }

                cx.notify();
            });
            self.appbar_registered = registered;
        }

        self.is_docking = false;
        cx.notify();
    }

    pub fn float_window(&mut self, cx: &mut Context<Self>) {
        if self.is_docking {
            return;
        }
        self.is_docking = true;
        self.dock_side = None;
        self.always_on_top = false;

        if let Some(main_handle) = self.main_window {
            let was_registered = self.appbar_registered;
            let _ = main_handle.update(cx, |_view, window, cx| {
                #[cfg(target_os = "windows")]
                if was_registered {
                    win32_appbar::remove_appbar(window);
                }
                #[cfg(not(target_os = "windows"))]
                let _ = was_registered;

                let work_area = work_area_for_window(window, cx);
                let target_bounds =
                    compute_float_bounds(work_area, size(px(FLOAT_WIDTH), px(FLOAT_HEIGHT)));

                #[cfg(target_os = "windows")]
                {
                    win32_appbar::set_window_bounds_and_topmost(window, target_bounds, false);
                    window.resize(target_bounds.size);
                }

                #[cfg(target_os = "macos")]
                {
                    macos_window::apply_bounds_and_level(window, target_bounds, false, false);
                }

                #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                {
                    window.resize(target_bounds.size);
                }

                cx.notify();
            });
            self.appbar_registered = false;
        }

        self.is_docking = false;
        cx.notify();
    }

    pub fn toggle_always_on_top(&mut self, cx: &mut Context<Self>) {
        self.always_on_top = !self.always_on_top;
        let always_on_top = self.always_on_top;

        if let Some(main_handle) = self.main_window {
            let _ = main_handle.update(cx, |_view, window, cx| {
                #[cfg(target_os = "windows")]
                win32_appbar::set_window_topmost(window, always_on_top);

                #[cfg(target_os = "macos")]
                macos_window::apply_level(window, always_on_top);

                #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                let _ = (window, always_on_top);

                cx.notify();
            });
        }

        cx.notify();
    }

    pub fn restore_main_window(&mut self, cx: &mut Context<Self>) {
        cx.activate(true);
        if let Some(main_handle) = self.main_window {
            let updated = main_handle
                .update(cx, |_view, window, cx| {
                    #[cfg(target_os = "windows")]
                    {
                        win32_appbar::restore_and_focus_window(window);
                        window.activate_window();
                    }

                    #[cfg(target_os = "macos")]
                    macos_window::restore_and_focus(window);

                    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                    window.activate_window();

                    cx.notify();
                })
                .is_ok();
            if updated {
                return;
            }
        }

        // If the main window was closed, re-open it.
        let bounds = centered_sidebar_bounds(cx);
        let config_store = self.config_store.clone();
        let task_store = self.task_store.clone();
        let wm_entity = cx.entity();
        if let Ok(handle) = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(320.0), px(200.0))),
                titlebar: None,
                app_owns_titlebar_drag: true,
                ..Default::default()
            },
            move |window, cx| {
                cx.new(|cx| {
                    MainSidebarView::new(window, config_store, task_store, Some(wm_entity), cx)
                })
            },
        ) {
            self.main_window = Some(handle);
            if let Some(side) = self.dock_side {
                self.dock_window(side, cx);
            } else if self.always_on_top {
                let always_on_top = self.always_on_top;
                let _ = handle.update(cx, |_view, window, _cx| {
                    #[cfg(target_os = "windows")]
                    win32_appbar::set_window_topmost(window, always_on_top);

                    #[cfg(target_os = "macos")]
                    macos_window::apply_level(window, always_on_top);

                    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                    let _ = (window, always_on_top);
                });
            }
        }
    }

    pub fn toggle_spotlight(&mut self, cx: &mut Context<Self>) {
        if let Some(existing) = self.spotlight_window.take() {
            let was_open = existing
                .update(cx, |view, window, cx| {
                    view.hide(window, cx);
                })
                .is_ok();
            if was_open {
                return;
            }
        }

        cx.activate(true);
        let cfg = self.config_store.clone();
        let tasks = self.task_store.clone();
        let main_window = self.main_window;
        let options = spotlight_window_options(cx);
        if let Ok(handle) = cx.open_window(options, move |window, cx| {
            cx.new(|cx| SpotlightView::new(window, cfg, tasks, main_window, cx))
        }) {
            self.spotlight_window = Some(handle);
        }
    }

    pub fn hide_spotlight(&mut self, cx: &mut Context<Self>) {
        if let Some(existing) = self.spotlight_window.take() {
            let _ = existing.update(cx, |view, window, cx| {
                view.hide(window, cx);
            });
        }
    }

    pub fn handle_poschanged_if_needed(&mut self, cx: &mut Context<Self>) {
        if win32_appbar::take_poschanged_pending() {
            if let Some(side) = self.dock_side {
                self.dock_window(side, cx);
            }
        }
    }

    pub fn cleanup_on_quit(&mut self, cx: &mut App) {
        if self.appbar_registered {
            if let Some(main_handle) = self.main_window {
                let _ = main_handle.update(cx, |_view, window, _cx| {
                    #[cfg(target_os = "windows")]
                    win32_appbar::remove_appbar(window);
                    #[cfg(not(target_os = "windows"))]
                    let _ = window;
                });
            }
            win32_appbar::cleanup_appbar();
            self.appbar_registered = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_dock_bounds_left_and_right() {
        let work_area = Bounds::new(point(px(0.0), px(25.0)), size(px(1920.0), px(1055.0)));

        let left = compute_dock_bounds(DockSide::Left, work_area, px(400.0));
        assert_eq!(left.origin, point(px(0.0), px(25.0)));
        assert_eq!(left.size, size(px(400.0), px(1055.0)));

        let right = compute_dock_bounds(DockSide::Right, work_area, px(400.0));
        assert_eq!(right.origin, point(px(1520.0), px(25.0)));
        assert_eq!(right.size, size(px(400.0), px(1055.0)));
    }

    #[test]
    fn test_compute_dock_bounds_secondary_monitor_offset() {
        let work_area = Bounds::new(point(px(-1440.0), px(0.0)), size(px(1440.0), px(900.0)));

        let left = compute_dock_bounds(DockSide::Left, work_area, px(400.0));
        assert_eq!(left.origin, point(px(-1440.0), px(0.0)));
        assert_eq!(left.size, size(px(400.0), px(900.0)));

        let right = compute_dock_bounds(DockSide::Right, work_area, px(400.0));
        assert_eq!(right.origin, point(px(-400.0), px(0.0)));
        assert_eq!(right.size, size(px(400.0), px(900.0)));
    }

    #[test]
    fn test_compute_dock_bounds_monitor_bounds_stable_across_work_area_changes() {
        // Full monitor bounds (does not change when AppBar docks)
        let display_bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(1920.0), px(1080.0)));
        let dock_right = compute_dock_bounds(DockSide::Right, display_bounds, px(400.0));
        assert_eq!(dock_right.origin, point(px(1520.0), px(0.0)));
        assert_eq!(dock_right.size, size(px(400.0), px(1080.0)));

        // If someone used work_area (which shrank from 1920 to 1520), it would compute 1120:
        let shrunken_work_area =
            Bounds::new(point(px(0.0), px(0.0)), size(px(1520.0), px(1040.0)));
        let bad_dock_right = compute_dock_bounds(DockSide::Right, shrunken_work_area, px(400.0));
        assert_eq!(bad_dock_right.origin, point(px(1120.0), px(0.0)));
    }

    #[test]
    fn test_compute_float_bounds_centered_in_work_area() {
        let work_area = Bounds::new(point(px(0.0), px(40.0)), size(px(1920.0), px(1000.0)));
        let float_bounds = compute_float_bounds(work_area, size(px(400.0), px(600.0)));

        assert_eq!(float_bounds.origin, point(px(760.0), px(240.0)));
        assert_eq!(float_bounds.size, size(px(400.0), px(600.0)));
    }
}
