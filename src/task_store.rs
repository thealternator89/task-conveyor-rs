use crate::config_store::ConfigStore;
use crate::ical::{fetch_and_select_event, normalize_ical_url};
use crate::paths::tasks_file_path;
use crate::task_engine::{
    CalendarEventSelector, CalendarSlotTarget, CommandOutcome, TaskEngine, TaskItem,
};
use crate::window_manager::WindowManager;
use chrono::Local;
use gpui::{Context, Entity, Task};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Loads persisted tasks from `<userData>/tasks.json`, gracefully returning an empty list
/// when the file does not exist or contains invalid JSON.
pub fn load_tasks_from(path: &Path) -> Vec<TaskItem> {
    if !path.exists() {
        return Vec::new();
    }
    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str::<Vec<TaskItem>>(&raw).unwrap_or_else(|err| {
            eprintln!("Failed to parse tasks.json ({path:?}), defaulting to empty list: {err}");
            Vec::new()
        }),
        Err(err) => {
            eprintln!("Failed to read tasks.json ({path:?}), defaulting to empty list: {err}");
            Vec::new()
        }
    }
}

/// Writes `tasks` to `path` atomically via a sibling `.tmp` file + rename.
pub fn write_tasks_atomic(path: &Path, tasks: &[TaskItem]) -> io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
    }

    let serialized = serde_json::to_string_pretty(tasks)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, serialized)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

pub struct TaskStore {
    pub engine: TaskEngine,
    pub warning: Option<String>,
    warning_timer: Option<Task<()>>,
    calendar_fetch_task: Option<Task<()>>,
    pub storage_path: PathBuf,
    config_store: Entity<ConfigStore>,
    window_manager: Option<Entity<WindowManager>>,
}

impl TaskStore {
    pub fn new(config_store: Entity<ConfigStore>, _cx: &mut Context<Self>) -> Self {
        let storage_path = tasks_file_path();
        let initial_tasks = load_tasks_from(&storage_path);
        Self {
            engine: TaskEngine::with_tasks(initial_tasks),
            warning: None,
            warning_timer: None,
            calendar_fetch_task: None,
            storage_path,
            config_store,
            window_manager: None,
        }
    }

    pub fn set_window_manager(&mut self, window_manager: Entity<WindowManager>) {
        self.window_manager = Some(window_manager);
    }

    pub fn tasks(&self) -> &[TaskItem] {
        &self.engine.tasks
    }

    pub fn history(&self) -> Option<&[TaskItem]> {
        self.engine.history.as_deref()
    }

    pub fn persist_tasks(&self, cx: &Context<Self>) {
        let tasks_snapshot = self.engine.tasks.clone();
        let path = self.storage_path.clone();
        cx.background_executor()
            .spawn(async move {
                if let Err(err) = write_tasks_atomic(&path, &tasks_snapshot) {
                    eprintln!("Failed to persist tasks.json: {err}");
                }
            })
            .detach();
    }

    pub fn set_warning(&mut self, msg: impl Into<String>, cx: &mut Context<Self>) {
        self.warning = Some(msg.into());
        cx.notify();

        let executor = cx.background_executor().clone();
        self.warning_timer = Some(cx.spawn(async move |this, cx| {
            executor.timer(Duration::from_secs(3)).await;
            let _ = this.update(cx, |store, cx| {
                store.warning = None;
                cx.notify();
            });
        }));
    }

    pub fn execute_command(&mut self, raw_text: &str, cx: &mut Context<Self>) -> CommandOutcome {
        let outcome = self.engine.execute(raw_text);

        match &outcome {
            CommandOutcome::TasksUpdated => {
                self.persist_tasks(cx);
                cx.notify();
            }
            CommandOutcome::Warning(msg) => {
                self.set_warning(msg.clone(), cx);
            }
            CommandOutcome::OpenConfig => {
                self.config_store.read(cx).open_config(cx);
            }
            CommandOutcome::OpenAutocompleteConfig => {
                self.config_store.read(cx).open_autocomplete_config(cx);
            }
            CommandOutcome::SetTheme(target) => {
                let target = *target;
                self.config_store.update(cx, |config, cx| {
                    config.apply_theme_target(target, cx);
                });
            }
            CommandOutcome::Quit => {
                if let Some(wm) = self.window_manager.clone() {
                    wm.update(cx, |wm, cx| {
                        wm.cleanup_on_quit(cx);
                    });
                }
                cx.quit();
            }
            CommandOutcome::TogglePin => {
                if let Some(wm) = self.window_manager.clone() {
                    wm.update(cx, |wm, cx| {
                        wm.toggle_always_on_top(cx);
                    });
                }
            }
            CommandOutcome::Dock(side) => {
                let side = *side;
                if let Some(wm) = self.window_manager.clone() {
                    wm.update(cx, |wm, cx| {
                        wm.dock_window(side, cx);
                    });
                }
            }
            CommandOutcome::Float => {
                if let Some(wm) = self.window_manager.clone() {
                    wm.update(cx, |wm, cx| {
                        wm.float_window(cx);
                    });
                }
            }
            CommandOutcome::FetchCalendarEvent { selector, target } => {
                return self.start_calendar_fetch(*selector, *target, cx);
            }
            CommandOutcome::NoOp => {}
        }

        outcome
    }

    fn start_calendar_fetch(
        &mut self,
        selector: CalendarEventSelector,
        target: CalendarSlotTarget,
        cx: &mut Context<Self>,
    ) -> CommandOutcome {
        let raw_url = self.config_store.read(cx).app_config.ical_url.clone();
        let Some(url) = normalize_ical_url(&raw_url) else {
            let msg = "No iCal feed URL configured. Run /config to set \"icalUrl\".".to_string();
            self.set_warning(msg.clone(), cx);
            return CommandOutcome::Warning(msg);
        };

        self.warning_timer = None;
        self.warning = Some("Fetching calendar...".to_string());
        cx.notify();

        let executor = cx.background_executor().clone();
        self.calendar_fetch_task = Some(cx.spawn(async move |this, cx| {
            let now = Local::now();
            let fetch_result = executor
                .spawn(async move { fetch_and_select_event(&url, selector, now) })
                .await;

            let _ = this.update(cx, |store, cx| {
                store.warning = None;
                match fetch_result {
                    Ok(summary) => match store.engine.apply_calendar_event(&summary, target) {
                        CommandOutcome::TasksUpdated => {
                            store.persist_tasks(cx);
                            cx.notify();
                        }
                        CommandOutcome::Warning(msg) => {
                            store.set_warning(msg, cx);
                        }
                        _ => {
                            cx.notify();
                        }
                    },
                    Err(err_msg) => {
                        store.set_warning(err_msg, cx);
                    }
                }
            });
        }));

        CommandOutcome::FetchCalendarEvent { selector, target }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_and_persist_tasks_atomic() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("TaskConveyor/tasks.json");

        // Missing file returns empty Vec
        assert!(load_tasks_from(&path).is_empty());

        let sample = vec![
            TaskItem {
                id: "1".to_string(),
                text: "Ship Phase 2 #urgent".to_string(),
                completed: false,
                important: true,
                is_break: false,
            },
            TaskItem {
                id: "2".to_string(),
                text: "Break ☕".to_string(),
                completed: false,
                important: false,
                is_break: true,
            },
        ];

        write_tasks_atomic(&path, &sample).unwrap();
        assert!(path.exists());
        assert!(!path.with_extension("json.tmp").exists());

        let loaded = load_tasks_from(&path);
        assert_eq!(loaded, sample);

        // Corrupt JSON gracefully returns empty Vec
        fs::write(&path, "{ invalid json").unwrap();
        assert!(load_tasks_from(&path).is_empty());
    }
}
