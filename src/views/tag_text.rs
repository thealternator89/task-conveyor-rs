use crate::theme;
use gpui::{Div, FontWeight, SharedString, div, prelude::*, px, rems};

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
    let mut container = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_baseline()
        .gap_x(rems(0.28))
        .gap_y(rems(0.15));

    if show_break_badge {
        let (break_bg, break_fg) = if is_dark {
            (gpui::rgb(0x0f5132), gpui::rgb(0x75b798))
        } else {
            (gpui::rgb(0xd1e7dd), gpui::rgb(0x0f5132))
        };
        container = container.child(
            div()
                .px(rems(0.35))
                .py(rems(0.05))
                .rounded(px(4.0))
                .bg(break_bg)
                .text_color(break_fg)
                .text_size(rems(0.75))
                .font_weight(FontWeight::BOLD)
                .child("BREAK"),
        );
    }

    for word in text.split_whitespace() {
        let word_owned = SharedString::from(word.to_string());
        if is_tag_token(word) {
            let (bg, fg) = theme::tag_badge_colors(word, is_dark, in_current_card);
            container = container.child(
                div()
                    .px(rems(0.35))
                    .py(rems(0.05))
                    .rounded(px(4.0))
                    .bg(bg)
                    .text_color(fg)
                    .text_size(rems(0.85))
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
}
