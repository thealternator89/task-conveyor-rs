use rfd::{MessageDialog, MessageLevel};
use single_instance::SingleInstance;

pub const APP_INSTANCE_ID: &str = "TaskConveyor";
pub const ALREADY_RUNNING_MESSAGE: &str = "Another instance of TaskConveyor is already running.";

#[cfg(target_os = "macos")]
pub fn platform_instance_id(app_id: &str) -> String {
    std::env::temp_dir()
        .join(format!("{app_id}.lock"))
        .to_string_lossy()
        .into_owned()
}

#[cfg(not(target_os = "macos"))]
pub fn platform_instance_id(app_id: &str) -> String {
    app_id.to_string()
}

pub struct SingleInstanceGuard {
    instance: SingleInstance,
}

impl SingleInstanceGuard {
    pub fn is_single(&self) -> bool {
        self.instance.is_single()
    }
}

pub fn acquire_single_instance_lock() -> Option<SingleInstanceGuard> {
    acquire_single_instance_lock_with_name(&platform_instance_id(APP_INSTANCE_ID), true)
}

pub fn acquire_single_instance_lock_with_name(
    app_id: &str,
    show_dialog_on_conflict: bool,
) -> Option<SingleInstanceGuard> {
    match SingleInstance::new(app_id) {
        Ok(instance) => {
            if instance.is_single() {
                Some(SingleInstanceGuard { instance })
            } else {
                if show_dialog_on_conflict {
                    show_already_running_dialog();
                }
                None
            }
        }
        Err(err) => {
            eprintln!("Failed to initialize single-instance lock ({app_id}): {err}");
            None
        }
    }
}

pub fn show_already_running_dialog() {
    MessageDialog::new()
        .set_level(MessageLevel::Error)
        .set_title(APP_INSTANCE_ID)
        .set_description(ALREADY_RUNNING_MESSAGE)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_instance_blocks_second_instance_and_releases_on_drop() {
        let unique_id = platform_instance_id(&format!("TaskConveyorTest-{}", std::process::id()));

        let first = acquire_single_instance_lock_with_name(&unique_id, false)
            .expect("first instance should acquire lock");
        assert!(first.is_single());

        let second = acquire_single_instance_lock_with_name(&unique_id, false);
        assert!(
            second.is_none(),
            "second instance with same name should be rejected while first guard is held"
        );

        drop(first);

        let third = acquire_single_instance_lock_with_name(&unique_id, false)
            .expect("lock should be re-acquirable after dropping first guard");
        assert!(third.is_single());

        drop(third);

        #[cfg(target_os = "macos")]
        let _ = std::fs::remove_file(&unique_id);
    }
}
