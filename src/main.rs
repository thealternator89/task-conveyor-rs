pub mod assets;
pub mod autocomplete;
pub mod config_store;
pub mod expansions;
pub mod paths;
pub mod task_engine;
pub mod task_store;
pub mod theme;

use assets::Assets;
use config_store::ConfigStore;
use gpui::{
    App, Bounds, Context, Entity, Window, WindowBounds, WindowOptions, div, prelude::*, px, size,
};
use gpui_platform::application;
use task_store::TaskStore;
use theme::ActiveTheme;

struct PhaseTwoStatusView {
    config_store: Entity<ConfigStore>,
    task_store: Entity<TaskStore>,
}

impl PhaseTwoStatusView {
    fn new(
        config_store: Entity<ConfigStore>,
        task_store: Entity<TaskStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&config_store, |_, _, cx| cx.notify()).detach();
        cx.observe(&task_store, |_, _, cx| cx.notify()).detach();
        Self {
            config_store,
            task_store,
        }
    }
}

impl Render for PhaseTwoStatusView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_theme = cx.global::<ActiveTheme>();
        let palette = active_theme.palette;
        let config = self.config_store.read(cx);
        let tasks = self.task_store.read(cx);

        div()
            .flex()
            .flex_col()
            .gap_3()
            .bg(palette.sidebar_bg)
            .size_full()
            .p_4()
            .justify_center()
            .items_center()
            .border_1()
            .border_color(palette.border)
            .text_color(palette.task_text)
            .child(
                div()
                    .text_xl()
                    .child("TaskConveyor — Phase 2 Initialized"),
            )
            .child(div().text_sm().text_color(palette.muted_text).child(format!(
                "Theme: {:?} (dark={}) | Hotkey: {} ({})",
                config.app_config.theme,
                active_theme.is_dark,
                config.app_config.global_hotkey,
                config.formatted_hotkey
            )))
            .child(div().text_sm().text_color(palette.muted_text).child(format!(
                "Autocomplete: {} tags, {} projects, {} mentions, {} expansions",
                config.autocomplete.tags.len(),
                config.autocomplete.projects.len(),
                config.autocomplete.mentions.len(),
                config.autocomplete.expansions.len()
            )))
            .child(div().text_sm().text_color(palette.muted_text).child(format!(
                "Loaded Tasks: {}",
                tasks.tasks().len()
            )))
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

        let bounds = Bounds::centered(None, size(px(400.0), px(600.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|cx| PhaseTwoStatusView::new(config_store, task_store, cx)),
        )
        .unwrap();
        cx.activate(true);
    });
}
