use crate::config_store::ConfigStore;
use crate::paths::tasks_file_path;
use crate::task_engine::{CommandOutcome, TaskEngine, TaskItem};
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
    pub storage_path: PathBuf,
    config_store: Entity<ConfigStore>,
}

impl TaskStore {
    pub fn new(config_store: Entity<ConfigStore>, _cx: &mut Context<Self>) -> Self {
        let storage_path = tasks_file_path();
        let initial_tasks = load_tasks_from(&storage_path);
        Self {
            engine: TaskEngine::with_tasks(initial_tasks),
            warning: None,
            warning_timer: None,
            storage_path,
            config_store,
        }
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
                cx.quit();
            }
            CommandOutcome::TogglePin
            | CommandOutcome::Dock(_)
            | CommandOutcome::Float
            | CommandOutcome::NoOp => {}
        }

        outcome
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
