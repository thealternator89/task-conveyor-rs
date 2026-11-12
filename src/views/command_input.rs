use crate::autocomplete::{
    AutocompleteMatch, AutocompleteResult, apply_completion, get_suggestions,
};
use crate::config_store::ConfigStore;
use crate::expansions::{auto_capitalize_first_letter, expand_on_space};
use crate::theme::ActiveTheme;
use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, KeyBinding,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    ShapedLine, SharedString, Style, TextAlign, TextRun, UTF16Selection, UnderlineStyle, Window,
    actions, div, fill, point, prelude::*, px, relative, rems, rgba, size,
};
use std::ops::Range;

actions!(
    command_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        Home,
        End,
        SelectHome,
        SelectEnd,
        SelectAll,
        Copy,
        Cut,
        Paste,
        AcceptSuggestion,
        NextSuggestion,
        PrevSuggestion,
        DismissOrHide,
        Submit,
    ]
);

pub const KEY_CONTEXT: &str = "CommandInput";

pub fn bind_command_input_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some(KEY_CONTEXT)),
        KeyBinding::new("delete", Delete, Some(KEY_CONTEXT)),
        KeyBinding::new("left", Left, Some(KEY_CONTEXT)),
        KeyBinding::new("right", Right, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-left", SelectLeft, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-right", SelectRight, Some(KEY_CONTEXT)),
        KeyBinding::new("home", Home, Some(KEY_CONTEXT)),
        KeyBinding::new("end", End, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-home", SelectHome, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-end", SelectEnd, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-left", Home, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-right", End, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-cmd-left", SelectHome, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-cmd-right", SelectEnd, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-a", SelectAll, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-a", SelectAll, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-c", Copy, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-c", Copy, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-x", Cut, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-x", Cut, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-v", Paste, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-v", Paste, Some(KEY_CONTEXT)),
        KeyBinding::new("tab", AcceptSuggestion, Some(KEY_CONTEXT)),
        KeyBinding::new("down", NextSuggestion, Some(KEY_CONTEXT)),
        KeyBinding::new("up", PrevSuggestion, Some(KEY_CONTEXT)),
        KeyBinding::new("escape", DismissOrHide, Some(KEY_CONTEXT)),
        KeyBinding::new("enter", Submit, Some(KEY_CONTEXT)),
    ]);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandInputEvent {
    Submit(String),
    DismissSpotlight,
}

impl EventEmitter<CommandInputEvent> for CommandInputView {}

pub struct CommandInputView {
    pub value: String,
    pub caret_pos: usize,
    pub selection_anchor: Option<usize>,
    pub marked_range: Option<Range<usize>>,
    pub selected_index: usize,
    pub dismissed: bool,
    pub focus_handle: FocusHandle,
    pub is_spotlight: bool,
    pub placeholder: SharedString,
    pub scroll_offset_x: Pixels,
    pub last_bounds: Option<Bounds<Pixels>>,
    pub last_shaped_line: Option<ShapedLine>,
    is_selecting: bool,
    config_store: Entity<ConfigStore>,
}

impl CommandInputView {
    pub fn new(
        config_store: Entity<ConfigStore>,
        is_spotlight: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&config_store, |_, _, cx| cx.notify()).detach();

        let placeholder = if is_spotlight {
            SharedString::from("Type a task or /command...")
        } else {
            SharedString::from("Type a task...")
        };

        Self {
            value: String::new(),
            caret_pos: 0,
            selection_anchor: None,
            marked_range: None,
            selected_index: 0,
            dismissed: false,
            focus_handle: cx.focus_handle(),
            is_spotlight,
            placeholder,
            scroll_offset_x: px(0.0),
            last_bounds: None,
            last_shaped_line: None,
            is_selecting: false,
            config_store,
        }
    }

    pub fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.value.clear();
        self.caret_pos = 0;
        self.selection_anchor = None;
        self.marked_range = None;
        self.selected_index = 0;
        self.dismissed = false;
        self.scroll_offset_x = px(0.0);
        self.last_bounds = None;
        self.last_shaped_line = None;
        self.is_selecting = false;
        window.focus(&self.focus_handle, cx);
        window.notify_text_input_changed(&self.focus_handle);
        cx.notify();
    }

    pub fn current_suggestions(&self, cx: &App) -> AutocompleteResult {
        let autocomplete = &self.config_store.read(cx).autocomplete;
        get_suggestions(autocomplete, &self.value, self.caret_pos)
    }

    pub fn show_suggestions(&self, cx: &App) -> bool {
        if self.dismissed {
            return false;
        }
        !self.current_suggestions(cx).matches.is_empty()
    }

    pub fn select_suggestion(
        &mut self,
        matched: AutocompleteMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (new_text, new_caret) = apply_completion(&self.value, &matched);
        self.value = new_text;
        self.caret_pos = new_caret;
        self.selection_anchor = None;
        self.marked_range = None;
        self.selected_index = 0;
        self.dismissed = false;
        window.focus(&self.focus_handle, cx);
        window.notify_text_input_changed(&self.focus_handle);
        cx.notify();
    }

    pub fn selected_range(&self) -> Range<usize> {
        if let Some(anchor) = self.selection_anchor {
            anchor.min(self.caret_pos)..anchor.max(self.caret_pos)
        } else {
            self.caret_pos..self.caret_pos
        }
    }

    fn selection_reversed(&self) -> bool {
        self.selection_anchor.is_some_and(|a| self.caret_pos < a)
    }

    fn clamp_char_boundary(&self, offset: usize) -> usize {
        clamp_str_char_boundary(&self.value, offset)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        prev_str_char_boundary(&self.value, offset)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        next_str_char_boundary(&self.value, offset)
    }

    fn move_to(&mut self, offset: usize, window: &Window, cx: &mut Context<Self>) {
        self.caret_pos = self.clamp_char_boundary(offset);
        self.selection_anchor = None;
        self.dismissed = false;
        self.selected_index = 0;
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, window: &Window, cx: &mut Context<Self>) {
        let target = self.clamp_char_boundary(offset);
        let anchor = self.selection_anchor.unwrap_or(self.caret_pos);
        self.caret_pos = target;
        self.selection_anchor = if anchor == target { None } else { Some(anchor) };
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify();
    }

    fn left(&mut self, _: &Left, window: &mut Window, cx: &mut Context<Self>) {
        let range = self.selected_range();
        if range.is_empty() {
            let prev = self.previous_boundary(self.caret_pos);
            self.move_to(prev, window, cx);
        } else {
            self.move_to(range.start, window, cx);
        }
    }

    fn right(&mut self, _: &Right, window: &mut Window, cx: &mut Context<Self>) {
        let range = self.selected_range();
        if range.is_empty() {
            let next = self.next_boundary(self.caret_pos);
            self.move_to(next, window, cx);
        } else {
            self.move_to(range.end, window, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        let prev = self.previous_boundary(self.caret_pos);
        self.select_to(prev, window, cx);
    }

    fn select_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        let next = self.next_boundary(self.caret_pos);
        self.select_to(next, window, cx);
    }

    fn home(&mut self, _: &Home, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, window, cx);
    }

    fn end(&mut self, _: &End, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.value.len(), window, cx);
    }

    fn select_home(&mut self, _: &SelectHome, window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(0, window, cx);
    }

    fn select_end(&mut self, _: &SelectEnd, window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.value.len(), window, cx);
    }

    fn select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        if !self.value.is_empty() {
            self.selection_anchor = Some(0);
            self.caret_pos = self.value.len();
            window.notify_text_selection_changed(&self.focus_handle);
            cx.notify();
        }
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range().is_empty() {
            let prev = self.previous_boundary(self.caret_pos);
            if prev == self.caret_pos {
                return;
            }
            self.selection_anchor = Some(self.caret_pos);
            self.caret_pos = prev;
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range().is_empty() {
            let next = self.next_boundary(self.caret_pos);
            if next == self.caret_pos {
                return;
            }
            self.selection_anchor = Some(self.caret_pos);
            self.caret_pos = next;
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let range = self.selected_range();
        if !range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.value[range].to_string()));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        let range = self.selected_range();
        if !range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.value[range].to_string()));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let single_line = text.replace(['\r', '\n'], " ");
            self.replace_text_in_range(None, &single_line, window, cx);
        }
    }

    fn accept_suggestion(
        &mut self,
        _: &AcceptSuggestion,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dismissed {
            return;
        }
        let res = self.current_suggestions(cx);
        if res.matches.is_empty() {
            return;
        }
        let idx = self.selected_index.min(res.matches.len() - 1);
        let matched = res.matches[idx].clone();
        self.select_suggestion(matched, window, cx);
    }

    fn next_suggestion(
        &mut self,
        _: &NextSuggestion,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dismissed {
            return;
        }
        let count = self.current_suggestions(cx).matches.len();
        if count > 0 {
            self.selected_index = (self.selected_index + 1) % count;
            cx.notify();
        }
    }

    fn prev_suggestion(
        &mut self,
        _: &PrevSuggestion,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dismissed {
            return;
        }
        let count = self.current_suggestions(cx).matches.len();
        if count > 0 {
            self.selected_index = (self.selected_index + count - 1) % count;
            cx.notify();
        }
    }

    fn dismiss_or_hide(
        &mut self,
        _: &DismissOrHide,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.show_suggestions(cx) {
            self.dismissed = true;
            cx.notify();
        } else {
            cx.emit(CommandInputEvent::DismissSpotlight);
        }
    }

    fn submit(&mut self, _: &Submit, window: &mut Window, cx: &mut Context<Self>) {
        let submitted = self.value.clone();
        self.value.clear();
        self.caret_pos = 0;
        self.selection_anchor = None;
        self.marked_range = None;
        self.dismissed = false;
        self.selected_index = 0;
        self.scroll_offset_x = px(0.0);
        window.notify_text_input_changed(&self.focus_handle);
        cx.emit(CommandInputEvent::Submit(submitted));
        cx.notify();
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.value.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_shaped_line.as_ref())
        else {
            return 0;
        };

        let local_x = (position.x - bounds.left() + self.scroll_offset_x).max(px(0.0));
        let raw_idx = line.closest_index_for_x(local_x);

        // `line` may include an inline ghost suffix inserted at `self.caret_pos`.
        let ghost_len = line.text.len().saturating_sub(self.value.len());
        let mapped_idx = if ghost_len > 0 {
            if raw_idx <= self.caret_pos {
                raw_idx
            } else if raw_idx <= self.caret_pos + ghost_len {
                self.caret_pos
            } else {
                raw_idx - ghost_len
            }
        } else {
            raw_idx
        };

        self.clamp_char_boundary(mapped_idx)
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;
        window.focus(&self.focus_handle, cx);
        window.request_virtual_keyboard();

        let idx = self.index_for_mouse_position(event.position);
        if event.modifiers.shift {
            self.select_to(idx, window, cx);
        } else {
            self.move_to(idx, window, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, _cx: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting {
            let idx = self.index_for_mouse_position(event.position);
            self.select_to(idx, window, cx);
        }
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        utf16_to_utf8_offset(&self.value, offset)
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        utf8_to_utf16_offset(&self.value, offset)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }
}

impl Focusable for CommandInputView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for CommandInputView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.value[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let selected_range = self.selected_range();
        Some(UTF16Selection {
            range: self.range_to_utf16(&selected_range),
            reversed: self.selection_reversed(),
        })
    }

    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.range_from_utf16(&range_utf16);
        self.caret_pos = range.end;
        self.selection_anchor = if range.is_empty() {
            None
        } else {
            Some(range.start)
        };
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify();
    }

    fn text_length_utf16(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.value.encode_utf16().count())
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.marked_range = None;
        window.notify_text_input_changed(&self.focus_handle);
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range());

        // Intercept Space insertion when selection is collapsed and no IME mark is active.
        if new_text == " " && range.is_empty() && self.marked_range.is_none() {
            let expansions = self.config_store.read(cx).autocomplete.expansions.clone();
            let result = expand_on_space(&self.value, range.start, &expansions);
            if result.expanded {
                self.value = result.new_text;
                self.caret_pos = result.new_caret_pos;
                self.selection_anchor = None;
                self.marked_range = None;
                self.dismissed = false;
                self.selected_index = 0;
                window.notify_text_input_changed(&self.focus_handle);
                cx.notify();
                return;
            }
        }

        let prev_value = self.value.clone();
        let candidate = format!(
            "{}{}{}",
            &self.value[..range.start],
            new_text,
            &self.value[range.end..]
        );

        let final_value = auto_capitalize_first_letter(&candidate, &prev_value);
        let new_caret = if final_value != candidate {
            final_value.len()
        } else {
            range.start + new_text.len()
        };

        self.value = final_value;
        self.caret_pos = self.clamp_char_boundary(new_caret);
        self.selection_anchor = None;
        self.marked_range = None;
        self.dismissed = false;
        self.selected_index = 0;
        window.notify_text_input_changed(&self.focus_handle);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range());

        self.value = format!(
            "{}{}{}",
            &self.value[..range.start],
            new_text,
            &self.value[range.end..]
        );

        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }

        let insertion_start_utf16 = self.offset_to_utf16(range.start);
        let inserted_length_utf16 = new_text.encode_utf16().count();
        let selected = new_selected_range_utf16
            .map(|sel| {
                self.range_from_utf16(
                    &(insertion_start_utf16 + sel.start.min(inserted_length_utf16)
                        ..insertion_start_utf16 + sel.end.min(inserted_length_utf16)),
                )
            })
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());

        self.caret_pos = self.clamp_char_boundary(selected.end);
        self.selection_anchor = if selected.is_empty() {
            None
        } else {
            Some(self.clamp_char_boundary(selected.start))
        };
        self.dismissed = false;
        self.selected_index = 0;

        window.notify_text_input_changed(&self.focus_handle);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_shaped_line.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        let scroll = self.scroll_offset_x;
        Some(Bounds::from_corners(
            point(
                bounds.left() + line.x_for_index(range.start) - scroll,
                bounds.top(),
            ),
            point(
                bounds.left() + line.x_for_index(range.end) - scroll,
                bounds.bottom(),
            ),
        ))
    }

    fn selection_bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Bounds<Pixels>> {
        self.bounds_for_range(range_utf16, bounds, window, cx)
            .into_iter()
            .collect()
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let utf8_index = self.index_for_mouse_position(point);
        Some(self.offset_to_utf16(utf8_index))
    }
}

struct CommandInputElement {
    input: Entity<CommandInputView>,
}

struct CommandInputPrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
    scroll_offset_x: Pixels,
}

impl Element for CommandInputElement {
    type RequestLayoutState = ();
    type PrepaintState = CommandInputPrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let palette = cx.global::<ActiveTheme>().palette;
        let input = self.input.read(cx);
        let value = input.value.clone();
        let caret_pos = input.caret_pos.min(value.len());
        let selected_range = input.selected_range();
        let marked_range = input.marked_range.clone();
        let mut scroll_offset_x = input.scroll_offset_x;
        let text_style = window.text_style();
        let font = text_style.font();
        let font_size = text_style.font_size.to_pixels(window.rem_size());

        let (display_text, runs) = if value.is_empty() {
            let placeholder = input.placeholder.clone();
            let run = TextRun {
                len: placeholder.len(),
                font,
                color: palette.muted_text.into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            (placeholder, vec![run])
        } else {
            let suggestions = input.current_suggestions(cx);
            let show_ghost =
                !input.dismissed && !suggestions.matches.is_empty() && !suggestions.ghost_suffix.is_empty();
            let ghost_suffix = if show_ghost {
                suggestions.ghost_suffix.as_str()
            } else {
                ""
            };

            let before_caret = &value[..caret_pos];
            let after_caret = &value[caret_pos..];
            let full_text = SharedString::from(format!("{before_caret}{ghost_suffix}{after_caret}"));

            let typed_run = TextRun {
                len: value.len(),
                font: font.clone(),
                color: palette.input_text.into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };

            let mut built_runs = Vec::with_capacity(3);
            if show_ghost {
                if !before_caret.is_empty() {
                    built_runs.push(TextRun {
                        len: before_caret.len(),
                        ..typed_run.clone()
                    });
                }
                built_runs.push(TextRun {
                    len: ghost_suffix.len(),
                    font: font.clone(),
                    color: palette.ghost_suffix.into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                });
                if !after_caret.is_empty() {
                    built_runs.push(TextRun {
                        len: after_caret.len(),
                        ..typed_run
                    });
                }
            } else if let Some(marked) = marked_range {
                let start = marked.start.min(value.len());
                let end = marked.end.min(value.len());
                if start > 0 {
                    built_runs.push(TextRun {
                        len: start,
                        ..typed_run.clone()
                    });
                }
                if end > start {
                    built_runs.push(TextRun {
                        len: end - start,
                        underline: Some(UnderlineStyle {
                            color: Some(palette.input_text.into()),
                            thickness: px(1.0),
                            wavy: false,
                        }),
                        ..typed_run.clone()
                    });
                }
                if value.len() > end {
                    built_runs.push(TextRun {
                        len: value.len() - end,
                        ..typed_run
                    });
                }
            } else {
                built_runs.push(typed_run);
            }

            (full_text, built_runs)
        };

        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);

        let caret_x = if value.is_empty() {
            px(0.0)
        } else {
            line.x_for_index(caret_pos)
        };

        let visible_width = bounds.size.width;
        if value.is_empty() || line.width <= visible_width {
            scroll_offset_x = px(0.0);
        } else if visible_width > px(4.0) {
            let max_visible_x = visible_width - px(2.0);
            if caret_x - scroll_offset_x > max_visible_x {
                scroll_offset_x = caret_x - max_visible_x;
            } else if caret_x < scroll_offset_x {
                scroll_offset_x = caret_x;
            }
        }

        let cursor_screen_x = bounds.left() + caret_x - scroll_offset_x;
        let cursor = Some(fill(
            Bounds::new(
                point(cursor_screen_x, bounds.top()),
                size(px(1.5), bounds.bottom() - bounds.top()),
            ),
            palette.input_text,
        ));

        let selection = if !selected_range.is_empty() && !value.is_empty() {
            let start_x = bounds.left() + line.x_for_index(selected_range.start) - scroll_offset_x;
            let end_x = bounds.left() + line.x_for_index(selected_range.end) - scroll_offset_x;
            Some(fill(
                Bounds::from_corners(
                    point(start_x, bounds.top()),
                    point(end_x, bounds.bottom()),
                ),
                rgba(0x0d6efd40),
            ))
        } else {
            None
        };

        CommandInputPrepaintState {
            line: Some(line),
            cursor,
            selection,
            scroll_offset_x,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );

        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }

        let line = prepaint.line.take().unwrap();
        let paint_origin = point(bounds.origin.x - prepaint.scroll_offset_x, bounds.origin.y);
        let _ = line.paint(
            paint_origin,
            window.line_height(),
            TextAlign::Left,
            None,
            window,
            cx,
        );

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        let scroll_offset_x = prepaint.scroll_offset_x;
        self.input.update(cx, |input, _cx| {
            input.last_shaped_line = Some(line);
            input.last_bounds = Some(bounds);
            input.scroll_offset_x = scroll_offset_x;
        });
    }
}

impl Render for CommandInputView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.global::<ActiveTheme>().palette;
        let is_focused = self.focus_handle.is_focused(window);

        let container = div()
            .id(if self.is_spotlight {
                "spotlight-command-input"
            } else {
                "footer-command-input"
            })
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::accept_suggestion))
            .on_action(cx.listener(Self::next_suggestion))
            .on_action(cx.listener(Self::prev_suggestion))
            .on_action(cx.listener(Self::dismiss_or_hide))
            .on_action(cx.listener(Self::submit))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .overflow_hidden();

        if self.is_spotlight {
            container
                .py(px(4.0))
                .text_size(rems(1.15))
                .line_height(rems(1.5))
                .text_color(palette.input_text)
                .child(CommandInputElement {
                    input: cx.entity(),
                })
        } else {
            let border_color = if is_focused {
                gpui::rgb(0x86b7fe)
            } else {
                palette.input_border
            };

            container
                .px(rems(0.5))
                .py(rems(0.25))
                .rounded(px(4.0))
                .bg(palette.input_bg)
                .border_1()
                .border_color(border_color)
                .text_size(rems(0.875))
                .line_height(rems(1.4))
                .text_color(palette.input_text)
                .child(CommandInputElement {
                    input: cx.entity(),
                })
        }
    }
}

pub fn clamp_str_char_boundary(s: &str, offset: usize) -> usize {
    let mut idx = offset.min(s.len());
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

pub fn prev_str_char_boundary(s: &str, offset: usize) -> usize {
    let clamped = clamp_str_char_boundary(s, offset);
    s[..clamped]
        .char_indices()
        .next_back()
        .map(|(i, _)| i)
        .unwrap_or(0)
}

pub fn next_str_char_boundary(s: &str, offset: usize) -> usize {
    let clamped = clamp_str_char_boundary(s, offset);
    s[clamped..]
        .chars()
        .next()
        .map(|ch| clamped + ch.len_utf8())
        .unwrap_or(s.len())
}

pub fn utf16_to_utf8_offset(s: &str, offset: usize) -> usize {
    let mut utf8_offset = 0;
    let mut utf16_count = 0;

    for ch in s.chars() {
        if utf16_count >= offset {
            break;
        }
        utf16_count += ch.len_utf16();
        utf8_offset += ch.len_utf8();
    }

    utf8_offset
}

pub fn utf8_to_utf16_offset(s: &str, offset: usize) -> usize {
    let mut utf16_offset = 0;
    let mut utf8_count = 0;

    for ch in s.chars() {
        if utf8_count >= offset {
            break;
        }
        utf8_count += ch.len_utf8();
        utf16_offset += ch.len_utf16();
    }

    utf16_offset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf8_utf16_roundtrip_and_boundaries() {
        // String with 2-byte ('é'), 3-byte ('☕'), and 4-byte ('🚀') UTF-8 characters
        let s = "Café ☕ 🚀!";
        // 'C'(1,1), 'a'(1,1), 'f'(1,1), 'é'(2,1), ' '(1,1), '☕'(3,1), ' '(1,1), '🚀'(4,2), '!'(1,1)
        assert_eq!(utf8_to_utf16_offset(s, 0), 0);
        assert_eq!(utf8_to_utf16_offset(s, 5), 4); // after "Café"
        assert_eq!(utf16_to_utf8_offset(s, 4), 5);

        let rocket_utf8_end = "Café ☕ 🚀".len();
        let rocket_utf16_end = utf8_to_utf16_offset(s, rocket_utf8_end);
        // 4 ("Café") + 1 (" ") + 1 ("☕") + 1 (" ") + 2 ("🚀") = 9 UTF-16 code units
        assert_eq!(rocket_utf16_end, 9);
        assert_eq!(utf16_to_utf8_offset(s, 9), rocket_utf8_end);

        // Stepping backward and forward across multi-byte char boundaries
        assert_eq!(prev_str_char_boundary(s, rocket_utf8_end), "Café ☕ ".len());
        assert_eq!(
            next_str_char_boundary(s, "Café ☕ ".len()),
            rocket_utf8_end
        );
        // Non-boundary offset clamps down safely
        assert_eq!(clamp_str_char_boundary(s, 4), 3); // middle of 'é' -> start of 'é'
    }
}
