use crate::theme;
use gpui::{Div, FontWeight, Pixels, Rems, SharedString, div, prelude::*, px, rems};

/// Returns `true` if `token` matches `^[#@$][\w-]+$`.
pub fn is_tag_token(token: &str) -> bool {
    let mut chars = token.chars();
    let Some(prefix) = chars.next() else {
        return false;
    };
    if !matches!(prefix, '#' | '@' | '$') {
        return false;
    }
    let rest = chars.as_str();
    !rest.is_empty()
        && rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Badge dimensions and font size, scaled proportionally between queue items and the active slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BadgeStyle {
    pub px: Rems,
    pub py: Rems,
    pub text_size: Rems,
    pub rounded: Pixels,
}

/// Returns the badge dimensions for tag, project, and mention tokens.
/// When `in_current_card` is true (active slot), dimensions scale up to match the larger
/// `1.35rem` active slot text rather than remaining at the compact queue dimensions.
pub fn tag_badge_style(in_current_card: bool) -> BadgeStyle {
    if in_current_card {
        BadgeStyle {
            px: rems(0.45),
            py: rems(0.08),
            text_size: rems(1.15),
            rounded: px(5.0),
        }
    } else {
        BadgeStyle {
            px: rems(0.35),
            py: rems(0.05),
            text_size: rems(0.85),
            rounded: px(4.0),
        }
    }
}

/// Returns the badge dimensions for the leading `BREAK` badge.
pub fn break_badge_style(in_current_card: bool) -> BadgeStyle {
    if in_current_card {
        BadgeStyle {
            px: rems(0.45),
            py: rems(0.08),
            text_size: rems(1.0),
            rounded: px(5.0),
        }
    } else {
        BadgeStyle {
            px: rems(0.35),
            py: rems(0.05),
            text_size: rems(0.75),
            rounded: px(4.0),
        }
    }
}

/// Returns `(gap_x, gap_y)` spacing between words and badges in the task container.
pub fn tag_container_gap(in_current_card: bool) -> (Rems, Rems) {
    if in_current_card {
        (rems(0.38), rems(0.20))
    } else {
        (rems(0.28), rems(0.15))
    }
}

/// Renders task text with inline `#tag`, `$project`, and `@mention` pill badges.
/// Ports `renderTaskTextWithTags` (`src/renderer/app.tsx:74-91`) and `.task-tag-badge` (`src/renderer/index.css:352-372`).
pub fn render_task_text_with_tags(
    text: &str,
    is_dark: bool,
    in_current_card: bool,
) -> impl IntoElement {
    render_task_text_with_options(text, is_dark, in_current_card, false)
}

/// Renders task text with inline tag badges and an optional leading `BREAK` pill badge
/// (used for `is_break` items in the "Up Next" queue, matching `src/renderer/app.tsx:937-938`).
pub fn render_task_text_with_options(
    text: &str,
    is_dark: bool,
    in_current_card: bool,
    show_break_badge: bool,
) -> Div {
    let (gap_x, gap_y) = tag_container_gap(in_current_card);
    let mut container = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_baseline()
        .gap_x(gap_x)
        .gap_y(gap_y);

    if show_break_badge {
        let (break_bg, break_fg) = if is_dark {
            (gpui::rgb(0x0f5132), gpui::rgb(0x75b798))
        } else {
            (gpui::rgb(0xd1e7dd), gpui::rgb(0x0f5132))
        };
        let bstyle = break_badge_style(in_current_card);
        container = container.child(
            div()
                .px(bstyle.px)
                .py(bstyle.py)
                .rounded(bstyle.rounded)
                .bg(break_bg)
                .text_color(break_fg)
                .text_size(bstyle.text_size)
                .font_weight(FontWeight::BOLD)
                .child("BREAK"),
        );
    }

    let style = tag_badge_style(in_current_card);
    for word in text.split_whitespace() {
        let word_owned = SharedString::from(word.to_string());
        if is_tag_token(word) {
            let (bg, fg) = theme::tag_badge_colors(word, is_dark, in_current_card);
            container = container.child(
                div()
                    .px(style.px)
                    .py(style.py)
                    .rounded(style.rounded)
                    .bg(bg)
                    .text_color(fg)
                    .text_size(style.text_size)
                    .font_weight(FontWeight::BOLD)
                    .child(word_owned),
            );
        } else {
            container = container.child(word_owned);
        }
    }

    container
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_tag_token() {
        assert!(is_tag_token("#urgent"));
        assert!(is_tag_token("$backend-api"));
        assert!(is_tag_token("@alice_99"));
        assert!(!is_tag_token("#"));
        assert!(!is_tag_token("$"));
        assert!(!is_tag_token("@"));
        assert!(!is_tag_token("plain"));
        assert!(!is_tag_token("#urgent!"));
        assert!(!is_tag_token("foo#bar"));
    }

    #[test]
    fn test_badge_style_scales_for_active_slot() {
        let queue_style = tag_badge_style(false);
        let active_style = tag_badge_style(true);

        assert_eq!(queue_style.text_size, rems(0.85));
        assert_eq!(active_style.text_size, rems(1.15));
        assert_eq!(queue_style.px, rems(0.35));
        assert_eq!(active_style.px, rems(0.45));
        assert_eq!(queue_style.py, rems(0.05));
        assert_eq!(active_style.py, rems(0.08));
        assert_eq!(queue_style.rounded, px(4.0));
        assert_eq!(active_style.rounded, px(5.0));
        assert!(active_style.text_size.0 > queue_style.text_size.0);
        assert!(active_style.px.0 > queue_style.px.0);
        assert!(active_style.py.0 > queue_style.py.0);
    }

    #[test]
    fn test_break_badge_style_scales_for_active_slot() {
        let queue_style = break_badge_style(false);
        let active_style = break_badge_style(true);

        assert_eq!(queue_style.text_size, rems(0.75));
        assert_eq!(active_style.text_size, rems(1.0));
        assert!(active_style.text_size.0 > queue_style.text_size.0);
    }

    #[test]
    fn test_tag_container_gap_scales_for_active_slot() {
        let (q_gx, q_gy) = tag_container_gap(false);
        let (a_gx, a_gy) = tag_container_gap(true);

        assert!(a_gx.0 > q_gx.0);
        assert!(a_gy.0 > q_gy.0);
    }
}
