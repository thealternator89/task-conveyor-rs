# TaskConveyor: Phased Rust + GPUI Implementation Plan

This document breaks down the port of **TaskConveyor** from Electron/React/TypeScript to **Rust + GPUI** (specified in [`GPUI_PORT_PLAN.md`](./GPUI_PORT_PLAN.md)) into **6 self-contained phases**.

### Starting Assumption
A target Rust repository has already been initialized with `Cargo.toml`, `gpui`, and a `src/main.rs` entry point that opens a basic "Hello World" GPUI window.

### Phase Design Principles
- **Single-Session Scope**: Each phase is sized (~300–550 lines of Rust) to be comfortably implemented, debugged, and verified in a single coding session.
- **Always Compilable & Runnable**: Every phase ends with a compiling binary (`cargo run`) and a concrete verification checklist (`cargo test` and/or visual/interactive checks).
- **Bottom-Up Foundation, Then Vertical Slices**: Pure domain logic and unit tests come first (Phase 1), followed by GPUI state entities and file I/O (Phase 2), the main sidebar UI (Phase 3), the custom command input and autocomplete popover (Phase 4), the Spotlight popup window + global hotkey (Phase 5), and finally OS window docking, Win32 AppBar FFI, tray, and lifecycle polish (Phase 6).

---

## Phase Overview

| Phase | Title | Target Modules | Primary Deliverable | Verification Method |
| :---: | :--- | :--- | :--- | :--- |
| **1** | **Core Domain Logic: Autocomplete, Expansions & Task Engine** | `src/autocomplete.rs`<br>`src/expansions.rs`<br>`src/task_engine.rs` | 100% of pure task queue mutations, slash command parsing, demotion guards, `#`/`$`/`@` matching, and space expansions. | `cargo test` (pure unit test suite, zero UI dependencies) |
| **2** | **Config Store, Task Persistence, File Watchers & Theme System** | `src/paths.rs`<br>`src/theme.rs`<br>`src/assets.rs`<br>`src/config_store.rs`<br>`src/task_store.rs` | `Entity<ConfigStore>`, `Entity<TaskStore>`, `Global<ActiveTheme>`, `tasks.json` / `config.json` / `autocomplete.json` I/O, `notify` watchers, embedded assets. | `cargo test` + startup logs confirming config/task load & live file-watcher reload |
| **3** | **Main Sidebar View & Task Queue UI** | `src/views/mod.rs`<br>`src/views/tag_text.rs`<br>`src/views/main_sidebar.rs` | Replaces "Hello World" window with the full `400x600` frameless sidebar: header, repeating chevron canvas, active/urgent/break task card, scrollable Up Next list, inline tag badges, and alert banners. | Visual inspection with seeded tasks (`normal`, `important`, `is_break`, `#tags`) in light & dark themes |
| **4** | **Custom Command Input, Inline Ghost Text & Autocomplete Popover** | `src/views/command_input.rs`<br>`src/views/autocomplete_popover.rs` | Custom GPUI text input (`Element` + `EntityInputHandler`), two-run `shape_line` inline ghost suffix, space expansion trigger, and `deferred` popover above the footer. | Full interactive task management & slash commands via the sidebar footer input |
| **5** | **Spotlight Overlay Window & Global Hotkey Integration** | `src/views/spotlight.rs`<br>`src/hotkey.rs` | Frameless `500x72` `WindowKind::PopUp` reusing `CommandInputView`, dynamic prefix badge, bottom hint lip, auto-hide on blur, and OS-wide hotkey toggle (`Option+Space` / `Ctrl+Space`). | Summon Spotlight from any app via global hotkey, submit tasks/commands, verify instant sidebar update |
| **6** | **Window Docking, Win32 AppBar FFI, System Tray & App Lifecycle** | `src/window_manager.rs`<br>`src/win32_appbar.rs`<br>`src/tray.rs`<br>`src/single_instance.rs` | `/dock [l\|r]`, `/float`, `/pin`, Windows `SHAppBarMessage` desktop reservation + `ABN_POSCHANGED`, system tray icon/menu, and single-instance lock. | End-to-end verification of `/dock`, `/float`, `/pin`, tray click/exit, and second-instance guard |

---

## Phase 1: Core Domain Logic — Autocomplete, Expansions & Task Engine

### Goal
Port all pure data structures, string manipulation algorithms, and task queue state transitions into standalone Rust modules with comprehensive unit tests before touching GPUI views.

### Dependencies to Add (`Cargo.toml`)
```toml
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
regex = "1.11"
```

### Files to Create / Modify
1. `src/autocomplete.rs` *(ports `src/renderer/autocomplete.ts:1-163` and sanitization from `src/main/autocomplete.ts:70-103`)*
2. `src/expansions.rs` *(ports `src/renderer/expansions.ts:1-112` and `autoCapitalizeFirstLetter` from `src/renderer/app.tsx:93-103`)*
3. `src/task_engine.rs` *(ports task list state machine and command parser from `src/renderer/app.tsx:521-849`)*
4. `src/main.rs` *(declare `mod autocomplete; mod expansions; mod task_engine;`)*

### Implementation Tasks

#### 1. `src/autocomplete.rs`
- Define types:
  - `AutocompleteConfig { pub tags: Vec<String>, pub projects: Vec<String>, pub mentions: Vec<String>, pub expansions: HashMap<String, String> }` with `#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]` and `#[serde(default)]`.
  - `AutocompleteType` (`Tag`, `Project`, `Mention`), `ActiveToken`, `AutocompleteMatch`, and `AutocompleteResult`.
- Implement config sanitization helpers (ported from `src/main/autocomplete.ts:70-103`):
  - `sanitize_list(items: Vec<String>) -> Vec<String>`: trim whitespace, strip leading `#`, `$`, or `@`, and deduplicate case-insensitively while preserving first-seen canonical casing.
  - `sanitize_expansions(map: HashMap<String, String>) -> HashMap<String, String>`: trim keys and values, drop entries where either is empty.
- Implement `detect_token_at_caret(text: &str, caret_pos: usize) -> Option<ActiveToken>` (`src/renderer/autocomplete.ts:30-77`):
  - Note: Handle UTF-8 byte indices safely (ensure `caret_pos <= text.len()` and `text.is_char_boundary(caret_pos)`).
  - Scan backward from `caret_pos` to find `start_index` (first non-whitespace byte).
  - Handle priority prefixes at index `0`: if `text` starts with `!!` followed by `#`/`$`/`@` and `caret_pos >= 2`, advance `start_index = 2`; if `text` starts with `!` followed by `#`/`$`/`@` and `caret_pos >= 1`, advance `start_index = 1`.
  - Verify character at `start_index` is `#`, `$`, or `@`.
  - Scan forward from `caret_pos` to find `end_index` (next whitespace or end of string).
  - Extract `query = &text[start_index + 1 .. caret_pos]`.
- Implement `get_suggestions(config: &AutocompleteConfig, text: &str, caret_pos: usize) -> AutocompleteResult` (`src/renderer/autocomplete.ts:79-146`):
  - Select candidate list (`tags` for `#`, `projects` for `$`, `mentions` for `@`).
  - Rank into two tiers preserving config order:
    - Tier 1: `item.to_lowercase().starts_with(&query_lower)` (or all items if `query` is empty).
    - Tier 2: `item.to_lowercase().contains(&query_lower)` (excluding Tier 1).
  - Populate `ghost_suffix` **only** when `!matches.is_empty() && caret_pos == token.end_index` and `matches[0].value.to_lowercase().starts_with(&query_lower)`. Slice the remaining characters from `matches[0].value` using character count (`chars().skip(query.chars().count()).collect()`).
- Implement `apply_completion(text: &str, matched: &AutocompleteMatch) -> (String, usize)` (`src/renderer/autocomplete.ts:148-163`):
  - Replace `text[matched.start_index..matched.end_index]` with `format!("{}{} ", matched.prefix, matched.value)` and return `(new_text, new_caret_pos)`.

#### 2. `src/expansions.rs`
- Implement `auto_capitalize_first_letter(new_value: &str, prev_value: &str) -> String` (`src/renderer/app.tsx:93-103`):
  - When `prev_value` matches `^!{0,2}\s*$` and `new_value` matches `^(!{0,2}\s*)([a-z])$`, uppercase that single ASCII/Unicode lowercase letter.
- Implement `expand_on_space(text: &str, caret_pos: usize, expansions: &HashMap<String, String>) -> ExpansionResult` (`src/renderer/expansions.ts:15-112`):
  - Return `expanded: false` if `expansions` is empty, `caret_pos == 0`, `caret_pos > text.len()`, or the char before `caret_pos` is whitespace.
  - Find `word_start_index` by scanning backward to whitespace.
  - Determine `is_first_word` and `word_key_start_index`:
    - If `word_start_index == 0`: strip leading `!!` (if `caret_pos >= 2`) or `!` (if `caret_pos >= 1`) to get `word_key_start_index`; set `is_first_word = true`.
    - Else if `text[..word_start_index]` matches `^!{0,2}\s*$`, set `is_first_word = true`.
  - Extract `raw_word = &text[word_key_start_index..caret_pos]`.
  - Do not expand if `raw_word` is empty or starts with `/`, `#`, `$`, or `@`.
  - Lookup:
    - If `is_first_word`: try exact match first, then case-insensitive key match; uppercase the first character of the expanded replacement (`expanded_text`).
    - If not `is_first_word`: exact case-sensitive match only.
  - Replace `text[word_key_start_index..caret_pos]` with `format!("{expanded_text} ")` and return `new_caret_pos = word_key_start_index + expanded_text.len() + 1`.

#### 3. `src/task_engine.rs`
- Define `TaskItem` (`src/renderer/app.tsx:18-24`):
  ```rust
  #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
  ```
- Define `CommandOutcome` returned by `TaskEngine::execute(&mut self, raw_text: &str) -> CommandOutcome`:
  - `TasksUpdated` (tasks & history mutated)
  - `Warning(String)` (validation / help message to display for 3s)
  - `TogglePin` (`/p` or `/pin`)
  - `Dock(DockSide)` (`/dock [l|left|r|right]`, defaulting to `Right` if argument omitted)
  - `Float` (`/float` or `/undock`)
  - `OpenConfig` (`/config`)
  - `OpenAutocompleteConfig` (`/tags` or `/autocomplete`)
  - `SetTheme(ThemeTarget)` (`/theme [dark|light|system|auto]`, `/dark`, `/light`, or `Toggle` when `/theme` has no arg)
  - `Quit` (`/x`, `/exit`, `/q`, `/quit`)
  - `NoOp` (empty/whitespace input)
- Implement `TaskEngine { pub tasks: Vec<TaskItem>, pub history: Option<Vec<TaskItem>> }` with exact parity for all queue commands (`src/renderer/app.tsx:521-849`):
  - **Demotion Guard (`is_demotion`)**: If `tasks.first().is_some_and(|t| t.important)`, reject moving index `0` to `!= 0` or moving `!= 0` to `0`.
  - **`/d[one] [b[reak]]`**:
    - Empty list -> `Warning("No tasks to complete.")`.
    - Save `history`, remove index `0`. If `b`/`break` argument present, prepend `TaskItem { text: "Break ☕", is_break: true, .. }` at index `0`.
  - **`/b[reak]`**:
    - If `tasks[0].important` -> `Warning("Cannot insert break: current task is important and cannot be demoted.")`.
    - If `tasks[0].is_break` -> `Warning("Already on a break.")`.
    - Save `history`, prepend `"Break ☕"` (`is_break: true`) at index `0`.
  - **`/m[ove] x u|d [y]`** and **`/m[ove] x y`**:
    - Relative move clamps `to_index` to `0..tasks.len() - 1` (`from_index.saturating_sub(offset)` for `u`, `(from_index + offset).min(tasks.len() - 1)` for `d`).
    - Out-of-bounds `from_index` or `to_index` -> `Warning("Move index out of bounds.")`.
    - If `tasks[from_index].is_break` -> `Warning("Cannot move a break.")`.
    - If `is_demotion` -> `Warning("Action rejected: current task is important and cannot be demoted.")`.
    - **Special Break Replacement Rule (`app.tsx:599-601`)**: When moving an item to `to_index == 0` while `tasks[0].is_break` is true, remove the break at index `0` and place the moved item at index `0`.
  - **`/r[emove] x`** (matches `^\/r(?:e(?:move)?)?\s+(\d+)$`):
    - Out of bounds -> `Warning("Remove index out of bounds.")`. Save `history`, remove at `x`.
  - **`/e[dit] x <text>`**:
    - Out of bounds -> `Warning("Edit index out of bounds.")`. Save `history`, set `tasks[x].text = new_text` and `tasks[x].is_break = false`.
  - **`/u[ndo]`**:
    - Restore `tasks` from `history.take()`, or `Warning("Nothing to undo.")`.
  - **`/i[mportant]`**:
    - Empty list -> `Warning("No current task to mark important.")`.
    - If `tasks[0].is_break` -> `Warning("Cannot mark a break as important.")`.
    - Save `history`, toggle `tasks[0].important`.
  - **`/c[lear]`**:
    - Empty list -> `Warning("List is already empty.")`. Save `history`, clear `tasks`.
  - **`/h[elp]` or `/?`**:
    - Return `Warning("Commands: /done [b], /break, /move x y, /move x u|d [y], /remove x, /edit x text, /undo, /important, /pin, /dock [l|r], /float, /config, /tags, /theme [dark|light|system], /clear, /exit, /help (Use #tag, $project, @mention + [Tab])")`.
  - **Unrecognized `/` command**:
    - Return `Warning(format!("Unrecognized command: {text}"))`.
  - **Task Insertion (`!!`, `!`, default)**:
    - `!!<text>`: If trimmed slice after `!!` is non-empty: check `tasks[0].important` -> `Warning("Cannot prepend task: current task is important and cannot be demoted.")`. Save `history`. If `tasks[0].is_break`, replace `tasks[0]` with the new task (`app.tsx:821-823`); otherwise insert at index `0`.
    - `!<text>`: If trimmed slice after `!` is non-empty: save `history`, insert at index `1` (or index `0` if list is empty).
    - `<text>`: Save `history`, push to back of `tasks`.

### Phase 1 Verification (Definition of Done)
- Run `cargo test` and verify 100% pass rate across unit tests covering:
  - Token detection with `#`, `$`, `@`, `!#tag`, `!!$proj`, mid-word carets, and multi-byte UTF-8 strings.
  - Tier 1 (`starts_with`) vs Tier 2 (`contains`) sorting and `ghost_suffix` generation.
  - First-word case-insensitive expansion + auto-capitalization vs second-word case-sensitive expansion.
  - Every slash command, demotion guard rejection, break replacement on `!!` and `/move x 0`, and 1-level `/undo`.

---

## Phase 2: Config Store, Task Persistence, File Watchers & Theme System

### Goal
Create the GPUI `Entity<ConfigStore>`, `Entity<TaskStore>`, `Global<ActiveTheme>`, asset embedding, file persistence (`config.json`, `autocomplete.json`, `tasks.json`), and live `notify` file watchers.

### Dependencies to Add (`Cargo.toml`)
```toml
dirs = "6.0"
notify = "8.0"
smol = "2.0"
open = "5.3"
rust-embed = "8.5"
```

### Files to Create / Modify
1. `src/paths.rs` *(userData resolution and legacy `task-conveyor` -> `TaskConveyor` migration)*
2. `src/assets.rs` *(embeds `assets/` via `rust-embed` and implements `gpui::AssetSource`)*
3. `src/theme.rs` *(ports `src/renderer/index.css` light/dark colors and tag HSL badge math)*
4. `src/config_store.rs` *(ports `src/main/config.ts` and `src/main/autocomplete.ts`)*
5. `src/task_store.rs` *(wraps `TaskEngine` in a GPUI `Entity<TaskStore>` with disk persistence and 3s warning timer)*
6. `src/main.rs` *(initialize `Assets`, `ConfigStore`, `TaskStore`, and `ActiveTheme` global on app startup)*

### Implementation Tasks

#### 1. `src/paths.rs` (`src/main/index.ts:44-66`)
- Resolve `<AppData>/TaskConveyor` (using `dirs::config_dir()` or `dirs::data_dir()` matching platform conventions: `~/Library/Application Support/TaskConveyor` on macOS, `%APPDATA%\TaskConveyor` on Windows, `~/.config/TaskConveyor` on Linux).
- Implement `migrate_legacy_user_data()`:
  - Check if `<AppData>/task-conveyor` exists.
  - If `<AppData>/TaskConveyor` does not exist, copy the directory recursively; otherwise copy `config.json` and `autocomplete.json` if missing in the new directory.
- Provide path helpers for `config.json`, `autocomplete.json`, and `tasks.json`.

#### 2. `src/assets.rs`
- Export two SVG files into the project's `assets/icons/` folder:
  - `assets/icons/terminal.svg` (FontAwesome `faTerminal` path)
  - `assets/icons/thumbtack.svg` (FontAwesome `faThumbtack` path)
- Ensure `assets/logo-full.png`, `assets/icon.icns`, and `assets/icon.ico` are in `assets/`.
- Implement `gpui::AssetSource` using `#[derive(RustEmbed)] #[folder = "assets"] pub struct Assets;`.

#### 3. `src/theme.rs` (`src/renderer/index.css` & `src/renderer/app.tsx:57-72`)
- Define `ThemeMode` (`System`, `Light`, `Dark`) and `ActiveTheme { pub mode: ThemeMode, pub is_dark: bool, pub palette: ThemePalette }` implementing `gpui::Global`.
- Populate `ThemePalette` with exact colors from `src/renderer/index.css`:
  - **Sidebar**:
    - Background: `#f8f9fa` (light) / `#141618` (dark)
    - Chevron strokes: `(rgba(0,0,0,0.04), rgba(255,255,255,0.65))` (light) / `(rgba(0,0,0,0.40), rgba(255,255,255,0.04))` (dark)
    - Header & Footer bg: `#ffffff` (light) / `#1a1d20` (dark), border: `#dee2e6` (light) / `#2b3035` (dark)
    - Empty card & Task item bg: `#ffffff` (light) / `#1e2124` (dark), border: `#dee2e6` (light) / `#2b3035` (dark), hover border: `#ced4da` / `#3d4349`
    - Task text: `#343a40` (light) / `#e9ecef` (dark); Muted text: `#6c757d` (light) / `#adb5bd` (dark)
    - Number badge: bg `#e9ecef`, fg `#495057` (light) / bg `#2b3035`, fg `#adb5bd` (dark)
    - Kbd shortcut: bg `#f8f9fa`, fg `#212529`, border `#ced4da` (light) / bg `#2b3035`, fg `#dee2e6`, border `#3d4349` (dark)
  - **Spotlight & Popover**:
    - Spotlight bg: `#ffffff` (light) / `#1e2124` (dark)
    - Spotlight lip & Popover header bg: `#f8f9fa`, border `#edf2f7` (light) / bg `#16181a`, border `#2b3035` (dark)
    - Ghost suffix: `#9499a0` @ `0.75` alpha (light) / `#adb5bd` @ `0.55` alpha (dark)
    - Popover active item bg: `#e9ecef` (light) / `#2b3035` (dark)
    - Type pills (`TAG`, `PROJECT`, `MENTION`):
      - `TAG`: bg `#cfe2ff`, fg `#084298` (swapped in dark mode)
      - `PROJECT`: bg `#d1e7dd`, fg `#0f5132` (swapped in dark mode)
      - `MENTION`: bg `#e0cffc`, fg `#3d0a91` (swapped in dark mode)
- Implement `hash_code(s: &str) -> i32` matching JS `str.charCodeAt(i) + ((hash << 5) - hash)` (wrapping `i32` arithmetic over UTF-16 code units `s.encode_utf16()`) so tag hues (`hue = hash.unsigned_abs() % 360`) match the Electron app identically:
  - Light task item badge: `bg = hsla(hue/360, 0.75, 0.90, 1.0)`, `fg = hsla(hue/360, 0.75, 0.25, 1.0)`
  - Dark task item badge: `bg = hsla(hue/360, 0.45, 0.24, 1.0)`, `fg = hsla(hue/360, 0.85, 0.85, 1.0)`
  - Inside `.current-task-card` (both themes): `bg = hsla(hue/360, 0.75, 0.90, 1.0)`, `fg = hsla(hue/360, 0.75, 0.20, 1.0)`

#### 4. `src/config_store.rs` (`src/main/config.ts` & `src/main/autocomplete.ts`)
- Implement `ensure_config_file()` and `ensure_autocomplete_file()`:
  - Default `config.json`: `{ "globalHotkey": "Option+Space" (macOS) | "Ctrl+Space" (other), "theme": "system" }`.
  - Default `autocomplete.json`: `{ "tags": [], "projects": [], "mentions": [], "expansions": { "rpr": "review PR" } }`.
  - If `autocomplete.json` exists on disk but lacks the `"expansions"` key, inject `"expansions": { "rpr": "review PR" }` and write it back (`src/main/autocomplete.ts:48-58`).
- Implement `format_hotkey_for_display(accelerator: &str) -> String` (`src/main/config.ts:115-144`).
- Implement `ConfigStore` entity:
  - Holds `app_config`, `autocomplete`, `active_hotkey`, `formatted_hotkey`, `restart_required`, and `_watcher_task: Task<()>`.
  - Spawns a `notify::RecommendedWatcher` watching the `<userData>` directory (so atomic saves/renames in external editors like VS Code or Vim are caught reliably).
  - Debounces changes (200ms for `config.json`, 150ms for `autocomplete.json`) on `cx.background_executor()`, reloads the respective struct, sets `restart_required = true` if `globalHotkey != active_hotkey`, updates `Global<ActiveTheme>` if `theme` changed, and calls `cx.notify()`.
  - Provides `open_config(&self, cx)` and `open_autocomplete_config(&self, cx)` using `open::that(&path)` on the background executor.

#### 5. `src/task_store.rs`
- Implement `TaskStore` entity wrapping `TaskEngine`:
  - Loads initial tasks from `<userData>/tasks.json` (gracefully defaulting to `Vec::new()` if missing or corrupt).
  - `persist_tasks(&self, cx)`: clones `self.engine.tasks` and writes `<userData>/tasks.json` asynchronously via a temp file + rename on `cx.background_executor()`.
  - `set_warning(&mut self, msg: String, cx: &mut Context<Self>)`: sets `self.warning = Some(msg)`, calls `cx.notify()`, and spawns a 3-second timer task that clears `self.warning = None` and calls `cx.notify()`.
  - `execute_command(&mut self, raw_text: &str, cx: &mut Context<Self>)`: calls `self.engine.execute(raw_text)` and handles `CommandOutcome` (persisting tasks, setting warning, or calling `ConfigStore` / `cx.quit()`).

### Phase 2 Verification (Definition of Done)
- Run `cargo test` for `format_hotkey_for_display`, `hash_code`, and JSON config migration/sanitization.
- Run `cargo run`: verify `<AppData>/TaskConveyor/config.json` and `autocomplete.json` are created on disk with expected defaults, and editing `autocomplete.json` or `config.json` in an external editor triggers the `notify` watcher callback within 200ms.

---

## Phase 3: Main Sidebar View & Task Queue UI

### Goal
Replace the "Hello World" window with the complete `MainSidebarView` layout (`src/renderer/app.tsx:872-991`, `src/renderer/index.css:18-388`), observing `TaskStore` and `ConfigStore` and rendering all task states, badges, gradients, and animations.

### Files to Create / Modify
1. `src/views/mod.rs`
2. `src/views/tag_text.rs` *(renders task text with inline `#tag`, `$project`, `@mention` colored pill badges)*
3. `src/views/main_sidebar.rs` *(main sidebar window view)*
4. `src/main.rs` *(configure `WindowOptions` for the `400x600` frameless sidebar and mount `MainSidebarView`)*

### Implementation Tasks

#### 1. Window Configuration (`src/main.rs` -> `src/main/window.ts:285-315`)
- Configure `WindowOptions`:
  - Size: `size(px(400.0), px(600.0))`, centered in `cx.primary_display()` work area (`visible_bounds()`).
  - Minimum size: `min_size: Some(size(px(320.0), px(200.0)))`.
  - Frameless: `titlebar: None`.
- Observe OS window appearance changes via `cx.observe_window_appearance(window, ...)` so `ThemeMode::System` automatically updates `Global<ActiveTheme>` when the OS switches between light and dark mode.

#### 2. Repeating Conveyor-Belt Chevron Background (`src/views/main_sidebar.rs` -> `index.css:18-31`)
- Render a background `canvas(...)` element with `.absolute().inset_0()` inside the root `.relative().flex().flex_col().size_full().overflow_hidden().bg(palette.sidebar_bg)`:
  - Loop `y` in `px(96.0)` steps from `bounds.origin.y` to `bounds.origin.y + bounds.size.height`.
  - For each tile, stroke two 3-point chevron paths (`(left, y + 68) -> (mid_x, y + 28) -> (right, y + 68)` with `px(2.0)` shadow stroke, and `(left, y + 70) -> (mid_x, y + 30) -> (right, y + 70)` with `px(1.5)` highlight stroke) using `PathBuilder::stroke`.

#### 3. Inline Tag/Project/Mention Badge Renderer (`src/views/tag_text.rs` -> `app.tsx:74-91`, `index.css:352-372`)
- Implement `render_task_text_with_tags(text: &str, is_dark: bool, in_current_card: bool) -> impl IntoElement`:
  - Split `text` by whitespace into words and render inside `div().flex().flex_row().flex_wrap().items_baseline().gap_x(rems(0.28)).gap_y(rems(0.15))`.
  - If a token matches `^[#@$][\w-]+$`, compute its HSL colors via `theme::tag_badge_colors(token, is_dark, in_current_card)` and render a pill `div().px(rems(0.35)).py(rems(0.05)).rounded(px(4.0)).bg(bg).text_color(fg).text_size(rems(0.85)).font_weight(FontWeight::BOLD).child(token)`.
  - Otherwise render the word as plain text.

#### 4. `MainSidebarView` Layout Sections (`src/views/main_sidebar.rs`)
- **Header (`app.tsx:875-885`, `index.css:33-50`)**:
  - `div().flex().items_center().justify_between().px_4().py_2().bg(palette.header_bg).border_b_1().border_color(palette.border)`.
  - Left side: `img("logo-full.png").h(px(24.0))` + `"TaskConveyor"` bold heading.
  - Right side: When `always_on_top` is true, render a subtle blue badge containing `svg().path("icons/thumbtack.svg")`.
  - Window drag: When `!is_docked`, attach `.on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())`.
- **Scrollable Content Area (`app.tsx:888-950`, `index.css:62-113, 247-350`)**:
  - `div().id("sidebar-content").flex_1().overflow_y_scroll().p_4()`.
  - **Alerts**:
    - If `config_store.read(cx).restart_required`: info banner `"Restart TaskConveyor to use your new global hotkey."`
    - Else if `let Some(warning) = &task_store.read(cx).warning`: amber warning banner `{warning}`.
  - **Currently Doing Section**:
    - Uppercase muted label `"CURRENTLY DOING"`.
    - If `tasks.first()` exists:
      - Card with `rounded(px(12.0))`, `p(rems(1.5))`, white text, and `135deg` `linear_gradient`:
        - Normal: `#0d6efd` -> `#0b5ed7`, shadow `rgba(13, 110, 253, 0.35)`
        - Break (`is_break`): `#198754` -> `#157347`, shadow `rgba(25, 135, 84, 0.35)`
        - Urgent (`important`): `#dc3545` -> `#bb2d3b`, with GPUI `.with_animation("urgent-pulse", Animation::new(Duration::from_secs(2)).repeat().with_easing(pulsating_between(0.0, 1.0)), ...)` oscillating box-shadow blur between `15px` (`alpha 0.35`) and `25px` (`alpha 0.60`).
      - Circular `0` badge (`bg: rgba(255,255,255,0.25)`, `20x20px`) + `render_task_text_with_tags(&task.text, is_dark, true)`.
    - If empty:
      - Solid bordered `.empty-task-card` with `"No active task"` and `"Press [hotkey] to add a task."`
  - **Up Next (`remaining_tasks.len()`) Section**:
    - Uppercase muted label `format!("UP NEXT ({})", remaining_tasks.len())`.
    - For each `(idx, task)` in `tasks.iter().enumerate().skip(1)`:
      - `.task-item` card (`rounded(px(8.0))`, `px_4()`, `py_3()`, `mb_2()`, border + hover border/shadow).
      - Circular `{idx}` badge (`20x20px`) + optional green `BREAK` badge (when `task.is_break`) + `render_task_text_with_tags(&task.text, is_dark, false)`.
    - If `remaining_tasks.is_empty()`:
      - Dashed border card (`.empty-task-card-dashed`) displaying `"No tasks lined up"`.
- **Footer Container (`app.tsx:953-989`, `index.css:52-60`)**:
  - `div().p_3().bg(palette.footer_bg).border_t_1().border_color(palette.border).relative()`.
  - Placeholder slot for `CommandInputView` (wired in Phase 4) + bottom row with `Global Hotkey: <kbd>` and `Type <kbd>/help</kbd> for commands`.

### Phase 3 Verification (Definition of Done)
- Run `cargo run`: the `400x600` frameless sidebar window opens centered on screen with the conveyor chevron pattern, logo, header drag movement, and empty state cards.
- Seed `tasks.json` with sample tasks (including an `important: true` task, an `isBreak: true` task, and tasks with `#urgent $backend @alice` tags) and verify gradients, pulsing urgent shadow animation, tag pill colors, scrolling, and live light/dark switching via `config.json`.

---

## Phase 4: Custom Command Input, Inline Ghost Text & Autocomplete Popover

### Goal
Build the shared GPUI `CommandInputView` (`Element` + `EntityInputHandler`) and `AutocompletePopover`, enabling full keyboard task entry, inline ghost completion, space expansions, auto-capitalization, and slash command execution in the sidebar footer.

### Files to Create / Modify
1. `src/views/command_input.rs` *(custom single-line GPUI text input with two-run `shape_line` ghost suffix and keyboard/IME/clipboard handlers)*
2. `src/views/autocomplete_popover.rs` *(ports `src/renderer/AutocompletePopover.tsx` using `gpui::deferred`)*
3. `src/views/main_sidebar.rs` *(mount `Entity<CommandInputView>` and `AutocompletePopover` in the footer and subscribe to `CommandInputEvent::Submit`)*

### Implementation Tasks

#### 1. `CommandInputView` State & Keybindings (`src/views/command_input.rs`)
- Define GPUI actions with `actions!(command_input, [Backspace, Delete, Left, Right, Home, End, SelectAll, Copy, Cut, Paste, AcceptSuggestion, NextSuggestion, PrevSuggestion, DismissOrHide, Submit]);` and register keybindings in `KeyContext` `"CommandInput"`.
- Maintain state in `CommandInputView`:
  - `value: String`
  - `caret_pos: usize` (UTF-8 byte offset, always on char boundary)
  - `selection_anchor: Option<usize>` (for Shift+Arrow / Cmd+A selection & clipboard copy/cut/paste)
  - `selected_index: usize`
  - `dismissed: bool`
  - `focus_handle: FocusHandle`
  - `is_spotlight: bool`
  - `scroll_offset_x: Pixels` (keeps caret visible when typing long task descriptions)
  - `last_bounds: Option<Bounds<Pixels>>` and `last_shaped_line: Option<ShapedLine>` (for mouse click-to-place-caret via `shaped_line.closest_index_for_x(mouse_x)`).
- Implement `gpui::EntityInputHandler` for OS text input / IME composition:
  - In `replace_text_in_range`: apply `auto_capitalize_first_letter(&new_value, &prev_value)` when typing at the end of a prefix-only input (`app.tsx:183, 447`).
  - Intercept `" "` (space) insertion when selection is collapsed: call `expand_on_space(&self.value, self.caret_pos, &config.expansions)`. If `result.expanded`, set `self.value = result.new_text`, `self.caret_pos = result.new_caret_pos`, reset `dismissed = false`, `selected_index = 0`, and return early (`app.tsx:205-225, 469-489`).
- Implement special key handlers (`app.tsx:226-257, 490-515`):
  - **`Tab` (`AcceptSuggestion`)**: When `show_suggestions` is true, call `apply_completion(&self.value, active_match)` and update `value` and `caret_pos`.
  - **`ArrowDown` / `ArrowUp`**: When `show_suggestions` is true, cycle `selected_index` modulo `matches.len()`.
  - **`Escape` (`DismissOrHide`)**: If `show_suggestions` is true, set `self.dismissed = true`. Otherwise emit `CommandInputEvent::DismissSpotlight`.
  - **`Enter` (`Submit`)**: Emit `CommandInputEvent::Submit(self.value.clone())`, clear `self.value`, and reset `caret_pos = 0`, `dismissed = false`, `selected_index = 0`.

#### 2. Single-Line `Element` Painting with Inline Ghost Text (`GPUI_PORT_PLAN.md` §3.1.1)
- In `CommandInputElement::paint`:
  - If `self.value.is_empty()`: shape and paint placeholder `"Type a task..."` in muted color.
  - Otherwise, compute `get_suggestions(&autocomplete_config, &self.value, self.caret_pos)`.
  - Construct `display_text = format!("{}{}", self.value, if show_suggestions { &ghost_suffix } else { "" })`.
  - Shape a single line via `window.text_system().shape_line(...)` with two `TextRun`s:
    1. `TextRun { len: self.value.len(), color: palette.input_text, .. }`
    2. `TextRun { len: ghost_suffix.len(), color: palette.ghost_suffix, .. }` (when `show_suggestions && !ghost_suffix.is_empty()`).
  - Compute caret X position via `shaped_line.x_for_index(self.caret_pos)` and paint a `px(1.5)` wide cursor quad when `focus_handle.is_focused(window)` (plus selection highlight quad if `selection_anchor != Some(caret_pos)`).

#### 3. `AutocompletePopover` (`src/views/autocomplete_popover.rs` -> `AutocompletePopover.tsx:1-72`, `index.css:445-594`)
- Render only in `MainSidebarView` when `show_suggestions` is true (`!matches.is_empty() && !dismissed`).
- Wrap in `gpui::deferred(...)` with `.with_priority(1)` inside the footer container, positioned `.absolute().left_0().right_0().bottom(DefiniteLength::Fraction(1.0)).mb(px(6.0))` (`bottom: calc(100% + 6px)`).
- Render header bar: `"SUGGESTIONS ({matches.len()})"` on left, `"Tab to accept"` with `<kbd>` badge on right.
- Render up to 8 items (`matches.iter().take(8).enumerate()`):
  - Highlight active row (`idx == selected_index`) and hover state.
  - Render type pill (`TAG`, `PROJECT`, `MENTION`) with theme-aware pill colors + `{match.prefix}{match.value}`.
  - Attach `.on_mouse_down(MouseButton::Left, ...)` calling `command_input.update(cx, |input, cx| input.select_suggestion(match_clone, window, cx))` so clicking an item completes the token and keeps focus in the input.

### Phase 4 Verification (Definition of Done)
- Run `cargo run` and test in the sidebar footer input:
  - Type `rpr` + `Space` -> expands to `Review PR ` (and `rpr` as second word -> `review PR `).
  - Type `fix bug` -> auto-capitalizes to `Fix bug`; `!fix bug` -> `!Fix bug`; `!!fix bug` -> `!!Fix bug`.
  - Type `#`, `$`, `@` -> popover appears above footer input AND gray ghost suffix appears inline right at the caret; `Up`/`Down` cycles selection; `Tab` or clicking a row inserts canonical casing + trailing space; `Escape` dismisses the popover.
  - Execute `/done`, `/done b`, `/break`, `/important`, `/move`, `/remove`, `/edit`, `/undo`, `/clear`, `/theme`, `/config`, `/tags`, `/help`, `/exit` directly from the footer input.

---

## Phase 5: Spotlight Overlay Window & Global Hotkey Integration

### Goal
Create the second GPUI window (`SpotlightView` using `WindowKind::PopUp`) sharing `TaskStore` and `ConfigStore`, and wire the OS-wide global shortcut (`global-hotkey`) to summon/dismiss it from anywhere.

### Dependencies to Add (`Cargo.toml`)
```toml
global-hotkey = "0.6"
rfd = "0.15"
```

### Files to Create / Modify
1. `src/views/spotlight.rs` *(ports `SpotlightInput` from `src/renderer/app.tsx:105-322` and `src/renderer/index.css:115-245`)*
2. `src/hotkey.rs` *(parses Electron accelerators and bridges `GlobalHotKeyEvent` into GPUI)*
3. `src/main.rs` *(register global hotkey and manage Spotlight window creation/toggling)*

### Implementation Tasks

#### 1. `SpotlightView` (`src/views/spotlight.rs` -> `app.tsx:105-322`, `index.css:115-245`)
- Window options (`src/main/window.ts:339-374`):
  - Size: `500x72` (`size(px(500.0), px(72.0))`), centered on primary display.
  - `titlebar: None`, `kind: WindowKind::PopUp` (always-on-top, skip-taskbar), `is_resizable: false`, `focus: true`.
- Reuse `Entity<CommandInputView>` configured with `is_spotlight: true` (`1.15rem` font size, inline ghost suffix enabled, no popover).
- Layout:
  - **Main Row (`min-height: 46px`, `px(12.0)` horizontal padding)**:
    - Left: `svg().path("icons/terminal.svg").size(rems(1.2)).text_color(palette.spotlight_icon).mr(px(10.0))`
    - Center: `CommandInputView` (`flex_1()`)
    - Right: Dynamic `.spotlight-badge` (`app.tsx:263-271`):
      - If `show_suggestions`: `"[Tab] Tag"` / `"[Tab] Project"` / `"[Tab] Mention"`
      - Else if `value.starts_with("!!")`: `"⚡ Current Slot"`
      - Else if `value.starts_with("!")`: `"➡️ Next Slot"`
      - Else if `value.starts_with("/")`: `"Command"`
      - Else: `"📥 End of List"`
  - **Bottom Lip (`height: 24px`, `px(12.0)` padding, `border-top`)**:
    - Left hints (`gap: 10px`): `! next`, `!! current`, `# tag`, `$ project`, `@ mention` (with monospace key badges).
    - Right hint: `Tab accept`.
- Subscriptions & Window Activation:
  - Subscribe to `CommandInputEvent::Submit(text)`: call `task_store.update(cx, |s, cx| s.execute_command(&text, cx))` and hide the Spotlight window.
  - Subscribe to `CommandInputEvent::DismissSpotlight`: hide the Spotlight window.
  - Observe window activation via `cx.observe_window_activation(window, |view, window, cx| ...)`: when `!window.is_window_active()`, automatically hide the Spotlight window (`window.ts:363-367`).
  - When shown/toggled open: call `command_input.update(cx, |input, cx| input.reset(window, cx))` to clear text, reset caret/dismissed state, and focus the input (`window.ts:388`, `app.tsx:158-166`).

#### 2. Global Hotkey Registration & Event Bridge (`src/hotkey.rs` -> `src/main/index.ts:115-124, 166-175`)
- Implement `parse_accelerator(accel: &str) -> Result<HotKey, String>`:
  - Normalize Electron modifier aliases before parsing:
    - `CommandOrControl` / `CmdOrCtrl` -> `Super` (on `target_os = "macos"`) or `Control` (on Windows/Linux)
    - `Command` / `Cmd` -> `Super`
    - `Option` -> `Alt`
- Register the configured `globalHotkey` via `GlobalHotKeyManager::new()` on startup:
  - If registration succeeds, record `active_hotkey = Some(target_hotkey)` on `ConfigStore`.
  - If registration fails, display a native error dialog via `rfd::MessageDialog::new().set_level(MessageLevel::Error).set_title("Global Hotkey Error").set_description(...)` (`src/main/index.ts:171-174`).
- Spawn a foreground task polling/receiving from `GlobalHotKeyEvent::receiver()` (filtering for `HotKeyState::Pressed`) to toggle the Spotlight window open or closed.

### Phase 5 Verification (Definition of Done)
- Press `Option+Space` (macOS) or `Ctrl+Space` (Windows/Linux) while another application is focused:
  - Spotlight `500x72` window appears centered and focused with empty input.
  - Typing `!`, `!!`, `#`, `$`, `@` updates the right-hand badge (`➡️ Next Slot`, `⚡ Current Slot`, `[Tab] Tag`, etc.) and renders inline ghost completions.
  - Pressing `Enter` adds the task or runs the slash command (e.g., `/done`, `/important`), immediately updating the Main Sidebar window and hiding Spotlight.
  - Pressing `Escape` (or clicking outside Spotlight) hides Spotlight cleanly.

---

## Phase 6: Window Docking, Win32 AppBar FFI, System Tray & App Lifecycle

### Goal
Complete `Entity<WindowManager>` for `/dock [l|r]`, `/float`, and `/pin`, implement Windows `SHAppBarMessage` desktop work-area reservation and `ABN_POSCHANGED` handling, add the system tray icon/menu, and enforce single-instance startup.

### Dependencies to Add (`Cargo.toml`)
```toml
tray-icon = "0.19"
muda = "0.15"
image = { version = "0.25", default-features = false, features = ["png", "ico"] }
single-instance = "0.3"
raw-window-handle = "0.6"

[target.'cfg(target_os = "windows")'.dependencies]
windows-sys = { version = "0.59", features = [
    "Win32_Foundation",
    "Win32_UI_Shell",
    "Win32_UI_WindowsAndMessaging",
] }
```

### Files to Create / Modify
1. `src/window_manager.rs` *(ports `src/main/window.ts` docking, floating, pinning, and Spotlight lifecycle)*
2. `src/win32_appbar.rs` *(ports `koffi` `SHAppBarMessage` & `ABN_POSCHANGED` from `src/main/window.ts:15-105, 162-209, 265-283, 317-326`)*
3. `src/tray.rs` *(ports `createTray` from `src/main/index.ts:81-113`)*
4. `src/single_instance.rs` *(ports `requestSingleInstanceLock` from `src/main/index.ts:68-76, 126-129`)*
5. `src/task_store.rs` & `src/main.rs` *(connect `CommandOutcome::Dock`, `Float`, `TogglePin` to `WindowManager`, wire tray & single-instance lock)*

### Implementation Tasks

#### 1. `WindowManager` (`src/window_manager.rs` -> `src/main/window.ts:151-263`)
- Maintain `dock_side: Option<DockSide>`, `always_on_top: bool`, `is_docking: bool` (re-entry guard), `main_window: Option<WindowHandle<MainSidebarView>>`, and `spotlight_window: Option<WindowHandle<SpotlightView>>`.
- **`dock_window(&mut self, side: DockSide, cx: &mut Context<Self>)`**:
  - Guard with `if self.is_docking { return; } self.is_docking = true;`.
  - Query the display matching the main window's current bounds (or `cx.primary_display()`).
  - On Windows (`#[cfg(target_os = "windows")]`): invoke `win32_appbar::dock_appbar(hwnd, side, display_bounds, scale_factor, 400.0)` and resize/reposition the window to the returned rect.
  - On macOS/Linux: snap window to `display.visible_bounds()` (`x = work_area.x` for `Left`, `x = work_area.x + work_area.width - 400.0` for `Right`, `y = work_area.y`, `width = 400.0`, `height = work_area.height`).
  - Set `self.dock_side = Some(side)` and `self.always_on_top = true`, update window always-on-top level, and call `cx.notify()` (which disables header drag and shows the thumbtack badge in `MainSidebarView`).
- **`float_window(&mut self, cx: &mut Context<Self>)`**:
  - On Windows: if `appbar_registered`, call `win32_appbar::remove_appbar(hwnd)` and set `appbar_registered = false`.
  - Center the window (`400x600`) in `display.visible_bounds()`.
  - Set `self.dock_side = None` and `self.always_on_top = false`, update window always-on-top level, and call `cx.notify()`.
- **`toggle_always_on_top(&mut self, cx: &mut Context<Self>)`**:
  - Flip `self.always_on_top`, update window level, and call `cx.notify()`.
- **`restore_main_window(&mut self, cx: &mut Context<Self>)`**:
  - Unminimize/show and focus `main_window` (re-creating it if closed on macOS).

#### 2. Win32 AppBar FFI (`src/win32_appbar.rs` -> `src/main/window.ts:45-105, 162-209, 317-330`)
- Compile only on `#[cfg(target_os = "windows")]`.
- Extract `HWND` from GPUI `Window` via `raw_window_handle::HasWindowHandle`.
- Implement `SHAppBarMessage` calls (`ABM_NEW`, `ABM_QUERYPOS`, `ABM_SETPOS`, `ABM_REMOVE`) with `uCallbackMessage: MY_APPBAR_MSG_ID` (`1124`), converting logical DIP coordinates to physical pixels (`(coord * scale_factor).round() as i32`) and back.
- Install a window subclass callback (`SetWindowSubclass`) on `HWND` listening for `msg == MY_APPBAR_MSG_ID && wparam == ABN_POSCHANGED as usize` to trigger re-docking when taskbar or monitor resolution changes.
- Ensure `cleanup_appbar()` (`ABM_REMOVE`) runs on window close and app quit (`src/main/index.ts:206`, `src/main/window.ts:265-283, 328-330`).

#### 3. System Tray (`src/tray.rs` -> `src/main/index.ts:81-113`)
- Decode embedded icon (`assets/icon.ico` or `assets/logo-full.png`) via the `image` crate and resize to `16x16` RGBA.
- Build a `muda::Menu` with a single `"Exit"` `MenuItem`.
- Create `TrayIconBuilder::new()` with `.with_tooltip("TaskConveyor")`, `.with_icon(icon)`, `.with_icon_as_template(false)` (preserving full color on macOS menu bar, matching `trayIcon.setTemplateImage(false)` at `src/main/index.ts:94`), and `.with_menu(Box::new(menu))`.
- Bridge `TrayIconEvent::receiver()` (left click -> `window_manager.restore_main_window`) and `MenuEvent::receiver()` (`"Exit"` -> clean up AppBar & quit) into GPUI via a foreground task.

#### 4. Single-Instance Lock & Platform Lifecycle (`src/single_instance.rs` -> `src/main/index.ts:68-76, 197-221`)
- At the very start of `main()`, acquire `SingleInstance::new("TaskConveyor")`.
- If `!instance.is_single()`: show `"Another instance of TaskConveyor is already running."` via `rfd::MessageDialog` and return immediately (`src/main/index.ts:70-75`).
- Register cleanup hook on app quit (`cx.on_app_quit(...)`) to unregister Win32 AppBar, unregister global hotkeys, and drop the tray icon.

### Phase 6 Verification (Definition of Done)
- Run `/dock`, `/dock left`, `/dock right` -> sidebar snaps to the chosen screen edge, enables always-on-top (thumbtack badge appears), disables header dragging, and reserves desktop work area on Windows.
- Run `/float` or `/undock` -> sidebar unregisters AppBar, centers at `400x600`, unpins always-on-top, and re-enables header dragging.
- Run `/pin` -> toggles always-on-top and the header thumbtack badge.
- Click the system tray icon -> restores and focuses the main sidebar; right-click tray icon -> `"Exit"` cleanly quits the app.
- Launch a second instance of the binary while the first is running -> displays the native error dialog and exits immediately.
