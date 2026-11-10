use crate::autocomplete::{AutocompleteConfig, sanitize_expansions, sanitize_list};
use crate::paths::{
    AUTOCOMPLETE_FILE_NAME, CONFIG_FILE_NAME, legacy_user_data_dir, user_data_dir,
};
use crate::task_engine::ThemeTarget;
use crate::theme::{self, ActiveTheme, ThemeMode};
use gpui::{App, Context, Task, WindowAppearance};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn default_hotkey() -> String {
    if cfg!(target_os = "macos") {
        "Option+Space".to_string()
    } else {
        "Ctrl+Space".to_string()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default = "default_hotkey")]
    pub global_hotkey: String,
    #[serde(default)]
    pub theme: ThemeMode,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            global_hotkey: default_hotkey(),
            theme: ThemeMode::System,
        }
    }
}

pub fn default_autocomplete_config() -> AutocompleteConfig {
    let mut expansions = HashMap::new();
    expansions.insert("rpr".to_string(), "review PR".to_string());
    AutocompleteConfig {
        tags: Vec::new(),
        projects: Vec::new(),
        mentions: Vec::new(),
        expansions,
    }
}

/// Ensures `<userData>/config.json` exists, migrating from `<legacyUserData>/config.json` if needed,
/// or writing default `config.json`.
/// Ports `src/main/config.ts:31-58`.
pub fn ensure_config_file() -> PathBuf {
    ensure_config_file_in(&user_data_dir(), Some(&legacy_user_data_dir()))
}

pub fn ensure_config_file_in(user_data: &Path, legacy_user_data: Option<&Path>) -> PathBuf {
    let file_path = user_data.join(CONFIG_FILE_NAME);

    if !user_data.exists()
        && let Err(err) = fs::create_dir_all(user_data)
    {
        eprintln!("Failed to create userData directory {user_data:?}: {err}");
    }

    if !file_path.exists() {
        if let Some(legacy_dir) = legacy_user_data {
            let legacy_path = legacy_dir.join(CONFIG_FILE_NAME);
            if legacy_path.exists() && fs::copy(&legacy_path, &file_path).is_ok() {
                return file_path;
            }
        }

        let default_cfg = AppConfig::default();
        if let Ok(serialized) = serde_json::to_string_pretty(&default_cfg) {
            if let Err(err) = fs::write(&file_path, serialized) {
                eprintln!("Failed to create default config.json: {err}");
            }
        }
    }

    file_path
}

/// Loads and validates `config.json`.
/// Ports `src/main/config.ts:60-84`.
pub fn load_config() -> AppConfig {
    let file_path = ensure_config_file();
    load_config_from(&file_path)
}

pub fn load_config_from(file_path: &Path) -> AppConfig {
    match fs::read_to_string(file_path) {
        Ok(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(Value::Object(map)) => {
                let global_hotkey = map
                    .get("globalHotkey")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(default_hotkey);

                let theme = match map.get("theme").and_then(Value::as_str) {
                    Some("light") => ThemeMode::Light,
                    Some("dark") => ThemeMode::Dark,
                    Some("system") => ThemeMode::System,
                    _ => ThemeMode::System,
                };

                AppConfig {
                    global_hotkey,
                    theme,
                }
            }
            _ => AppConfig::default(),
        },
        Err(err) => {
            eprintln!("Error reading config.json: {err}");
            AppConfig::default()
        }
    }
}

/// Updates `theme` (and/or `globalHotkey`) in `config.json` while preserving any extra user keys.
/// Ports `src/main/config.ts:86-104`.
pub fn save_theme_to_config_file(file_path: &Path, theme: ThemeMode) -> AppConfig {
    let mut map: Map<String, Value> = fs::read_to_string(file_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|val| match val {
            Value::Object(m) => Some(m),
            _ => None,
        })
        .unwrap_or_default();

    if !map.contains_key("globalHotkey") {
        map.insert("globalHotkey".to_string(), Value::String(default_hotkey()));
    }
    map.insert(
        "theme".to_string(),
        Value::String(theme.as_str().to_string()),
    );

    if let Ok(serialized) = serde_json::to_string_pretty(&Value::Object(map)) {
        if let Err(err) = fs::write(file_path, serialized) {
            eprintln!("Failed to write config.json: {err}");
        }
    }

    load_config_from(file_path)
}

/// Ensures `<userData>/autocomplete.json` exists and injects default `"expansions"` if missing.
/// Ports `src/main/autocomplete.ts:29-68`.
pub fn ensure_autocomplete_file() -> PathBuf {
    ensure_autocomplete_file_in(&user_data_dir(), Some(&legacy_user_data_dir()))
}

pub fn ensure_autocomplete_file_in(user_data: &Path, legacy_user_data: Option<&Path>) -> PathBuf {
    let file_path = user_data.join(AUTOCOMPLETE_FILE_NAME);

    if !user_data.exists()
        && let Err(err) = fs::create_dir_all(user_data)
    {
        eprintln!("Failed to create userData directory {user_data:?}: {err}");
    }

    if !file_path.exists()
        && let Some(legacy_dir) = legacy_user_data
    {
        let legacy_path = legacy_dir.join(AUTOCOMPLETE_FILE_NAME);
        if legacy_path.exists() {
            let _ = fs::copy(&legacy_path, &file_path);
        }
    }

    if file_path.exists() {
        if let Ok(raw) = fs::read_to_string(&file_path)
            && let Ok(Value::Object(mut map)) = serde_json::from_str::<Value>(&raw)
            && !map.contains_key("expansions")
        {
            map.insert("expansions".to_string(), json!({ "rpr": "review PR" }));
            if let Ok(serialized) = serde_json::to_string_pretty(&Value::Object(map)) {
                let _ = fs::write(&file_path, serialized);
            }
        }
    } else {
        let default_cfg = default_autocomplete_config();
        if let Ok(serialized) = serde_json::to_string_pretty(&default_cfg) {
            if let Err(err) = fs::write(&file_path, serialized) {
                eprintln!("Failed to create default autocomplete.json: {err}");
            }
        }
    }

    file_path
}

/// Loads and sanitizes `autocomplete.json`.
/// Ports `src/main/autocomplete.ts:105-121`.
pub fn load_autocomplete_config() -> AutocompleteConfig {
    let file_path = ensure_autocomplete_file();
    load_autocomplete_config_from(&file_path)
}

pub fn load_autocomplete_config_from(file_path: &Path) -> AutocompleteConfig {
    match fs::read_to_string(file_path) {
        Ok(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(Value::Object(map)) => {
                let extract_strings = |key: &str| -> Vec<String> {
                    map.get(key)
                        .and_then(Value::as_array)
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                                .collect()
                        })
                        .unwrap_or_default()
                };

                let raw_expansions: HashMap<String, String> = map
                    .get("expansions")
                    .and_then(Value::as_object)
                    .map(|obj| {
                        obj.iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    })
                    .unwrap_or_default();

                AutocompleteConfig {
                    tags: sanitize_list(extract_strings("tags")),
                    projects: sanitize_list(extract_strings("projects")),
                    mentions: sanitize_list(extract_strings("mentions")),
                    expansions: sanitize_expansions(raw_expansions),
                }
            }
            _ => default_autocomplete_config(),
        },
        Err(err) => {
            eprintln!("Error reading autocomplete.json: {err}");
            default_autocomplete_config()
        }
    }
}

/// Formats an Electron accelerator string for display in UI badges.
/// Ports `src/main/config.ts:115-144`.
pub fn format_hotkey_for_display(accelerator: &str) -> String {
    format_hotkey_for_platform(accelerator, cfg!(target_os = "macos"))
}

pub fn format_hotkey_for_platform(accelerator: &str, is_macos: bool) -> String {
    if accelerator.is_empty() {
        return String::new();
    }

    if is_macos {
        accelerator
            .split('+')
            .map(|part| {
                let p = part.trim();
                let lower = p.to_ascii_lowercase();
                match lower.as_str() {
                    "cmdorctrl" | "commandorcontrol" | "cmd" | "command" => "⌘".to_string(),
                    "option" | "alt" => "⌥".to_string(),
                    "shift" => "⇧".to_string(),
                    "ctrl" | "control" => "⌃".to_string(),
                    _ => p.to_string(),
                }
            })
            .collect::<Vec<_>>()
            .join("")
    } else {
        accelerator
            .split('+')
            .map(|part| {
                let p = part.trim();
                let lower = p.to_ascii_lowercase();
                match lower.as_str() {
                    "cmdorctrl" | "commandorcontrol" => "Ctrl".to_string(),
                    "option" => "Alt".to_string(),
                    "command" | "cmd" => "Win".to_string(),
                    "ctrl" | "control" => "Ctrl".to_string(),
                    _ => {
                        let mut chars = p.chars();
                        match chars.next() {
                            Some(first) => {
                                let upper: String = first.to_uppercase().collect();
                                format!("{upper}{}", chars.as_str())
                            }
                            None => String::new(),
                        }
                    }
                }
            })
            .collect::<Vec<_>>()
            .join("+")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WatchedFile {
    Config,
    Autocomplete,
}

pub struct ConfigStore {
    pub app_config: AppConfig,
    pub autocomplete: AutocompleteConfig,
    pub active_hotkey: Option<String>,
    pub formatted_hotkey: String,
    pub restart_required: bool,
    pub is_dark: bool,
    pub config_path: PathBuf,
    pub autocomplete_path: PathBuf,
    _watcher_task: Task<()>,
}

impl ConfigStore {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let config_path = ensure_config_file();
        let autocomplete_path = ensure_autocomplete_file();

        let app_config = load_config_from(&config_path);
        let autocomplete = load_autocomplete_config_from(&autocomplete_path);

        let active_hotkey = Some(app_config.global_hotkey.clone());
        let formatted_hotkey = format_hotkey_for_display(&app_config.global_hotkey);

        let os_is_dark = theme::is_appearance_dark(cx.window_appearance());
        let is_dark = app_config.theme.resolve_is_dark(os_is_dark);

        cx.set_global(ActiveTheme::from_is_dark(is_dark, app_config.theme));
        Self::sync_platform_window_appearance(app_config.theme, cx);

        let watch_dir = user_data_dir();
        let watcher_task =
            Self::spawn_watcher(watch_dir, config_path.clone(), autocomplete_path.clone(), cx);

        Self {
            app_config,
            autocomplete,
            active_hotkey,
            formatted_hotkey,
            restart_required: false,
            is_dark,
            config_path,
            autocomplete_path,
            _watcher_task: watcher_task,
        }
    }

    fn sync_platform_window_appearance(mode: ThemeMode, cx: &App) {
        let appearance = match mode {
            ThemeMode::Dark => Some(WindowAppearance::Dark),
            ThemeMode::Light => Some(WindowAppearance::Light),
            ThemeMode::System => None,
        };
        cx.set_window_appearance(appearance);
    }

    fn spawn_watcher(
        watch_dir: PathBuf,
        config_path: PathBuf,
        autocomplete_path: PathBuf,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let (tx, rx) = smol::channel::unbounded::<WatchedFile>();

        let watcher_result = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                if matches!(event.kind, EventKind::Access(_)) {
                    return;
                }
                for path in &event.paths {
                    if path.file_name() == Some(OsStr::new(CONFIG_FILE_NAME)) {
                        let _ = tx.try_send(WatchedFile::Config);
                    } else if path.file_name() == Some(OsStr::new(AUTOCOMPLETE_FILE_NAME)) {
                        let _ = tx.try_send(WatchedFile::Autocomplete);
                    }
                }
            }
        });

        cx.spawn(async move |this, cx| {
            let mut _watcher: Option<RecommendedWatcher> = match watcher_result {
                Ok(mut watcher) => {
                    if let Err(err) = watcher.watch(&watch_dir, RecursiveMode::NonRecursive) {
                        eprintln!("Failed to watch userData directory {watch_dir:?}: {err}");
                        None
                    } else {
                        Some(watcher)
                    }
                }
                Err(err) => {
                    eprintln!("Failed to initialize file watcher: {err}");
                    None
                }
            };

            let mut config_debounce_task: Option<Task<()>> = None;
            let mut autocomplete_debounce_task: Option<Task<()>> = None;

            while let Ok(changed) = rx.recv().await {
                match changed {
                    WatchedFile::Config => {
                        let config_path = config_path.clone();
                        let this = this.clone();
                        let executor = cx.background_executor().clone();
                        config_debounce_task = Some(cx.spawn(async move |cx| {
                            executor.timer(Duration::from_millis(200)).await;
                            let updated_config = executor
                                .spawn(async move { load_config_from(&config_path) })
                                .await;

                            let _ = this.update(cx, |store, cx| {
                                let hotkey_changed = store.active_hotkey.as_deref()
                                    != Some(updated_config.global_hotkey.as_str());
                                if hotkey_changed {
                                    store.restart_required = true;
                                }

                                let theme_changed = updated_config.theme != store.app_config.theme;
                                store.app_config = updated_config;
                                store.formatted_hotkey =
                                    format_hotkey_for_display(&store.app_config.global_hotkey);

                                if theme_changed {
                                    Self::sync_platform_window_appearance(
                                        store.app_config.theme,
                                        cx,
                                    );
                                    let os_is_dark =
                                        theme::is_appearance_dark(cx.window_appearance());
                                    store.is_dark =
                                        store.app_config.theme.resolve_is_dark(os_is_dark);
                                    cx.set_global(ActiveTheme::from_is_dark(
                                        store.is_dark,
                                        store.app_config.theme,
                                    ));
                                    cx.refresh_windows();
                                }

                                println!(
                                    "[ConfigStore] Live-reloaded config.json (hotkey={}, theme={:?}, restart_required={})",
                                    store.app_config.global_hotkey,
                                    store.app_config.theme,
                                    store.restart_required
                                );
                                cx.notify();
                            });
                        }));
                    }
                    WatchedFile::Autocomplete => {
                        let autocomplete_path = autocomplete_path.clone();
                        let this = this.clone();
                        let executor = cx.background_executor().clone();
                        autocomplete_debounce_task = Some(cx.spawn(async move |cx| {
                            executor.timer(Duration::from_millis(150)).await;
                            let updated_auto = executor
                                .spawn(async move {
                                    ensure_autocomplete_file();
                                    load_autocomplete_config_from(&autocomplete_path)
                                })
                                .await;

                            let _ = this.update(cx, |store, cx| {
                                store.autocomplete = updated_auto;
                                println!(
                                    "[ConfigStore] Live-reloaded autocomplete.json (tags={}, projects={}, mentions={}, expansions={})",
                                    store.autocomplete.tags.len(),
                                    store.autocomplete.projects.len(),
                                    store.autocomplete.mentions.len(),
                                    store.autocomplete.expansions.len()
                                );
                                cx.notify();
                            });
                        }));
                    }
                }
                // Keep debounce tasks alive across iterations
                let _ = (&config_debounce_task, &autocomplete_debounce_task);
            }
        })
    }

    pub fn set_active_hotkey(&mut self, hotkey: Option<String>, cx: &mut Context<Self>) {
        self.active_hotkey = hotkey;
        if let Some(ref active) = self.active_hotkey {
            self.formatted_hotkey = format_hotkey_for_display(active);
            self.restart_required = *active != self.app_config.global_hotkey;
        }
        cx.notify();
    }

    pub fn apply_theme_target(&mut self, target: ThemeTarget, cx: &mut Context<Self>) {
        let mode = match target {
            ThemeTarget::Dark => ThemeMode::Dark,
            ThemeTarget::Light => ThemeMode::Light,
            ThemeTarget::System => ThemeMode::System,
            ThemeTarget::Toggle => {
                if self.is_dark {
                    ThemeMode::Light
                } else {
                    ThemeMode::Dark
                }
            }
        };
        self.set_theme(mode, cx);
    }

    pub fn set_theme(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.app_config.theme = mode;
        Self::sync_platform_window_appearance(mode, cx);
        let os_is_dark = theme::is_appearance_dark(cx.window_appearance());
        self.is_dark = mode.resolve_is_dark(os_is_dark);

        cx.set_global(ActiveTheme::from_is_dark(self.is_dark, mode));
        cx.refresh_windows();
        cx.notify();

        let path = self.config_path.clone();
        cx.background_executor()
            .spawn(async move {
                save_theme_to_config_file(&path, mode);
            })
            .detach();
    }

    pub fn sync_os_appearance(&mut self, cx: &mut Context<Self>) {
        if self.app_config.theme == ThemeMode::System {
            let os_is_dark = theme::is_appearance_dark(cx.window_appearance());
            if self.is_dark != os_is_dark {
                self.is_dark = os_is_dark;
                cx.set_global(ActiveTheme::from_is_dark(self.is_dark, ThemeMode::System));
                cx.refresh_windows();
                cx.notify();
            }
        }
    }

    pub fn open_config(&self, cx: &App) {
        let path = self.config_path.clone();
        cx.background_executor()
            .spawn(async move {
                let _ = ensure_config_file();
                if let Err(err) = open::that(&path) {
                    eprintln!("Failed to open config.json: {err}");
                }
            })
            .detach();
    }

    pub fn open_autocomplete_config(&self, cx: &App) {
        let path = self.autocomplete_path.clone();
        cx.background_executor()
            .spawn(async move {
                let _ = ensure_autocomplete_file();
                if let Err(err) = open::that(&path) {
                    eprintln!("Failed to open autocomplete.json: {err}");
                }
            })
            .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_format_hotkey_for_display_macos_and_other() {
        assert_eq!(format_hotkey_for_platform("", true), "");
        assert_eq!(format_hotkey_for_platform("Option+Space", true), "⌥Space");
        assert_eq!(
            format_hotkey_for_platform("CommandOrControl+Shift+K", true),
            "⌘⇧K"
        );
        assert_eq!(format_hotkey_for_platform("Ctrl+Alt+P", true), "⌃⌥P");

        assert_eq!(
            format_hotkey_for_platform("Option+Space", false),
            "Alt+Space"
        );
        assert_eq!(
            format_hotkey_for_platform("CommandOrControl+space", false),
            "Ctrl+Space"
        );
        assert_eq!(format_hotkey_for_platform("Cmd+Shift+k", false), "Win+Shift+K");
    }

    #[test]
    fn test_ensure_and_load_config_file() {
        let tmp = TempDir::new().unwrap();
        let user_data = tmp.path().join("TaskConveyor");

        let config_path = ensure_config_file_in(&user_data, None);
        assert!(config_path.exists());

        let loaded = load_config_from(&config_path);
        assert_eq!(loaded.global_hotkey, default_hotkey());
        assert_eq!(loaded.theme, ThemeMode::System);

        // Save theme while preserving globalHotkey and custom fields
        fs::write(
            &config_path,
            r#"{"globalHotkey": "Ctrl+Shift+Space", "customField": 42}"#,
        )
        .unwrap();
        let updated = save_theme_to_config_file(&config_path, ThemeMode::Dark);
        assert_eq!(updated.global_hotkey, "Ctrl+Shift+Space");
        assert_eq!(updated.theme, ThemeMode::Dark);

        let raw_json: Value =
            serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
        assert_eq!(raw_json["customField"], json!(42));
        assert_eq!(raw_json["theme"], json!("dark"));
    }

    #[test]
    fn test_ensure_autocomplete_injects_missing_expansions_and_sanitizes() {
        let tmp = TempDir::new().unwrap();
        let user_data = tmp.path().join("TaskConveyor");
        fs::create_dir_all(&user_data).unwrap();

        // Write an existing autocomplete.json that lacks the "expansions" key
        let auto_path = user_data.join(AUTOCOMPLETE_FILE_NAME);
        fs::write(
            &auto_path,
            r##"{
  "tags": [" #Urgent ", "urgent", "backend"],
  "projects": ["$Apollo"],
  "mentions": ["@Alice"]
}"##,
        )
        .unwrap();

        let ensured_path = ensure_autocomplete_file_in(&user_data, None);
        assert_eq!(ensured_path, auto_path);

        // Verify "expansions": { "rpr": "review PR" } was injected into the file on disk
        let raw_on_disk: Value =
            serde_json::from_str(&fs::read_to_string(&auto_path).unwrap()).unwrap();
        assert_eq!(raw_on_disk["expansions"]["rpr"], json!("review PR"));

        // Verify load_autocomplete_config_from sanitizes prefixes and deduplicates case-insensitively
        let loaded = load_autocomplete_config_from(&auto_path);
        assert_eq!(loaded.tags, vec!["Urgent", "backend"]);
        assert_eq!(loaded.projects, vec!["Apollo"]);
        assert_eq!(loaded.mentions, vec!["Alice"]);
        assert_eq!(
            loaded.expansions.get("rpr").map(String::as_str),
            Some("review PR")
        );
    }

    #[test]
    fn test_notify_watcher_detects_config_and_autocomplete_changes() {
        let tmp = TempDir::new().unwrap();
        let user_data = tmp.path().join("TaskConveyor");
        let config_path = ensure_config_file_in(&user_data, None);
        let auto_path = ensure_autocomplete_file_in(&user_data, None);

        let (tx, rx) = smol::channel::unbounded::<WatchedFile>();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                if matches!(event.kind, EventKind::Access(_)) {
                    return;
                }
                for path in &event.paths {
                    if path.file_name() == Some(OsStr::new(CONFIG_FILE_NAME)) {
                        let _ = tx.try_send(WatchedFile::Config);
                    } else if path.file_name() == Some(OsStr::new(AUTOCOMPLETE_FILE_NAME)) {
                        let _ = tx.try_send(WatchedFile::Autocomplete);
                    }
                }
            }
        })
        .unwrap();

        watcher
            .watch(&user_data, RecursiveMode::NonRecursive)
            .unwrap();

        save_theme_to_config_file(&config_path, ThemeMode::Dark);
        fs::write(&auto_path, r#"{"tags":["live-reload"]}"#).unwrap();

        let mut saw_config = false;
        let mut saw_auto = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline && (!saw_config || !saw_auto) {
            if let Ok(evt) = rx.try_recv() {
                match evt {
                    WatchedFile::Config => saw_config = true,
                    WatchedFile::Autocomplete => saw_auto = true,
                }
            } else {
                std::thread::sleep(Duration::from_millis(20));
            }
        }

        assert!(saw_config, "Expected notify watcher event for config.json");
        assert!(
            saw_auto,
            "Expected notify watcher event for autocomplete.json"
        );
    }
}
