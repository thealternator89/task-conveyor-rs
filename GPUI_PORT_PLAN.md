# TaskConveyor: Electron to Rust + GPUI Porting Plan

This document provides a complete technical extraction and architectural blueprint for porting **TaskConveyor** from TypeScript/Electron/React to **Rust + GPUI**.

---

## Architecture Overview: Electron vs. Rust + GPUI

In the current Electron implementation, TaskConveyor runs **three OS processes**:
1. **Main Process** (`src/main/`): Manages window creation, Win32 AppBar docking via `koffi`, system tray, global hotkeys, OS theme observation, and JSON config file watching (`config.json`, `autocomplete.json`).
2. **Main Sidebar Renderer** (`src/renderer/app.tsx` -> `<MainApp />`): Owns the task queue state in `window.localStorage`, executes slash commands, and renders the docked/floating sidebar UI.
3. **Spotlight Overlay Renderer** (`src/renderer/app.tsx?window=spotlight` -> `<SpotlightInput />`): Renders the floating command input bar and relays submitted strings back through `ipcMain` (`submit-task` -> `task-added`) to `<MainApp />` for execution.

In **Rust + GPUI**, the entire application runs in a **single native process** with two GPUI windows (`WindowHandle<MainSidebarView>` and `WindowHandle<SpotlightView>`) sharing in-memory `Entity<T>` (formerly `Model<T>`) state directly on the GPUI foreground executor. All IPC serialization, preload bridges, duplicate state synchronization, and cross-window command relays are eliminated.

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                        Rust + GPUI Single Process                            │
│                                                                              │
│  ┌─────────────────────┐  ┌─────────────────────┐  ┌──────────────────────┐  │
│  │  Entity<TaskStore>  │  │ Entity<ConfigStore> │  │Entity<WindowManager> │  │
│  │  - tasks: Vec<Task> │  │ - config.json       │  │ - dock_side          │  │
│  │  - history (undo)   │  │ - autocomplete.json │  │ - always_on_top      │  │
│  │  - warning banner   │  │ - notify watchers   │  │ - Win32 AppBar state │  │
│  │  - tasks.json I/O   │  │ - Global<Theme>     │  │ - Window handles     │  │
│  └──────────▲──────────┘  └──────────▲──────────┘  └──────────▲───────────┘  │
│             │                        │                        │              │
│             └────────────────────────┼────────────────────────┘              │
│                                      │ cx.observe() / model.update()         │
│                     ┌────────────────┴────────────────┐                      │
│                     ▼                                 ▼                      │
│     ┌───────────────────────────────┐ ┌───────────────────────────────┐      │
│     │ Window 1: MainSidebarView     │ │ Window 2: SpotlightView       │      │
│     │ - Header (drag/pin/logo)      │ │ - WindowKind::PopUp           │      │
│     │ - Urgent/Active Task Card     │ │ - Auto-hides on blur          │      │
│     │ - Scrollable "Up Next" List   │ │ - Shared CommandInputView     │      │
│     │ - Footer + AutocompletePopover│ │   (inline ghost text only)    │      │
│     │ - Shared CommandInputView     │ └───────────────────────────────┘      │
│     └───────────────────────────────┘                                        │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 1. IPC Boundaries (`ipcRenderer` <-> `ipcMain`)

All IPC channels are defined across:
- `src/main/ipc.ts` (`HANDLER_CHANNELS`, `LISTENER_CHANNELS`, `registerIpcHandlers`, and `send*` helpers)
- `src/main/preload.ts` (`contextBridge.exposeInMainWorld('api', ...)`)
- `src/main/autocomplete.ts` (`broadcastAutocompleteUpdate`)
- `src/renderer/app.tsx` (`ElectronAPI` interface and `useEffect` hooks)

### 1.1 Renderer -> Main: Fire-and-Forget Commands (`ipcRenderer.send` -> `ipcMain.on`)

| Channel Name | Preload Method (`window.api.*`) | Payload Schema (TS) | Sender Location | Main Process Handler (`src/main/ipc.ts`) | Native / OS Operation vs. Pure Renderer State | GPUI Port Replacement |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `'submit-task'` | `sendTaskCommand(text)` (`preload.ts:4`) | `text: string` | `SpotlightInput.handleKeyDown` on `Enter` (`app.tsx:252`) | `ipc.ts:138-141`: Calls `sendTaskAdded(mainWindow, text)` and `spotlightWindow.hide()`. | **Hybrid**: Task/command string is **pure renderer state** (only crossed IPC because `SpotlightInput` and `MainApp` lived in separate `BrowserWindow` processes). `spotlightWindow.hide()` is **native window management**. | Direct call on shared `Entity<TaskStore>`: `task_store.update(cx, \|s, cx\| s.execute_command(text, ...))` + `window_mgr.update(cx, \|w, cx\| w.hide_spotlight(cx))`. |
| `'hide-spotlight'` | `hideSpotlight()` (`preload.ts:5`) | `void` | `SpotlightInput.handleKeyDown` on `Escape` when suggestions are closed (`app.tsx:247`) | `ipc.ts:143-145`: Calls `spotlightWindow.hide()`. | **Native Window Management** | `window_mgr.update(cx, \|w, cx\| w.hide_spotlight(cx))` |
| `'quit-app'` | `quitApp()` (`preload.ts:6`) | `void` | `MainApp.executeCommand` on `/x`, `/exit`, `/q`, `/quit` (`app.tsx:790`) | `ipc.ts:147-149`: Calls `app.quit()`. | **Native App Lifecycle** | `cx.quit()` (`gpui::App::quit`) |
| `'dock-window'` | `dockWindow(side)` (`preload.ts:7`) | `side: 'left' \| 'right'` (`DockSide`) | `MainApp.executeCommand` on `/dock [l\|r]` (`app.tsx:738`) | `ipc.ts:151-153` -> `dockMainWindow(side)` (`window.ts:151-230`): Registers Win32 AppBar (`SHAppBarMessage`) or snaps to OS `display.workArea`, sets `alwaysOnTop(true)`, emits `'always-on-top-changed'` & `'dock-changed'`. | **Native OS Window Management & Win32 FFI** | `window_mgr.update(cx, \|w, cx\| w.dock_window(side, cx))` |
| `'float-window'` | `floatWindow()` (`preload.ts:8`) | `void` | `MainApp.executeCommand` on `/float` or `/undock` (`app.tsx:745`) | `ipc.ts:155-157` -> `floatMainWindow()` (`window.ts:232-263`): Unregisters Win32 AppBar (`ABM_REMOVE`), centers `400x600` window in `display.workArea`, sets `alwaysOnTop(false)`, emits `'always-on-top-changed'` & `'dock-changed'`. | **Native OS Window Management & Win32 FFI** | `window_mgr.update(cx, \|w, cx\| w.float_window(cx))` |
| `'toggle-always-on-top'` | `toggleAlwaysOnTop()` (`preload.ts:31`) | `void` | `MainApp.executeCommand` on `/p` or `/pin` (`app.tsx:729`) | `ipc.ts:159-165`: Flips `!win.isAlwaysOnTop()`, calls `win.setAlwaysOnTop(state)`, emits `'always-on-top-changed'`. | **Native Window Management** | `window_mgr.update(cx, \|w, cx\| w.toggle_always_on_top(cx))` |

---

### 1.2 Renderer -> Main: Request/Response Invocations (`ipcRenderer.invoke` -> `ipcMain.handle`)

| Channel Name | Preload Method (`window.api.*`) | Request Schema | Response Schema (TS) | Caller Location | Main Process Handler (`src/main/ipc.ts`) | Native / OS Operation vs. Pure State | GPUI Port Replacement |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `'get-always-on-top'` | `getInitialAlwaysOnTop()` (`preload.ts:39`) | `void` | `boolean` | `MainApp` mount `useEffect` (`app.tsx:374`) | `ipc.ts:168-170`: Queries `mainWindow.isAlwaysOnTop() ?? false`. | **Native Window State Query** | Read `window_mgr.read(cx).always_on_top` directly during `Render::render`. |
| `'get-dock-side'` | `getDockSide()` (`preload.ts:9`) | `void` | `'left' \| 'right' \| null` | `MainApp` mount `useEffect` (`app.tsx:381`) | `ipc.ts:172-174`: Reads `currentDockSide` variable in `window.ts:43`. | **Pure Application State** (only lived in main process because window docking logic was in `window.ts`). | Read `window_mgr.read(cx).dock_side` directly. |
| `'get-autocomplete-data'` | `getAutocompleteData()` (`preload.ts:50`) | `void` | `AutocompleteConfig`: `{ tags: string[]; projects: string[]; mentions: string[]; expansions: Record<string, string> }` | `SpotlightInput` mount (`app.tsx:120`) & `MainApp` mount (`app.tsx:420`) | `ipc.ts:176-178` -> `loadAutocompleteConfig()` (`src/main/autocomplete.ts:105`): Ensures, reads, parses, and sanitizes `<userData>/autocomplete.json`. | **Native File System I/O (`fs.readFileSync`) + Pure Sanitization** | Loaded once at startup into `Entity<ConfigStore>`; both views read `config_store.read(cx).autocomplete`. |
| `'open-autocomplete-config'` | `openAutocompleteConfig()` (`preload.ts:51`) | `void` | `void` | `MainApp.executeCommand` on `/tags` or `/autocomplete` (`app.tsx:759`) | `ipc.ts:180-182` -> `openAutocompleteConfigFile()` (`src/main/autocomplete.ts:123`): Ensures file exists and calls `shell.openPath(filePath)`. | **Native OS Shell Call (`shell.openPath`)** | `open::that(&config_store.read(cx).autocomplete_path)` on background executor. |
| `'get-config'` | `getConfig()` (`preload.ts:41`) | `void` | `AppConfig`: `{ globalHotkey: string; theme?: ThemeMode }` | Declared in `preload.ts:41` and `app.tsx:40`, **never called in renderer**. | `ipc.ts:184-186` -> `loadConfig()` (`src/main/config.ts:60`): Reads `<userData>/config.json`. | **Native File System I/O (`fs.readFileSync`)** (Unused IPC channel). | Eliminated; `Entity<ConfigStore>` holds `AppConfig` in memory. |
| `'open-config'` | `openConfig()` (`preload.ts:42`) | `void` | `void` | `MainApp.executeCommand` on `/config` (`app.tsx:752`) | `ipc.ts:188-190` -> `openConfigFile()` (`src/main/config.ts:106`): Ensures file exists and calls `shell.openPath(filePath)`. | **Native OS Shell Call (`shell.openPath`)** | `open::that(&config_store.read(cx).config_path)` on background executor. |
| `'get-hotkey-string'` | `getHotkeyString()` (`preload.ts:40`) | `void` | `string` (e.g. `'⌥Space'` or `'Ctrl+Space'`) | `MainApp` mount `useEffect` (`app.tsx:407`) | `ipc.ts:192-197`: Resolves active/configured hotkey and runs `formatHotkeyForDisplay()` (`config.ts:115`). | **In-Memory State + Pure String Formatting** | Read `config_store.read(cx).formatted_hotkey` directly. |
| `'get-theme'` | `getTheme()` (`preload.ts:59`) | `void` | `{ theme: 'system' \| 'light' \| 'dark'; isDark: boolean }` | `renderer.ts:35`, `SpotlightInput` mount (`app.tsx:133`), `MainApp` mount (`app.tsx:388`) | `ipc.ts:199-204`: Reads `nativeTheme.themeSource` and `nativeTheme.shouldUseDarkColors`. | **Native OS Theme State Query** | Read `cx.global::<ActiveTheme>()` or `window.appearance()`. |
| `'set-theme'` | `setTheme(theme)` (`preload.ts:60`) | `theme: 'system' \| 'light' \| 'dark'` | `{ theme: 'system' \| 'light' \| 'dark'; isDark: boolean }` | `MainApp.executeCommand` on `/theme`, `/dark`, `/light` (`app.tsx:781`) | `ipc.ts:206-216`: Sets `nativeTheme.themeSource`, writes `config.json` via `saveConfig({ theme })`, broadcasts `'theme-changed'`, returns `{ theme, isDark }`. | **Native OS Theme Control + Disk Write (`fs.writeFileSync`)** | `config_store.update(cx, \|c, cx\| c.set_theme(theme, cx))` updates `Global<ActiveTheme>`, saves `config.json`, and calls `cx.notify()`. |

---

### 1.3 Main -> Renderer: Push Events & Broadcasts (`webContents.send` -> `ipcRenderer.on`)

| Channel Name | Preload Subscription (`window.api.*`) | Payload Schema (TS) | Target Window(s) | Main Process Trigger | Renderer Handler (`src/renderer/app.tsx`) | Native / OS Operation vs. Pure State | GPUI Port Replacement |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `'task-added'` | `onTaskAdded(cb)` (`preload.ts:17-23`) | `text: string` | `mainWindow` | `sendTaskAdded` (`ipc.ts:110, 139`) upon receiving `'submit-task'` from Spotlight | `MainApp` (`app.tsx:853`): Calls `executeCommand(rawText)`. *(Note: even slash commands like `/done` or `/dock` typed into Spotlight take this path).* | **Pure Renderer State Relay** | Eliminated: `SpotlightView` invokes `TaskStore::execute_command` directly. |
| `'spotlight-shown'` | `onSpotlightShown(cb)` (`preload.ts:24-30`) | `void` | `spotlightWindow` | `toggleSpotlightWindow()` (`window.ts:388`) when global hotkey opens Spotlight | `SpotlightInput` (`app.tsx:158`): Clears `value = ''`, `dismissed = false`, `selectedIndex = 0`, `caretPos = 0`, and focuses `<input>`. | **Pure UI State Reset + Focus** | `SpotlightView::on_shown(window, cx)` resets `CommandInputView` and calls `window.focus(&input_focus_handle)`. |
| `'always-on-top-changed'` | `onAlwaysOnTopChanged(cb)` (`preload.ts:32-38`) | `state: boolean` | `mainWindow` | `ipc.ts:163`, `window.ts:223` (`dockMainWindow`), `window.ts:259` (`floatMainWindow`) | `MainApp` (`app.tsx:375`): Updates `alwaysOnTop` state to toggle thumbtack icon badge in header. | **Window State -> UI Sync** | `MainSidebarView` observes `Entity<WindowManager>` via `cx.observe()`. |
| `'dock-changed'` | `onDockChanged(cb)` (`preload.ts:10-16`) | `side: 'left' \| 'right' \| null` | `mainWindow` | `window.ts:226` (`dockMainWindow`), `window.ts:262` (`floatMainWindow`) | `MainApp` (`app.tsx:382`): Updates `dockSide` state to toggle `.is-docked` class on `.sidebar-header`. | **Window State -> UI Sync** | `MainSidebarView` observes `Entity<WindowManager>` via `cx.observe()`. |
| `'hotkey-config-changed'` | `onHotkeyConfigChanged(cb)` (`preload.ts:43-49`) | `{ newHotkey: string; activeHotkey: string }` | `mainWindow` | `startWatchingConfig` callback (`src/main/index.ts:180`) when `newHotkey !== activeHotkey` | `MainApp` (`app.tsx:411`): Sets `restartRequired = true` to display info banner. | **File Watcher Event -> UI State** | `ConfigStore` dynamically re-registers the hotkey via `global-hotkey` crate at runtime (or sets `restart_required` on `ConfigStore` and calls `cx.notify()`). |
| `'autocomplete-updated'` | `onAutocompleteUpdated(cb)` (`preload.ts:52-58`) | `AutocompleteConfig` | `[mainWindow, spotlightWindow]` | `startWatchingAutocompleteConfig` (`src/main/index.ts:162`, `src/main/autocomplete.ts:161`) | `SpotlightInput` (`app.tsx:124`) & `MainApp` (`app.tsx:424`): Replaces `autocompleteConfig` state. | **File Watcher Event -> Shared Config State** | `ConfigStore` reloads `autocomplete.json` and calls `cx.notify()`; both views observe `ConfigStore`. |
| `'theme-changed'` | `onThemeChanged(cb)` (`preload.ts:61-67`) | `{ theme: ThemeMode; isDark: boolean }` | `[mainWindow, spotlightWindow]` | `nativeTheme.on('updated')` (`index.ts:149`), config watcher (`index.ts:189`), `set-theme` IPC (`ipc.ts:210`) | `SpotlightInput` (`app.tsx:139`) & `MainApp` (`app.tsx:395`): Sets `data-bs-theme` attribute and `isDark` state. | **OS Theme Event / Config Watcher -> UI Theme** | `cx.observe_window_appearance()` + `ConfigStore` updating `Global<ActiveTheme>` and calling `cx.refresh_windows()`. |

---

## 2. Native Integrations

### 2.1 Complete Inventory & Rust/GPUI Mapping

| Native Integration | Current Electron Implementation | Source Locations | Target Approach: Standard Rust Crates vs. GPUI Built-in APIs | Recommended Crates / GPUI APIs |
| :--- | :--- | :--- | :--- | :--- |
| **App Data & Config Paths** | `app.getPath('userData')`, `app.getPath('appData')`, `path.join` | `src/main/index.ts:46-48`<br>`src/main/config.ts:26-29, 40`<br>`src/main/autocomplete.ts:24-27, 38` | **Standard Rust Crate** | `dirs` (or `directories`): `dirs::config_dir()` / `dirs::data_dir()` -> `<AppData>/TaskConveyor` (with legacy migration check from `<AppData>/task-conveyor`). |
| **JSON File Read/Write (`config.json`, `autocomplete.json`)** | `fs.existsSync`, `fs.mkdirSync`, `fs.readFileSync`, `fs.writeFileSync`, `fs.cpSync`, `fs.copyFileSync` | `src/main/index.ts:50-63`<br>`src/main/config.ts:31-104`<br>`src/main/autocomplete.ts:29-121` | **Standard Rust Crates + GPUI Background Executor** | `std::fs`, `serde`, `serde_json` for strongly-typed serialization with `#[serde(default)]`. Use `cx.background_executor().spawn(...)` for non-blocking writes. |
| **Task Queue Persistence (`localStorage`)** | `localStorage.getItem('taskconveyor-tasks')` (fallback `'task-conveyor-tasks'`), `localStorage.setItem(...)` | `src/renderer/app.tsx:325-332, 368-370` | **Standard Rust Crates + GPUI Background Executor** | Replace browser `localStorage` with `<userData>/tasks.json` using `serde_json` + atomic write (`tempfile` or `.tmp` rename) on `cx.background_executor()`. |
| **Config File Watchers** | `fs.watch(filePath)` + `setTimeout` debounce (200ms for `config.json`, 150ms for `autocomplete.json`) | `src/main/config.ts:146-188`<br>`src/main/autocomplete.ts:132-185` | **Standard Rust Crate + GPUI Async Spawn** | `notify` (`RecommendedWatcher` watching the parent `<userData>` directory so atomic editor renames don't break the watch descriptor) + `smol::channel` + `cx.background_executor().timer(Duration::from_millis(150))` for debouncing. |
| **Open File in Default Editor (`/config`, `/tags`)** | `await shell.openPath(filePath)` | `src/main/config.ts:106-113`<br>`src/main/autocomplete.ts:123-130` | **Standard Rust Crate** *(Preferred over `cx.open_url` for local files)* | `open::that(&file_path)` (from the `open` crate). While GPUI has `cx.open_url()`, `open::that` handles local filesystem paths reliably via `open` (macOS), `ShellExecuteW` (Windows), and `xdg-open` (Linux). |
| **System Tray Icon & Context Menu** | `new Tray(trayIcon)`, `tray.setToolTip('TaskConveyor')`, `Menu.buildFromTemplate([{ label: 'Exit', click: () => app.quit() }])`, `tray.on('click', restoreMainWindow)`, `trayIcon.setTemplateImage(false)` | `src/main/index.ts:81-113, 209-212` | **Standard Rust Crates** *(GPUI does not have a built-in system tray API)* | `tray-icon` + `muda` + `image` crate (resize bundled icon to 16x16 RGBA, `with_icon_as_template(false)`). Poll/bridge `TrayIconEvent::receiver()` and `MenuEvent::receiver()` into GPUI via `cx.spawn()`. |
| **macOS Dock Icon** | `app.dock.setIcon(appIcon)` | `src/main/index.ts:154-156` | **GPUI Built-in / Bundler** | Handled natively by macOS `.app` bundle `Info.plist` (`CFBundleIconFile` -> `assets/icon.icns`), or at runtime in dev mode via `objc2-app-kit` (`NSApplication::setApplicationIconImage`). |
| **Global Shortcut (`Option+Space` / `Ctrl+Space`)** | `globalShortcut.register(accelerator, () => toggleSpotlightWindow())`, `globalShortcut.unregisterAll()` | `src/main/index.ts:115-124, 166-175, 208` | **Standard Rust Crate** *(GPUI's `cx.bind_keys` only handles focused-app shortcuts, not OS-wide global hotkeys)* | `global-hotkey` crate (`GlobalHotKeyManager`). Bridge `GlobalHotKeyEvent::receiver()` via an async channel to `cx.spawn()` on the foreground thread to call `window_mgr.update(cx, \|w, cx\| w.toggle_spotlight(cx))`. |
| **Native Error Dialogs** | `dialog.showErrorBox(title, content)` (on single-instance conflict and hotkey registration failure) | `src/main/index.ts:70-74, 171-174` | **Hybrid: `rfd` Crate (pre-window) & GPUI `window.prompt()` (in-window)** | - Pre-window startup (single-instance lock failure): `rfd::MessageDialog::new().set_level(MessageLevel::Error)...show()`.<br>- After window creation (hotkey error): GPUI's built-in `window.prompt(PromptLevel::Critical, ...)` or `rfd::MessageDialog`. |
| **Single-Instance Lock & Second-Instance Restore** | `app.requestSingleInstanceLock()`, `app.on('second-instance', () => restoreMainWindow())` | `src/main/index.ts:68-76, 126-129` | **Standard Rust Crates** | `single-instance` crate for lock acquisition + `interprocess` (`LocalSocketListener` / `LocalSocketStream`) so a second launched process can send a `"restore"` ping to the primary process before exiting. |
| **Clipboard (Copy/Cut/Paste in Input)** | Implicitly provided by Chromium `<input type="text">` | `src/renderer/app.tsx:282-292, 968-978` | **GPUI Built-in Platform APIs** | `cx.write_to_clipboard(ClipboardItem::new_string(text))` and `cx.read_from_clipboard()` inside the custom GPUI `CommandInputView` keybinding actions (`Copy`, `Cut`, `Paste`). |
| **OS Dark/Light Theme Detection** | `nativeTheme.themeSource`, `nativeTheme.shouldUseDarkColors`, `nativeTheme.on('updated', ...)` | `src/main/index.ts:147-151, 186-190`<br>`src/main/ipc.ts:80-88, 199-216` | **GPUI Built-in Platform APIs** | `window.appearance()` (`WindowAppearance::Dark \| VibrantDark \| Light \| VibrantLight`) and `cx.observe_window_appearance(window, \|view, window, cx\| ...)`. |
| **Display & Work Area Queries** | `screen.getPrimaryDisplay()`, `screen.getDisplayMatching(bounds)`, `display.workArea`, `display.scaleFactor` | `src/main/window.ts:158-159, 211, 245-246, 291-292` | **GPUI Built-in Platform APIs** | `cx.primary_display()`, `cx.displays()`, `PlatformDisplay::bounds()`, `PlatformDisplay::visible_bounds()` (work area excluding taskbar/menu bar). |
| **Win32 AppBar Desktop Space Reservation** | `koffi` loading `shell32.dll` -> `SHAppBarMessage` (`ABM_NEW`, `ABM_QUERYPOS`, `ABM_SETPOS`, `ABM_REMOVE`) + `win.getNativeWindowHandle()` + `win.hookWindowMessage(1124, ...)` (`ABN_POSCHANGED`) | `src/main/window.ts:15-105, 162-209, 235-240, 265-283, 317-326` | **Standard Rust Crates (`windows-sys` + `raw-window-handle`)** | Extract `HWND` from GPUI `Window` via `raw_window_handle::HasWindowHandle`, then call `windows_sys::Win32::UI::Shell::SHAppBarMessage` directly. Use `SetWindowSubclass` (`comctl32`) to listen for `MY_APPBAR_MSG_ID` (`ABN_POSCHANGED`). |
| **Frameless Window, PopUp / Spotlight Blur, & Dragging** | `frame: false`, `skipTaskbar: true`, `alwaysOnTop: true`, `spotlightWindow.on('blur', () => spotlightWindow.hide())`, CSS `-webkit-app-region: drag` | `src/main/window.ts:297-311, 344-367`<br>`src/renderer/index.css:37-45` | **GPUI Built-in Platform APIs** | - `WindowOptions { titlebar: None, kind: WindowKind::PopUp, .. }` for Spotlight/always-on-top/skip-taskbar.<br>- `cx.observe_window_activation(window, ...)` to hide Spotlight when ` !window.is_window_active()`.<br>- `window.start_window_move()` on header `on_mouse_down` when `!is_docked`. |
| **Child Processes** | None (`child_process` is never used; only `shell.openPath` launches an external editor) | N/A | **None required** (beyond `open::that`) | N/A |

---

### 2.2 Deep Dive: Win32 AppBar FFI Port (`koffi` -> `windows-sys`)

In `src/main/window.ts:45-105`, the Electron app uses `koffi` at runtime to define `RECT` and `APPBARDATA` structs and call `SHAppBarMessage` in `shell32.dll`. In Rust, this becomes a zero-overhead compile-time `#![cfg(target_os = "windows")]` module:

```rust
#[cfg(target_os = "windows")]
mod win32_appbar {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::UI::Shell::{
        SHAppBarMessage, ABE_LEFT, ABE_RIGHT, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS,
        ABN_POSCHANGED, APPBARDATA,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::WM_USER;

    pub const MY_APPBAR_MSG_ID: u32 = 1124; // Matches window.ts:21

    pub fn call_appbar(msg: u32, hwnd: HWND, edge: u32, rc: RECT) -> (usize, RECT) {
        let mut abd = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: hwnd,
            uCallbackMessage: MY_APPBAR_MSG_ID,
            uEdge: edge,
            rc,
            lParam: 0,
        };
        let res = unsafe { SHAppBarMessage(msg, &mut abd) };
        (res, abd.rc)
    }
}
```

### 2.3 Deep Dive: Accelerator Parsing (`formatHotkeyForDisplay` + `global-hotkey`)

`src/main/config.ts:115-144` formats Electron accelerator strings (e.g., `Option+Space` -> `⌥Space` on macOS, `Ctrl+Space` on Windows/Linux) and `src/main/index.ts:115-124` passes them to `globalShortcut.register`.
- Keep a direct port of `format_hotkey_for_display(accelerator: &str) -> String` in Rust for the UI badge (`app.tsx:918, 983`).
- Normalize Electron-style modifier aliases (`CommandOrControl` / `CmdOrCtrl` -> `Super` on macOS, `Control` on Windows/Linux; `Option` -> `Alt`) before passing the string to `global_hotkey::hotkey::HotKey::from_str(...)`.

---

## 3. DOM/CSS Dependencies & Layout Translation

### 3.1 Layout Patterns Requiring Rethinking in GPUI (Taffy)

GPUI uses **Taffy** for Flexbox layout plus custom GPU shaders for quads, shadows, paths, and text shaping. Below are the specific DOM/CSS patterns in `src/renderer/index.css` and `src/renderer/app.tsx` that do not translate 1:1 and require rethinking:

#### 1. Input Ghost Text Mirror Overlay (`index.css:390-443`, `app.tsx:277-293, 963-979`)
- **Current DOM/CSS Pattern**:
  - Wraps `<input>` inside `.input-autocomplete-wrapper` (`position: relative; display: flex; align-items: center`).
  - Places a sibling `.ghost-text-mirror` (`position: absolute; inset: 0; pointer-events: none; white-space: pre; overflow: hidden`) directly on top of the `<input>`.
  - Inside `.ghost-text-mirror`, renders `<span class="invisible-typed">{value.slice(0, caretPos)}</span>` (`visibility: hidden`) followed by `<span class="ghost-suffix">{ghostSuffix}</span>` so the browser's font shaper advances past the typed characters and renders the gray completion suffix right at the caret.
- **Why It Needs Rethinking in GPUI**:
  - GPUI has no native `<input>` DOM element; text inputs are custom GPUI views/elements built with GPUI's `TextSystem` (`window.text_system().shape_line(...)`) and `EntityInputHandler`.
  - Furthermore, GPUI does not have `visibility: hidden` inline text spans inside an HTML `<input>`.
- **GPUI Solution**:
  - Inside our custom `CommandInputElement` (implementing `gpui::Element`), shape the single line combining `typed_text` + `ghost_suffix` (remembering that `getSuggestions` in `src/renderer/autocomplete.ts:134` only populates `ghostSuffix` when `caretPos === endIndex`).
  - Pass two `TextRun`s to `window.text_system().shape_line(full_text, font_size, &[typed_run, ghost_run])`:
    1. `typed_run`: `len: text.len()`, `color: theme.text_primary`
    2. `ghost_run`: `len: ghost_suffix.len()`, `color: theme.ghost_suffix_color`
  - Paint the caret quad at `shaped_line.x_for_index(caret_pos)`. This completely eliminates the twin-overlay alignment fragility while guaranteeing pixel-exact kerning between the typed prefix and ghost suffix.

#### 2. Repeating Conveyor-Belt SVG Background (`index.css:18-31`)
- **Current DOM/CSS Pattern**:
  - `.sidebar-container` uses an inline data-URI SVG with `viewBox='0 0 100 96'`, `preserveAspectRatio='none'`, `background-size: 100% 96px`, and `background-repeat: repeat-y`:
    ```xml
    <svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 96' preserveAspectRatio='none'>
      <path d='M0 68 L50 28 L100 68' fill='none' stroke='rgba(0, 0, 0, 0.04)' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'/>
      <path d='M0 70 L50 30 L100 70' fill='none' stroke='rgba(255, 255, 255, 0.65)' stroke-width='1.5' stroke-linecap='round' stroke-linejoin='round'/>
    </svg>
    ```
- **Why It Needs Rethinking in GPUI**:
  - GPUI's `svg()` element renders single static SVG assets from the `AssetSource` with uniform scaling; GPUI does not support CSS `background-repeat: repeat-y` or non-uniform `preserveAspectRatio="none"` stretching on SVG backgrounds.
- **GPUI Solution**:
  - Use GPUI's `canvas(...)` element positioned `.absolute().inset_0()` behind the sidebar content!
  - In the `canvas` paint closure, loop `tile_y` in `px(96.0)` increments from `0.0` to `bounds.size.height` and stroke the two 3-point chevron paths (`(left, tile_y + 68) -> (center_x, tile_y + 28) -> (right, tile_y + 68)` and `+2px` offset) using `gpui::PathBuilder::stroke(px(2.0))`:
    ```rust
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _cx| {
            let mut y = bounds.origin.y;
            let mid_x = bounds.origin.x + bounds.size.width * 0.5;
            let right_x = bounds.origin.x + bounds.size.width;
            while y < bounds.origin.y + bounds.size.height {
                for (offset_y, width, color) in [
                    (px(68.0), px(2.0), shadow_stroke),
                    (px(70.0), px(1.5), highlight_stroke),
                ] {
                    let mut builder = PathBuilder::stroke(width);
                    builder.move_to(point(bounds.origin.x, y + offset_y));
                    builder.line_to(point(mid_x, y + offset_y - px(40.0)));
                    builder.line_to(point(right_x, y + offset_y));
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, color);
                    }
                }
                y += px(96.0);
            }
        },
    )
    .absolute()
    .inset_0()
    ```

#### 3. Inline Tag/Project/Mention Badges in Wrapping Task Text (`app.tsx:57-91`, `index.css:352-372`)
- **Current DOM/CSS Pattern**:
  - `renderTaskTextWithTags(text)` splits task strings on `/(?<=^|\s)([#@$][\w-]+)/g` and wraps each `#tag`, `$project`, or `@mention` in `<span style={{ '--tag-hue': hue }} className="task-tag-badge">`.
  - `.task-tag-badge` uses `display: inline-block; padding: 0.1rem 0.35rem; border-radius: 4px; font-size: 0.85em; font-weight: bold` and deterministic HSL colors derived from `hashCode(tag)`:
    - Light mode (`index.css:354-355`): `bg = hsl(hue, 75%, 90%)`, `fg = hsl(hue, 75%, 25%)`
    - Dark mode (`index.css:365-366`): `bg = hsl(hue, 45%, 24%)`, `fg = hsl(hue, 85%, 85%)`
    - Inside `.current-task-card` (`index.css:369-372`): always uses light-mode high-contrast badge (`bg = hsl(hue, 75%, 90%)`, `fg = hsl(hue, 75%, 20%)`).
- **Why It Needs Rethinking in GPUI**:
  - Taffy does not support CSS inline flow (`display: inline` mixed with `display: inline-block` pill boxes that wrap mid-sentence).
- **GPUI Solution (Two Viable Options)**:
  - **Option A (Recommended for exact pill padding + border-radius): Word-Token `flex_wrap` Row**:
    - Split the task text into whitespace-separated tokens and render inside `div().flex().flex_row().flex_wrap().items_baseline().gap_x(rems(0.28)).gap_y(rems(0.15))`.
    - Normal words render as `div().child(word)`, while `[#@$][\w-]+` tokens render as `div().px(rems(0.35)).py(rems(0.05)).rounded(px(4.0)).bg(hsla(hue / 360.0, s_bg, l_bg, 1.0)).text_color(hsla(hue / 360.0, s_fg, l_fg, 1.0)).font_weight(FontWeight::BOLD).text_size(rems(0.85)).child(token)`.
  - **Option B (`StyledText::with_highlights`)**:
    - Pass the full string to `StyledText::new(text).with_highlights(highlights)` where each `[#@$][\w-]+` byte range has `HighlightStyle { color: Some(fg), background_color: Some(bg), font_weight: Some(FontWeight::BOLD), .. }`. This supports arbitrary multi-line character wrapping (`text-break`), though `HighlightStyle` background rectangles do not have horizontal pill padding.

#### 4. Autocomplete Popover Positioning & Scroll (`index.css:445-594`, `AutocompletePopover.tsx:33-71`)
- **Current DOM/CSS Pattern**:
  - `.sidebar-footer` has `position: relative`.
  - `.autocomplete-popover` has `position: absolute; left: 0; right: 0; bottom: calc(100% + 6px); z-index: 1050; max-height: 220px; overflow-y: auto`.
  - Renders up to 8 items (`matches.slice(0, 8)`), so in practice all 8 items fit within ~220px without needing complex nested scrolling.
- **GPUI Solution**:
  - Wrap the popover in `gpui::deferred(...)` with `.with_priority(1)` inside the `position: relative` footer container:
    ```rust
    deferred(
        div()
            .id("autocomplete-popover")
            .occlude()
            .absolute()
            .left_3()
            .right_3()
            .bottom( DefiniteLength::Fraction(1.0) ) // or offset above footer input
            .mb(px(6.0))
            .max_h(px(220.0))
            .overflow_y_scroll()
            // ...
    )
    ```
  - Use `on_mouse_down(MouseButton::Left, ...)` (matching `onMouseDown` in `AutocompletePopover.tsx:54-57`) so clicking a suggestion selects it without blurring the text input.

#### 5. Urgent Task Card Pulsing Animation & Gradients (`index.css:247-293`)
- **Current DOM/CSS Pattern**:
  - `.current-task-card` uses `background: linear-gradient(135deg, #0d6efd, #0b5ed7)` (blue), `.break-task` uses `linear-gradient(135deg, #198754, #157347)` (green), and `.urgent-task` uses `linear-gradient(135deg, #dc3545, #bb2d3b)` (red).
  - `.urgent-task` applies `animation: pulse 2s infinite` oscillating `box-shadow` blur from `15px` (`alpha: 0.35`) to `25px` (`alpha: 0.60`).
  - `.current-task-card:hover` applies `transform: translateY(-2px)` and elevated `box-shadow`.
- **GPUI Solution**:
  - **Gradients**: Use `gpui::linear_gradient(135.0, linear_color_stop(rgb(0x0d6efd), 0.0), linear_color_stop(rgb(0x0b5ed7), 1.0))` directly on `.bg(...)`.
  - **Pulsing Shadow**: Attach `.with_animation("urgent-pulse", Animation::new(Duration::from_secs(2)).repeat().with_easing(pulsating_between(0.0, 1.0)), |div, delta| { ... })` when `current_task.important` is `true`, interpolating `BoxShadow { blur_radius: px(15.0 + 10.0 * delta), color: rgba(0xdc354500 | alpha), .. }`.

#### 6. Scroll Regions (`index.css:62-88`)
- **Current DOM/CSS Pattern**:
  - Single main scroll container: `.sidebar-content` (`flex: 1; overflow-y: auto; padding: 1rem`) between fixed `.sidebar-header` and `.sidebar-footer`.
- **GPUI Solution**:
  - `div().id("sidebar-content").flex_1().overflow_y_scroll().p_4()` inside the root `div().flex().flex_col().size_full().overflow_hidden()`. No nested scroll containers exist in the task view.

---

### 3.2 Third-Party Web Library Audit (`package.json`)

| npm Package | Version | Actual Usage in Codebase | GPUI / Rust Replacement Strategy | Complexity |
| :--- | :--- | :--- | :--- | :--- |
| `bootstrap` | `^5.3.8` | Imported in `src/renderer/renderer.ts:29-30`. Used **exclusively** for CSS utility classes (`d-flex`, `gap-2`, `fw-bold`, `small`, `text-muted`, `rounded-3`, etc.), `.alert`, `.badge`, `.form-control-sm`, and `[data-bs-theme="dark"\|"light"]` CSS variable scoping. **Zero Bootstrap JS widgets are used.** | Replace utility classes with GPUI's built-in Tailwind-like `Styled` trait methods (`.flex()`, `.gap_2()`, `.font_weight(FontWeight::BOLD)`, `.text_sm()`, `.rounded_lg()`, etc.) and a `ThemePalette` struct. | **Trivial** |
| `@popperjs/core` | `^2.11.8` | Transitive peer dependency of `bootstrap.bundle.min.js`. **Completely unused** in application code (the autocomplete popover is positioned with static CSS `.autocomplete-popover-above`, not Popper). | **Drop completely.** | **None** |
| `@fortawesome/react-fontawesome` + `@fortawesome/free-solid-svg-icons` | `^3.5.0` / `^7.3.1` | Only **2 icons** are rendered in the entire app:<br>1. `faTerminal` (`app.tsx:276` in Spotlight input)<br>2. `faThumbtack` (`app.tsx:882` in Sidebar header when pinned)<br>*(Note: `free-brands-svg-icons` and `free-regular-svg-icons` in `package.json` are 100% unused).* | Export `terminal.svg` and `thumbtack.svg` into `assets/icons/`, embed via `rust-embed` (`AssetSource`), and render with GPUI's `svg().path("icons/terminal.svg").size(rems(1.0)).text_color(...)`. | **Trivial** |
| `react` + `react-dom` | `^19.2.8` | Renders `<App />` (`<SpotlightInput />` vs. `<MainApp />`) and `<AutocompletePopover />` using `useState`, `useEffect`, `useRef`. | Replace with GPUI `Entity<T>` views implementing `gpui::Render`. | **Standard Port** |
| `koffi` | `^3.1.6` | C FFI bindings for `shell32.dll` (`SHAppBarMessage`) in `src/main/window.ts:50`. | Replace with `windows-sys` + `raw-window-handle` crates on `#![cfg(target_os = "windows")]`. | **Low** |
| `electron-squirrel-startup` | `^1.0.1` | Windows Squirrel installer shortcut handler in `src/main/index.ts:38`. | Drop (or replace with standard Rust installer tooling like `cargo-packager` / `wix` / `nsis`). | **None** |

> **Note on Rich Web Dependencies**: The codebase uses **no** charting libraries, **no** Markdown/HTML renderers, and **no** rich-text editors (ProseMirror/Monaco/Slate). The only custom interactive widget is the single-line command input with prefix/tag autocomplete (`#`, `$`, `@`), space-triggered text expansions (`src/renderer/expansions.ts`), and auto-capitalization (`app.tsx:93-103`).

---

## 4. State & Reactivity Architecture

### 4.1 Current State Management Analysis

- **External State Libraries**: **None** (no Redux, Zustand, MobX, Recoil, or React Context).
- **Current State Topology & Pain Points**:
  1. **Split Process State**: Window metadata (`currentDockSide`, `isAppBarRegistered`, `activeHotkey`) lives in Main process module globals (`src/main/window.ts:40-43`, `src/main/index.ts:78-79`), while task queue state (`tasks`, `history`) lives in `<MainApp />` React state backed by `window.localStorage` (`src/renderer/app.tsx:325-370`).
  2. **Duplicated Renderer State**: Both `<SpotlightInput />` and `<MainApp />` independently fetch and subscribe to `autocompleteConfig` (`app.tsx:107-129` and `app.tsx:337-342, 418-429`) and `theme` (`app.tsx:131-146` and `app.tsx:351, 386-403`).
  3. **Duplicated Input & Autocomplete Logic**: `<SpotlightInput />` (`app.tsx:174-261`) and `<MainApp />` (`app.tsx:439-519`) duplicate the exact same state (`value`/`inputValue`, `selectedIndex`, `dismissed`, `caretPos`) and keyboard handlers (`expandOnSpace`, `Tab`, `ArrowDown`, `ArrowUp`, `Escape`, `autoCapitalizeFirstLetter`).
  4. **Stale Closure Workarounds**: `<MainApp />` maintains manual `useRef` mirrors (`tasksRef`, `historyRef`, `isDarkRef` at `app.tsx:354-366`) because `executeCommand` is captured once inside the `window.api.onTaskAdded` `useEffect(..., [])` closure (`app.tsx:852-857`).

---

### 4.2 Proposed GPUI Entity & Reactivity Model (`Entity<T>` / `Model<T>` + `cx.notify()`)

*(Note: In current GPUI versions, `Model<T>` and `View<T>` were unified into `Entity<T>` with `Context<T>` calling `cx.notify()`. The design below applies identically to both `Entity<T>` and `Model<T>`.)*

#### Entity Graph & Ownership

```
App (GPUI Application Context)
 ├── Global: ActiveTheme (ThemePalette + ThemeMode + is_dark)
 │
 ├── Entity<ConfigStore> (Singleton Model)
 │    ├── app_config: AppConfig (config.json)
 │    ├── autocomplete: AutocompleteConfig (autocomplete.json)
 │    ├── active_hotkey: Option<String>
 │    ├── formatted_hotkey: String
 │    ├── restart_required: bool
 │    └── _watcher_task: Task<()> (notify file watcher for config.json & autocomplete.json)
 │
 ├── Entity<WindowManager> (Singleton Model)
 │    ├── dock_side: Option<DockSide>
 │    ├── always_on_top: bool
 │    ├── main_window: Option<WindowHandle<MainSidebarView>>
 │    ├── spotlight_window: Option<WindowHandle<SpotlightView>>
 │    └── appbar_registered: bool (Windows only)
 │
 ├── Entity<TaskStore> (Singleton Model)
 │    ├── tasks: Vec<TaskItem> (persisted to tasks.json)
 │    ├── history: Option<Vec<TaskItem>> (1-level undo buffer)
 │    ├── warning: Option<String>
 │    └── _warning_timer: Option<Task<()>> (3s auto-dismiss timer)
 │
 ├── Window 1: Entity<MainSidebarView> (implements Render)
 │    ├── observes: Entity<TaskStore>, Entity<ConfigStore>, Entity<WindowManager>
 │    ├── subscribes: Entity<CommandInputView> (footer_input)
 │    └── child: Entity<CommandInputView> (variant: Footer, renders AutocompletePopover)
 │
 └── Window 2: Entity<SpotlightView> (implements Render)
      ├── observes: Entity<ConfigStore>
      ├── subscribes: Entity<CommandInputView> (spotlight_input)
      └── child: Entity<CommandInputView> (variant: Spotlight, inline ghost text only)
```

---

#### Concrete Rust Entity Definitions & Reactivity Flows

```rust
use gpui::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

// ============================================================================
// 1. Domain Types
// ============================================================================

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskItem {
    pub id: String,
    pub text: String,
    pub completed: bool,
    #[serde(default)]
    pub important: bool,
    #[serde(default)]
    pub is_break: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockSide {
    Left,
    Right,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AutocompleteConfig {
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub projects: Vec<String>,
    #[serde(default)]
    pub mentions: Vec<String>,
    #[serde(default)]
    pub expansions: HashMap<String, String>,
}

// ============================================================================
// 2. Entity<TaskStore>: Centralized Task & Command Execution Model
// ============================================================================

pub struct TaskStore {
    pub tasks: Vec<TaskItem>,
    pub history: Option<Vec<TaskItem>>,
    pub warning: Option<String>,
    warning_timer: Option<Task<()>>,
    storage_path: PathBuf,
    config_store: Entity<ConfigStore>,
    window_manager: Entity<WindowManager>,
}

impl TaskStore {
    /// Replaces MainApp.executeCommand (src/renderer/app.tsx:533-849).
    /// Can be called directly from EITHER MainSidebarView OR SpotlightView!
    pub fn execute_command(&mut self, raw_text: &str, cx: &mut Context<Self>) {
        let text = raw_text.trim();
        if text.is_empty() {
            return;
        }

        // Dispatch slash commands (/done, /break, /move, /remove, /edit, /undo,
        // /important, /clear, /pin, /dock, /float, /config, /tags, /theme, /exit, /help)
        // or task insertion prefixes (!!, !, default append).
        // ...

        self.persist_tasks(cx);
        cx.notify(); // Triggers MainSidebarView re-render automatically
    }

    pub fn set_warning(&mut self, msg: impl Into<String>, cx: &mut Context<Self>) {
        self.warning = Some(msg.into());
        cx.notify();

        // Replaces the 3000ms useEffect timer in app.tsx:432-437
        self.warning_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(3)).await;
            let _ = this.update(cx, |store, cx| {
                store.warning = None;
                cx.notify();
            });
        }));
    }

    fn persist_tasks(&self, cx: &mut Context<Self>) {
        let tasks_snapshot = self.tasks.clone();
        let path = self.storage_path.clone();
        cx.background_executor()
            .spawn(async move {
                if let Ok(json) = serde_json::to_string_pretty(&tasks_snapshot) {
                    let _ = std::fs::write(path, json);
                }
            })
            .detach();
    }
}

// ============================================================================
// 3. Entity<ConfigStore>: Config, Autocomplete, Theme & File Watchers
// ============================================================================

pub struct ConfigStore {
    pub global_hotkey: String,
    pub active_hotkey: Option<String>,
    pub formatted_hotkey: String,
    pub theme_mode: ThemeMode,
    pub is_dark: bool,
    pub autocomplete: AutocompleteConfig,
    pub restart_required: bool,
    pub config_path: PathBuf,
    pub autocomplete_path: PathBuf,
    _watcher_task: Task<()>,
}

impl ConfigStore {
    pub fn set_theme(&mut self, mode: ThemeMode, os_is_dark: bool, cx: &mut Context<Self>) {
        self.theme_mode = mode;
        self.is_dark = match mode {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => os_is_dark,
        };
        cx.set_global(ActiveTheme::from_is_dark(self.is_dark, mode));
        self.save_config_async(cx);
        cx.notify();
    }
}

// ============================================================================
// 4. Entity<CommandInputView>: Reusable Input + Autocomplete + Expansions
// ============================================================================

pub enum CommandInputEvent {
    Submit(String),
    DismissSpotlight,
}

impl EventEmitter<CommandInputEvent> for CommandInputView {}

pub struct CommandInputView {
    pub value: String,
    pub caret_pos: usize,
    pub selected_index: usize,
    pub dismissed: bool,
    pub focus_handle: FocusHandle,
    pub is_spotlight: bool,
    config_store: Entity<ConfigStore>,
}

impl CommandInputView {
    pub fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.value.clear();
        self.caret_pos = 0;
        self.selected_index = 0;
        self.dismissed = false;
        window.focus(&self.focus_handle);
        cx.notify();
    }
}
```

#### How Reactivity Flows in GPUI vs. Electron

| Event / User Action | Old Electron Flow | New GPUI Flow (`Entity<T>` + `cx.notify()`) |
| :--- | :--- | :--- |
| **User submits task/command in Spotlight** | `SpotlightInput` -> `ipcRenderer.send('submit-task', text)` -> `ipcMain` hides Spotlight & calls `mainWindow.webContents.send('task-added', text)` -> `MainApp` `onTaskAdded` reads `tasksRef.current`, mutates React state, writes `localStorage`. | `SpotlightView` receives `CommandInputEvent::Submit(text)` -> calls `task_store.update(cx, \|s, cx\| s.execute_command(&text, cx))` and `window_mgr.update(cx, \|w, cx\| w.hide_spotlight(cx))`. `TaskStore` calls `cx.notify()`, immediately re-rendering `MainSidebarView`. |
| **User edits `autocomplete.json` in external editor** | `fs.watch` in main process -> 150ms `setTimeout` -> `loadAutocompleteConfig()` -> `win.webContents.send('autocomplete-updated', config)` to both windows -> two separate React `setAutocompleteConfig` calls. | `notify` watcher channel in `ConfigStore` -> 150ms timer -> updates `config_store.autocomplete` and calls `cx.notify()`. Both `MainSidebarView` and `SpotlightView` observe `ConfigStore` via `cx.observe(&config_store, ...)` and re-render with the new suggestions. |
| **User runs `/theme dark` or OS appearance changes** | `ipcRenderer.invoke('set-theme', 'dark')` -> `nativeTheme.themeSource = 'dark'` -> `saveConfig` -> `webContents.send('theme-changed')` -> DOM `setAttribute('data-bs-theme', 'dark')`. | `ConfigStore::set_theme` (or `cx.observe_window_appearance`) updates `Global<ActiveTheme>` and calls `cx.notify()`. Views read `cx.global::<ActiveTheme>()` during `render()`. |
| **User runs `/dock left` or `/pin`** | `ipcRenderer.send('dock-window', 'left')` -> `dockMainWindow` in `window.ts` -> `sendAlwaysOnTopChanged` + `sendDockChanged` IPC events back to `MainApp`. | `WindowManager::dock_window` positions the window, updates `self.dock_side = Some(DockSide::Left)` and `self.always_on_top = true`, and calls `cx.notify()`. `MainSidebarView` observes `WindowManager` and updates the header pin badge and drag handler. |

---

## 5. Recommended Cargo Crate Stack & Project Structure

### 5.1 Proposed `Cargo.toml` Dependencies

```toml
[package]
name = "task-conveyor"
version = "1.0.0"
edition = "2021"

[dependencies]
gpui = { git = "https://github.com/zed-industries/zed" } # or crates.io release
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
dirs = "6.0"
notify = "8.0"
smol = "2.0"
open = "5.3"
global-hotkey = "0.6"
tray-icon = "0.19"
muda = "0.15"
image = { version = "0.25", default-features = false, features = ["png", "ico"] }
rust-embed = "8.5"
rfd = "0.15"
single-instance = "0.3"
raw-window-handle = "0.6"
regex = "1.11"

[target.'cfg(target_os = "windows")'.dependencies]
windows-sys = { version = "0.59", features = [
    "Win32_Foundation",
    "Win32_UI_Shell",
    "Win32_UI_WindowsAndMessaging",
] }
```

### 5.2 Source-to-Target File Mapping

| Electron Source File | Lines | Rust + GPUI Target Module | Migration Notes |
| :--- | :---: | :--- | :--- |
| `src/main/index.ts` | 222 | `src/main.rs` | Single-instance lock, legacy userData migration, `Application::new().run(...)`, tray setup (`tray-icon` + `muda`), global hotkey registration (`global-hotkey`). |
| `src/main/ipc.ts` | 217 | *Eliminated* | Replaced by direct in-process `Entity<T>` method calls and `cx.observe()` / `cx.subscribe()`. |
| `src/main/preload.ts` | 68 | *Eliminated* | No renderer process boundary exists in GPUI. |
| `src/main/window.ts` | 391 | `src/window_manager.rs` + `src/win32_appbar.rs` | Manages `WindowHandle<MainSidebarView>` and `WindowHandle<SpotlightView>`, display work-area docking, and Win32 `SHAppBarMessage` FFI. |
| `src/main/config.ts` | 188 | `src/config_store.rs` | `AppConfig` load/save, `format_hotkey_for_display`, `open::that`, and `notify` file watcher. |
| `src/main/autocomplete.ts` | 185 | `src/config_store.rs` | `AutocompleteConfig` load/sanitize (`sanitize_list`, `sanitize_expansions`), `open::that`, and `notify` file watcher. |
| `src/renderer/autocomplete.ts` | 163 | `src/autocomplete.rs` | Pure functions (`detect_token_at_caret`, `get_suggestions`, `apply_completion`). Direct 1:1 port to Rust with unit tests. |
| `src/renderer/expansions.ts` | 112 | `src/expansions.rs` | Pure function (`expand_on_space`). Direct 1:1 port to Rust with unit tests. |
| `src/renderer/AutocompletePopover.tsx` | 72 | `src/views/autocomplete_popover.rs` | Stateless `IntoElement` component rendered inside `deferred(...)` above the footer input. |
| `src/renderer/app.tsx` | 1003 | `src/task_store.rs`<br>`src/views/command_input.rs`<br>`src/views/main_sidebar.rs`<br>`src/views/spotlight.rs` | Splits the monolithic 1,003-line file into a clean `TaskStore` entity, a shared `CommandInputView`, and the two window root views. |
| `src/renderer/index.css` | 594 | `src/theme.rs` | Light/Dark color palettes (`ActiveTheme` Global), tag HSL calculation (`hash_code`), chevron `PathBuilder` canvas, and linear gradients. |
