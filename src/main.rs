pub mod assets;
pub mod autocomplete;
pub mod config_store;
pub mod expansions;
pub mod hotkey;
pub mod paths;
pub mod task_engine;
pub mod task_store;
pub mod theme;
pub mod views;

use assets::Assets;
use config_store::ConfigStore;
use gpui::{
    App, Bounds, Entity, WindowBounds, WindowHandle, WindowOptions, point, prelude::*, px, size,
};
use gpui_platform::application;
use task_store::TaskStore;
use views::command_input::bind_command_input_keys;
use views::main_sidebar::MainSidebarView;
use views::spotlight::{SpotlightView, spotlight_window_options};

fn centered_sidebar_bounds(cx: &App) -> Bounds<gpui::Pixels> {
    let target_size = size(px(400.0), px(600.0));
    if let Some(display) = cx.primary_display() {
        let work_area = display.visible_bounds();
        let origin = point(
            work_area.origin.x + (work_area.size.width - target_size.width) * 0.5,
            work_area.origin.y + (work_area.size.height - target_size.height) * 0.5,
        );
        Bounds::new(origin, target_size)
    } else {
        Bounds::centered(None, target_size, cx)
    }
}

fn toggle_spotlight_window(
    spotlight_window: &mut Option<WindowHandle<SpotlightView>>,
    config_store: &Entity<ConfigStore>,
    task_store: &Entity<TaskStore>,
    main_window: Option<WindowHandle<MainSidebarView>>,
    cx: &mut App,
) {
    if let Some(existing) = spotlight_window.take() {
        let was_open = existing
            .update(cx, |view, window, cx| {
                view.hide(window, cx);
            })
            .is_ok();
        if was_open {
            return;
        }
    }

    cx.activate(true);
    let cfg = config_store.clone();
    let tasks = task_store.clone();
    if let Ok(handle) = cx.open_window(spotlight_window_options(cx), move |window, cx| {
        cx.new(|cx| SpotlightView::new(window, cfg, tasks, main_window, cx))
    }) {
        *spotlight_window = Some(handle);
    }
}

fn main() {
    paths::migrate_legacy_user_data();

    application().with_assets(Assets).run(|cx: &mut App| {
        bind_command_input_keys(cx);
        let config_store = cx.new(ConfigStore::new);
        let task_store = cx.new(|cx| TaskStore::new(config_store.clone(), cx));

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
                    move |window, cx| {
                        cx.new(|cx| MainSidebarView::new(window, config_store, task_store, cx))
                    }
                },
            )
            .ok();

        let mut spotlight_window: Option<WindowHandle<SpotlightView>> = None;
        let hotkey_config_store = config_store.clone();
        let hotkey_task_store = task_store.clone();
        let hotkey_subscription =
            hotkey::register_global_hotkey(&config_store, cx, move |cx: &mut App| {
                toggle_spotlight_window(
                    &mut spotlight_window,
                    &hotkey_config_store,
                    &hotkey_task_store,
                    main_window,
                    cx,
                );
            });

        if let Some(sub) = hotkey_subscription {
            cx.on_app_quit(move |_cx| {
                sub.unregister();
                async {}
            })
            .detach();
        }

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
