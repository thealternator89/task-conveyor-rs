use crate::autocomplete::AutocompleteType;
use crate::config_store::ConfigStore;
use crate::task_store::TaskStore;
use crate::theme::{ActiveTheme, ThemePalette};
use crate::views::command_input::{CommandInputEvent, CommandInputView};
use crate::views::main_sidebar::MainSidebarView;
use gpui::{
    App, Bounds, Context, Entity, FontWeight, Pixels, SharedString, Window, WindowBounds,
    WindowHandle, WindowKind, WindowOptions, div, point, prelude::*, px, rems, size, svg,
};

pub const SPOTLIGHT_WIDTH: f32 = 500.0;
pub const SPOTLIGHT_HEIGHT: f32 = 72.0;

pub fn centered_spotlight_bounds(cx: &App) -> Bounds<Pixels> {
    let target_size = size(px(SPOTLIGHT_WIDTH), px(SPOTLIGHT_HEIGHT));
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

pub fn spotlight_window_options(cx: &App) -> WindowOptions {
    let bounds = centered_spotlight_bounds(cx);
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: None,
        kind: WindowKind::PopUp,
        is_resizable: false,
        is_minimizable: false,
        is_movable: false,
        focus: true,
        show: true,
        ..Default::default()
    }
}

/// Computes the dynamic right-hand badge label for the Spotlight input row.
/// Ports `src/renderer/app.tsx:263-271`:
/// - When suggestions are active: `"[Tab] Tag"` / `"[Tab] Project"` / `"[Tab] Mention"`
/// - Else if `value` starts with `"!!"`: `"⚡ Current Slot"`
/// - Else if `value` starts with `"!"`: `"➡️ Next Slot"`
/// - Else if `value` starts with `"/"`: `"Command"`
/// - Else: `"📥 End of List"`
pub fn spotlight_badge_text(
    value: &str,
    active_suggestion_type: Option<AutocompleteType>,
) -> &'static str {
    if let Some(token_type) = active_suggestion_type {
        token_type.spotlight_badge_label()
    } else if value.starts_with("!!") {
        "⚡ Current Slot"
    } else if value.starts_with('!') {
        "➡️ Next Slot"
    } else if value.starts_with('/') {
        "Command"
    } else {
        "📥 End of List"
    }
}

pub struct SpotlightView {
    config_store: Entity<ConfigStore>,
    task_store: Entity<TaskStore>,
    main_window: Option<WindowHandle<MainSidebarView>>,
    pub command_input: Entity<CommandInputView>,
}

impl SpotlightView {
    pub fn new(
        window: &mut Window,
        config_store: Entity<ConfigStore>,
        task_store: Entity<TaskStore>,
        main_window: Option<WindowHandle<MainSidebarView>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let command_input = cx.new(|cx| CommandInputView::new(config_store.clone(), true, cx));

        cx.observe(&config_store, |_, _, cx| cx.notify()).detach();
        cx.observe(&command_input, |_, _, cx| cx.notify()).detach();

        cx.subscribe_in(
            &command_input,
            window,
            |this, _input, event: &CommandInputEvent, window, cx| match event {
                CommandInputEvent::Submit(text) => {
                    let outcome = this
                        .task_store
                        .update(cx, |store, cx| store.execute_command(text, cx));
                    if let Some(main_win) = this.main_window {
                        let _ = main_win.update(cx, |sidebar, _win, cx| {
                            sidebar.apply_window_outcome(&outcome, cx);
                        });
                    }
                    this.hide(window, cx);
                }
                CommandInputEvent::DismissSpotlight => {
                    this.hide(window, cx);
                }
            },
        )
        .detach();

        cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                this.hide(window, cx);
            }
        })
        .detach();

        cx.observe_window_appearance(window, |this, _window, cx| {
            this.config_store.update(cx, |cfg, cx| {
                cfg.sync_os_appearance(cx);
            });
        })
        .detach();

        let mut view = Self {
            config_store,
            task_store,
            main_window,
            command_input,
        };
        view.on_shown(window, cx);
        view
    }

    /// Resets the Spotlight input state (`value = ""`, `caret_pos = 0`, `dismissed = false`,
    /// `selected_index = 0`) and focuses the text input.
    /// Ports `src/main/window.ts:388` & `src/renderer/app.tsx:158-166`.
    pub fn on_shown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.command_input.update(cx, |input, cx| {
            input.reset(window, cx);
        });
        window.activate_window();
        cx.notify();
    }

    pub fn hide(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        window.remove_window();
    }

    fn render_lip_hint(key: &str, label: &str, palette: ThemePalette) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .px(px(4.0))
                    .py(px(0.5))
                    .rounded(px(3.0))
                    .bg(palette.spotlight_lip_key_bg)
                    .text_color(palette.spotlight_lip_key_fg)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(rems(0.68))
                    .child(SharedString::from(key.to_string())),
            )
            .child(SharedString::from(label.to_string()))
    }
}

impl Render for SpotlightView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.global::<ActiveTheme>().palette;

        let (value, active_suggestion_type) = {
            let input = self.command_input.read(cx);
            let suggestions = input.current_suggestions(cx);
            let show_suggestions = !input.dismissed && !suggestions.matches.is_empty();
            let active_type = if show_suggestions {
                let idx = input.selected_index.min(suggestions.matches.len() - 1);
                Some(suggestions.matches[idx].token_type)
            } else {
                None
            };
            (input.value.clone(), active_type)
        };

        let badge_label = spotlight_badge_text(&value, active_suggestion_type);

        div()
            .id("spotlight-container")
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .bg(palette.spotlight_bg)
            .text_color(palette.input_text)
            .border_1()
            .border_color(palette.border)
            .child(
                // Main Row (min-height: 46px, 12px horizontal padding)
                div()
                    .id("spotlight-main-row")
                    .flex()
                    .items_center()
                    .flex_1()
                    .min_h(px(46.0))
                    .px(px(12.0))
                    .child(
                        svg()
                            .path("icons/terminal.svg")
                            .size(rems(1.2))
                            .text_color(palette.spotlight_icon)
                            .mr(px(10.0))
                            .flex_shrink_0(),
                    )
                    .child(div().flex_1().min_w_0().child(self.command_input.clone()))
                    .child(
                        div()
                            .id("spotlight-badge")
                            .ml(px(8.0))
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded(px(4.0))
                            .bg(palette.number_badge_bg)
                            .text_color(palette.muted_text)
                            .text_size(rems(0.75))
                            .font_weight(FontWeight::SEMIBOLD)
                            .flex_shrink_0()
                            .child(badge_label),
                    ),
            )
            .child(
                // Bottom Lip (height: 24px, 12px horizontal padding, border-top)
                div()
                    .id("spotlight-lip")
                    .h(px(24.0))
                    .px(px(12.0))
                    .bg(palette.spotlight_lip_bg)
                    .border_t_1()
                    .border_color(palette.spotlight_lip_border)
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(rems(0.70))
                    .text_color(palette.muted_text)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(Self::render_lip_hint("!", "next", palette))
                            .child(Self::render_lip_hint("!!", "current", palette))
                            .child(Self::render_lip_hint("#", "tag", palette))
                            .child(Self::render_lip_hint("$", "project", palette))
                            .child(Self::render_lip_hint("@", "mention", palette)),
                    )
                    .child(Self::render_lip_hint("Tab", "accept", palette)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spotlight_badge_text_priority_and_suggestions() {
        // Default state -> End of List
        assert_eq!(spotlight_badge_text("", None), "📥 End of List");
        assert_eq!(spotlight_badge_text("Fix bug", None), "📥 End of List");

        // Single `!` -> Next Slot
        assert_eq!(spotlight_badge_text("!", None), "➡️ Next Slot");
        assert_eq!(spotlight_badge_text("!Next task", None), "➡️ Next Slot");

        // Double `!!` -> Current Slot
        assert_eq!(spotlight_badge_text("!!", None), "⚡ Current Slot");
        assert_eq!(
            spotlight_badge_text("!!Urgent task", None),
            "⚡ Current Slot"
        );

        // Leading `/` -> Command
        assert_eq!(spotlight_badge_text("/", None), "Command");
        assert_eq!(spotlight_badge_text("/help", None), "Command");
        assert_eq!(spotlight_badge_text("/done", None), "Command");

        // Active autocomplete suggestion overrides prefix badges
        assert_eq!(
            spotlight_badge_text("!!#urg", Some(AutocompleteType::Tag)),
            "[Tab] Tag"
        );
        assert_eq!(
            spotlight_badge_text("!$apo", Some(AutocompleteType::Project)),
            "[Tab] Project"
        );
        assert_eq!(
            spotlight_badge_text("Ping @al", Some(AutocompleteType::Mention)),
            "[Tab] Mention"
        );
        assert_eq!(
            spotlight_badge_text("/edit 1 #urg", Some(AutocompleteType::Tag)),
            "[Tab] Tag"
        );
    }
}
