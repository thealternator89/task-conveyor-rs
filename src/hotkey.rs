use crate::config_store::ConfigStore;
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use gpui::{App, Entity, Task};
use rfd::{MessageDialog, MessageLevel};
use std::str::FromStr;

/// Normalizes Electron-style accelerator modifier and key aliases for `global-hotkey` parsing:
/// - `CommandOrControl` / `CmdOrCtrl` -> `Super` (macOS) or `Control` (Windows/Linux)
/// - `Command` / `Cmd` -> `Super`
/// - `Option` / `Opt` -> `Alt`
/// - `Ctrl` -> `Control`
/// - `Return` -> `Enter`
/// - `Esc` -> `Escape`
pub fn normalize_accelerator_for_platform(accel: &str, is_macos: bool) -> String {
    accel
        .split('+')
        .map(|part| {
            let trimmed = part.trim();
            match trimmed.to_ascii_lowercase().as_str() {
                "commandorcontrol" | "cmdorctrl" | "commandorctrl" | "cmdorcontrol" => {
                    if is_macos {
                        "Super".to_string()
                    } else {
                        "Control".to_string()
                    }
                }
                "command" | "cmd" => "Super".to_string(),
                "option" | "opt" => "Alt".to_string(),
                "ctrl" => "Control".to_string(),
                "return" => "Enter".to_string(),
                "esc" => "Escape".to_string(),
                "plus" => "Equal".to_string(),
                _ => trimmed.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Parses an Electron accelerator string into a `global_hotkey::hotkey::HotKey` for the current OS.
/// Ports `src/main/index.ts:115-124`.
pub fn parse_accelerator(accel: &str) -> Result<HotKey, String> {
    parse_accelerator_for_platform(accel, cfg!(target_os = "macos"))
}

pub fn parse_accelerator_for_platform(accel: &str, is_macos: bool) -> Result<HotKey, String> {
    let trimmed = accel.trim();
    if trimmed.is_empty() {
        return Err("Accelerator string cannot be empty.".to_string());
    }
    let normalized = normalize_accelerator_for_platform(trimmed, is_macos);
    HotKey::from_str(&normalized)
        .map_err(|err| format!("Invalid hotkey \"{trimmed}\" (normalized: \"{normalized}\"): {err}"))
}

pub struct GlobalHotkeySubscription {
    pub manager: GlobalHotKeyManager,
    pub hotkey: HotKey,
    _event_task: Task<()>,
}

impl GlobalHotkeySubscription {
    pub fn unregister(&self) {
        let _ = self.manager.unregister(self.hotkey);
    }
}

/// Registers the configured `globalHotkey` via `GlobalHotKeyManager::new()` and spawns a foreground
/// task listening to `GlobalHotKeyEvent::receiver()` (filtered for `HotKeyState::Pressed`) that
/// invokes `on_toggle`.
///
/// - On success: sets `active_hotkey = Some(target_hotkey)` on `ConfigStore`.
/// - On failure: sets `active_hotkey = None` on `ConfigStore` and shows a native error dialog via `rfd`.
///
/// Ports `src/main/index.ts:115-124, 166-175`.
pub fn register_global_hotkey(
    config_store: &Entity<ConfigStore>,
    cx: &mut App,
    mut on_toggle: impl FnMut(&mut App) + 'static,
) -> Option<GlobalHotkeySubscription> {
    let target_hotkey = config_store.read(cx).app_config.global_hotkey.clone();

    let hotkey = match parse_accelerator(&target_hotkey) {
        Ok(hk) => hk,
        Err(err) => {
            config_store.update(cx, |cfg, cx| cfg.set_active_hotkey(None, cx));
            show_hotkey_error_dialog(&target_hotkey, &err);
            return None;
        }
    };

    let manager = match GlobalHotKeyManager::new() {
        Ok(mgr) => mgr,
        Err(err) => {
            config_store.update(cx, |cfg, cx| cfg.set_active_hotkey(None, cx));
            show_hotkey_error_dialog(&target_hotkey, &err.to_string());
            return None;
        }
    };

    if let Err(err) = manager.register(hotkey) {
        config_store.update(cx, |cfg, cx| cfg.set_active_hotkey(None, cx));
        show_hotkey_error_dialog(&target_hotkey, &err.to_string());
        return None;
    }

    config_store.update(cx, |cfg, cx| {
        cfg.set_active_hotkey(Some(target_hotkey.clone()), cx);
    });

    let expected_id = hotkey.id();
    let (tx, rx) = smol::channel::unbounded::<GlobalHotKeyEvent>();

    std::thread::Builder::new()
        .name("global-hotkey-bridge".to_string())
        .spawn(move || {
            let receiver = GlobalHotKeyEvent::receiver();
            while let Ok(event) = receiver.recv() {
                if event.state == HotKeyState::Pressed && event.id == expected_id {
                    if tx.send_blocking(event).is_err() {
                        break;
                    }
                }
            }
        })
        .ok();

    let event_task = cx.spawn(async move |cx| {
        while let Ok(_event) = rx.recv().await {
            let _ = cx.update(|cx| {
                on_toggle(cx);
            });
        }
    });

    Some(GlobalHotkeySubscription {
        manager,
        hotkey,
        _event_task: event_task,
    })
}

fn show_hotkey_error_dialog(hotkey_str: &str, details: &str) {
    eprintln!("Failed to register global hotkey \"{hotkey_str}\": {details}");
    MessageDialog::new()
        .set_level(MessageLevel::Error)
        .set_title("Global Hotkey Error")
        .set_description(format!(
            "Failed to register global hotkey \"{hotkey_str}\". It may be in use by another application or invalid.\n\nDetails: {details}"
        ))
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;
    use global_hotkey::hotkey::{Code, Modifiers};

    #[test]
    fn test_normalize_and_parse_electron_accelerators() {
        // Option+Space -> Alt + Space
        let hk_opt = parse_accelerator_for_platform("Option+Space", true).unwrap();
        assert_eq!(hk_opt.mods, Modifiers::ALT);
        assert_eq!(hk_opt.key, Code::Space);

        // Ctrl+Space -> Control + Space
        let hk_ctrl = parse_accelerator_for_platform("Ctrl+Space", false).unwrap();
        assert_eq!(hk_ctrl.mods, Modifiers::CONTROL);
        assert_eq!(hk_ctrl.key, Code::Space);

        // CommandOrControl / CmdOrCtrl -> Super on macOS, Control on Windows/Linux
        let hk_cmd_mac = parse_accelerator_for_platform("CommandOrControl+Shift+K", true).unwrap();
        assert_eq!(hk_cmd_mac.mods, Modifiers::SUPER | Modifiers::SHIFT);
        assert_eq!(hk_cmd_mac.key, Code::KeyK);

        let hk_cmd_win = parse_accelerator_for_platform("CommandOrControl+Shift+K", false).unwrap();
        assert_eq!(hk_cmd_win.mods, Modifiers::CONTROL | Modifiers::SHIFT);
        assert_eq!(hk_cmd_win.key, Code::KeyK);

        let hk_short_mac = parse_accelerator_for_platform("CmdOrCtrl+Space", true).unwrap();
        assert_eq!(hk_short_mac.mods, Modifiers::SUPER);
        assert_eq!(hk_short_mac.key, Code::Space);

        let hk_short_win = parse_accelerator_for_platform("CmdOrCtrl+Space", false).unwrap();
        assert_eq!(hk_short_win.mods, Modifiers::CONTROL);
        assert_eq!(hk_short_win.key, Code::Space);

        // Command / Cmd -> Super on all platforms
        let hk_command = parse_accelerator_for_platform("Command+Option+P", false).unwrap();
        assert_eq!(hk_command.mods, Modifiers::SUPER | Modifiers::ALT);
        assert_eq!(hk_command.key, Code::KeyP);

        let hk_cmd = parse_accelerator_for_platform("Cmd+Return", true).unwrap();
        assert_eq!(hk_cmd.mods, Modifiers::SUPER);
        assert_eq!(hk_cmd.key, Code::Enter);
    }

    #[test]
    fn test_parse_accelerator_rejects_invalid_inputs() {
        assert!(parse_accelerator_for_platform("", true).is_err());
        assert!(parse_accelerator_for_platform("   ", false).is_err());
        assert!(parse_accelerator_for_platform("Option+", true).is_err());
        assert!(parse_accelerator_for_platform("Option+NotAKey", true).is_err());
        assert!(parse_accelerator_for_platform("Shift+Ctrl", false).is_err());
    }
}
