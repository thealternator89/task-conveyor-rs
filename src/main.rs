pub mod assets;
pub mod autocomplete;
pub mod config_store;
pub mod expansions;
pub mod paths;
pub mod task_engine;
pub mod task_store;
pub mod theme;
pub mod views;

use assets::Assets;
use config_store::ConfigStore;
use gpui::{App, Bounds, WindowBounds, WindowOptions, point, prelude::*, px, size};
use gpui_platform::application;
use task_store::TaskStore;
use views::main_sidebar::MainSidebarView;

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

fn main() {
    paths::migrate_legacy_user_data();

    application().with_assets(Assets).run(|cx: &mut App| {
        let config_store = cx.new(ConfigStore::new);
        let task_store = cx.new(|cx| TaskStore::new(config_store.clone(), cx));

        {
            let cfg = config_store.read(cx);
            let tasks = task_store.read(cx);
            println!(
                "[TaskConveyor] Startup complete:\n  config: {:?} (hotkey={}, theme={:?})\n  autocomplete: {:?} (tags={}, projects={}, mentions={}, expansions={})\n  tasks: {:?} (count={})",
                cfg.config_path,
                cfg.app_config.global_hotkey,
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

        let bounds = centered_sidebar_bounds(cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(320.0), px(200.0))),
                titlebar: None,
                app_owns_titlebar_drag: true,
                ..Default::default()
            },
            |window, cx| cx.new(|cx| MainSidebarView::new(window, config_store, task_store, cx)),
        )
        .unwrap();
        cx.activate(true);
    });
}
