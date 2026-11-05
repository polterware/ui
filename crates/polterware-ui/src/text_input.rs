//! Single-line text field. GPUI has no built-in input; this follows `gpui` `examples/input.rs`.
//! Password mode shows one bullet per character and keeps the real text for copy and paste.
//! Keys move and delete by character, word (Option), and line (Command) as macOS fields do,
//! and Command-Z undoes the last edit: a run of typing or of deleting undoes as one.
//!
//! It wears `Input` (`.control-field.pw-field-control`): the field fill (#1a1a1a to #171717,
//! #1e1e1e to #1b1b1b under the pointer, swapped at once as a gradient cannot ease), the
//! control edge and drop, and placeholder text in `--color-text-disabled`. Focused, the edge
//! gives way to a 1px primary ring inside. Every field is round-ended, as the buttons are:
//! `sm` is 32px, 12px text, padded 3 steps; `md` 36px, 13px text, padded 3.5 steps, the
//! `SearchBar` shape [`TextInput::pill`] names. [`TextInput::bare`] drops the face for a search
//! that sits in a popover's header (`.pw-filter-popover-search`): 13px text, no edge or ring.
//! [`TextInput::title`] sets it as the Composer's title (`.commander-search-input`): 18px
//! semibold at line-height 1.35.
//!
//! [`TextInput::wrap`] soft-wraps the value at the field's width, as a web `<textarea>` does,
//! and the field grows to its lines. It is still one line of text: Enter submits and no
//! newline is typed or pasted. The caret, the selection, a click or a drag, Home and End, and
//! Up and Down move over the visual lines; the placeholder wraps too.
//!
//! [`TextInput::multiline`] is the `<textarea>` itself, as gpui-kit's multi-line `Input` is:
//! a field that wraps and keeps hard line breaks. Enter and pasted newlines go into the text,
//! each line of it a paragraph that wraps on its own, and only what holds the field submits
//! it (the Composer's Cmd+Enter). [`TextInput::chat`] is the chat box's: the same field, but
//! Enter submits and Shift+Enter types the newline.
//!
//! A field can drive a menu that hangs from it, as Tiptap's slash menu does from the editor:
//! [`TextInput::on_menu_key`] sees Up, Down, Enter, Tab, and Escape before the field does and
//! keeps the ones it answers. [`TextInput::selection`], [`TextInput::select`], and
//! [`TextInput::range_bounds`] give what holds the field its selection and where a range was
//! drawn, [`TextInput::set_highlight`] marks a range (the slash query's
//! `--color-surface-selected`), and [`TextInput::mono`] sets the field in the mono face, as a
//! code block's.

use std::ops::Range;
use std::rc::Rc;

use gpui::{
    App, AvailableSpace, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, KeyBinding, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine, SharedString, Size, Style,
    TextAlign, TextRun, TextStyle, UTF16Selection, UnderlineStyle, Window, WrappedLine, actions,
    div, fill, point, prelude::*, px, relative, size,
};
use unicode_segmentation::UnicodeSegmentation;

use super::material::{self, Edge, face};
use crate::Handler;
use crate::theme::{Theme, tw};

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
        Newline,
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
        Up,
        Down,
        SelectUp,
        SelectDown,
        MenuTab,
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
        KeyBinding::new("shift-enter", Newline, Some("TextInput")),
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
        // Only a field that wraps takes the vertical arrows; the rest leave them to what holds
        // the field, such as Commander's list.
        KeyBinding::new("up", Up, Some(WRAPPED_CONTEXT)),
        KeyBinding::new("down", Down, Some(WRAPPED_CONTEXT)),
        KeyBinding::new("shift-up", SelectUp, Some(WRAPPED_CONTEXT)),
        KeyBinding::new("shift-down", SelectDown, Some(WRAPPED_CONTEXT)),
        // Tab picks a menu's row only in a field that drives one; elsewhere it moves focus.
        KeyBinding::new("tab", MenuTab, Some("TextInput && menu")),
    ]);
}

/// Matches only a field that wraps, whose key context is `TextInput wrapped`.
const WRAPPED_CONTEXT: &str = "TextInput && wrapped";

type TextCallback = Rc<dyn Fn(&str, &mut Window, &mut App)>;
type MenuKeyCallback = Rc<dyn Fn(MenuKey, &mut Window, &mut App) -> bool>;

/// A key a menu that hangs from the field may answer before the field does: see
/// [`TextInput::on_menu_key`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKey {
    Up,
    Down,
    Enter,
    Tab,
    Escape,
}

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
    /// A corner radius in place of the round ends.
    radius: Option<f32>,
    bare: bool,
    title: bool,
    /// Soft-wraps the text at the field's width: see [`Self::wrap`].
    wrap: bool,
    /// Keeps hard line breaks: see [`Self::multiline`].
    multiline: bool,
    /// Enter submits a multi-line field and Shift+Enter breaks the line: see [`Self::chat`].
    chat: bool,
    /// The visual lines of the last paint of a field that wraps.
    wrapped: Option<WrapLayout>,
    /// A caret on the break between two visual lines is drawn at the start of the lower one,
    /// unless it was put at the end of the upper one (End, a click past its end, Down onto a
    /// shorter line). The offset it holds for; any other move forgets it.
    upstream_at: Option<usize>,
    /// The x a run of Up and Down keeps the caret near, so a short line does not pull it left.
    goal_x: Option<Pixels>,
    /// A bare field's text size and line height, and its ink, when not the defaults.
    quiet: Option<(f32, f32)>,
    /// Secondary ink in place of primary, for prose set in it such as a summary.
    secondary: bool,
    /// A bare field set in the text it edits: its size, line height, and weight, in primary
    /// ink. See [`Self::prose`].
    prose: Option<(f32, f32, gpui::FontWeight)>,
    on_submit: Option<TextCallback>,
    on_cancel: Option<CancelCallback>,
    on_menu_key: Option<MenuKeyCallback>,
    /// Set in the mono face: see [`Self::mono`].
    mono: bool,
    /// A range marked with the selected surface: see [`Self::set_highlight`].
    highlight: Option<Range<usize>>,
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
            radius: None,
            bare: false,
            title: false,
            wrap: false,
            multiline: false,
            chat: false,
            wrapped: None,
            upstream_at: None,
            goal_x: None,
            quiet: None,
            secondary: false,
            prose: None,
            on_submit: None,
            on_cancel: None,
            on_menu_key: None,
            mono: false,
            highlight: None,
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

    /// `SearchBar`. Every field is round-ended now; this only keeps older call sites reading
    /// as they did.
    pub fn pill(mut self) -> Self {
        self.pill = true;
        self
    }

    /// A corner radius in place of the round ends, for an app whose fields keep the web's
    /// (`.pw-field-control.control-field-sm`'s 6px).
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius);
        self
    }

    /// `.pw-filter-popover-search`: no fill, edge, or focus ring.
    pub fn bare(mut self) -> Self {
        self.bare = true;
        self
    }

    /// The Composer's title: 18px semibold. Pair it with [`Self::bare`] for Commander's search.
    pub fn title(mut self) -> Self {
        self.title = true;
        self
    }

    /// The Composer's description (`.pw-composer__description`): bare, 13.5px secondary.
    pub fn description(mut self) -> Self {
        self.bare = true;
        self.quiet = Some((13.5, 13.5 * 1.5));
        self
    }

    /// A Composer field's value (`.pw-composer__field-input`): bare, 12.5px secondary, for a
    /// labelled row such as [`crate::patterns::composer_field`].
    pub fn inline(mut self) -> Self {
        self.bare = true;
        self.quiet = Some((12.5, 12.5 * 1.5));
        self
    }

    /// A bare field in the type of the text it edits in place, such as a document's 24px
    /// title or a Markdown block's 15px prose: that size, line height, and weight, in primary
    /// ink. Pair it with [`Self::wrap`] for text that runs over lines.
    pub fn prose(mut self, size: f32, line_height: f32, weight: gpui::FontWeight) -> Self {
        self.bare = true;
        self.prose = Some((size, line_height, weight));
        self
    }

    /// The text in secondary ink (`--color-text-secondary`), as a document's summary is set.
    /// Pair it with [`Self::prose`].
    pub fn secondary(mut self) -> Self {
        self.secondary = true;
        self
    }

    /// Soft-wraps the text (and the placeholder) at the field's width, and the field grows to
    /// its lines, as a `<textarea>` that sizes to its text does. Pair it with
    /// [`Self::inline`] or [`Self::description`] for the Composer's fields. Enter still
    /// submits; Up and Down move between the visual lines, Home and End to a line's start and
    /// end. Off by default: a field is one line that scrolls past its width.
    pub fn wrap(mut self) -> Self {
        self.wrap = true;
        self
    }

    /// A `<textarea>`: [`Self::wrap`] with hard line breaks, as the Composer's description
    /// (`rows={3}`) takes them. Enter types a newline and a paste keeps its lines; the field
    /// grows to its lines and submits only through what holds it.
    pub fn multiline(mut self) -> Self {
        self.wrap = true;
        self.multiline = true;
        self
    }

    /// A chat box's field: [`Self::multiline`], but Enter submits through
    /// [`Self::on_submit`] and Shift+Enter types the newline, as a chat app's composer does.
    pub fn chat(mut self) -> Self {
        self.wrap = true;
        self.multiline = true;
        self.chat = true;
        self
    }

    /// `text` as this field holds it: one line, or its lines with every break a `\n`.
    fn clean(&self, text: &str) -> String {
        if self.multiline {
            text.replace("\r\n", "\n").replace('\r', "\n")
        } else {
            single_line(text)
        }
    }

    pub fn set_placeholder(&mut self, placeholder: impl Into<SharedString>) {
        self.placeholder = placeholder.into();
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

    /// Hands Up, Down, Enter, Tab, and Escape to a menu that hangs from the field before the
    /// field moves, submits, or cancels: a key the callback answers `true` to stops there,
    /// as Tiptap's `handleKeyDown` keeps the slash menu's keys. Tab is the field's only while
    /// the callback is set. The callback runs inside the field's update, so it must not read
    /// the field; work that does is deferred (`Window::defer`).
    pub fn on_menu_key(
        mut self,
        callback: impl Fn(MenuKey, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.on_menu_key = Some(Rc::new(callback));
        self
    }

    /// The field in the mono face (`--font-mono`), as a code block's text is set.
    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    /// The selected bytes; empty at the caret.
    pub fn selection(&self) -> Range<usize> {
        self.selected_range.clone()
    }

    /// Where the caret is: the selection's moving end.
    pub fn cursor(&self) -> usize {
        self.cursor_offset()
    }

    /// Selects `range` (clamped to the text), the caret at its end.
    pub fn select(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        let range = clamp_range(&self.content, range);
        let start = self.next_boundary_at(range.start);
        let end = self.next_boundary_at(range.end).max(start);
        self.selected_range = start..end;
        self.selection_reversed = false;
        self.forget_caret_hints();
        cx.notify();
    }

    /// Replaces the text with `text` and selects `selection` in it, as one edit that Undo
    /// takes back, as a toolbar's Bold does to the text it marks.
    pub fn edit(
        &mut self,
        text: impl Into<String>,
        selection: Range<usize>,
        cx: &mut Context<Self>,
    ) {
        let text = self.clean(&text.into());
        if text != self.content.as_ref() {
            self.last_edit = None;
            self.record(EditKind::Other, &(0..self.content.len()));
            self.content = SharedString::from(text);
            self.marked_range = None;
        }
        self.select(selection, cx);
    }

    /// Sets the field in another type, as a block edited in place is when it turns into a
    /// heading: see [`Self::prose`].
    pub fn set_prose(
        &mut self,
        size: f32,
        line_height: f32,
        weight: gpui::FontWeight,
        cx: &mut Context<Self>,
    ) {
        if self.prose != Some((size, line_height, weight)) {
            self.bare = true;
            self.prose = Some((size, line_height, weight));
            cx.notify();
        }
    }

    /// Where the text was drawn in the last frame, in window coordinates.
    pub fn bounds(&self) -> Option<Bounds<Pixels>> {
        self.last_bounds
    }

    /// `offset`, or the next character boundary when it falls inside a character.
    fn next_boundary_at(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.content.len());
        while !self.content.is_char_boundary(offset) {
            offset += 1;
        }
        offset
    }

    /// Marks `range` of the text with the selected surface behind it, as the slash menu's
    /// `::highlight(pw-md-slash)` marks the query; `None` clears it.
    pub fn set_highlight(&mut self, range: Option<Range<usize>>, cx: &mut Context<Self>) {
        let range = range.filter(|range| !range.is_empty());
        if self.highlight != range {
            self.highlight = range;
            cx.notify();
        }
    }

    /// Where `range` was drawn in the last frame, in window coordinates: the box around the
    /// visual lines it covers, or, for an empty range, the caret's line at no width. `None`
    /// before the field is drawn.
    pub fn range_bounds(&self, range: Range<usize>) -> Option<Bounds<Pixels>> {
        let bounds = self.last_bounds?;
        let range = clamp_range(&self.content, range);
        if !self.wrap {
            let line = self.last_layout.as_ref()?;
            let start = line.x_for_index(self.display_offset_for_content(range.start));
            let end = line.x_for_index(self.display_offset_for_content(range.end));
            return Some(Bounds::from_corners(
                point(bounds.left() + start, bounds.top()),
                point(bounds.left() + end, bounds.bottom()),
            ));
        }
        let wrapped = self.wrapped.as_ref()?;
        let line_height = wrapped.line_height;
        if self.content.is_empty() {
            return Some(Bounds::new(bounds.origin, size(px(0.0), line_height)));
        }
        let start = self.display_offset_for_content(range.start);
        let end = self.display_offset_for_content(range.end);
        if start == end {
            let upstream = self.upstream_at == Some(range.start);
            let caret = wrapped.point_for_index(start, upstream);
            return Some(Bounds::new(
                bounds.origin + caret,
                size(px(0.0), line_height),
            ));
        }
        let mut covered: Option<Bounds<Pixels>> = None;
        for (ix, line) in wrapped.lines.iter().enumerate() {
            let from = start.max(line.start);
            let to = end.min(line.end);
            if from > to || (from == to && line.start < line.end) {
                continue;
            }
            let top = bounds.top() + line_height * ix as f32;
            let rect = Bounds::from_corners(
                point(bounds.left() + wrapped.x_in_line(ix, from), top),
                point(bounds.left() + wrapped.x_in_line(ix, to), top + line_height),
            );
            covered = Some(covered.map_or(rect, |covered| covered.union(&rect)));
        }
        covered
    }

    /// Whether a menu that hangs from the field answered `key`.
    fn menu_key(&self, key: MenuKey, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let callback = self.on_menu_key.clone();
        callback.is_some_and(|callback| callback(key, window, cx))
    }

    fn menu_tab(&mut self, _: &MenuTab, window: &mut Window, cx: &mut Context<Self>) {
        if !self.menu_key(MenuKey::Tab, window, cx) {
            cx.propagate();
        }
    }

    pub fn text(&self) -> &str {
        self.content.as_ref()
    }

    /// Replaces the text, with the caret at its end. It starts a new history: undo does not
    /// go back past it.
    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        let cleaned = self.clean(&text.into());
        self.undo.clear();
        self.redo.clear();
        self.last_edit = None;
        self.content = SharedString::from(cleaned);
        self.selected_range = self.content.len()..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
        self.forget_caret_hints();
        cx.notify();
    }

    fn submit(&mut self, _: &Submit, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu_key(MenuKey::Enter, window, cx) {
            return;
        }
        if self.multiline && !self.chat {
            self.type_newline(window, cx);
            return;
        }
        if let Some(callback) = self.on_submit.clone() {
            let text = self.content.to_string();
            callback(&text, window, cx);
        }
    }

    /// Shift+Enter: a newline where the field keeps them, else Enter's submit.
    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        if self.multiline {
            self.type_newline(window, cx);
        } else {
            self.submit(&Submit, window, cx);
        }
    }

    fn type_newline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.last_edit = None;
        self.replace_text_in_range(None, "\n", window, cx);
        self.last_edit = Some((EditKind::Other, self.cursor_offset()));
    }

    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu_key(MenuKey::Escape, window, cx) {
            return;
        }
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
        self.select_to(self.line_start(self.cursor_offset()), cx);
    }

    fn select_end(&mut self, _: &SelectEnd, _: &mut Window, cx: &mut Context<Self>) {
        let (offset, upstream) = self.line_end(self.cursor_offset());
        self.select_to(offset, cx);
        self.upstream_at = upstream.then_some(offset);
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
            self.select_to(self.line_start(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete_to_end(&mut self, _: &DeleteToEnd, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.line_end(self.cursor_offset()).0, cx);
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
        self.forget_caret_hints();
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
        self.move_to(self.line_start(self.cursor_offset()), cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let (offset, upstream) = self.line_end(self.cursor_offset());
        self.move_to(offset, cx);
        self.upstream_at = upstream.then_some(offset);
    }

    fn up(&mut self, _: &Up, window: &mut Window, cx: &mut Context<Self>) {
        if !self.menu_key(MenuKey::Up, window, cx) {
            self.move_vertically(true, false, cx);
        }
    }

    fn down(&mut self, _: &Down, window: &mut Window, cx: &mut Context<Self>) {
        if !self.menu_key(MenuKey::Down, window, cx) {
            self.move_vertically(false, false, cx);
        }
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(true, true, cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(false, true, cx);
    }

    /// Moves the caret (or the selection's moving end) a visual line up or down, keeping its
    /// x as far as the line allows. On the first line Up goes to the start of the text and on
    /// the last Down goes to its end, as macOS fields do.
    fn move_vertically(&mut self, up: bool, select: bool, cx: &mut Context<Self>) {
        let cursor = self.cursor_offset();
        let extreme = if up { 0 } else { self.content.len() };
        let (target, upstream, goal_x) = self
            .visual_line(cursor)
            .and_then(|(wrapped, line)| {
                let next = adjacent_line(line, wrapped.lines.len(), up)?;
                let display = self.display_offset_for_content(cursor);
                let here = wrapped.point_for_index(display, self.upstream_at == Some(cursor));
                let goal = self.goal_x.unwrap_or(here.x);
                let (index, upstream) = wrapped.index_in_line(next, goal);
                Some((self.wrapped_content_offset(index), upstream, Some(goal)))
            })
            .unwrap_or((extreme, false, None));
        if select {
            self.select_to(target, cx);
        } else {
            self.move_to(target, cx);
        }
        self.upstream_at = upstream.then_some(target);
        self.goal_x = goal_x;
    }

    /// The wrapped layout and the visual line `offset` is on, once a field that wraps has been
    /// drawn with text in it.
    fn visual_line(&self, offset: usize) -> Option<(&WrapLayout, usize)> {
        if !self.wrap || self.content.is_empty() {
            return None;
        }
        let wrapped = self.wrapped.as_ref()?;
        let display = self.display_offset_for_content(offset);
        let line = line_for_index(&wrapped.lines, display, self.upstream_at == Some(offset));
        Some((wrapped, line))
    }

    /// Where Home lands: the start of the visual line `offset` is on, or the text's start in a
    /// field that does not wrap.
    fn line_start(&self, offset: usize) -> usize {
        match self.visual_line(offset) {
            Some((wrapped, line)) => self.wrapped_content_offset(wrapped.lines[line].start),
            None => 0,
        }
    }

    /// Where End lands: the end of the visual line `offset` is on, or the text's end in a field
    /// that does not wrap. The flag is whether the caret stays on the upper line of a break.
    fn line_end(&self, offset: usize) -> (usize, bool) {
        match self.visual_line(offset) {
            Some((wrapped, line)) => {
                let (end, upstream) = wrapped.line_end(line);
                (self.wrapped_content_offset(end), upstream)
            }
            None => (self.content.len(), false),
        }
    }

    /// The text offset of a display offset taken from a layout that may be a frame old: always
    /// on a character boundary.
    fn wrapped_content_offset(&self, display_offset: usize) -> usize {
        let mut offset = self.content_offset_for_display(display_offset);
        while !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
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
        self.pointer_to(event.position, event.modifiers.shift, cx);
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.pointer_to(event.position, true, cx);
        }
    }

    /// Puts the caret under the pointer, or drags the selection's moving end to it.
    fn pointer_to(&mut self, position: Point<Pixels>, extend: bool, cx: &mut Context<Self>) {
        let (offset, upstream) = self.caret_for_mouse_position(position);
        if extend {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
        }
        self.upstream_at = upstream.then_some(offset);
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
            let text = self.clean(&text);
            self.replace_text_in_range(None, &text, window, cx);
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
        self.forget_caret_hints();
        cx.notify();
    }

    /// Drops what the caret remembers of where it was put on a wrapped field: any move that does
    /// not set it afresh, and any edit, leaves it behind.
    fn forget_caret_hints(&mut self) {
        self.upstream_at = None;
        self.goal_x = None;
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

    /// The text offset under `position` and whether the caret there stays on the upper line of a
    /// break (a click past the end of a wrapped line).
    fn caret_for_mouse_position(&self, position: Point<Pixels>) -> (usize, bool) {
        if !self.wrap {
            return (self.index_for_mouse_position(position), false);
        }
        let (Some(bounds), Some(wrapped)) = (self.last_bounds.as_ref(), self.wrapped.as_ref())
        else {
            return (0, false);
        };
        if self.content.is_empty() || position.y < bounds.top() {
            return (0, false);
        }
        if position.y > bounds.bottom() {
            return (self.content.len(), false);
        }
        let (index, upstream) = wrapped.index_for_point(position - bounds.origin);
        (self.wrapped_content_offset(index), upstream)
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.forget_caret_hints();
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
        let new_text = self.clean(new_text);
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
        self.forget_caret_hints();
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
        let new_text = self.clean(new_text);
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
        self.forget_caret_hints();
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if self.wrap {
            let wrapped = self.wrapped.as_ref()?;
            let range = self.range_from_utf16(&range_utf16);
            let start =
                wrapped.point_for_index(self.display_offset_for_content(range.start), false);
            let end = wrapped.point_for_index(self.display_offset_for_content(range.end), false);
            // A range that runs onto a later line is given as far as the end of its first.
            let right = if end.y == start.y {
                end.x
            } else {
                bounds.size.width
            };
            return Some(Bounds::from_corners(
                point(bounds.left() + start.x, bounds.top() + start.y),
                point(
                    bounds.left() + right,
                    bounds.top() + start.y + wrapped.line_height,
                ),
            ));
        }
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
        if self.wrap {
            if self.content.is_empty() {
                return Some(0);
            }
            let (index, _) = self.wrapped.as_ref()?.index_for_point(line_point);
            return Some(self.offset_to_utf16(self.wrapped_content_offset(index)));
        }
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

/// The byte ranges of the visual lines a wrapped text breaks into, from the breaks gpui made:
/// the offsets the second and later lines start at. It always holds a line, which is empty for
/// an empty text.
fn lines_from_breaks(breaks: &[usize], len: usize) -> Vec<Range<usize>> {
    let mut lines = Vec::with_capacity(breaks.len() + 1);
    let mut start = 0;
    for &at in breaks {
        let at = at.clamp(start, len);
        lines.push(start..at);
        start = at;
    }
    lines.push(start..len);
    lines
}

/// The visual line a caret at `index` is drawn on. A break between two lines belongs to the
/// lower one, which starts there, unless `upstream` keeps the caret at the end of the upper.
/// The end of the text is on the last line.
fn line_for_index(lines: &[Range<usize>], index: usize, upstream: bool) -> usize {
    let last = lines.len().saturating_sub(1);
    let line = lines
        .iter()
        .position(|range| index < range.end)
        .unwrap_or(last);
    if upstream && line > 0 && index == lines[line].start {
        line - 1
    } else {
        line
    }
}

/// Where End lands on `line`, and whether the caret there stays on `line`: it must on a break,
/// where the offset is also the start of the next line.
fn line_end_caret(lines: &[Range<usize>], line: usize) -> (usize, bool) {
    (lines[line].end, line + 1 < lines.len())
}

/// The line Up (`up`) or Down moves to from `line`, if there is one.
fn adjacent_line(line: usize, count: usize, up: bool) -> Option<usize> {
    if up {
        line.checked_sub(1)
    } else {
        (line + 1 < count).then_some(line + 1)
    }
}

/// The line at `y` from the top of `count` lines of `line_height` each: the first above them
/// and the last below.
fn line_at_y(count: usize, line_height: f32, y: f32) -> usize {
    if line_height <= 0.0 || y <= 0.0 {
        return 0;
    }
    ((y / line_height) as usize).min(count.saturating_sub(1))
}

/// Where a caret lands on `line` when the nearest text offset to a point on it is `index`,
/// which may lie on another line, and whether it stays on `line` (the point was past its end,
/// where the offset is also the start of the next line).
fn clamp_to_line(lines: &[Range<usize>], line: usize, index: usize) -> (usize, bool) {
    let range = &lines[line];
    if index >= range.end && line + 1 < lines.len() {
        (range.end, true)
    } else {
        (index.clamp(range.start, range.end), false)
    }
}

/// A wrapped field as it was last drawn: its text shaped to its width, and the visual lines.
struct WrapLayout {
    /// Each paragraph (the text between two newlines, all of it in a field without any)
    /// shaped, and the offset it starts at; none when gpui shaped nothing, as it may for no
    /// text.
    paragraphs: Vec<(usize, WrappedLine)>,
    /// The range of the shown text each visual line holds: see [`lines_from_breaks`]. A
    /// paragraph's last line holds the newline after it, so the lines still meet end to start.
    lines: Vec<Range<usize>>,
    /// Whether each line ends at a newline, where the caret goes before it rather than on the
    /// next line's start.
    hard: Vec<bool>,
    /// The paragraph each line belongs to.
    owner: Vec<usize>,
    line_height: Pixels,
}

impl WrapLayout {
    /// `paragraphs` are `text`'s lines as gpui shaped them, one per newline-separated part.
    fn new(paragraphs: Vec<WrappedLine>, text: &str, line_height: Pixels) -> Self {
        let mut layout = Self {
            paragraphs: Vec::with_capacity(paragraphs.len()),
            lines: Vec::new(),
            hard: Vec::new(),
            owner: Vec::new(),
            line_height,
        };
        let count = paragraphs.len();
        let mut start = 0;
        for (index, (shaped, part)) in paragraphs.into_iter().zip(text.split('\n')).enumerate() {
            let breaks: Vec<usize> = shaped
                .wrap_boundaries
                .iter()
                .filter_map(|boundary| {
                    let run = shaped.unwrapped_layout.runs.get(boundary.run_ix)?;
                    Some(run.glyphs.get(boundary.glyph_ix)?.index)
                })
                .collect();
            let mut lines = lines_from_breaks(&breaks, part.len());
            let last = index + 1 < count;
            if last && let Some(line) = lines.last_mut() {
                // The newline closes the paragraph's last line.
                line.end += 1;
            }
            let lines_here = lines.len();
            layout.lines.extend(
                lines
                    .into_iter()
                    .map(|line| line.start + start..line.end + start),
            );
            layout
                .hard
                .extend((0..lines_here).map(|ix| last && ix + 1 == lines_here));
            layout.owner.extend(std::iter::repeat_n(index, lines_here));
            layout.paragraphs.push((start, shaped));
            start += part.len() + 1;
        }
        if layout.lines.is_empty() {
            layout.lines = lines_from_breaks(&[], text.len());
            layout.hard = vec![false];
            layout.owner = vec![0];
        }
        layout
    }

    /// The x of `index` in `paragraph` (an offset into the whole text), from its line's start.
    fn x_at(shaped: &WrappedLine, start: usize, index: usize) -> Pixels {
        let layout = &shaped.unwrapped_layout;
        layout.x_for_index(index.saturating_sub(start).min(layout.len))
    }

    /// The x of `index` from the left edge of `line`, which `index` is clamped onto.
    fn x_in_line(&self, line: usize, index: usize) -> Pixels {
        let Some((start, shaped)) = self.paragraphs.get(self.owner[line]) else {
            return px(0.0);
        };
        let range = &self.lines[line];
        Self::x_at(shaped, *start, index.clamp(range.start, range.end))
            - Self::x_at(shaped, *start, range.start)
    }

    /// Where the caret at `index` is drawn, from the top left of the text.
    fn point_for_index(&self, index: usize, upstream: bool) -> Point<Pixels> {
        let line = line_for_index(&self.lines, index, upstream);
        point(self.x_in_line(line, index), self.line_height * line as f32)
    }

    /// Where End lands on `line`, and whether the caret there stays on `line`: before the
    /// newline that ends a paragraph, else as [`line_end_caret`] puts it.
    fn line_end(&self, line: usize) -> (usize, bool) {
        if self.hard[line] {
            (self.lines[line].end - 1, false)
        } else {
            line_end_caret(&self.lines, line)
        }
    }

    /// The offset nearest `x` on `line`, and whether the caret there stays on `line`.
    fn index_in_line(&self, line: usize, x: Pixels) -> (usize, bool) {
        let Some((start, shaped)) = self.paragraphs.get(self.owner[line]) else {
            return (self.lines[line].start, false);
        };
        let range = &self.lines[line];
        let layout = &shaped.unwrapped_layout;
        let closest =
            start + layout.closest_index_for_x(Self::x_at(shaped, *start, range.start) + x);
        if self.hard[line] {
            (closest.clamp(range.start, range.end - 1), false)
        } else {
            clamp_to_line(&self.lines, line, closest)
        }
    }

    /// The offset nearest `point`, from the top left of the text, and whether the caret there
    /// stays on its line.
    fn index_for_point(&self, point: Point<Pixels>) -> (usize, bool) {
        let line = line_at_y(
            self.lines.len(),
            f32::from(self.line_height),
            f32::from(point.y),
        );
        self.index_in_line(line, point.x)
    }
}

/// What a field shapes: the text it shows, the runs that style it, and the sizes.
struct Shaping {
    text: SharedString,
    runs: Vec<TextRun>,
    font_size: Pixels,
    line_height: Pixels,
}

impl Shaping {
    /// The text wrapped at `width` (none: not wrapped): a paragraph for each part between
    /// newlines (one in a field that keeps none), or none for no text.
    fn wrapped(&self, width: Option<Pixels>, window: &Window) -> Vec<WrappedLine> {
        window
            .text_system()
            .shape_text(self.text.clone(), self.font_size, &self.runs, width, None)
            .map(|lines| lines.into_vec())
            .unwrap_or_default()
    }

    /// The height of the paragraphs' visual lines, and at least one.
    fn height(&self, paragraphs: &[WrappedLine]) -> Pixels {
        let height = paragraphs
            .iter()
            .map(|line| line.size(self.line_height).height)
            .fold(px(0.0), |total, height| total + height);
        height.max(self.line_height)
    }

    /// The widest of the paragraphs.
    fn width(&self, paragraphs: &[WrappedLine]) -> Pixels {
        paragraphs
            .iter()
            .map(|line| line.size(self.line_height).width)
            .fold(px(0.0), Pixels::max)
    }
}

/// The runs that style `len` bytes of text in `color`, the `marked` range of a composition
/// underlined and the `highlight` range on its fill.
fn text_runs(
    len: usize,
    style: &TextStyle,
    color: Hsla,
    marked: Option<Range<usize>>,
    highlight: Option<(Range<usize>, Hsla)>,
) -> Vec<TextRun> {
    let mut runs = vec![TextRun {
        len,
        font: style.font(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }];
    if let Some(marked) = marked {
        runs = restyle(runs, marked, |run| {
            run.underline = Some(UnderlineStyle {
                color: Some(run.color),
                thickness: px(1.0),
                wavy: false,
            });
        });
    }
    if let Some((range, fill)) = highlight {
        runs = restyle(runs, range, |run| run.background_color = Some(fill));
    }
    runs
}

/// `runs` split where `range` starts and ends, the part inside it changed by `apply`.
fn restyle(runs: Vec<TextRun>, range: Range<usize>, apply: impl Fn(&mut TextRun)) -> Vec<TextRun> {
    let mut styled = Vec::with_capacity(runs.len() + 2);
    let mut start = 0;
    for run in runs {
        let end = start + run.len;
        let from = range.start.clamp(start, end);
        let to = range.end.clamp(from, end);
        for (len, inside) in [(from - start, false), (to - from, true), (end - to, false)] {
            if len == 0 {
                continue;
            }
            let mut piece = TextRun { len, ..run.clone() };
            if inside {
                apply(&mut piece);
            }
            styled.push(piece);
        }
        start = end;
    }
    styled
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    wrapped: Option<WrapLayout>,
    cursor: Option<PaintQuad>,
    /// One quad, or one a visual line in a field that wraps.
    selection: Vec<PaintQuad>,
}

impl TextElement {
    /// The shown text (the placeholder while the field is empty) with its runs.
    fn shaping(&self, window: &Window, cx: &App) -> Shaping {
        let theme = Theme::global(cx);
        let input = self.input.read(cx);
        let text = input.display_text();
        let style = window.text_style();
        let color = if input.content.is_empty() {
            theme.color_text_disabled.into()
        } else {
            style.color
        };
        let marked = input.marked_range.as_ref().map(|range| {
            if input.password {
                input.display_offset_for_content(range.start)
                    ..input.display_offset_for_content(range.end)
            } else {
                range.clone()
            }
        });
        let highlight = input
            .highlight
            .clone()
            .filter(|_| !input.content.is_empty() && !input.password)
            .map(|range| (range, theme.color_surface_selected.into()));
        Shaping {
            runs: text_runs(text.len(), &style, color, marked, highlight),
            font_size: style.font_size.to_pixels(window.rem_size()),
            line_height: window.line_height(),
            text,
        }
    }

    /// A field that wraps: the text shaped at the width it was given, its caret or the quads
    /// of its selection, a line each.
    fn prepaint_wrapped(
        &self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> PrepaintState {
        let theme = *Theme::global(cx);
        let shaping = self.shaping(window, cx);
        // The slack absorbs layout rounding: a line as wide as its box stays one line.
        let paragraphs = shaping.wrapped(Some(bounds.size.width + px(0.5)), window);
        let wrapped = WrapLayout::new(paragraphs, &shaping.text, shaping.line_height);
        let input = self.input.read(cx);
        let cursor = input.cursor_offset();
        let empty = input.content.is_empty();
        let (selection_start, selection_end, display_cursor, upstream) = if empty {
            (0, 0, 0, false)
        } else {
            (
                input.display_offset_for_content(input.selected_range.start),
                input.display_offset_for_content(input.selected_range.end),
                input.display_offset_for_content(cursor),
                input.upstream_at == Some(cursor),
            )
        };
        let mut selection = Vec::new();
        let mut cursor_quad = None;
        if selection_start == selection_end {
            let caret = wrapped.point_for_index(display_cursor, upstream);
            cursor_quad = Some(fill(
                Bounds::new(
                    point(bounds.left() + caret.x, bounds.top() + caret.y),
                    size(px(1.5), wrapped.line_height),
                ),
                theme.color_text_primary,
            ));
        } else {
            for (ix, range) in wrapped.lines.iter().enumerate() {
                let from = selection_start.max(range.start);
                let to = selection_end.min(range.end);
                if from >= to {
                    continue;
                }
                let top = bounds.top() + wrapped.line_height * ix as f32;
                selection.push(fill(
                    Bounds::from_corners(
                        point(bounds.left() + wrapped.x_in_line(ix, from), top),
                        point(
                            bounds.left() + wrapped.x_in_line(ix, to),
                            top + wrapped.line_height,
                        ),
                    ),
                    theme.color_accent,
                ));
            }
        }
        PrepaintState {
            line: None,
            wrapped: Some(wrapped),
            cursor: cursor_quad,
            selection,
        }
    }
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
        if !self.input.read(cx).wrap {
            style.size.height = window.line_height().into();
            return (window.request_layout(style, [], cx), ());
        }
        // The measure only reports a height for the width it is given; the lines it paints are
        // shaped in prepaint at the final width (see `wrap_text`).
        style.min_size.width = px(0.0).into();
        let shaping = self.shaping(window, cx);
        let id = window.request_measured_layout(style, move |known, available, window, _| {
            let width = known.width.or(match available.width {
                AvailableSpace::Definite(room) => Some(room),
                _ => None,
            });
            let paragraphs = shaping.wrapped(width, window);
            Size {
                width: known.width.unwrap_or(shaping.width(&paragraphs)),
                height: known.height.unwrap_or(shaping.height(&paragraphs)),
            }
        });
        (id, ())
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
        if self.input.read(cx).wrap {
            return self.prepaint_wrapped(bounds, window, cx);
        }
        let theme = *Theme::global(cx);
        let (selected_range, cursor, password, content_empty) = {
            let input = self.input.read(cx);
            (
                input.selected_range.clone(),
                input.cursor_offset(),
                input.password,
                input.content.is_empty(),
            )
        };
        let shaping = self.shaping(window, cx);
        let line =
            window
                .text_system()
                .shape_line(shaping.text, shaping.font_size, &shaping.runs, None);

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
                Vec::new(),
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
                vec![fill(
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
                )],
                None,
            )
        };
        PrepaintState {
            line: Some(line),
            wrapped: None,
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
        for selection in prepaint.selection.drain(..) {
            window.paint_quad(selection);
        }
        let wrapped = prepaint.wrapped.take();
        let line = prepaint.line.take();
        if let Some(line) = &line {
            let _ = line.paint_background(bounds.origin, window.line_height(), window, cx);
            line.paint(bounds.origin, window.line_height(), window, cx)
                .unwrap();
        }
        if let Some(wrapped) = &wrapped {
            // Each paragraph under the visual lines of those before it.
            let mut top = bounds.origin.y;
            for (_, paragraph) in &wrapped.paragraphs {
                let _ = paragraph.paint_background(
                    point(bounds.origin.x, top),
                    wrapped.line_height,
                    TextAlign::Left,
                    Some(bounds),
                    window,
                    cx,
                );
                let _ = paragraph.paint(
                    point(bounds.origin.x, top),
                    wrapped.line_height,
                    TextAlign::Left,
                    Some(bounds),
                    window,
                    cx,
                );
                top += paragraph.size(wrapped.line_height).height;
            }
        }

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = line;
            input.wrapped = wrapped;
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::global(cx);
        // Round-ended unless a radius is set, padded so the text keeps clear of the curve.
        let (height, text, line, pad) = match self.size {
            FieldSize::Sm => (theme.field_height_sm, 12.0, 15.0, tw(3.0)),
            FieldSize::Md => (theme.field_height_md, 13.0, 16.9, tw(3.5)),
        };
        // A field with corners keeps the web's padding (`.control-field-sm`, `-md`).
        let pad = match (self.radius, self.size) {
            (None, _) => pad,
            (Some(_), FieldSize::Sm) => tw(2.0),
            (Some(_), FieldSize::Md) => tw(2.5),
        };
        let radius = self.radius.unwrap_or(height / 2.0).min(height / 2.0);
        let focused = self.focus_handle.is_focused(window);
        let hover_fill = face(theme.field_fill_hover_top, theme.field_fill_hover_bottom);
        let bare = self.bare;
        let wrap = self.wrap;
        let (height, text, line) = if self.title {
            (24.3, 18.0, 24.3)
        } else if let Some((text, line, _)) = self.prose {
            (line, text, line)
        } else if let Some((text, line)) = self.quiet {
            (line, text, line)
        } else if bare {
            (19.5, 13.0, 19.5)
        } else {
            (height, text, line)
        };
        div()
            .id(("text-input", cx.entity_id()))
            .relative()
            .flex()
            .w_full()
            .key_context(match (wrap, self.on_menu_key.is_some()) {
                (false, false) => "TextInput",
                (true, false) => "TextInput wrapped",
                (false, true) => "TextInput menu",
                (true, true) => "TextInput wrapped menu",
            })
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
            .on_action(cx.listener(Self::newline))
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
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::menu_tab))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .map(|field| {
                // A field that wraps is as tall as its lines, and never shorter than one.
                if wrap {
                    field.min_h(px(height))
                } else {
                    field.h(px(height))
                }
            })
            .items_center()
            .when(wrap && !bare, |field| field.py(px((height - line) / 2.0)))
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
            .when(self.title, |field| {
                field.font_weight(gpui::FontWeight::SEMIBOLD)
            })
            .when_some(self.prose, |field, (_, _, weight)| {
                field.font_weight(weight)
            })
            .when(self.mono, |field| field.font_family(theme.font_mono))
            .text_color(if self.quiet.is_some() || self.secondary {
                theme.color_text_secondary
            } else {
                theme.color_text_primary
            })
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
// A visual line is one range, so a text of one line is a list of one.
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{
        Bounds, ClipboardItem, Entity, EntityInputHandler, Focusable, Modifiers, MouseButton,
        Pixels, TestAppContext, VisualTestContext, point, prelude::*, px, size,
    };

    use super::{
        TextInput, adjacent_line, clamp_to_line, line_at_y, line_end_caret, line_for_index,
        lines_from_breaks,
    };
    use crate::test_support::{draw, open};

    /// Three visual lines of a 12-byte text: "aaaaa", "bbbbb", "cc".
    fn three_lines() -> Vec<std::ops::Range<usize>> {
        lines_from_breaks(&[5, 10], 12)
    }

    #[test]
    fn breaks_become_contiguous_line_ranges() {
        assert_eq!(three_lines(), [0..5, 5..10, 10..12]);
        assert_eq!(lines_from_breaks(&[], 7), [0..7]);
        assert_eq!(lines_from_breaks(&[], 0), [0..0]);
        // A break past the text, or before the last, cannot make a range run backwards.
        assert_eq!(lines_from_breaks(&[4, 2, 99], 6), [0..4, 4..4, 4..6, 6..6]);
    }

    #[test]
    fn an_index_maps_to_its_line_and_back() {
        let lines = three_lines();
        for (ix, range) in lines.iter().enumerate() {
            // Every offset of a line maps to it, and its start maps back to its start.
            for index in range.clone() {
                assert_eq!(line_for_index(&lines, index, false), ix);
            }
            let line = line_for_index(&lines, range.start, false);
            assert_eq!(lines[line].start, range.start);
        }
        assert_eq!(line_for_index(&lines, 0, false), 0);
        // The end of the text is on the last line.
        assert_eq!(line_for_index(&lines, 12, false), 2);
        // Past the text clamps onto the last line too, as a stale offset may.
        assert_eq!(line_for_index(&lines, 40, false), 2);
    }

    #[test]
    fn a_break_belongs_to_the_lower_line_unless_the_caret_is_upstream() {
        let lines = three_lines();
        assert_eq!(line_for_index(&lines, 5, false), 1);
        assert_eq!(line_for_index(&lines, 5, true), 0);
        assert_eq!(line_for_index(&lines, 10, true), 1);
        // Upstream changes nothing away from a break, at the very start, or at the text's end.
        assert_eq!(line_for_index(&lines, 7, true), 1);
        assert_eq!(line_for_index(&lines, 0, true), 0);
        assert_eq!(line_for_index(&lines, 12, true), 2);
    }

    #[test]
    fn end_stays_on_the_upper_line_of_a_break_and_ends_the_last_line() {
        let lines = three_lines();
        assert_eq!(line_end_caret(&lines, 0), (5, true));
        assert_eq!(line_end_caret(&lines, 1), (10, true));
        assert_eq!(line_end_caret(&lines, 2), (12, false));
        // Pressing End at that spot again stays put: it is the same line.
        let (end, upstream) = line_end_caret(&lines, 0);
        let line = line_for_index(&lines, end, upstream);
        assert_eq!(line_end_caret(&lines, line), (5, true));
        // A one-line text ends at its end.
        assert_eq!(line_end_caret(&[0..9], 0), (9, false));
    }

    #[test]
    fn up_and_down_step_to_the_neighbouring_line_or_run_out() {
        assert_eq!(adjacent_line(1, 3, true), Some(0));
        assert_eq!(adjacent_line(1, 3, false), Some(2));
        assert_eq!(adjacent_line(0, 3, true), None);
        assert_eq!(adjacent_line(2, 3, false), None);
        assert_eq!(adjacent_line(0, 1, false), None);
    }

    #[test]
    fn a_height_picks_a_line_and_clamps_to_the_first_and_last() {
        assert_eq!(line_at_y(3, 20.0, 0.0), 0);
        assert_eq!(line_at_y(3, 20.0, 19.9), 0);
        assert_eq!(line_at_y(3, 20.0, 20.0), 1);
        assert_eq!(line_at_y(3, 20.0, 59.0), 2);
        assert_eq!(line_at_y(3, 20.0, 500.0), 2);
        assert_eq!(line_at_y(3, 20.0, -4.0), 0);
        assert_eq!(line_at_y(1, 20.0, 30.0), 0);
        assert_eq!(line_at_y(3, 0.0, 30.0), 0);
    }

    #[test]
    fn a_point_past_a_lines_end_keeps_the_caret_on_it() {
        let lines = three_lines();
        // Inside the line.
        assert_eq!(clamp_to_line(&lines, 1, 7), (7, false));
        // Past the end of a line that is followed by another: at its end, upstream.
        assert_eq!(clamp_to_line(&lines, 0, 5), (5, true));
        assert_eq!(clamp_to_line(&lines, 0, 9), (5, true));
        // Before the start of the line.
        assert_eq!(clamp_to_line(&lines, 1, 2), (5, false));
        // Past the last line's end is the end of the text.
        assert_eq!(clamp_to_line(&lines, 2, 40), (12, false));
    }

    const LONG: &str = "Review a diff for bugs, missing tests, and risky changes before it merges, \
                        then say what to fix first and why it matters to whoever reads the report.";

    /// The ranges of the visual lines a wrapped field drew last.
    fn lines_of(
        input: &Entity<TextInput>,
        cx: &mut VisualTestContext,
    ) -> Vec<std::ops::Range<usize>> {
        cx.update(|_, cx| input.read(cx).wrapped.as_ref().unwrap().lines.clone())
    }

    /// The visual line the caret is on.
    fn caret_line(input: &Entity<TextInput>, cx: &mut VisualTestContext) -> usize {
        cx.update(|_, cx| {
            let input = input.read(cx);
            input.visual_line(input.cursor_offset()).unwrap().1
        })
    }

    fn caret(input: &Entity<TextInput>, cx: &mut VisualTestContext) -> usize {
        cx.update(|_, cx| input.read(cx).cursor_offset())
    }

    /// A wrapped field holding `LONG`, drawn, with the caret at its end.
    fn wrapped_field(cx: &mut TestAppContext) -> (Entity<TextInput>, VisualTestContext) {
        let (input, mut cx) = field(cx, |field| field.inline().wrap());
        cx.update(|_, cx| input.update(cx, |input, cx| input.set_text(LONG, cx)));
        draw(&mut cx);
        (input, cx)
    }

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

    /// The field's bounds and line height as it last drew.
    fn geometry(input: &Entity<TextInput>, cx: &mut VisualTestContext) -> (Bounds<Pixels>, Pixels) {
        cx.update(|_, cx| {
            let input = input.read(cx);
            (
                input.last_bounds.unwrap(),
                input.wrapped.as_ref().unwrap().line_height,
            )
        })
    }

    #[gpui::test]
    fn a_long_value_wraps_into_lines_and_the_field_grows_to_them(cx: &mut TestAppContext) {
        let (input, mut cx) = wrapped_field(cx);
        let lines = lines_of(&input, &mut cx);
        assert!(lines.len() > 1, "a long value wraps: {lines:?}");
        assert_eq!(lines.first().unwrap().start, 0);
        assert_eq!(lines.last().unwrap().end, LONG.len());
        assert!(lines.windows(2).all(|pair| pair[0].end == pair[1].start));
        let (bounds, line_height) = geometry(&input, &mut cx);
        let wanted = f32::from(line_height) * lines.len() as f32;
        assert!(
            (f32::from(bounds.size.height) - wanted).abs() < 1.0,
            "the field is {} tall for {} lines of {}",
            f32::from(bounds.size.height),
            lines.len(),
            f32::from(line_height),
        );
    }

    #[gpui::test]
    fn a_short_value_and_the_placeholder_follow_the_width(cx: &mut TestAppContext) {
        let (input, mut cx) = field(cx, |field| field.inline().wrap().placeholder(LONG));
        assert_eq!(text(&input, &mut cx), "");
        let placeholder = lines_of(&input, &mut cx);
        assert!(
            placeholder.len() > 1,
            "the placeholder wraps: {placeholder:?}"
        );
        assert_eq!(placeholder.last().unwrap().end, LONG.len());
        cx.simulate_input("short");
        draw(&mut cx);
        assert_eq!(lines_of(&input, &mut cx), [0..5]);
        assert_eq!(caret(&input, &mut cx), 5);
    }

    #[gpui::test]
    fn home_and_end_go_to_the_ends_of_the_visual_line(cx: &mut TestAppContext) {
        let (input, mut cx) = wrapped_field(cx);
        let lines = lines_of(&input, &mut cx);
        let last = lines.len() - 1;
        cx.simulate_keystrokes("home");
        assert_eq!(caret(&input, &mut cx), lines[last].start);
        assert_eq!(caret_line(&input, &mut cx), last);
        cx.simulate_keystrokes("end");
        assert_eq!(caret(&input, &mut cx), LONG.len());
        // On an upper line End stays on it, at the break, rather than dropping to the next.
        cx.simulate_keystrokes("up home end");
        assert_eq!(caret(&input, &mut cx), lines[last - 1].end);
        assert_eq!(caret_line(&input, &mut cx), last - 1);
        cx.simulate_keystrokes("end");
        assert_eq!(caret_line(&input, &mut cx), last - 1);
        // Typing there goes in at the break and the caret moves on.
        cx.simulate_input("|");
        assert_eq!(caret(&input, &mut cx), lines[last - 1].end + 1);
    }

    #[gpui::test]
    fn shift_home_selects_to_the_line_start_and_command_delete_removes_it(cx: &mut TestAppContext) {
        let (input, mut cx) = wrapped_field(cx);
        let lines = lines_of(&input, &mut cx);
        let last = lines.len() - 1;
        cx.simulate_keystrokes("shift-home");
        assert_eq!(
            cx.update(|_, cx| input.read(cx).selected_range.clone()),
            lines[last].start..LONG.len()
        );
        cx.simulate_keystrokes("cmd-backspace");
        assert_eq!(text(&input, &mut cx), LONG[..lines[last].start]);
    }

    #[gpui::test]
    fn up_and_down_move_between_visual_lines(cx: &mut TestAppContext) {
        let (input, mut cx) = wrapped_field(cx);
        let lines = lines_of(&input, &mut cx);
        let last = lines.len() - 1;
        cx.simulate_keystrokes("home up");
        assert_eq!(caret_line(&input, &mut cx), last - 1);
        assert_eq!(caret(&input, &mut cx), lines[last - 1].start);
        cx.simulate_keystrokes("down");
        assert_eq!(caret(&input, &mut cx), lines[last].start);
        // Up from the first line goes to the start, Down from the last to the end.
        for _ in 0..lines.len() {
            cx.simulate_keystrokes("up");
        }
        assert_eq!(caret(&input, &mut cx), 0);
        for _ in 0..lines.len() {
            cx.simulate_keystrokes("down");
        }
        assert_eq!(caret(&input, &mut cx), LONG.len());
        // Shift extends the selection from where the caret was.
        cx.simulate_keystrokes("shift-up");
        let range = cx.update(|_, cx| input.read(cx).selected_range.clone());
        assert_eq!(range.end, LONG.len());
        assert!(
            lines[last - 1].contains(&range.start),
            "{range:?} in {lines:?}"
        );
    }

    #[gpui::test]
    fn down_then_up_returns_the_caret_to_its_column(cx: &mut TestAppContext) {
        let (input, mut cx) = wrapped_field(cx);
        // Put the caret a few characters into the first line, go down and up again.
        cx.update(|_, cx| input.update(cx, |input, cx| input.move_to(4, cx)));
        cx.simulate_keystrokes("down");
        assert_eq!(caret_line(&input, &mut cx), 1);
        cx.simulate_keystrokes("up");
        assert_eq!(caret(&input, &mut cx), 4);
    }

    #[gpui::test]
    fn a_click_and_a_drag_work_over_the_wrapped_lines(cx: &mut TestAppContext) {
        let (input, mut cx) = wrapped_field(cx);
        let lines = lines_of(&input, &mut cx);
        let (bounds, line_height) = geometry(&input, &mut cx);
        let on_line = |line: usize| bounds.top() + line_height * (line as f32 + 0.5);
        let left = bounds.left() + px(0.5);
        cx.simulate_click(point(left, on_line(1)), Modifiers::none());
        assert_eq!(caret(&input, &mut cx), lines[1].start);
        assert_eq!(caret_line(&input, &mut cx), 1);
        // Past the end of the first line is its end, drawn on it rather than on the second.
        let right = bounds.right() - px(0.5);
        cx.simulate_click(point(right, on_line(0)), Modifiers::none());
        assert_eq!(caret(&input, &mut cx), lines[0].end);
        assert_eq!(caret_line(&input, &mut cx), 0);
        // A drag from the first line to the last selects across the lines between.
        let last = lines.len() - 1;
        let none = Modifiers::none();
        cx.simulate_mouse_down(point(left, on_line(0)), MouseButton::Left, none);
        cx.simulate_mouse_move(point(left, on_line(last)), MouseButton::Left, none);
        cx.simulate_mouse_up(point(left, on_line(last)), MouseButton::Left, none);
        assert_eq!(
            cx.update(|_, cx| input.read(cx).selected_range.clone()),
            0..lines[last].start
        );
    }

    #[gpui::test]
    fn the_input_method_finds_a_range_on_its_visual_line(cx: &mut TestAppContext) {
        let (input, mut cx) = wrapped_field(cx);
        let lines = lines_of(&input, &mut cx);
        let (bounds, line_height) = geometry(&input, &mut cx);
        let at = |range: std::ops::Range<usize>, cx: &mut VisualTestContext| {
            cx.update(|window, cx| {
                input
                    .update(cx, |input, cx| {
                        input.bounds_for_range(range, bounds, window, cx)
                    })
                    .unwrap()
            })
        };
        let first = at(0..1, &mut cx);
        assert_eq!(first.top(), bounds.top());
        let second = at(lines[1].start..lines[1].start + 1, &mut cx);
        assert_eq!(second.top(), bounds.top() + line_height);
        // A point on the second line is a character index in it.
        let found = cx.update(|window, cx| {
            input.update(cx, |input, cx| {
                input.character_index_for_point(
                    point(bounds.left() + px(0.5), bounds.top() + line_height * 1.5),
                    window,
                    cx,
                )
            })
        });
        assert_eq!(found, Some(lines[1].start));
    }

    #[gpui::test]
    fn enter_submits_a_wrapped_field_and_no_newline_goes_in(cx: &mut TestAppContext) {
        let submitted: Rc<RefCell<Vec<String>>> = Rc::default();
        let sink = submitted.clone();
        let (input, mut cx) = field(cx, move |field| {
            field
                .inline()
                .wrap()
                .on_submit(move |text, _, _| sink.borrow_mut().push(text.to_string()))
        });
        cx.simulate_input("one\ntwo three");
        cx.simulate_keystrokes("enter");
        assert_eq!(submitted.borrow().as_slice(), ["onetwo three"]);
        assert_eq!(text(&input, &mut cx), "onetwo three");
    }

    #[gpui::test]
    fn a_multiline_field_types_a_newline_on_enter_and_keeps_its_lines(cx: &mut TestAppContext) {
        let submitted: Rc<RefCell<Vec<String>>> = Rc::default();
        let sink = submitted.clone();
        let (input, mut cx) = field(cx, move |field| {
            field
                .description()
                .multiline()
                .on_submit(move |text, _, _| sink.borrow_mut().push(text.to_string()))
        });
        cx.simulate_input("one");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("two");
        draw(&mut cx);
        assert_eq!(text(&input, &mut cx), "one\ntwo");
        assert!(submitted.borrow().is_empty(), "Enter does not submit");
        // The newline closes the first line, so the lines still meet.
        assert_eq!(lines_of(&input, &mut cx), [0..4, 4..7]);
        let (bounds, line_height) = geometry(&input, &mut cx);
        assert!((f32::from(bounds.size.height) - 2.0 * f32::from(line_height)).abs() < 1.0);
        // End stops before the newline, and Down goes on into the next paragraph.
        cx.simulate_keystrokes("up end");
        assert_eq!(caret(&input, &mut cx), 3);
        assert_eq!(caret_line(&input, &mut cx), 0);
        cx.simulate_keystrokes("down");
        assert_eq!(caret_line(&input, &mut cx), 1);
        cx.simulate_keystrokes("home");
        assert_eq!(caret(&input, &mut cx), 4);
        // Backspace at a line's start joins it to the one above.
        cx.simulate_keystrokes("backspace");
        assert_eq!(text(&input, &mut cx), "onetwo");
    }

    #[gpui::test]
    fn a_chat_field_submits_on_enter_and_breaks_the_line_on_shift_enter(cx: &mut TestAppContext) {
        let submitted: Rc<RefCell<Vec<String>>> = Rc::default();
        let sink = submitted.clone();
        let (input, mut cx) = field(cx, move |field| {
            field
                .description()
                .chat()
                .on_submit(move |text, _, _| sink.borrow_mut().push(text.to_string()))
        });
        cx.simulate_input("one");
        cx.simulate_keystrokes("shift-enter");
        cx.simulate_input("two");
        draw(&mut cx);
        assert_eq!(text(&input, &mut cx), "one\ntwo");
        assert!(submitted.borrow().is_empty(), "Shift+Enter does not submit");
        assert_eq!(lines_of(&input, &mut cx), [0..4, 4..7]);
        cx.simulate_keystrokes("enter");
        assert_eq!(submitted.borrow().as_slice(), ["one\ntwo"]);
    }

    #[gpui::test]
    fn a_multiline_field_keeps_pasted_breaks_and_an_empty_line(cx: &mut TestAppContext) {
        let (input, mut cx) = field(cx, |field| field.description().multiline());
        cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("a\r\n\nb".into())));
        cx.simulate_keystrokes("cmd-v");
        draw(&mut cx);
        assert_eq!(text(&input, &mut cx), "a\n\nb");
        assert_eq!(lines_of(&input, &mut cx), [0..2, 2..3, 3..4]);
        // The empty line holds the caret on its own.
        cx.simulate_keystrokes("up");
        assert_eq!(caret(&input, &mut cx), 2);
        assert_eq!(caret_line(&input, &mut cx), 1);
        cx.update(|_, cx| input.update(cx, |input, cx| input.set_text("x\ny", cx)));
        assert_eq!(text(&input, &mut cx), "x\ny");
    }

    #[gpui::test]
    fn a_field_that_does_not_wrap_takes_no_vertical_arrows(cx: &mut TestAppContext) {
        let (input, mut cx) = field(cx, |field| field.inline());
        cx.simulate_input("hello");
        cx.simulate_keystrokes("up");
        assert_eq!(caret(&input, &mut cx), 5);
        cx.simulate_keystrokes("home down");
        assert_eq!(caret(&input, &mut cx), 0);
        assert!(cx.update(|_, cx| input.read(cx).wrapped.is_none()));
        // Home and End still go to the ends of the text.
        cx.simulate_keystrokes("end");
        assert_eq!(caret(&input, &mut cx), 5);
    }
}
