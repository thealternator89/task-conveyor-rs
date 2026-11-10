use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const APP_DIR_NAME: &str = "TaskConveyor";
pub const LEGACY_APP_DIR_NAME: &str = "task-conveyor";

pub const CONFIG_FILE_NAME: &str = "config.json";
pub const AUTOCOMPLETE_FILE_NAME: &str = "autocomplete.json";
pub const TASKS_FILE_NAME: &str = "tasks.json";

/// Returns the platform base configuration/application support directory:
/// - macOS: `~/Library/Application Support`
/// - Windows: `%APPDATA%` (`Roaming`)
/// - Linux: `~/.config` (or `$XDG_CONFIG_HOME`)
pub fn app_data_dir() -> PathBuf {
    dirs::config_dir()
        .or_else(dirs::data_dir)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Returns `<AppData>/TaskConveyor`.
pub fn user_data_dir() -> PathBuf {
    app_data_dir().join(APP_DIR_NAME)
}

/// Returns the legacy `<AppData>/task-conveyor` directory path.
pub fn legacy_user_data_dir() -> PathBuf {
    app_data_dir().join(LEGACY_APP_DIR_NAME)
}

/// Returns `<AppData>/TaskConveyor/config.json`.
pub fn config_file_path() -> PathBuf {
    user_data_dir().join(CONFIG_FILE_NAME)
}

/// Returns `<AppData>/TaskConveyor/autocomplete.json`.
pub fn autocomplete_file_path() -> PathBuf {
    user_data_dir().join(AUTOCOMPLETE_FILE_NAME)
}

/// Returns `<AppData>/TaskConveyor/tasks.json`.
pub fn tasks_file_path() -> PathBuf {
    user_data_dir().join(TASKS_FILE_NAME)
}

/// Migrates user data from `<AppData>/task-conveyor` to `<AppData>/TaskConveyor` if needed.
/// Ports `src/main/index.ts:44-66`.
pub fn migrate_legacy_user_data() {
    let base = app_data_dir();
    if let Err(err) = migrate_legacy_user_data_in(&base) {
        eprintln!("Failed to migrate userData from task-conveyor to TaskConveyor: {err}");
    }
}

/// Testable implementation of legacy userData migration within a given `app_data` root.
pub fn migrate_legacy_user_data_in(app_data: &Path) -> io::Result<()> {
    let old_user_data = app_data.join(LEGACY_APP_DIR_NAME);
    let new_user_data = app_data.join(APP_DIR_NAME);

    if !old_user_data.exists() {
        return Ok(());
    }

    if !new_user_data.exists() {
        copy_dir_recursive(&old_user_data, &new_user_data)?;
    } else {
        for file_name in [CONFIG_FILE_NAME, AUTOCOMPLETE_FILE_NAME, TASKS_FILE_NAME] {
            let old_file = old_user_data.join(file_name);
            let new_file = new_user_data.join(file_name);
            if old_file.exists() && !new_file.exists() {
                fs::copy(&old_file, &new_file)?;
            }
        }
    }

    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_migrate_legacy_directory_when_new_dir_missing() {
        let tmp = TempDir::new().unwrap();
        let legacy_dir = tmp.path().join(LEGACY_APP_DIR_NAME);
        fs::create_dir_all(legacy_dir.join("nested")).unwrap();
        fs::write(
            legacy_dir.join(CONFIG_FILE_NAME),
            r#"{"globalHotkey":"Ctrl+Space"}"#,
        )
        .unwrap();
        fs::write(
            legacy_dir.join(AUTOCOMPLETE_FILE_NAME),
            r#"{"tags":["urgent"]}"#,
        )
        .unwrap();
        fs::write(legacy_dir.join("nested/extra.txt"), "hello").unwrap();

        migrate_legacy_user_data_in(tmp.path()).unwrap();

        let new_dir = tmp.path().join(APP_DIR_NAME);
        assert!(new_dir.join(CONFIG_FILE_NAME).exists());
        assert!(new_dir.join(AUTOCOMPLETE_FILE_NAME).exists());
        assert_eq!(
            fs::read_to_string(new_dir.join("nested/extra.txt")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn test_migrate_missing_files_without_overwriting_existing() {
        let tmp = TempDir::new().unwrap();
        let legacy_dir = tmp.path().join(LEGACY_APP_DIR_NAME);
        let new_dir = tmp.path().join(APP_DIR_NAME);
        fs::create_dir_all(&legacy_dir).unwrap();
        fs::create_dir_all(&new_dir).unwrap();

        fs::write(legacy_dir.join(CONFIG_FILE_NAME), "legacy-config").unwrap();
        fs::write(legacy_dir.join(AUTOCOMPLETE_FILE_NAME), "legacy-auto").unwrap();
        fs::write(new_dir.join(CONFIG_FILE_NAME), "new-config").unwrap();

        migrate_legacy_user_data_in(tmp.path()).unwrap();

        // Existing config.json in TaskConveyor is preserved
        assert_eq!(
            fs::read_to_string(new_dir.join(CONFIG_FILE_NAME)).unwrap(),
            "new-config"
        );
        // Missing autocomplete.json is copied from task-conveyor
        assert_eq!(
            fs::read_to_string(new_dir.join(AUTOCOMPLETE_FILE_NAME)).unwrap(),
            "legacy-auto"
        );
    }
}
