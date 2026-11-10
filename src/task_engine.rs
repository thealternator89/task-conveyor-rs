use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

pub const HELP_MESSAGE: &str = "Commands: /done [b], /break, /move x y, /move x u|d [y], /remove x, /edit x text, /undo, /important, /pin, /dock [l|r], /float, /config, /tags, /theme [dark|light|system], /clear, /exit, /help (Use #tag, $project, @mention + [Tab])";

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DockSide {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeTarget {
    Dark,
    Light,
    System,
    Toggle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandOutcome {
    TasksUpdated,
    Warning(String),
    TogglePin,
    Dock(DockSide),
    Float,
    OpenConfig,
    OpenAutocompleteConfig,
    SetTheme(ThemeTarget),
    Quit,
    NoOp,
}

static RE_DONE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^/d(?:one)?(?:\s+(b(?:reak)?))?$").unwrap());
static RE_BREAK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^/b(?:reak)?$").unwrap());
static RE_MOVE_REL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^/m(?:ove)?\s+(\d+)\s+([ud])(?:\s+(\d+))?$").unwrap());
static RE_MOVE_ABS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^/m(?:ove)?\s+(\d+)\s+(\d+)$").unwrap());
static RE_REMOVE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^/r(?:e(?:move)?)?\s+(\d+)$").unwrap());
static RE_EDIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^/e(?:dit)?\s+(\d+)\s+(.+)$").unwrap());
static RE_UNDO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^/u(?:ndo)?$").unwrap());
static RE_IMPORTANT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^/i(?:mportant)?$").unwrap());
static RE_CLEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^/c(?:lear)?$").unwrap());
static RE_PIN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^/p(?:in)?$").unwrap());
static RE_DOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^/dock(?:\s+(l|left|r|right))?$").unwrap());
static RE_FLOAT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^/(?:float|undock)$").unwrap());
static RE_CONFIG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^/config$").unwrap());
static RE_TAGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^/(?:tags|autocomplete)$").unwrap());
static RE_THEME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^/theme(?:\s+(dark|light|system|auto))?$").unwrap());
static RE_DARK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^/dark$").unwrap());
static RE_LIGHT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^/light$").unwrap());
static RE_QUIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^/(?:x|exit|q|quit)$").unwrap());
static RE_HELP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^/(?:h(?:elp)?|\?)$").unwrap());

#[derive(Clone, Debug, Default)]
pub struct TaskEngine {
    pub tasks: Vec<TaskItem>,
    pub history: Option<Vec<TaskItem>>,
    next_id_counter: u64,
}

impl TaskEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_tasks(tasks: Vec<TaskItem>) -> Self {
        Self {
            tasks,
            history: None,
            next_id_counter: 0,
        }
    }

    fn generate_id(&mut self) -> String {
        self.next_id_counter = self.next_id_counter.wrapping_add(1);
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        format!("{millis}-{}", self.next_id_counter)
    }

    fn is_demotion(&self, from_index: usize, to_index: usize) -> bool {
        self.tasks.first().is_some_and(|t| t.important)
            && ((from_index == 0 && to_index != 0) || (from_index != 0 && to_index == 0))
    }

    fn save_history(&mut self) {
        self.history = Some(self.tasks.clone());
    }

    pub fn execute(&mut self, raw_text: &str) -> CommandOutcome {
        let text = raw_text.trim();
        if text.is_empty() {
            return CommandOutcome::NoOp;
        }

        // /d[one] [b[reak]]
        if let Some(caps) = RE_DONE.captures(text) {
            if self.tasks.is_empty() {
                return CommandOutcome::Warning("No tasks to complete.".to_string());
            }
            self.save_history();
            self.tasks.remove(0);
            if caps.get(1).is_some() {
                let break_item = TaskItem {
                    id: self.generate_id(),
                    text: "Break ☕".to_string(),
                    completed: false,
                    important: false,
                    is_break: true,
                };
                self.tasks.insert(0, break_item);
            }
            return CommandOutcome::TasksUpdated;
        }

        // /b[reak]
        if RE_BREAK.is_match(text) {
            if self.tasks.first().is_some_and(|t| t.important) {
                return CommandOutcome::Warning(
                    "Cannot insert break: current task is important and cannot be demoted."
                        .to_string(),
                );
            }
            if self.tasks.first().is_some_and(|t| t.is_break) {
                return CommandOutcome::Warning("Already on a break.".to_string());
            }
            self.save_history();
            let break_item = TaskItem {
                id: self.generate_id(),
                text: "Break ☕".to_string(),
                completed: false,
                important: false,
                is_break: true,
            };
            self.tasks.insert(0, break_item);
            return CommandOutcome::TasksUpdated;
        }

        // /m[ove] x u|d [y]
        if let Some(caps) = RE_MOVE_REL.captures(text) {
            let Ok(from_index) = caps[1].parse::<usize>() else {
                return CommandOutcome::Warning("Move index out of bounds.".to_string());
            };
            if self.tasks.is_empty() || from_index >= self.tasks.len() {
                return CommandOutcome::Warning("Move index out of bounds.".to_string());
            }
            let offset = caps
                .get(3)
                .and_then(|m| m.as_str().parse::<usize>().ok())
                .unwrap_or(1);
            let dir = caps[2].to_ascii_lowercase();
            let to_index = if dir == "u" {
                from_index.saturating_sub(offset)
            } else {
                (from_index + offset).min(self.tasks.len() - 1)
            };

            return self.perform_move(from_index, to_index);
        }

        // /m[ove] x y
        if let Some(caps) = RE_MOVE_ABS.captures(text) {
            let (Ok(from_index), Ok(to_index)) =
                (caps[1].parse::<usize>(), caps[2].parse::<usize>())
            else {
                return CommandOutcome::Warning("Move index out of bounds.".to_string());
            };
            if from_index >= self.tasks.len() || to_index >= self.tasks.len() {
                return CommandOutcome::Warning("Move index out of bounds.".to_string());
            }

            return self.perform_move(from_index, to_index);
        }

        // /r[emove] x
        if let Some(caps) = RE_REMOVE.captures(text) {
            let Ok(index) = caps[1].parse::<usize>() else {
                return CommandOutcome::Warning("Remove index out of bounds.".to_string());
            };
            if index >= self.tasks.len() {
                return CommandOutcome::Warning("Remove index out of bounds.".to_string());
            }
            self.save_history();
            self.tasks.remove(index);
            return CommandOutcome::TasksUpdated;
        }

        // /e[dit] x <text>
        if let Some(caps) = RE_EDIT.captures(text) {
            let Ok(index) = caps[1].parse::<usize>() else {
                return CommandOutcome::Warning("Edit index out of bounds.".to_string());
            };
            if index >= self.tasks.len() {
                return CommandOutcome::Warning("Edit index out of bounds.".to_string());
            }
            let new_text = caps[2].trim();
            self.save_history();
            self.tasks[index].text = new_text.to_string();
            self.tasks[index].is_break = false;
            return CommandOutcome::TasksUpdated;
        }

        // /u[ndo]
        if RE_UNDO.is_match(text) {
            if let Some(prev) = self.history.take() {
                self.tasks = prev;
                return CommandOutcome::TasksUpdated;
            } else {
                return CommandOutcome::Warning("Nothing to undo.".to_string());
            }
        }

        // /i[mportant]
        if RE_IMPORTANT.is_match(text) {
            if self.tasks.is_empty() {
                return CommandOutcome::Warning("No current task to mark important.".to_string());
            }
            if self.tasks[0].is_break {
                return CommandOutcome::Warning("Cannot mark a break as important.".to_string());
            }
            self.save_history();
            self.tasks[0].important = !self.tasks[0].important;
            return CommandOutcome::TasksUpdated;
        }

        // /c[lear]
        if RE_CLEAR.is_match(text) {
            if self.tasks.is_empty() {
                return CommandOutcome::Warning("List is already empty.".to_string());
            }
            self.save_history();
            self.tasks.clear();
            return CommandOutcome::TasksUpdated;
        }

        // /p[in]
        if RE_PIN.is_match(text) {
            return CommandOutcome::TogglePin;
        }

        // /dock [l|left|r|right]
        if let Some(caps) = RE_DOCK.captures(text) {
            let side = match caps.get(1).map(|m| m.as_str().to_ascii_lowercase()) {
                Some(ref s) if s == "l" || s == "left" => DockSide::Left,
                _ => DockSide::Right,
            };
            return CommandOutcome::Dock(side);
        }

        // /float or /undock
        if RE_FLOAT.is_match(text) {
            return CommandOutcome::Float;
        }

        // /config
        if RE_CONFIG.is_match(text) {
            return CommandOutcome::OpenConfig;
        }

        // /tags or /autocomplete
        if RE_TAGS.is_match(text) {
            return CommandOutcome::OpenAutocompleteConfig;
        }

        // /dark, /light, /theme [dark|light|system|auto]
        if RE_DARK.is_match(text) {
            return CommandOutcome::SetTheme(ThemeTarget::Dark);
        }
        if RE_LIGHT.is_match(text) {
            return CommandOutcome::SetTheme(ThemeTarget::Light);
        }
        if let Some(caps) = RE_THEME.captures(text) {
            let target = match caps.get(1).map(|m| m.as_str().to_ascii_lowercase()) {
                Some(ref s) if s == "dark" => ThemeTarget::Dark,
                Some(ref s) if s == "light" => ThemeTarget::Light,
                Some(ref s) if s == "system" || s == "auto" => ThemeTarget::System,
                _ => ThemeTarget::Toggle,
            };
            return CommandOutcome::SetTheme(target);
        }

        // /x, /exit, /q, /quit
        if RE_QUIT.is_match(text) {
            return CommandOutcome::Quit;
        }

        // /h[elp] or /?
        if RE_HELP.is_match(text) {
            return CommandOutcome::Warning(HELP_MESSAGE.to_string());
        }

        // Unrecognized slash command
        if text.starts_with('/') {
            return CommandOutcome::Warning(format!("Unrecognized command: {text}"));
        }

        // Task Insertion: `!!<text>`, `!<text>`, or `<text>`
        if let Some(rest) = text.strip_prefix("!!") {
            let task_text = rest.trim();
            if task_text.is_empty() {
                return CommandOutcome::NoOp;
            }
            if self.tasks.first().is_some_and(|t| t.important) {
                return CommandOutcome::Warning(
                    "Cannot prepend task: current task is important and cannot be demoted."
                        .to_string(),
                );
            }
            self.save_history();
            let item = TaskItem {
                id: self.generate_id(),
                text: task_text.to_string(),
                completed: false,
                important: false,
                is_break: false,
            };
            if self.tasks.first().is_some_and(|t| t.is_break) {
                self.tasks[0] = item;
            } else {
                self.tasks.insert(0, item);
            }
            return CommandOutcome::TasksUpdated;
        }

        if let Some(rest) = text.strip_prefix('!') {
            let task_text = rest.trim();
            if task_text.is_empty() {
                return CommandOutcome::NoOp;
            }
            self.save_history();
            let item = TaskItem {
                id: self.generate_id(),
                text: task_text.to_string(),
                completed: false,
                important: false,
                is_break: false,
            };
            if self.tasks.is_empty() {
                self.tasks.push(item);
            } else {
                self.tasks.insert(1, item);
            }
            return CommandOutcome::TasksUpdated;
        }

        self.save_history();
        let item = TaskItem {
            id: self.generate_id(),
            text: text.to_string(),
            completed: false,
            important: false,
            is_break: false,
        };
        self.tasks.push(item);
        CommandOutcome::TasksUpdated
    }

    fn perform_move(&mut self, from_index: usize, to_index: usize) -> CommandOutcome {
        if self.tasks[from_index].is_break {
            return CommandOutcome::Warning("Cannot move a break.".to_string());
        }
        if self.is_demotion(from_index, to_index) {
            return CommandOutcome::Warning(
                "Action rejected: current task is important and cannot be demoted.".to_string(),
            );
        }

        self.save_history();
        if to_index == 0 && from_index != 0 && self.tasks[0].is_break {
            let moved = self.tasks.remove(from_index);
            self.tasks[0] = moved;
        } else if from_index != to_index {
            let moved = self.tasks.remove(from_index);
            self.tasks.insert(to_index, moved);
        }
        CommandOutcome::TasksUpdated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(engine: &TaskEngine) -> Vec<&str> {
        engine.tasks.iter().map(|t| t.text.as_str()).collect()
    }

    #[test]
    fn test_task_insertions_and_break_replacement() {
        let mut engine = TaskEngine::new();

        // Standard append
        assert_eq!(engine.execute("Task A"), CommandOutcome::TasksUpdated);
        assert_eq!(engine.execute("Task B"), CommandOutcome::TasksUpdated);
        assert_eq!(texts(&engine), vec!["Task A", "Task B"]);

        // `!` inserts at index 1
        assert_eq!(engine.execute("!Task Next"), CommandOutcome::TasksUpdated);
        assert_eq!(texts(&engine), vec!["Task A", "Task Next", "Task B"]);

        // `!!` inserts at index 0
        assert_eq!(engine.execute("!!Task Urgent"), CommandOutcome::TasksUpdated);
        assert_eq!(
            texts(&engine),
            vec!["Task Urgent", "Task A", "Task Next", "Task B"]
        );

        // Insert a break at index 0, then `!!` replaces the break
        assert_eq!(engine.execute("/break"), CommandOutcome::TasksUpdated);
        assert!(engine.tasks[0].is_break);
        assert_eq!(
            texts(&engine),
            vec!["Break ☕", "Task Urgent", "Task A", "Task Next", "Task B"]
        );

        assert_eq!(
            engine.execute("!!Replace Break"),
            CommandOutcome::TasksUpdated
        );
        assert!(!engine.tasks[0].is_break);
        assert_eq!(
            texts(&engine),
            vec!["Replace Break", "Task Urgent", "Task A", "Task Next", "Task B"]
        );
    }

    #[test]
    fn test_done_and_break_commands() {
        let mut engine = TaskEngine::new();
        assert_eq!(
            engine.execute("/done"),
            CommandOutcome::Warning("No tasks to complete.".to_string())
        );

        engine.execute("Task 1");
        engine.execute("Task 2");

        // `/done b` completes index 0 and prepends a break
        assert_eq!(engine.execute("/done b"), CommandOutcome::TasksUpdated);
        assert_eq!(texts(&engine), vec!["Break ☕", "Task 2"]);
        assert!(engine.tasks[0].is_break);

        // Cannot start another break when already on a break
        assert_eq!(
            engine.execute("/b"),
            CommandOutcome::Warning("Already on a break.".to_string())
        );

        // Completing the break moves to Task 2
        assert_eq!(engine.execute("/d"), CommandOutcome::TasksUpdated);
        assert_eq!(texts(&engine), vec!["Task 2"]);
    }

    #[test]
    fn test_important_and_demotion_guards() {
        let mut engine = TaskEngine::new();
        assert_eq!(
            engine.execute("/important"),
            CommandOutcome::Warning("No current task to mark important.".to_string())
        );

        engine.execute("Task 0");
        engine.execute("Task 1");
        engine.execute("/break");
        assert_eq!(
            engine.execute("/important"),
            CommandOutcome::Warning("Cannot mark a break as important.".to_string())
        );
        engine.execute("/done"); // finish break

        // Mark Task 0 important
        assert_eq!(engine.execute("/i"), CommandOutcome::TasksUpdated);
        assert!(engine.tasks[0].important);

        // Demotion guards reject:
        // 1. `/break`
        assert_eq!(
            engine.execute("/break"),
            CommandOutcome::Warning(
                "Cannot insert break: current task is important and cannot be demoted.".to_string()
            )
        );
        // 2. `!!` prepend
        assert_eq!(
            engine.execute("!!New top task"),
            CommandOutcome::Warning(
                "Cannot prepend task: current task is important and cannot be demoted.".to_string()
            )
        );
        // 3. `/move 0 1` or `/move 0 d`
        assert_eq!(
            engine.execute("/move 0 1"),
            CommandOutcome::Warning(
                "Action rejected: current task is important and cannot be demoted.".to_string()
            )
        );
        assert_eq!(
            engine.execute("/move 0 d"),
            CommandOutcome::Warning(
                "Action rejected: current task is important and cannot be demoted.".to_string()
            )
        );
        // 4. `/move 1 0` or `/move 1 u`
        assert_eq!(
            engine.execute("/move 1 0"),
            CommandOutcome::Warning(
                "Action rejected: current task is important and cannot be demoted.".to_string()
            )
        );
        assert_eq!(
            engine.execute("/move 1 u"),
            CommandOutcome::Warning(
                "Action rejected: current task is important and cannot be demoted.".to_string()
            )
        );

        // Toggle important off -> moves allowed again
        assert_eq!(engine.execute("/important"), CommandOutcome::TasksUpdated);
        assert!(!engine.tasks[0].important);
        assert_eq!(engine.execute("/move 1 0"), CommandOutcome::TasksUpdated);
        assert_eq!(texts(&engine), vec!["Task 1", "Task 0"]);
    }

    #[test]
    fn test_move_commands_and_break_replacement() {
        let mut engine = TaskEngine::new();
        engine.execute("T0");
        engine.execute("T1");
        engine.execute("T2");

        // Relative move down & up with clamping
        assert_eq!(engine.execute("/m 0 d 2"), CommandOutcome::TasksUpdated);
        assert_eq!(texts(&engine), vec!["T1", "T2", "T0"]);

        assert_eq!(engine.execute("/m 2 u 10"), CommandOutcome::TasksUpdated);
        assert_eq!(texts(&engine), vec!["T0", "T1", "T2"]);

        // Break at index 0 cannot be moved, but moving another task to 0 replaces the break
        engine.execute("/break");
        assert_eq!(texts(&engine), vec!["Break ☕", "T0", "T1", "T2"]);
        assert_eq!(
            engine.execute("/m 0 1"),
            CommandOutcome::Warning("Cannot move a break.".to_string())
        );

        // Move index 2 ("T1") to 0 -> replaces "Break ☕"
        assert_eq!(engine.execute("/m 2 0"), CommandOutcome::TasksUpdated);
        assert_eq!(texts(&engine), vec!["T1", "T0", "T2"]);
    }

    #[test]
    fn test_remove_edit_clear_undo() {
        let mut engine = TaskEngine::new();
        engine.execute("First");
        engine.execute("Second");

        // Edit
        assert_eq!(
            engine.execute("/e 1 Second Edited"),
            CommandOutcome::TasksUpdated
        );
        assert_eq!(texts(&engine), vec!["First", "Second Edited"]);

        // Undo edit
        assert_eq!(engine.execute("/u"), CommandOutcome::TasksUpdated);
        assert_eq!(texts(&engine), vec!["First", "Second"]);
        assert_eq!(
            engine.execute("/undo"),
            CommandOutcome::Warning("Nothing to undo.".to_string())
        );

        // Remove (`/r`, `/re`, `/remove`)
        assert_eq!(engine.execute("/re 0"), CommandOutcome::TasksUpdated);
        assert_eq!(texts(&engine), vec!["Second"]);

        // Clear & Undo
        assert_eq!(engine.execute("/clear"), CommandOutcome::TasksUpdated);
        assert!(engine.tasks.is_empty());
        assert_eq!(engine.execute("/undo"), CommandOutcome::TasksUpdated);
        assert_eq!(texts(&engine), vec!["Second"]);
    }

    #[test]
    fn test_window_config_theme_quit_help_outcomes() {
        let mut engine = TaskEngine::new();
        assert_eq!(engine.execute("/pin"), CommandOutcome::TogglePin);
        assert_eq!(engine.execute("/p"), CommandOutcome::TogglePin);
        assert_eq!(
            engine.execute("/dock"),
            CommandOutcome::Dock(DockSide::Right)
        );
        assert_eq!(
            engine.execute("/dock l"),
            CommandOutcome::Dock(DockSide::Left)
        );
        assert_eq!(
            engine.execute("/dock right"),
            CommandOutcome::Dock(DockSide::Right)
        );
        assert_eq!(engine.execute("/float"), CommandOutcome::Float);
        assert_eq!(engine.execute("/undock"), CommandOutcome::Float);
        assert_eq!(engine.execute("/config"), CommandOutcome::OpenConfig);
        assert_eq!(
            engine.execute("/tags"),
            CommandOutcome::OpenAutocompleteConfig
        );
        assert_eq!(
            engine.execute("/autocomplete"),
            CommandOutcome::OpenAutocompleteConfig
        );
        assert_eq!(
            engine.execute("/theme"),
            CommandOutcome::SetTheme(ThemeTarget::Toggle)
        );
        assert_eq!(
            engine.execute("/theme dark"),
            CommandOutcome::SetTheme(ThemeTarget::Dark)
        );
        assert_eq!(
            engine.execute("/theme auto"),
            CommandOutcome::SetTheme(ThemeTarget::System)
        );
        assert_eq!(
            engine.execute("/dark"),
            CommandOutcome::SetTheme(ThemeTarget::Dark)
        );
        assert_eq!(
            engine.execute("/light"),
            CommandOutcome::SetTheme(ThemeTarget::Light)
        );
        assert_eq!(engine.execute("/exit"), CommandOutcome::Quit);
        assert_eq!(engine.execute("/q"), CommandOutcome::Quit);
        assert_eq!(
            engine.execute("/help"),
            CommandOutcome::Warning(HELP_MESSAGE.to_string())
        );
        assert_eq!(
            engine.execute("/?"),
            CommandOutcome::Warning(HELP_MESSAGE.to_string())
        );
        assert_eq!(
            engine.execute("/unknown"),
            CommandOutcome::Warning("Unrecognized command: /unknown".to_string())
        );
    }
}
