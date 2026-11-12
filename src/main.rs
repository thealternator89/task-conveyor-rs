pub mod assets;
pub mod autocomplete;
pub mod config_store;
pub mod expansions;
pub mod hotkey;
pub mod paths;
pub mod single_instance;
pub mod task_engine;
pub mod task_store;
pub mod theme;
pub mod tray;
pub mod views;
pub mod win32_appbar;
pub mod window_manager;

use assets::Assets;
use config_store::ConfigStore;
use gpui::{
    App, WindowBounds, WindowOptions, prelude::*, px, size,
};
use gpui_platform::application;
use single_instance::acquire_single_instance_lock;
use task_store::TaskStore;
use tray::setup_system_tray;
use views::command_input::bind_command_input_keys;
use views::main_sidebar::MainSidebarView;
use window_manager::{WindowManager, centered_sidebar_bounds};

fn main() {
    let _instance_guard = match acquire_single_instance_lock() {
        Some(guard) => guard,
        None => return,
    };

    paths::migrate_legacy_user_data();

    application().with_assets(Assets).run(|cx: &mut App| {
        bind_command_input_keys(cx);
        let config_store = cx.new(ConfigStore::new);
        let task_store = cx.new(|cx| TaskStore::new(config_store.clone(), cx));
        let window_manager =
            cx.new(|_cx| WindowManager::new(config_store.clone(), task_store.clone()));

        task_store.update(cx, |store, _| {
            store.set_window_manager(window_manager.clone());
        });

        let bounds = centered_sidebar_bounds(cx);
        let main_window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(320.0), px(200.0))),
                    titlebar: None,
                    app_owns_titlebar_drag: true,
                    ..Default::default()
                },
                {
                    let config_store = config_store.clone();
                    let task_store = task_store.clone();
                    let window_manager = window_manager.clone();
                    move |window, cx| {
                        cx.new(|cx| {
                            MainSidebarView::new(
                                window,
                                config_store,
                                task_store,
                                Some(window_manager),
                                cx,
                            )
                        })
                    }
                },
            )
            .ok();

        window_manager.update(cx, |wm, _| {
            wm.set_main_window(main_window);
        });

        let _tray = setup_system_tray(window_manager.clone(), cx);

        let hotkey_wm = window_manager.clone();
        let hotkey_subscription =
            hotkey::register_global_hotkey(&config_store, cx, move |cx: &mut App| {
                hotkey_wm.update(cx, |wm, cx| {
                    wm.toggle_spotlight(cx);
                });
            });

        #[cfg(target_os = "windows")]
        {
            let timer_wm = window_manager.clone();
            cx.spawn(async move |cx| {
                let executor = cx.background_executor().clone();
                loop {
                    executor.timer(std::time::Duration::from_millis(500)).await;
                    let should_continue = cx
                        .update(|cx| {
                            timer_wm.update(cx, |wm, cx| {
                                wm.handle_poschanged_if_needed(cx);
                            });
                        })
                        .is_ok();
                    if !should_continue {
                        break;
                    }
                }
            })
            .detach();
        }

        let quit_wm = window_manager.clone();
        cx.on_app_quit(move |cx| {
            if let Some(sub) = &hotkey_subscription {
                sub.unregister();
            }
            quit_wm.update(cx, |wm, cx| {
                wm.cleanup_on_quit(cx);
            });
            async {}
        })
        .detach();

        {
            let cfg = config_store.read(cx);
            let tasks = task_store.read(cx);
            println!(
                "[TaskConveyor] Startup complete:\n  config: {:?} (hotkey={}, active_hotkey={:?}, theme={:?})\n  autocomplete: {:?} (tags={}, projects={}, mentions={}, expansions={})\n  tasks: {:?} (count={})",
                cfg.config_path,
                cfg.app_config.global_hotkey,
                cfg.active_hotkey,
                cfg.app_config.theme,
                cfg.autocomplete_path,
                cfg.autocomplete.tags.len(),
                cfg.autocomplete.projects.len(),
                cfg.autocomplete.mentions.len(),
                cfg.autocomplete.expansions.len(),
                tasks.storage_path,
                tasks.tasks().len()
            );
        }

        cx.activate(true);
    });
}
