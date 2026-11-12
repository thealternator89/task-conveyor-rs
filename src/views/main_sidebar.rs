use crate::config_store::ConfigStore;
use crate::task_engine::{DockSide, TaskItem};
use crate::task_store::TaskStore;
use crate::theme::{ActiveTheme, ThemePalette};
use crate::views::tag_text::{render_task_text_with_options, render_task_text_with_tags};
use gpui::{
    Animation, AnimationExt as _, BoxShadow, Context, Entity, FontWeight, MouseButton, PathBuilder,
    SharedString, Window, canvas, div, img, linear_color_stop, linear_gradient, point, prelude::*,
     pulsating_between, px, rems, rgb, svg,
};
use std::time::Duration;

pub struct MainSidebarView {
    config_store: Entity<ConfigStore>,
    task_store: Entity<TaskStore>,
    pub dock_side: Option<DockSide>,
    pub always_on_top: bool,
}

impl MainSidebarView {
    pub fn new(
        window: &mut Window,
        config_store: Entity<ConfigStore>,
        task_store: Entity<TaskStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&config_store, |_, _, cx| cx.notify()).detach();
        cx.observe(&task_store, |_, _, cx| cx.notify()).detach();
        cx.observe_window_appearance(window, |this, _window, cx| {
            this.config_store.update(cx, |cfg, cx| {
                cfg.sync_os_appearance(cx);
            });
        })
        .detach();

        Self {
            config_store,
            task_store,
            dock_side: None,
            always_on_top: false,
        }
    }

    fn render_chevron_background(&self, palette: ThemePalette) -> impl IntoElement {
        let (shadow_stroke, highlight_stroke) = palette.chevron_strokes();

        canvas(
            |_, _, _| (),
            move |bounds, _, window, _cx| {
                let mut y = bounds.origin.y;
                let mid_x = bounds.origin.x + bounds.size.width * 0.5;
                let right_x = bounds.origin.x + bounds.size.width;
                let max_y = bounds.origin.y + bounds.size.height;

                while y < max_y {
                    for (offset_y, width, color) in [
                        (px(68.0), px(2.0), shadow_stroke),
                        (px(70.0), px(1.5), highlight_stroke),
                    ] {
                        let mut builder = PathBuilder::stroke(width);
                        builder.move_to(point(bounds.origin.x, y + offset_y));
                        builder.line_to(point(mid_x, y + offset_y - px(40.0)));
                        builder.line_to(point(right_x, y + offset_y));
                        if let Ok(path) = builder.build() {
                            window.paint_path(path, color);
                        }
                    }
                    y += px(96.0);
                }
            },
        )
        .absolute()
        .inset_0()
    }

    fn render_header(&self, palette: ThemePalette) -> impl IntoElement {
        let is_docked = self.dock_side.is_some();

        let mut header = div()
            .id("sidebar-header")
            .flex()
            .items_center()
            .justify_between()
            .px_4()
            .py_2()
            .bg(palette.header_bg)
            .border_b_1()
            .border_color(palette.border)
            .when(!is_docked, |this| {
                this.on_mouse_down(MouseButton::Left, |_, window, _| {
                    window.start_window_move();
                })
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(img("logo-full.png").h(px(24.0)))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_size(rems(1.25))
                            .text_color(palette.text_primary)
                            .child("TaskConveyor"),
                    ),
            );

        if self.always_on_top {
            header = header.child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(rems(0.45))
                    .py(rems(0.2))
                    .rounded(px(4.0))
                    .bg(palette.pill_tag_bg)
                    .child(
                        svg()
                            .path("icons/thumbtack.svg")
                            .size(rems(0.75))
                            .text_color(palette.pill_tag_fg),
                    ),
            );
        }

        header
    }

    fn render_alert_banner(
        &self,
        restart_required: bool,
        warning: Option<&str>,
        palette: ThemePalette,
    ) -> Option<impl IntoElement> {
        if restart_required {
            Some(
                div()
                    .py_2()
                    .px_3()
                    .mb_3()
                    .rounded(px(8.0))
                    .bg(palette.alert_info_bg)
                    .text_color(palette.alert_info_fg)
                    .border_1()
                    .border_color(palette.alert_info_border)
                    .text_size(rems(0.875))
                    .shadow(vec![BoxShadow {
                        color: rgb(0x000000).alpha(0.05).into(),
                        offset: point(px(0.0), px(1.0)),
                        blur_radius: px(2.0),
                        spread_radius: px(0.0),
                        inset: false,
                    }])
                    .child("Restart TaskConveyor to use your new global hotkey."),
            )
        } else {
            warning.map(|msg| {
                div()
                    .py_2()
                    .px_3()
                    .mb_3()
                    .rounded(px(8.0))
                    .bg(palette.alert_warning_bg)
                    .text_color(palette.alert_warning_fg)
                    .border_1()
                    .border_color(palette.alert_warning_border)
                    .text_size(rems(0.875))
                    .shadow(vec![BoxShadow {
                        color: rgb(0x000000).alpha(0.05).into(),
                        offset: point(px(0.0), px(1.0)),
                        blur_radius: px(2.0),
                        spread_radius: px(0.0),
                        inset: false,
                    }])
                    .child(SharedString::from(msg.to_string()))
            })
        }
    }

    fn render_current_task_card(&self, task: &TaskItem, is_dark: bool) -> impl IntoElement {
        let (grad_start, grad_end, shadow_rgb) = if task.important {
            (rgb(0xdc3545), rgb(0xbb2d3b), 0xdc3545)
        } else if task.is_break {
            (rgb(0x198754), rgb(0x157347), 0x198754)
        } else {
            (rgb(0x0d6efd), rgb(0x0b5ed7), 0x0d6efd)
        };

        let card = div()
            .id("current-task-card")
            .rounded(px(12.0))
            .p(rems(1.5))
            .text_color(gpui::white())
            .bg(linear_gradient(
                135.0,
                linear_color_stop(grad_start, 0.0),
                linear_color_stop(grad_end, 1.0),
            ))
            .shadow(vec![BoxShadow {
                color: rgb(shadow_rgb).alpha(0.35).into(),
                offset: point(px(0.0), px(4.0)),
                blur_radius: px(15.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .hover(move |style| {
                style.shadow(vec![BoxShadow {
                    color: rgb(shadow_rgb).alpha(0.45).into(),
                    offset: point(px(0.0), px(6.0)),
                    blur_radius: px(20.0),
                    spread_radius: px(0.0),
                    inset: false,
                }])
            })
            .child(
                div()
                    .flex()
                    .items_start()
                    .font_weight(FontWeight::BOLD)
                    .text_size(rems(1.35))
                    .child(
                        div()
                            .w(px(20.0))
                            .h(px(20.0))
                            .rounded(px(10.0))
                            .bg(gpui::white().alpha(0.25))
                            .text_color(gpui::white())
                            .text_size(rems(0.75))
                            .font_weight(FontWeight::BOLD)
                            .flex()
                            .items_center()
                            .justify_center()
                            .mr(px(10.0))
                            .mt(px(4.0))
                            .flex_shrink_0()
                            .child("0"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(render_task_text_with_tags(&task.text, is_dark, true)),
                    ),
            );

        if task.important {
            card.with_animation(
                "urgent-pulse",
                Animation::new(Duration::from_secs(2))
                    .repeat()
                    .with_easing(pulsating_between(0.0, 1.0)),
                move |this, delta| {
                    let blur = px(15.0 + 10.0 * delta);
                    let alpha = 0.35 + 0.25 * delta;
                    this.shadow(vec![BoxShadow {
                        color: rgb(0xdc3545).alpha(alpha).into(),
                        offset: point(px(0.0), px(4.0)),
                        blur_radius: blur,
                        spread_radius: px(0.0),
                        inset: false,
                    }])
                },
            )
            .into_any_element()
        } else {
            card.into_any_element()
        }
    }

    fn render_empty_active_card(
        &self,
        formatted_hotkey: &str,
        palette: ThemePalette,
    ) -> impl IntoElement {
        let hotkey_label = if formatted_hotkey.is_empty() {
            "..."
        } else {
            formatted_hotkey
        };

        div()
            .p_4()
            .rounded(px(8.0))
            .bg(palette.card_bg)
            .border_1()
            .border_color(palette.card_border)
            .text_color(palette.muted_text)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .shadow(vec![BoxShadow {
                color: rgb(0x000000).alpha(0.03).into(),
                offset: point(px(0.0), px(1.0)),
                blur_radius: px(2.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .child(div().text_size(rems(1.0)).child("No active task"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(rems(0.85))
                    .text_color(palette.muted_text)
                    .child("Press")
                    .child(Self::render_kbd_badge(hotkey_label, palette))
                    .child("to add a task."),
            )
    }

    fn render_up_next_item(
        &self,
        task_index: usize,
        task: &TaskItem,
        is_dark: bool,
        palette: ThemePalette,
    ) -> impl IntoElement {
        let base_shadow_alpha = if is_dark { 0.25 } else { 0.02 };
        let hover_shadow_alpha = if is_dark { 0.35 } else { 0.05 };

        div()
            .id(("task-item", task_index))
            .rounded(px(8.0))
            .px_4()
            .py_3()
            .mb_2()
            .bg(palette.task_item_bg)
            .border_1()
            .border_color(palette.card_border)
            .flex()
            .items_center()
            .justify_between()
            .shadow(vec![BoxShadow {
                color: rgb(0x000000).alpha(base_shadow_alpha).into(),
                offset: point(px(0.0), px(2.0)),
                blur_radius: px(4.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .hover(move |style| {
                style
                    .border_color(palette.card_hover_border)
                    .shadow(vec![BoxShadow {
                        color: rgb(0x000000).alpha(hover_shadow_alpha).into(),
                        offset: point(px(0.0), px(4.0)),
                        blur_radius: px(8.0),
                        spread_radius: px(0.0),
                        inset: false,
                    }])
            })
            .child(
                div()
                    .flex()
                    .items_start()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .w(px(20.0))
                            .h(px(20.0))
                            .rounded(px(10.0))
                            .bg(palette.number_badge_bg)
                            .text_color(palette.number_badge_fg)
                            .text_size(rems(0.75))
                            .font_weight(FontWeight::BOLD)
                            .flex()
                            .items_center()
                            .justify_center()
                            .mr(px(10.0))
                            .mt(px(2.0))
                            .flex_shrink_0()
                            .child(format!("{task_index}")),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(palette.task_text)
                            .child(render_task_text_with_options(
                                &task.text,
                                is_dark,
                                false,
                                task.is_break,
                            )),
                    ),
            )
    }

    fn render_kbd_badge(label: &str, palette: ThemePalette) -> impl IntoElement {
        div()
            .px(rems(0.35))
            .py(rems(0.1))
            .rounded(px(4.0))
            .bg(palette.kbd_bg)
            .text_color(palette.kbd_fg)
            .border_1()
            .border_color(palette.kbd_border)
            .text_size(rems(0.75))
            .child(SharedString::from(label.to_string()))
    }

    fn render_footer(&self, formatted_hotkey: &str, palette: ThemePalette) -> impl IntoElement {
        let hotkey_label = if formatted_hotkey.is_empty() {
            "..."
        } else {
            formatted_hotkey
        };

        div()
            .id("sidebar-footer")
            .p_3()
            .bg(palette.footer_bg)
            .border_t_1()
            .border_color(palette.border)
            .relative()
            .child(
                // Placeholder slot for CommandInputView (wired in Phase 4)
                div()
                    .id("command-input-placeholder")
                    .w_full()
                    .px(rems(0.5))
                    .py(rems(0.25))
                    .rounded(px(4.0))
                    .bg(palette.input_bg)
                    .border_1()
                    .border_color(palette.input_border)
                    .text_size(rems(0.875))
                    .text_color(palette.muted_text)
                    .child("Type a task..."),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .mt_2()
                    .text_size(rems(0.80))
                    .text_color(palette.muted_text)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child("Global Hotkey:")
                            .child(Self::render_kbd_badge(hotkey_label, palette)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child("Type")
                            .child(Self::render_kbd_badge("/help", palette))
                            .child("for commands"),
                    ),
            )
    }
}

impl Render for MainSidebarView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_theme = *cx.global::<ActiveTheme>();
        let palette = active_theme.palette;
        let is_dark = active_theme.is_dark;

        let (restart_required, formatted_hotkey) = {
            let cfg = self.config_store.read(cx);
            (cfg.restart_required, cfg.formatted_hotkey.clone())
        };

        let (tasks, warning) = {
            let store = self.task_store.read(cx);
            (store.tasks().to_vec(), store.warning.clone())
        };

        let current_task = tasks.first();
        let remaining_count = tasks.len().saturating_sub(1);

        div()
            .id("sidebar-container")
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .bg(palette.sidebar_bg)
            .text_color(palette.task_text)
            .child(self.render_chevron_background(palette))
            .child(self.render_header(palette))
            .child(
                div()
                    .id("sidebar-content")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_4()
                    .children(self.render_alert_banner(
                        restart_required,
                        warning.as_deref(),
                        palette,
                    ))
                    .child(
                        // CURRENTLY DOING section
                        div()
                            .mb_4()
                            .child(
                                div()
                                    .text_color(palette.muted_text)
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(rems(0.75))
                                    .mb_2()
                                    .child("CURRENTLY DOING"),
                            )
                            .child(if let Some(task) = current_task {
                                self.render_current_task_card(task, is_dark)
                                    .into_any_element()
                            } else {
                                self.render_empty_active_card(&formatted_hotkey, palette)
                                    .into_any_element()
                            }),
                    )
                    .child(
                        // UP NEXT section
                        div()
                            .child(
                                div()
                                    .text_color(palette.muted_text)
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(rems(0.75))
                                    .mb_2()
                                    .child(format!("UP NEXT ({remaining_count})")),
                            )
                            .child(if remaining_count > 0 {
                                div()
                                    .flex()
                                    .flex_col()
                                    .children(tasks.iter().enumerate().skip(1).map(|(idx, task)| {
                                        self.render_up_next_item(idx, task, is_dark, palette)
                                    }))
                                    .into_any_element()
                            } else {
                                div()
                                    .p_3()
                                    .rounded(px(6.0))
                                    .bg(palette.card_bg)
                                    .border_1()
                                    .border_dashed()
                                    .border_color(palette.card_border)
                                    .text_color(palette.muted_text)
                                    .text_size(rems(0.875))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .shadow(vec![BoxShadow {
                                        color: rgb(0x000000).alpha(0.03).into(),
                                        offset: point(px(0.0), px(1.0)),
                                        blur_radius: px(2.0),
                                        spread_radius: px(0.0),
                                        inset: false,
                                    }])
                                    .child("No tasks lined up")
                                    .into_any_element()
                            }),
                    ),
            )
            .child(self.render_footer(&formatted_hotkey, palette))
    }
}
