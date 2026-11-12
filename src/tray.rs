use crate::assets::Assets;
use crate::window_manager::WindowManager;
use gpui::{App, Entity, Task};
use image::imageops::FilterType;
use muda::{Menu, MenuEvent, MenuItem};
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

pub const TRAY_TOOLTIP: &str = "TaskConveyor";
pub const TRAY_EXIT_ID: &str = "quit";
pub const TRAY_ICON_SIZE: u32 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    RestoreMainWindow,
    ExitApp,
}

pub fn load_tray_icon_rgba() -> anyhow::Result<(Vec<u8>, u32, u32)> {
    let raw_bytes = Assets::get("icon.ico")
        .or_else(|| Assets::get("logo-full.png"))
        .ok_or_else(|| anyhow::anyhow!("Embedded tray icon asset not found"))?;

    let decoded = image::load_from_memory(raw_bytes.data.as_ref())?;
    let resized = decoded
        .resize_exact(TRAY_ICON_SIZE, TRAY_ICON_SIZE, FilterType::Lanczos3)
        .into_rgba8();
    let (width, height) = resized.dimensions();
    Ok((resized.into_raw(), width, height))
}

pub fn load_tray_icon() -> anyhow::Result<Icon> {
    let (rgba, width, height) = load_tray_icon_rgba()?;
    Ok(Icon::from_rgba(rgba, width, height)?)
}

pub struct SystemTraySubscription {
    pub tray_icon: TrayIcon,
    _event_task: Task<()>,
}

pub fn setup_system_tray(
    window_manager: Entity<WindowManager>,
    cx: &mut App,
) -> Option<SystemTraySubscription> {
    let icon = match load_tray_icon() {
        Ok(icon) => icon,
        Err(err) => {
            eprintln!("Failed to load system tray icon: {err}");
            return None;
        }
    };

    let menu = Menu::new();
    let quit_item = MenuItem::with_id(TRAY_EXIT_ID, "Exit", true, None);
    let quit_id = quit_item.id().clone();
    if let Err(err) = menu.append(&quit_item) {
        eprintln!("Failed to append Exit item to system tray menu: {err}");
    }

    let tray_icon = match TrayIconBuilder::new()
        .with_tooltip(TRAY_TOOLTIP)
        .with_icon(icon)
        .with_icon_as_template(false)
        .with_menu(Box::new(menu))
        .build()
    {
        Ok(tray) => tray,
        Err(err) => {
            eprintln!("Failed to create system tray icon: {err}");
            return None;
        }
    };

    let (tx, rx) = smol::channel::unbounded::<TrayAction>();

    let tx_tray = tx.clone();
    std::thread::Builder::new()
        .name("tray-icon-event-bridge".to_string())
        .spawn(move || {
            let receiver = TrayIconEvent::receiver();
            while let Ok(event) = receiver.recv() {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    if tx_tray.send_blocking(TrayAction::RestoreMainWindow).is_err() {
                        break;
                    }
                }
            }
        })
        .ok();

    let tx_menu = tx;
    std::thread::Builder::new()
        .name("tray-menu-event-bridge".to_string())
        .spawn(move || {
            let receiver = MenuEvent::receiver();
            while let Ok(event) = receiver.recv() {
                if event.id == quit_id {
                    if tx_menu.send_blocking(TrayAction::ExitApp).is_err() {
                        break;
                    }
                }
            }
        })
        .ok();

    let event_task = cx.spawn(async move |cx| {
        while let Ok(action) = rx.recv().await {
            let _ = cx.update(|cx| match action {
                TrayAction::RestoreMainWindow => {
                    window_manager.update(cx, |wm, cx| {
                        wm.restore_main_window(cx);
                    });
                }
                TrayAction::ExitApp => {
                    window_manager.update(cx, |wm, cx| {
                        wm.cleanup_on_quit(cx);
                    });
                    cx.quit();
                }
            });
        }
    });

    Some(SystemTraySubscription {
        tray_icon,
        _event_task: event_task,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_tray_icon_rgba_resizes_to_16x16() {
        let (rgba, width, height) =
            load_tray_icon_rgba().expect("embedded tray icon should decode and resize");
        assert_eq!(width, 16);
        assert_eq!(height, 16);
        assert_eq!(rgba.len(), (16 * 16 * 4) as usize);
    }

    #[test]
    fn test_load_tray_icon_constructs_valid_icon() {
        let _icon = load_tray_icon().expect("tray_icon::Icon should build from 16x16 RGBA");
    }
}
