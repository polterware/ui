//! Single-line text field. GPUI has no built-in input; this follows `gpui` `examples/input.rs`.
//! Password mode shows one bullet per character and keeps the real text for copy and paste.
//! Keys move and delete by character, word (Option), and line (Command) as macOS fields do,
//! and Command-Z undoes the last edit: a run of typing or of deleting undoes as one.
//!
//! It wears `Input` (`.control-field.pw-field-control`): the field fill (#1a1a1a to #171717,
//! #1e1e1e to #1b1b1b under the pointer, swapped at once as a gradient cannot ease), the
//! control edge and drop, and placeholder text in `--color-text-disabled`. Focused, the edge
//! gives way to a 1px primary ring inside. `sm` is 32px, radius 6, 12px text, padded 0.5rem;
//! `md` 36px, radius 12, 13px text, padded 0.625rem. [`TextInput::pill`] is `SearchBar`'s
//! shape: fully round, padded 3.5 steps. [`TextInput::bare`] drops the face for a search
//! that sits in a popover's header (`.pw-filter-popover-search`): 13px text, no edge or ring.

use std::ops::Range;
use std::rc::Rc;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId, InspectorElementId,
    IntoElement, KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    PaintQuad, Pixels, Point, ShapedLine, SharedString, Style, TextRun, UTF16Selection,
    UnderlineStyle, Window, actions, div, fill, point, prelude::*, px, relative, size,
};
use unicode_segmentation::UnicodeSegmentation;

use super::material::{self, Edge, face};
use crate::Handler;
use crate::theme::{Theme, rem, tw};

/// `ControlFieldSize`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldSize {
    Sm,
    Md,
}

const BULLET: char = '\u{2022}';

actions!(
    text_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        ShowCharacterPalette,
        Paste,
        Cut,
        Copy,
        Submit,
        Cancel,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        SelectHome,
        SelectEnd,
        DeleteWordLeft,
        DeleteWordRight,
        DeleteToStart,
        DeleteToEnd,
        Undo,
        Redo,
    ]
);

/// Edits kept for undo.
const HISTORY: usize = 200;

pub fn register_key_bindings(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some("TextInput")),
        KeyBinding::new("delete", Delete, Some("TextInput")),
        KeyBinding::new("left", Left, Some("TextInput")),
        KeyBinding::new("right", Right, Some("TextInput")),
        KeyBinding::new("shift-left", SelectLeft, Some("TextInput")),
        KeyBinding::new("shift-right", SelectRight, Some("TextInput")),
        KeyBinding::new("cmd-a", SelectAll, Some("TextInput")),
        KeyBinding::new("cmd-v", Paste, Some("TextInput")),
        KeyBinding::new("cmd-c", Copy, Some("TextInput")),
        KeyBinding::new("cmd-x", Cut, Some("TextInput")),
        KeyBinding::new("home", Home, Some("TextInput")),
        KeyBinding::new("end", End, Some("TextInput")),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, Some("TextInput")),
        KeyBinding::new("enter", Submit, Some("TextInput")),
        KeyBinding::new("escape", Cancel, Some("TextInput")),
        KeyBinding::new("alt-left", WordLeft, Some("TextInput")),
        KeyBinding::new("alt-right", WordRight, Some("TextInput")),
        KeyBinding::new("alt-shift-left", SelectWordLeft, Some("TextInput")),
        KeyBinding::new("alt-shift-right", SelectWordRight, Some("TextInput")),
        KeyBinding::new("cmd-left", Home, Some("TextInput")),
        KeyBinding::new("cmd-right", End, Some("TextInput")),
        KeyBinding::new("shift-home", SelectHome, Some("TextInput")),
        KeyBinding::new("shift-end", SelectEnd, Some("TextInput")),
        KeyBinding::new("cmd-shift-left", SelectHome, Some("TextInput")),
        KeyBinding::new("cmd-shift-right", SelectEnd, Some("TextInput")),
        KeyBinding::new("alt-backspace", DeleteWordLeft, Some("TextInput")),
        KeyBinding::new("alt-delete", DeleteWordRight, Some("TextInput")),
        KeyBinding::new("cmd-backspace", DeleteToStart, Some("TextInput")),
        KeyBinding::new("cmd-delete", DeleteToEnd, Some("TextInput")),
        KeyBinding::new("cmd-z", Undo, Some("TextInput")),
        KeyBinding::new("cmd-shift-z", Redo, Some("TextInput")),
    ]);
}

type TextCallback = Rc<dyn Fn(&str, &mut Window, &mut App)>;

/// The text and selection before an edit.
#[derive(Clone)]
struct Snapshot {
    content: SharedString,
    selected_range: Range<usize>,
}

/// The kind of the last edit, so a run of typing or deleting undoes as one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum EditKind {
    Insert,
    Delete,
    Other,
}
type CancelCallback = Handler;

pub struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    password: bool,
    size: FieldSize,
    pill: bool,
    bare: bool,
    on_submit: Option<TextCallback>,
    on_cancel: Option<CancelCallback>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// The last edit and where the caret was after it; `None` starts a new undo step.
    last_edit: Option<(EditKind, usize)>,
}

impl TextInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: SharedString::default(),
            placeholder: SharedString::default(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            password: false,
            size: FieldSize::Sm,
            pill: false,
            bare: false,
            on_submit: None,
            on_cancel: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
        }
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn size(mut self, size: FieldSize) -> Self {
        self.size = size;
        self
    }

    /// `SearchBar`: fully round, padded 3.5 steps.
    pub fn pill(mut self) -> Self {
        self.pill = true;
        self
    }

    /// `.pw-filter-popover-search`: no fill, edge, or focus ring.
    pub fn bare(mut self) -> Self {
        self.bare = true;
        self
    }

    pub fn password(mut self, password: bool) -> Self {
        self.password = password;
        self
    }

    pub fn on_submit(mut self, callback: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_submit = Some(Rc::new(callback));
        self
    }

    pub fn on_cancel(mut self, callback: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(callback));
        self
    }

    pub fn text(&self) -> &str {
        self.content.as_ref()
    }

    /// Replaces the text, with the caret at its end. It starts a new history: undo does not
    /// go back past it.
    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        let cleaned = single_line(&text.into());
        self.undo.clear();
        self.redo.clear();
        self.last_edit = None;
        self.content = SharedString::from(cleaned);
        self.selected_range = self.content.len()..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
        cx.notify();
    }

    fn submit(&mut self, _: &Submit, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(callback) = self.on_submit.clone() {
            let text = self.content.to_string();
            callback(&text, window, cx);
        }
    }

    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        match self.on_cancel.clone() {
            Some(callback) => callback(window, cx),
            // With nothing to cancel, Escape goes on to close what holds the field.
            None => cx.propagate(),
        }
    }

    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_word_start(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_word_end(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_word_start(self.cursor_offset()), cx);
    }

    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_word_end(self.cursor_offset()), cx);
    }

    fn select_home(&mut self, _: &SelectHome, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(0, cx);
    }

    fn select_end(&mut self, _: &SelectEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.content.len(), cx);
    }

    fn delete_word_left(
        &mut self,
        _: &DeleteWordLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_word_start(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete_word_right(
        &mut self,
        _: &DeleteWordRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_word_end(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete_to_start(&mut self, _: &DeleteToStart, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(0, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete_to_end(&mut self, _: &DeleteToEnd, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.content.len(), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(snapshot) = self.undo.pop() {
            let current = self.snapshot();
            self.redo.push(current);
            self.restore(snapshot, cx);
        }
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(snapshot) = self.redo.pop() {
            let current = self.snapshot();
            self.undo.push(current);
            self.restore(snapshot, cx);
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            content: self.content.clone(),
            selected_range: self.selected_range.clone(),
        }
    }

    fn restore(&mut self, snapshot: Snapshot, cx: &mut Context<Self>) {
        self.content = snapshot.content;
        self.selected_range = clamp_range(&self.content, snapshot.selected_range);
        self.selection_reversed = false;
        self.marked_range = None;
        self.last_edit = None;
        cx.notify();
    }

    /// Keeps the text as it is before an edit of `kind` over `range`, unless the edit goes on
    /// the run the last one started.
    fn record(&mut self, kind: EditKind, range: &Range<usize>) {
        let continues = match self.last_edit {
            Some((last, caret)) => {
                kind != EditKind::Other
                    && last == kind
                    && match kind {
                        EditKind::Insert => range.start == caret && range.is_empty(),
                        _ => range.end == caret || range.start == caret,
                    }
            }
            None => false,
        };
        if !continues {
            self.undo.push(self.snapshot());
            if self.undo.len() > HISTORY {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
    }

    /// Where Option-Left lands: the start of the word before `offset`.
    fn previous_word_start(&self, offset: usize) -> usize {
        self.content
            .split_word_bound_indices()
            .rev()
            .find(|(start, word)| *start < offset && is_word(word))
            .map(|(start, _)| start)
            .unwrap_or(0)
    }

    /// Where Option-Right lands: the end of the word after `offset`.
    fn next_word_end(&self, offset: usize) -> usize {
        self.content
            .split_word_bound_indices()
            .map(|(start, word)| (start + word.len(), word))
            .find(|(end, word)| *end > offset && is_word(word))
            .map(|(end, _)| end)
            .unwrap_or(self.content.len())
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.is_selecting = true;
        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.last_edit = None;
            self.replace_text_in_range(None, &single_line(&text), window, cx);
            self.last_edit = Some((EditKind::Other, self.cursor_offset()));
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.last_edit = None;
            self.replace_text_in_range(None, "", window, cx);
            self.last_edit = Some((EditKind::Other, self.cursor_offset()));
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.content.len());
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        let display_index = line.closest_index_for_x(position.x - bounds.left());
        self.content_offset_for_display(display_index)
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.content.len());
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }

    fn display_offset_for_content(&self, content_offset: usize) -> usize {
        if !self.password {
            return content_offset.min(self.content.len());
        }
        let offset = content_offset.min(self.content.len());
        self.content[..offset].chars().count() * BULLET.len_utf8()
    }

    fn content_offset_for_display(&self, display_offset: usize) -> usize {
        if !self.password {
            return display_offset.min(self.content.len());
        }
        let chars = display_offset / BULLET.len_utf8();
        self.content
            .chars()
            .take(chars)
            .map(|ch| ch.len_utf8())
            .sum()
    }

    fn display_text(&self) -> SharedString {
        if self.content.is_empty() {
            self.placeholder.clone()
        } else if self.password {
            SharedString::from(BULLET.to_string().repeat(self.content.chars().count()))
        } else {
            self.content.clone()
        }
    }
}

fn single_line(text: &str) -> String {
    text.replace(['\n', '\r'], "")
}

fn is_word(segment: &str) -> bool {
    segment.chars().any(char::is_alphanumeric)
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
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

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_text = single_line(new_text);
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        let range = clamp_range(&self.content, range);
        // Committing a composition finishes the edit its first mark recorded.
        let kind = if new_text.is_empty() {
            EditKind::Delete
        } else if range.is_empty() && !new_text.starts_with(char::is_whitespace) {
            EditKind::Insert
        } else {
            EditKind::Other
        };
        if self.marked_range.is_none() {
            self.record(kind, &range);
        }
        self.content =
            (self.content[0..range.start].to_owned() + &new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        self.last_edit = Some((kind, self.selected_range.end));
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_text = single_line(new_text);
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        let range = clamp_range(&self.content, range);
        if self.marked_range.is_none() {
            self.last_edit = None;
            self.record(EditKind::Other, &range);
        }
        self.content =
            (self.content[0..range.start].to_owned() + &new_text + &self.content[range.end..])
                .into();
        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        let start = self.display_offset_for_content(range.start);
        let end = self.display_offset_for_content(range.end);
        Some(Bounds::from_corners(
            point(bounds.left() + last_layout.x_for_index(start), bounds.top()),
            point(
                bounds.left() + last_layout.x_for_index(end),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let line_point = bounds.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;
        let display_index = last_layout.index_for_x(point.x - line_point.x)?;
        if self.content.is_empty() {
            return Some(0);
        }
        let utf8_index = self.content_offset_for_display(display_index);
        Some(self.offset_to_utf16(utf8_index))
    }
}

fn clamp_range(text: &str, range: Range<usize>) -> Range<usize> {
    let end = range.end.min(text.len());
    let start = range.start.min(end);
    start..end
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let theme = *Theme::global(cx);
        let (
            display_text,
            selected_range,
            cursor,
            marked_range,
            password,
            content_empty,
            placeholder_color,
        ) = {
            let input = self.input.read(cx);
            (
                input.display_text(),
                input.selected_range.clone(),
                input.cursor_offset(),
                input.marked_range.clone(),
                input.password,
                input.content.is_empty(),
                theme.color_text_disabled,
            )
        };
        let style = window.text_style();
        let text_color = if content_empty {
            placeholder_color.into()
        } else {
            style.color
        };
        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let display_marked = marked_range.as_ref().map(|range| {
            if password {
                let input = self.input.read(cx);
                input.display_offset_for_content(range.start)
                    ..input.display_offset_for_content(range.end)
            } else {
                range.clone()
            }
        });
        let runs = if let Some(marked) = display_marked {
            let end = marked.end.min(display_text.len());
            let start = marked.start.min(end);
            vec![
                TextRun {
                    len: start,
                    ..run.clone()
                },
                TextRun {
                    len: end - start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len().saturating_sub(end),
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);

        let display_cursor = if content_empty {
            0
        } else if password {
            self.input.read(cx).display_offset_for_content(cursor)
        } else {
            cursor
        };
        let (selection_start, selection_end) = if content_empty {
            (0, 0)
        } else if password {
            let input = self.input.read(cx);
            (
                input.display_offset_for_content(selected_range.start),
                input.display_offset_for_content(selected_range.end),
            )
        } else {
            (selected_range.start, selected_range.end)
        };
        let cursor_pos = line.x_for_index(display_cursor);
        let (selection, cursor_quad) = if selection_start == selection_end {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_pos, bounds.top()),
                        size(px(1.5), bounds.bottom() - bounds.top()),
                    ),
                    theme.color_text_primary,
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(selection_start),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(selection_end),
                            bounds.bottom(),
                        ),
                    ),
                    theme.color_accent,
                )),
                None,
            )
        };
        PrepaintState {
            line: Some(line),
            cursor: cursor_quad,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
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
        line.paint(bounds.origin, window.line_height(), window, cx)
            .unwrap();

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let (height, radius, text, line, pad) = match self.size {
            FieldSize::Sm => (theme.field_height_sm, 6.0, 12.0, 15.0, rem() / 2.0),
            FieldSize::Md => (theme.field_height_md, 12.0, 13.0, 16.9, rem() * 0.625),
        };
        let (radius, pad) = if self.pill {
            (height / 2.0, tw(3.5))
        } else {
            (radius, pad)
        };
        let focused = self.focus_handle.is_focused(window);
        let hover_fill = face(theme.field_fill_hover_top, theme.field_fill_hover_bottom);
        let bare = self.bare;
        let (height, text, line) = if bare {
            (19.5, 13.0, 19.5)
        } else {
            (height, text, line)
        };
        div()
            .id(("text-input", cx.entity_id()))
            .relative()
            .flex()
            .w_full()
            .key_context("TextInput")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::delete_word_left))
            .on_action(cx.listener(Self::delete_word_right))
            .on_action(cx.listener(Self::delete_to_start))
            .on_action(cx.listener(Self::delete_to_end))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .h(px(height))
            .items_center()
            .when(!bare, |field| {
                field
                    .px(px(pad))
                    .rounded(px(radius))
                    .bg(face(theme.field_fill_top, theme.field_fill_bottom))
                    .hover(move |style| style.bg(hover_fill))
                    .shadow(material::control_drop(&theme))
            })
            .text_size(px(text))
            .line_height(px(line))
            .text_color(theme.color_text_primary)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .child(TextElement { input: cx.entity() }),
            )
            .map(|field| {
                if bare {
                    field
                } else if focused {
                    field.child(
                        div()
                            .absolute()
                            .inset_0()
                            .rounded(px(radius))
                            .border_1()
                            .border_color(theme.color_primary),
                    )
                } else {
                    field.child(material::edge(Edge::Strong, radius, &theme))
                }
            })
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{
        Entity, EntityInputHandler, Focusable, TestAppContext, VisualTestContext, prelude::*, size,
    };

    use super::TextInput;
    use crate::test_support::{draw, open};

    /// A focused field in a window, built by `build`.
    fn field(
        cx: &mut TestAppContext,
        build: impl FnOnce(TextInput) -> TextInput + 'static,
    ) -> (Entity<TextInput>, VisualTestContext) {
        let input: Rc<RefCell<Option<Entity<TextInput>>>> = Rc::default();
        let build = RefCell::new(Some(build));
        let slot = input.clone();
        let mut cx = open(cx, size(400.0, 200.0), move |_, cx| {
            let entity = slot
                .borrow_mut()
                .get_or_insert_with(|| {
                    let build = build.borrow_mut().take().unwrap();
                    cx.new(|cx| build(TextInput::new(cx)))
                })
                .clone();
            entity.into_any_element()
        });
        let input = input.borrow().clone().unwrap();
        cx.update(|window, cx| input.focus_handle(cx).focus(window));
        draw(&mut cx);
        (input, cx)
    }

    fn text(input: &Entity<TextInput>, cx: &mut VisualTestContext) -> String {
        cx.update(|_, cx| input.read(cx).text().to_string())
    }

    #[gpui::test]
    fn typing_selecting_and_undo_edit_the_text(cx: &mut TestAppContext) {
        let (input, mut cx) = field(cx, |field| field);
        cx.simulate_input("hello world");
        assert_eq!(text(&input, &mut cx), "hello world");
        cx.simulate_keystrokes("alt-backspace");
        assert_eq!(text(&input, &mut cx), "hello ");
        cx.simulate_keystrokes("cmd-a");
        cx.simulate_input("x");
        assert_eq!(text(&input, &mut cx), "x");
        cx.simulate_keystrokes("cmd-z");
        assert_eq!(text(&input, &mut cx), "hello ");
        cx.simulate_keystrokes("cmd-z");
        assert_eq!(text(&input, &mut cx), "hello world");
        cx.simulate_keystrokes("cmd-shift-z cmd-shift-z");
        assert_eq!(text(&input, &mut cx), "x");
    }

    #[gpui::test]
    fn option_and_command_move_by_word_and_line(cx: &mut TestAppContext) {
        let (input, mut cx) = field(cx, |field| field);
        cx.simulate_input("one two three");
        cx.simulate_keystrokes("alt-left alt-left");
        cx.simulate_input("X");
        assert_eq!(text(&input, &mut cx), "one Xtwo three");
        cx.simulate_keystrokes("cmd-right alt-shift-left");
        cx.simulate_input("3");
        assert_eq!(text(&input, &mut cx), "one Xtwo 3");
        cx.simulate_keystrokes("cmd-backspace");
        assert_eq!(text(&input, &mut cx), "");
    }

    #[gpui::test]
    fn enter_submits_and_set_text_starts_the_history(cx: &mut TestAppContext) {
        let submitted: Rc<RefCell<Vec<String>>> = Rc::default();
        let sink = submitted.clone();
        let (input, mut cx) = field(cx, move |field| {
            field.on_submit(move |text, _, _| sink.borrow_mut().push(text.to_string()))
        });
        cx.update(|_, cx| input.update(cx, |input, cx| input.set_text("from code", cx)));
        draw(&mut cx);
        cx.simulate_input("!");
        cx.simulate_keystrokes("enter");
        assert_eq!(submitted.borrow().as_slice(), ["from code!"]);
        cx.simulate_keystrokes("cmd-z cmd-z");
        assert_eq!(text(&input, &mut cx), "from code");
    }

    #[gpui::test]
    fn a_password_field_keeps_the_real_text(cx: &mut TestAppContext) {
        let (input, mut cx) = field(cx, |field| field.password(true));
        cx.simulate_input("s3cret");
        assert_eq!(text(&input, &mut cx), "s3cret");
    }

    #[gpui::test]
    fn ime_composition_commits_into_the_text_as_one_undo_step(cx: &mut TestAppContext) {
        let (input, mut cx) = field(cx, |field| field);
        cx.update(|window, cx| {
            input.update(cx, |input, cx| {
                input.replace_and_mark_text_in_range(None, "に", None, window, cx);
                input.replace_and_mark_text_in_range(None, "にほ", None, window, cx);
                assert!(input.marked_text_range(window, cx).is_some());
                input.replace_text_in_range(None, "日本", window, cx);
                assert!(input.marked_text_range(window, cx).is_none());
            })
        });
        assert_eq!(text(&input, &mut cx), "日本");
        cx.simulate_keystrokes("cmd-z");
        assert_eq!(text(&input, &mut cx), "");
    }
}
