# TaskConveyor (Rust + GPUI)

[![Build Windows Executable](https://github.com/thealternator89/task-conveyor-rs/actions/workflows/build-windows.yml/badge.svg)](https://github.com/thealternator89/task-conveyor-rs/actions/workflows/build-windows.yml)
[![Rust](https://img.shields.io/badge/Rust-2024_Edition-orange.svg)](https://www.rust-lang.org/)
[![GPUI](https://img.shields.io/badge/GUI-GPUI-blue.svg)](https://github.com/zed-industries/zed)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](./LICENSE)

**TaskConveyor** is a blazing-fast, always-on desktop task manager that docks cleanly to the edge of your screen. Built for developers, power users, and keyboard maximalists, it delivers a **100% keyboard-driven, button-free interface** operated via a docked sidebar or a global Spotlight-style command bar.

Tasks automatically shift up like a conveyor belt as you finish them, complete with demotion guards for urgent items, inline tag autocomplete, and instant text expansions.

> [!NOTE]
> This repository is a high-performance port of the original TypeScript/Electron/React [TaskConveyor](https://github.com/thealternator89/task-conveyor) application to **Rust** and Zed's GPU-accelerated **GPUI** framework.

---

## Why the Rust + GPUI Port?

The original TaskConveyor was built with Electron, React, and TypeScript. While fully featured, it relied on three separate operating system processes (Main process + 2 Chromium renderers) communicating over asynchronous IPC channels, resulting in noticeable memory overhead (~150–300+ MB) and Electron runtime initialization delay.

| Metric / Aspect | Original (Electron + React) | Ported (Rust + GPUI) |
| :--- | :--- | :--- |
| **Architecture** | 3 OS processes (Main + 2 Chromium renderers) | **1 unified native process** |
| **Memory Footprint** | ~150 MB – 300+ MB RAM | **~20 MB – 30 MB RAM** |
| **Cold Startup Time** | ~1.5 – 3.0 seconds | **Sub-second / instant** |
| **Renderer** | Chromium DOM + CSS paint engine | **GPU-accelerated native GPUI rendering** |
| **Cross-Window State** | Asynchronous IPC serialization & bridges | **Zero-copy shared in-memory reactive state** |
| **Windows Docking** | Win32 FFI via `koffi` binding | **Direct Win32 API FFI (`windows-sys`)** |
| **Binary Footprint** | ~150 MB packaged Electron runtime | **Single compact native executable** |

---

## Key Features

- **Docked Sidebar & Floating Modes**:
  - Pinned sidebar docked to the right or left edge of your screen (`/dock [l|r]`).
  - **Native Windows AppBar**: On Windows, registers via Win32 `SHAppBarMessage` to carve out real desktop space so maximized windows do not cover the task conveyor. Automatically repositions when the taskbar or display resolution changes.
  - Can be undocked into a floating, draggable window anytime (`/float`).
- **Global Hotkey & Spotlight Overlay**:
  - Summon a floating command input bar from anywhere across your system:
    - **macOS**: `Option+Space` (`⌥Space`) by default.
    - **Windows / Linux**: `Ctrl+Space` by default.
    - Fully customizable via `config.json`.
  - Floating popup auto-hides on blur, ESC, or after submitting a task/command.
- **100% Keyboard Controlled**:
  - Zero buttons and no mouse required. Add, reorder, finish, edit, undo, and configure tasks purely by typing.
- **Conveyor Belt Workflow & Prefix Shortcuts**:
  - `<text>`: Add task to the **bottom** of the queue.
  - `!<text>`: Insert task as **next up** (index 1, directly beneath the active task).
  - `!!<text>`: Insert task as the **active task** (index 0), shifting previous tasks down.
- **Demotion Block Guard**:
  - Mark an urgent task as `/important` to lock it in the current slot (index 0).
  - Renders with an animated pulsing indicator and actively rejects any commands, prepends, or moves that would displace it until unlocked.
- **Autocomplete for Tags, Projects & Mentions**:
  - Type `#` for tags, `$` for projects, or `@` for mentions.
  - Interactive popup suggestions with instant `[Tab]` autocomplete and inline ghost-text completion.
  - Configured via `autocomplete.json` (accessible anytime via `/tags`).
- **Space-Triggered Text Expansions**:
  - Define custom snippets in `autocomplete.json` (e.g. `"rpr": "review PR"`).
  - Type the shortcut and press `[Space]` to expand. Expansions at the beginning of a line automatically capitalize (e.g. `rpr` expands to `Review PR`).
- **Break Insertion**:
  - Easily take a pause with `/break` or `/done break` to slip a `Break ☕` card into the active slot.
- **Adaptive Dark / Light / System Theming**:
  - Automatically matches your operating system appearance by default, or switch anytime via `/theme dark`, `/theme light`, or `/theme system`.
  - Styled with custom chevron conveyor background graphics drawn directly on GPUI's canvas.
- **Live File Watching**:
  - Edit your `config.json` or `autocomplete.json` in your favorite editor; changes are detected via `notify` file watchers and updated live without restarting.
- **Automatic Legacy Data Migration**:
  - Automatically migrates existing task queues and configuration files from the Electron app directory (`task-conveyor`) to `TaskConveyor` on first launch.
- **System Tray & Single-Instance Lock**:
  - Sits quietly in the system tray with quick restore and quit options.
  - Prevents accidental duplicate instances using native OS single-instance locking.

---

## Keyboard Command Reference

Enter any of the following commands in the Spotlight overlay or the sidebar footer command bar:

| Command | Action | Description |
| :--- | :--- | :--- |
| `<text>` | **Add Task** | Adds a new task to the bottom of the queue. |
| `!<text>` | **Next Task** | Inserts a task at index 1 (directly below the active task). |
| `!!<text>` | **Prepend Task** | Inserts a task at index 0 as active task. *(Blocked if current task is marked important)*. |
| `/d[one] [b[reak]]` | **Complete Task** | Completes active task (index 0). Passing `b` or `break` inserts a break immediately. |
| `/b[reak]` | **Take Break** | Inserts a `Break ☕` card at index 0. *(Blocked if current task is marked important)*. |
| `/m[ove] x y` | **Absolute Move** | Moves task at index `x` to index `y` (0-indexed). *(Blocked if it demotes an important task)*. |
| `/m[ove] x u\|d [y]` | **Relative Move** | Moves task `x` up (`u`) or down (`d`) by `y` spots (default 1). |
| `/r[emove] x` | **Remove Task** | Deletes task at index `x` (0-indexed). |
| `/e[dit] x <text>` | **Edit Task** | Replaces the text of task at index `x`. |
| `/u[ndo]` | **Undo** | Reverts the last list mutation command. |
| `/i[mportant]` | **Toggle Urgent** | Toggles important status on the active task (index 0), locking it against demotion. |
| `/p[in]` | **Toggle Pin** | Toggles always-on-top window pin state. |
| `/dock [l\|r]` | **Dock Sidebar** | Docks sidebar to the `left` (`l`) or `right` (`r`) edge of the primary monitor. |
| `/float` or `/undock`| **Float Window** | Undocks the sidebar into a centered floating window (`400x600`). |
| `/theme [dark\|light\|system]` | **Set Theme** | Sets theme to `dark`, `light`, or `system` (or toggles with bare `/theme`). Aliases: `/dark`, `/light`. |
| `/config` | **Open Config** | Opens `config.json` in your system's default editor. |
| `/tags` or `/autocomplete` | **Open Autocomplete** | Opens `autocomplete.json` in your system's default editor. |
| `/c[lear]` | **Clear All** | Removes all tasks from the queue. |
| `/x`, `/exit`, `/q[uit]` | **Exit App** | Cleanly exits TaskConveyor. |
| `/h[elp]` or `/?` | **Help** | Displays interactive help info in the task list. |

---

## Configuration & Customization

TaskConveyor stores its configuration and tasks in standard OS user directories:

- **macOS**: `~/Library/Application Support/TaskConveyor/`
- **Windows**: `%APPDATA%\TaskConveyor\`
- **Linux**: `~/.config/TaskConveyor/` (or `$XDG_CONFIG_HOME/TaskConveyor/`)

### `config.json`

Run `/config` inside TaskConveyor to open `config.json`:

```json
{
  "globalHotkey": "Option+Space",
  "theme": "system"
}
```

- `globalHotkey`: Any valid hotkey combination (e.g. `Option+Space`, `Ctrl+Space`, `Ctrl+Alt+T`, `Command+Shift+K`).
- `theme`: `"system"`, `"dark"`, or `"light"`.

### `autocomplete.json`

Run `/tags` or `/autocomplete` to open `autocomplete.json`:

```json
{
  "tags": ["bug", "feature", "urgent", "review"],
  "projects": ["taskconveyor", "core", "frontend"],
  "mentions": ["alice", "bob"],
  "expansions": {
    "rpr": "review PR",
    "wip": "work in progress",
    "mtg": "team meeting"
  }
}
```

- **`tags`** (`#`): Triggered by typing `#` and matched case-insensitively with `[Tab]`.
- **`projects`** (`$`): Triggered by typing `$`.
- **`mentions`** (`@`): Triggered by typing `@`.
- **`expansions`**: Key-value shortcuts expanded automatically when pressing `[Space]`.

### `tasks.json`

The active task list automatically syncs to `tasks.json` on any mutation, ensuring tasks survive app restarts.

---

## Architecture

TaskConveyor is structured around a reactive, entity-based architecture hosted on GPUI's cooperative executor:

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                        TaskConveyor (Rust + GPUI)                            │
│                                                                              │
│  ┌─────────────────────┐  ┌─────────────────────┐  ┌──────────────────────┐  │
│  │  Entity<TaskStore>  │  │ Entity<ConfigStore> │  │Entity<WindowManager> │  │
│  │  - Task items       │  │ - config.json       │  │ - dock_side          │  │
│  │  - 1-level history  │  │ - autocomplete.json │  │ - always_on_top      │  │
│  │  - tasks.json sync  │  │ - notify watchers   │  │ - Win32 AppBar state │  │
│  │  - TaskEngine logic │  │ - Global<Theme>     │  │ - Window handles     │  │
│  └──────────▲──────────┘  └──────────▲──────────┘  └──────────▲───────────┘  │
│             │                        │                        │              │
│             └────────────────────────┼────────────────────────┘              │
│                                      │ cx.observe() / cx.notify()            │
│                     ┌────────────────┴────────────────┐                      │
│                     ▼                                 ▼                      │
│     ┌───────────────────────────────┐ ┌───────────────────────────────┐      │
│     │ Window 1: MainSidebarView     │ │ Window 2: SpotlightView       │      │
│     │ - Docked or floating sidebar  │ │ - Popup overlay window        │      │
│     │ - Active/Urgent task card     │ │ - Auto-hides on blur / ESC    │      │
│     │ - Scrollable queue list       │ │ - Prefix badge & hint footer  │      │
│     │ - Autocomplete popover        │ │ - Shared CommandInputView     │      │
│     │ - Shared CommandInputView     │ └───────────────────────────────┘      │
│     └───────────────────────────────┘                                        │
└──────────────────────────────────────────────────────────────────────────────┘
```

- **`TaskEngine` (`src/task_engine.rs`)**: Pure domain state machine governing task mutations, regex command parsing, bounds checking, and demotion locks. Zero UI dependencies with full unit test coverage.
- **`ConfigStore` (`src/config_store.rs`)**: Manages configuration, autocomplete dictionaries, live file watchers (`notify`), and OS appearance synchronization.
- **`WindowManager` (`src/window_manager.rs`)**: Coordinates the main sidebar and Spotlight popup windows, multi-monitor work-area geometry, and Win32 AppBar registration.
- **`win32_appbar` (`src/win32_appbar.rs`)**: Win32 native integration reserving screen edge real estate via `SHAppBarMessage` (`ABM_NEW`, `ABM_SETPOS`, `ABM_REMOVE`).
- **`CommandInputView` (`src/views/command_input.rs`)**: Custom GPUI text input element featuring caret rendering, selection, IME/UTF-8 handling, inline ghost-text completion, and history navigation.

---

## Building & Development

### Prerequisites

- **Rust**: Version 1.85+ (Rust 2024 edition). Install via [rustup](https://rustup.rs/):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **Platform Dependencies**:
  - **macOS**: Xcode Command Line Tools (`xcode-select --install`).
  - **Windows**: Visual Studio 2022 / Build Tools with the **C++ build tools** workload.
  - **Linux**: Standard development libraries for X11 / Wayland and Fontconfig:
    ```bash
    sudo apt-get install build-essential libxkbcommon-dev libwayland-dev libx11-dev libfontconfig1-dev
    ```

### Clone & Run

```bash
# Clone the repository
git clone https://github.com/thealternator89/task-conveyor-rs.git
cd task-conveyor-rs

# Run in debug mode
cargo run

# Run tests
cargo test
```

### Release Build

To produce an optimized native binary:

```bash
cargo build --release
```

The resulting binary will be located at:
- **macOS / Linux**: `target/release/task-conveyor-rs`
- **Windows**: `target/release/task-conveyor-rs.exe`

---

## Origin & Credits

- Ported from the original Electron application: [**TaskConveyor**](https://github.com/thealternator89/task-conveyor) by [Mark Benson](https://github.com/thealternator89).
- Built using the [**GPUI**](https://github.com/zed-industries/zed) UI framework by the Zed Industries team.

---

## License

This project is licensed under the [MIT License](./LICENSE).
