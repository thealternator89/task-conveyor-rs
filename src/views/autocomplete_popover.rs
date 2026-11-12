use crate::autocomplete::AutocompleteMatch;
use crate::theme::ThemePalette;
use crate::views::command_input::CommandInputView;
use gpui::{
    BoxShadow, DefiniteLength, Entity, FontWeight, MouseButton, SharedString, deferred, div,
    point, prelude::*, px, rems, rgb,
};

pub fn render_autocomplete_popover(
    matches: &[AutocompleteMatch],
    selected_index: usize,
    command_input: Entity<CommandInputView>,
    palette: ThemePalette,
    is_dark: bool,
) -> impl IntoElement {
    let total_count = matches.len();
    let active_index = if total_count > 0 {
        selected_index % total_count
    } else {
        0
    };

    let window_start = if active_index >= 8 {
        active_index + 1 - 8
    } else {
        0
    };

    let shadow_alpha = if is_dark { 0.45 } else { 0.12 };

    deferred(
        div()
            .id("autocomplete-popover")
            .occlude()
            .absolute()
            .left_0()
            .right_0()
            .bottom(DefiniteLength::Fraction(1.0))
            .mb(px(6.0))
            .max_h(px(220.0))
            .overflow_y_scroll()
            .rounded(px(8.0))
            .bg(palette.popover_bg)
            .border_1()
            .border_color(palette.popover_border)
            .shadow(vec![BoxShadow {
                color: rgb(0x000000).alpha(shadow_alpha).into(),
                offset: point(px(0.0), px(4.0)),
                blur_radius: px(16.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .child(
                // Header bar: "SUGGESTIONS (N)" on left, "Tab to accept" on right
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(10.0))
                    .py(px(5.0))
                    .bg(palette.popover_header_bg)
                    .border_b_1()
                    .border_color(palette.popover_header_border)
                    .text_size(rems(0.70))
                    .text_color(palette.muted_text)
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .child(format!("SUGGESTIONS ({total_count})")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .px(px(4.0))
                                    .py(px(1.0))
                                    .rounded(px(3.0))
                                    .bg(palette.popover_kbd_bg)
                                    .text_color(palette.popover_kbd_fg)
                                    .border_1()
                                    .border_color(palette.popover_kbd_border)
                                    .text_size(rems(0.68))
                                    .child("Tab"),
                            )
                            .child("to accept"),
                    ),
            )
            .child(
                div().flex().flex_col().py(px(2.0)).children(
                    matches
                        .iter()
                        .enumerate()
                        .skip(window_start)
                        .take(8)
                        .map(|(idx, matched)| {
                            let is_active = idx == active_index;
                            let (pill_bg, pill_fg) = palette.type_pill_colors(matched.token_type);
                            let type_label = matched.token_type.label();
                            let prefix_str = SharedString::from(matched.prefix.to_string());
                            let value_str = SharedString::from(matched.value.clone());
                            let match_clone = matched.clone();
                            let input_handle = command_input.clone();

                            div()
                                .id(("autocomplete-item", idx))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .px(px(10.0))
                                .py(px(6.0))
                                .cursor_pointer()
                                .when(is_active, |this| this.bg(palette.popover_active_item_bg))
                                .hover(move |style| style.bg(palette.popover_active_item_bg))
                                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                    cx.stop_propagation();
                                    let selected = match_clone.clone();
                                    input_handle.update(cx, |input, cx| {
                                        input.select_suggestion(selected, window, cx);
                                    });
                                })
                                .child(
                                    div()
                                        .px(px(5.0))
                                        .py(px(1.0))
                                        .rounded(px(3.0))
                                        .bg(pill_bg)
                                        .text_color(pill_fg)
                                        .text_size(rems(0.65))
                                        .font_weight(FontWeight::BOLD)
                                        .child(type_label),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_baseline()
                                        .text_size(rems(0.85))
                                        .child(
                                            div()
                                                .text_color(palette.popover_item_prefix)
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(prefix_str),
                                        )
                                        .child(
                                            div()
                                                .text_color(palette.popover_item_value)
                                                .font_weight(FontWeight::MEDIUM)
                                                .child(value_str),
                                        ),
                                )
                        }),
                ),
            ),
    )
    .with_priority(1)
}
