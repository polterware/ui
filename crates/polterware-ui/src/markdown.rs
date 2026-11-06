//! Markdown in the document style: the body of a Knowledge doc or decision page, as the design
//! system's `MarkdownEditor class="pw-md-editor--document"` draws it (`markdown-editor.css`).
//! Prose is 15px at weight 450 on a 24px line in primary ink. Paragraphs, lists, quotes, and
//! code blocks stand 16px apart; `h1`–`h3` are 20/28, 17/24, and 15/24 semibold with 24px
//! above and 8px below (the larger margin wins between two blocks, as CSS margins collapse).
//! List items are 3px apart (`0.2em`) on a 22.5px indent (`1.5em`) per level, with a disc,
//! circle, or square, or the item's number, before the text; a task item has an 18px
//! checkbox 8px before its text and no indent, its text muted and struck through once done.
//! A quote is secondary ink 12px past a 2px strong rule; a code block is 12px mono on the
//! subtle surface, padded 12px, on the card radius; a rule is a 1px subtle line with 18.75px
//! around it. Inline, `strong` is semibold, `em` italic, `code` mono on the subtle surface,
//! and a link takes the info color with no underline.
//!
//! [`markdown_view`] draws a source read-only. [`MarkdownEditor`] edits it the way Tiptap's
//! editor does in place, one block at a time: a click turns a block into a field holding its
//! Markdown (gpui-kit's `Input`, here [`TextInput::prose`] set in the block's own type), and
//! Enter, Escape, a click elsewhere, or leaving the field writes it back into the source, so
//! the rest of the text keeps its exact spelling. Enter also opens the next block: another
//! item after an item (an empty one leaves the list for a paragraph), else a paragraph.
//! Clicking a task's box checks it off or opens it again. The editor sits on the design
//! system's gutter (`padding-inline: 32px 8px` pulled 32px out), so its text lines up with
//! what is above it and ends 8px short; a hovered block fills with the hover surface from 6px
//! before its text, and an empty editor reads its placeholder and is at least 96px tall.
//! [`MarkdownEditorEvent::Changed`] carries the source after every keystroke and write.
//!
//! A code block opens in place too: its code in a mono field of many lines in the code frame,
//! where Enter types a line and two empty lines at its end let Enter leave it for a
//! paragraph. Under the pointer a code block shows "Copy" at its top end; a press there copies
//! the code and the label reads "Copied" for 1.5s.
//!
//! The editing tools follow the design system's editor (`markdown-editor.tsx`). A `/` that
//! starts a block opens the slash menu under the caret ([`crate::markdown_tools`] holds its
//! rows): what follows the `/` filters the rows and is marked on the selected surface, ↑/↓ move
//! the lit row and wrap, Enter or Tab picks it, and Escape, "Close menu", or a press outside
//! closes it for that `/`. Selected text raises the selection toolbar over it, whose tools set
//! the selection bold (`**`) or italic (`_`) or turn the block into a heading, a list, a code
//! block, or a quote, and turn it back. The block under the pointer shows a "+" 40px before
//! its text: an item gets an empty item after it, any other block an empty paragraph with the
//! menu open on it, "Type to filter…" waiting where its text goes. A field takes the type of
//! the block its Markdown makes as it is typed, as `## ` makes a heading.
//!
//! Links do not open, as the design system's do not (`openOnClick: false`). GPUI draws no
//! italic, since it has no oblique of the bundled sans: an `em` is set upright.

use std::ops::Range;
use std::time::Duration;

use gpui::{
    AnyElement, App, Bounds, ClipboardItem, Context, CursorStyle, Div, ElementId, Entity,
    EventEmitter, Focusable, FontStyle, FontWeight, IntoElement, MouseButton, MouseDownEvent,
    Pixels, Point, Render, RenderOnce, ScrollHandle, SharedString, StrikethroughStyle,
    Subscription, Task, Window, anchored, canvas, deferred, div, point, prelude::*, px, svg,
};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::checkbox::checkbox_mark;
use crate::icons::{Icon, icon_colored};
use crate::markdown_tools::{
    Mark, SECTIONS, Shape, TOOLBAR_GROUPS, Tool, body, code_fences, filters, has_mark, is_item,
    shape_of, slash_query, slash_rows, toggle_mark, toggle_shape,
};
use crate::material::{self, floating_panel};
use crate::text_input::{MenuKey, TextInput};
use crate::theme::{Theme, tw};
use crate::tooltip::tooltip;
use crate::wrap::{SpanStyle, wrap_text};

/// `.pw-md-editor--document .ProseMirror`: 15px, weight 450, a 24px line.
const TEXT_SIZE: f32 = 15.0;
const LINE: f32 = 24.0;
const WEIGHT: FontWeight = FontWeight(450.0);
/// `:is(p, ul, ol, blockquote, pre) { margin: 0 0 16px }`.
const BLOCK_GAP: f32 = 16.0;
/// `:is(h1, h2, h3) { margin: 24px 0 8px }`.
const HEADING_TOP: f32 = 24.0;
const HEADING_BOTTOM: f32 = 8.0;
/// `li { margin: 0.2em 0 }` at 15px. Only task items sit this close: any other item's text is a
/// `p`, whose 16px margin wins as the margins collapse.
const ITEM_GAP: f32 = 3.0;
/// `ul, ol { padding-left: 1.5em }`.
const LIST_INDENT: f32 = 22.5;
/// The marker's room before an item's text, as an outside marker sits.
const MARKER_GAP: f32 = 6.0;
/// `hr { margin: 1.25em 0 }`.
const RULE_MARGIN: f32 = 18.75;
/// `blockquote { padding-left: 12px; border-left: 2px }`.
const QUOTE_PAD: f32 = 12.0;
/// `pre { padding: 12px }`, `pre code { font-size: 12px }`.
const CODE_PAD: f32 = 12.0;
const CODE_SIZE: f32 = 12.0;
/// A task's box and the gap before its text.
const TASK_BOX: f32 = 18.0;
const TASK_GAP: f32 = 8.0;
/// `.pw-md-editor[data-editable="true"] .pw-md-editor__surface { padding-inline: 32px 8px }`
/// pulled 32px out: only the end's 8px shows.
const GUTTER_END: f32 = 8.0;
/// `min-height: 96px` on an editable document.
const EDITABLE_MIN: f32 = 96.0;
/// The hover fill starts this far before a block's text and ends as far after it.
const HOVER_BLEED: f32 = 6.0;
/// And reaches this far over and under it.
const HOVER_RISE: f32 = 2.0;
/// How far a block's row reaches into the gutter: the surface's 32px `padding-inline-start`
/// and the "+"'s `left: -8px`, so the "+" starts this far before the text.
const GUTTER: f32 = 40.0;
/// `.pw-md-editor__tool`: 28px round, the "+" and the toolbar's.
const TOOL: f32 = 28.0;
/// The toolbar's 4px between its groups (`gap: 2px 4px`), and its height.
const TOOLBAR_GAP: f32 = 4.0;
const TOOLBAR_HEIGHT: f32 = TOOL + 6.0;
/// `.pw-md-editor__slash`: 240px wide and at most 300px tall.
const SLASH_WIDTH: f32 = 240.0;
const SLASH_MAX: f32 = 300.0;
/// A heading's line in the slash menu: 11px at 1.5.
const SLASH_LABEL_LINE: f32 = 16.5;
/// The menu and the toolbar hang 8px from what they open on (`gutter: 8`) and keep 8px from
/// the window's edge.
const POPUP_GAP: f32 = 8.0;
const POPUP_EDGE: f32 = 8.0;
/// The corner of a code block a press copies it in: 64 by 32.
const COPY_CORNER: (f32, f32) = (64.0, 32.0);
/// How long a copied code block's label reads "Copied".
const COPIED_FOR: Duration = Duration::from_millis(1500);

/// A run of inline text with its marks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub strong: bool,
    pub emphasis: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Marker {
    Bullet,
    Number(u64),
    /// A task, and whether it is done.
    Task(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub enum BlockKind {
    /// Level 1 to 6 (4–6 draw as 3).
    Heading(u8, Vec<Span>),
    Paragraph(Vec<Span>),
    /// One list item's own text; `list` counts the top-level lists, so two lists in a row
    /// stand apart while one list's items, nested or not, stay close.
    Item {
        marker: Marker,
        depth: usize,
        list: usize,
        text: Vec<Span>,
    },
    /// A quote's paragraphs.
    Quote(Vec<Vec<Span>>),
    Code {
        language: String,
        text: String,
    },
    Rule,
}

/// A block and where its Markdown is in the source.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub kind: BlockKind,
    pub source: Range<usize>,
}

/// The blocks of `source`, in order, with their source ranges (an item's stops before the
/// list nested in it; trailing blank lines are left out).
pub fn markdown_blocks(source: &str) -> Vec<Block> {
    let mut builder = Builder::new(source);
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        builder.event(event, range);
    }
    builder.blocks
}

#[derive(Default)]
struct Marks {
    strong: usize,
    emphasis: usize,
    strike: usize,
    links: Vec<String>,
}

struct ListState {
    next: Option<u64>,
}

struct ItemState {
    marker: Marker,
    depth: usize,
    list: usize,
    source: Range<usize>,
    text: Vec<Span>,
    /// Its block went out when a nested list began; later text is dropped.
    done: bool,
}

struct Builder<'a> {
    source: &'a str,
    blocks: Vec<Block>,
    marks: Marks,
    /// The paragraph or heading being read, outside lists and quotes.
    text: Option<(Vec<Span>, Range<usize>, Option<u8>)>,
    lists: Vec<ListState>,
    items: Vec<ItemState>,
    top_lists: usize,
    quote_depth: usize,
    quote: Option<(Vec<Vec<Span>>, Range<usize>)>,
    quote_text: Option<Vec<Span>>,
    code: Option<(String, String, Range<usize>)>,
}

impl<'a> Builder<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            blocks: Vec::new(),
            marks: Marks::default(),
            text: None,
            lists: Vec::new(),
            items: Vec::new(),
            top_lists: 0,
            quote_depth: 0,
            quote: None,
            quote_text: None,
            code: None,
        }
    }

    fn trimmed(&self, range: Range<usize>) -> Range<usize> {
        let end = self.source[..range.end.min(self.source.len())]
            .trim_end()
            .len()
            .max(range.start);
        range.start..end
    }

    fn push(&mut self, kind: BlockKind, range: Range<usize>) {
        let source = self.trimmed(range);
        self.blocks.push(Block { kind, source });
    }

    /// Where inline text goes now.
    fn target(&mut self) -> Option<&mut Vec<Span>> {
        if self.quote_depth > 0 {
            return Some(self.quote_text.get_or_insert_with(Vec::new));
        }
        if let Some(item) = self.items.last_mut() {
            return (!item.done).then_some(&mut item.text);
        }
        self.text.as_mut().map(|(spans, _, _)| spans)
    }

    fn inline(&mut self, text: &str, code: bool) {
        let span = Span {
            text: text.to_string(),
            strong: self.marks.strong > 0,
            emphasis: self.marks.emphasis > 0,
            strike: self.marks.strike > 0,
            code,
            link: self.marks.links.last().cloned(),
        };
        let Some(spans) = self.target() else {
            return;
        };
        match spans.last_mut() {
            Some(last)
                if last.strong == span.strong
                    && last.emphasis == span.emphasis
                    && last.strike == span.strike
                    && last.code == span.code
                    && last.link == span.link =>
            {
                last.text.push_str(&span.text);
            }
            _ => spans.push(span),
        }
    }

    /// The item's block goes out, its source stopping at `end`.
    fn flush_item(&mut self, end: usize) {
        let Some(item) = self.items.last_mut() else {
            return;
        };
        if item.done {
            return;
        }
        item.done = true;
        let kind = BlockKind::Item {
            marker: item.marker,
            depth: item.depth,
            list: item.list,
            text: std::mem::take(&mut item.text),
        };
        let range = item.source.start..end.min(item.source.end);
        self.push(kind, range);
    }

    fn event(&mut self, event: Event<'_>, range: Range<usize>) {
        if let Some((_, text, _)) = self.code.as_mut() {
            match event {
                Event::Text(value) => text.push_str(&value),
                Event::End(TagEnd::CodeBlock) => {
                    let (language, text, source) = self.code.take().unwrap_or_default();
                    let text = text.strip_suffix('\n').unwrap_or(&text).to_string();
                    if self.quote_depth > 0 || !self.items.is_empty() {
                        // Inside a quote or an item, the code reads as its text.
                        self.inline(&text, true);
                    } else {
                        self.push(BlockKind::Code { language, text }, source);
                    }
                }
                _ => {}
            }
            return;
        }
        match event {
            Event::Start(tag) => self.start(tag, range),
            Event::End(tag) => self.end(tag, range),
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
                self.inline(&text, false)
            }
            Event::InlineMath(text) | Event::DisplayMath(text) => self.inline(&text, true),
            Event::Code(text) => self.inline(&text, true),
            Event::SoftBreak => self.inline(" ", false),
            Event::HardBreak => self.inline("\n", false),
            Event::TaskListMarker(done) => {
                if let Some(item) = self.items.last_mut() {
                    item.marker = Marker::Task(done);
                }
            }
            Event::Rule => {
                if self.quote_depth == 0 && self.items.is_empty() {
                    self.push(BlockKind::Rule, range);
                }
            }
            Event::FootnoteReference(name) => self.inline(&format!("[^{name}]"), false),
        }
    }

    fn start(&mut self, tag: Tag<'_>, range: Range<usize>) {
        match tag {
            Tag::Paragraph | Tag::HtmlBlock => {
                if self.quote_depth > 0 {
                    self.quote_text.get_or_insert_with(Vec::new);
                } else if self.items.is_empty() {
                    self.text = Some((Vec::new(), range, None));
                } else if let Some(item) = self.items.last_mut()
                    && !item.text.is_empty()
                {
                    // A loose item's later paragraph runs on after the first.
                    item.text.push(Span {
                        text: " ".into(),
                        ..Span::default()
                    });
                }
            }
            Tag::Heading { level, .. } => {
                if self.quote_depth == 0 && self.items.is_empty() {
                    self.text = Some((Vec::new(), range, Some(heading_level(level))));
                } else if self.quote_depth > 0 {
                    self.quote_text.get_or_insert_with(Vec::new);
                }
            }
            Tag::BlockQuote(_) => {
                if self.quote_depth == 0 && self.items.is_empty() {
                    self.quote = Some((Vec::new(), range));
                }
                self.quote_depth += 1;
            }
            Tag::CodeBlock(kind) => {
                let language = match kind {
                    CodeBlockKind::Fenced(language) => language.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code = Some((language, String::new(), range));
            }
            Tag::List(start) => {
                if self.quote_depth > 0 {
                    return;
                }
                // A nested list ends its parent item's own text.
                if !self.items.is_empty() {
                    self.flush_item(range.start);
                } else {
                    self.top_lists += 1;
                }
                self.lists.push(ListState { next: start });
            }
            Tag::Item => {
                if self.quote_depth > 0 {
                    return;
                }
                let depth = self.lists.len().saturating_sub(1);
                let marker = match self.lists.last_mut() {
                    Some(ListState { next: Some(next) }) => {
                        let number = *next;
                        *next += 1;
                        Marker::Number(number)
                    }
                    _ => Marker::Bullet,
                };
                self.items.push(ItemState {
                    marker,
                    depth,
                    list: self.top_lists,
                    source: range,
                    text: Vec::new(),
                    done: false,
                });
            }
            Tag::Emphasis => self.marks.emphasis += 1,
            Tag::Strong => self.marks.strong += 1,
            Tag::Strikethrough => self.marks.strike += 1,
            Tag::Link { dest_url, .. } => self.marks.links.push(dest_url.to_string()),
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd, range: Range<usize>) {
        match tag {
            TagEnd::Paragraph | TagEnd::HtmlBlock | TagEnd::Heading(_) => {
                if self.quote_depth > 0 {
                    if let (Some(text), Some((paragraphs, _))) =
                        (self.quote_text.take(), self.quote.as_mut())
                    {
                        paragraphs.push(text);
                    }
                } else if let Some((spans, source, level)) = self.text.take() {
                    let kind = match level {
                        Some(level) => BlockKind::Heading(level, spans),
                        None => BlockKind::Paragraph(spans),
                    };
                    self.push(kind, source);
                }
            }
            TagEnd::BlockQuote(_) => {
                self.quote_depth = self.quote_depth.saturating_sub(1);
                if self.quote_depth == 0
                    && let Some((paragraphs, source)) = self.quote.take()
                {
                    self.push(BlockKind::Quote(paragraphs), source);
                }
            }
            TagEnd::List(_) => {
                if self.quote_depth == 0 {
                    self.lists.pop();
                }
            }
            TagEnd::Item => {
                if self.quote_depth > 0 {
                    return;
                }
                self.flush_item(range.end);
                self.items.pop();
            }
            TagEnd::Emphasis => self.marks.emphasis = self.marks.emphasis.saturating_sub(1),
            TagEnd::Strong => self.marks.strong = self.marks.strong.saturating_sub(1),
            TagEnd::Strikethrough => self.marks.strike = self.marks.strike.saturating_sub(1),
            TagEnd::Link => {
                self.marks.links.pop();
            }
            _ => {}
        }
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// How a block spaces itself from its neighbors.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Flow {
    Heading,
    Text,
    /// The list it belongs to, and whether it is a task.
    Item(usize, bool),
    Rule,
}

impl Flow {
    fn of(kind: &BlockKind) -> Self {
        match kind {
            BlockKind::Heading(..) => Flow::Heading,
            BlockKind::Item { list, marker, .. } => {
                Flow::Item(*list, matches!(marker, Marker::Task(_)))
            }
            BlockKind::Rule => Flow::Rule,
            _ => Flow::Text,
        }
    }

    fn top(self) -> f32 {
        match self {
            Flow::Heading => HEADING_TOP,
            Flow::Rule => RULE_MARGIN,
            Flow::Item(..) => ITEM_GAP,
            Flow::Text => 0.0,
        }
    }

    fn bottom(self) -> f32 {
        match self {
            Flow::Heading => HEADING_BOTTOM,
            Flow::Rule => RULE_MARGIN,
            _ => BLOCK_GAP,
        }
    }
}

/// The space above `next`: the larger of the two margins, as CSS collapses them, and one
/// task list's items 3px apart.
fn gap(previous: Option<Flow>, next: Flow) -> f32 {
    match (previous, next) {
        (None, next) => next.top(),
        (Some(Flow::Item(a, true)), Flow::Item(b, true)) if a == b => ITEM_GAP,
        (Some(previous), next) => previous.bottom().max(next.top()),
    }
}

/// The text size and line height of a heading level.
fn heading_type(level: u8) -> (f32, f32) {
    match level {
        1 => (20.0, 28.0),
        2 => (17.0, 24.0),
        _ => (15.0, 24.0),
    }
}

/// The type a block is set in, for its field while it is edited.
fn block_type(kind: &BlockKind) -> (f32, f32, FontWeight) {
    match kind {
        BlockKind::Heading(level, _) => {
            let (size, line) = heading_type(*level);
            (size, line, FontWeight::SEMIBOLD)
        }
        _ => (TEXT_SIZE, LINE, WEIGHT),
    }
}

/// Spans as one wrapping text with their marks; `done` mutes and strikes it all.
fn inline(spans: &[Span], done: bool, theme: &Theme) -> impl IntoElement {
    let mut text = String::new();
    let mut styles = Vec::new();
    for span in spans {
        let start = text.len();
        text.push_str(&span.text);
        let mut style = SpanStyle::default();
        if span.strong {
            style.highlight.font_weight = Some(FontWeight::SEMIBOLD);
        }
        if span.emphasis {
            style.highlight.font_style = Some(FontStyle::Italic);
        }
        if span.strike || done {
            style.highlight.strikethrough = Some(StrikethroughStyle {
                thickness: px(1.0),
                color: None,
            });
        }
        if span.code {
            style.font_family = Some(theme.font_mono.into());
            style.highlight.background_color = Some(theme.color_surface_subtle.into());
        }
        if span.link.is_some() {
            style.highlight.color = Some(theme.color_info.into());
        }
        if style != SpanStyle::default() && start < text.len() {
            styles.push((start..text.len(), style));
        }
    }
    wrap_text(text).with_styles(styles)
}

/// A list marker as an outside marker draws it: a disc, circle, or square by depth, or the
/// number, ending `MARKER_GAP` before the text.
fn marker(marker: Marker, depth: usize, theme: &Theme) -> Div {
    let glyph: SharedString = match marker {
        Marker::Number(number) => format!("{number}.").into(),
        _ => match depth {
            0 => "•".into(),
            1 => "◦".into(),
            _ => "▪".into(),
        },
    };
    div()
        .flex_none()
        .w(px(LIST_INDENT))
        .flex()
        .justify_end()
        .pr(px(MARKER_GAP))
        .text_color(theme.color_text_primary)
        .child(glyph)
}

/// One block drawn read-only. `on_task` checks a task off when its box is clicked.
fn block_element(block: &Block, on_task: Option<crate::Handler>, theme: &Theme) -> AnyElement {
    match &block.kind {
        BlockKind::Heading(level, spans) => {
            let (size, line) = heading_type(*level);
            div()
                .w_full()
                .text_size(px(size))
                .line_height(px(line))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.color_text_primary)
                .child(inline(spans, false, theme))
                .into_any_element()
        }
        BlockKind::Paragraph(spans) => div()
            .w_full()
            .child(inline(spans, false, theme))
            .into_any_element(),
        BlockKind::Item {
            marker: Marker::Task(done),
            depth,
            text,
            ..
        } => {
            let mut check = div()
                .id(("md-task", block.source.start))
                .flex_none()
                .mt(px((LINE - TASK_BOX) / 2.0))
                .child(checkbox_mark(*done, TASK_BOX, theme));
            if let Some(on_task) = on_task {
                check = check
                    .cursor(CursorStyle::PointingHand)
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        on_task(window, cx);
                    });
            }
            div()
                .w_full()
                .flex()
                .items_start()
                .gap(px(TASK_GAP))
                // A task list has no indent of its own; a nested one, the list's.
                .pl(px(LIST_INDENT * *depth as f32))
                .child(check)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .when(*done, |text| text.text_color(theme.color_text_muted))
                        .child(inline(text, *done, theme)),
                )
                .into_any_element()
        }
        BlockKind::Item {
            marker: kind,
            depth,
            text,
            ..
        } => div()
            .w_full()
            .flex()
            .items_start()
            .pl(px(LIST_INDENT * *depth as f32))
            .child(marker(*kind, *depth, theme))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(inline(text, false, theme)),
            )
            .into_any_element(),
        BlockKind::Quote(paragraphs) => div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(BLOCK_GAP))
            .pl(px(QUOTE_PAD))
            .border_l_2()
            .border_color(theme.color_border_strong)
            .text_color(theme.color_text_secondary)
            .children(
                paragraphs
                    .iter()
                    .map(|spans| div().w_full().child(inline(spans, false, theme))),
            )
            .into_any_element(),
        BlockKind::Code { text, .. } => code_frame(theme)
            .child(CodeText {
                text: text.clone(),
                offset: block.source.start,
            })
            .into_any_element(),
        BlockKind::Rule => div()
            .w_full()
            .h(px(1.0))
            .bg(theme.color_border_subtle)
            .into_any_element(),
    }
}

/// `pre code` keeps its hard lines at their natural width, including outside an open field.
#[derive(IntoElement)]
struct CodeText {
    text: String,
    offset: usize,
}

impl RenderOnce for CodeText {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let handle = crate::scroll::use_scroll(("md-code-horizontal", self.offset), window, cx)
            .read(cx)
            .handle()
            .clone();
        let width = self
            .text
            .split('\n')
            .map(|line| {
                crate::data_table::text_width(line, gpui::font(theme.font_mono), CODE_SIZE, window)
            })
            .fold(0.0_f32, f32::max);
        let lines = div()
            .w(px(width))
            .min_w_full()
            .flex_none()
            .flex()
            .flex_col()
            .children(self.text.split('\n').map(|line| {
                div()
                    .h(px(LINE))
                    .flex_none()
                    .whitespace_nowrap()
                    .child(SharedString::from(
                        if line.is_empty() { " " } else { line }.to_string(),
                    ))
            }));
        crate::scroll::horizontal_scroll_area(
            ("md-code-text", self.offset),
            &handle,
            lines,
            window,
            cx,
        )
    }
}

/// A code block's frame: 12px mono, padded 12px on the subtle surface at the card radius.
fn code_frame(theme: &Theme) -> Div {
    div()
        .w_full()
        .p(px(CODE_PAD))
        .rounded(px(theme.radius_card))
        .bg(theme.color_surface_subtle)
        .font_family(theme.font_mono)
        .text_size(px(CODE_SIZE))
        .font_weight(FontWeight::NORMAL)
}

/// The document's text style, which every block inherits.
fn document(theme: &Theme) -> Div {
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .text_size(px(TEXT_SIZE))
        .line_height(px(LINE))
        .font_weight(WEIGHT)
        .text_color(theme.color_text_primary)
}

/// `source` drawn read-only in the document style; an empty source draws nothing.
pub fn markdown_view(source: &str, cx: &App) -> Div {
    let theme = Theme::global(cx);
    let blocks = markdown_blocks(source);
    let mut root = document(theme);
    let mut previous = None;
    for block in &blocks {
        let flow = Flow::of(&block.kind);
        root = root.child(div().w_full().pt(px(gap(previous, flow))).child(
            if matches!(block.kind, BlockKind::Code { .. }) {
                ReadOnlyCode {
                    source: source[block.source.clone()].to_string(),
                    offset: block.source.start,
                }
                .into_any_element()
            } else {
                block_element(block, None, theme)
            },
        ));
        previous = Some(flow);
    }
    if let Some(last) = previous {
        root = root.pb(px(last.bottom()));
    }
    root
}

/// Read-only `MarkdownEditor` code: the same Copy corner and 1.5s feedback as the editor.
#[derive(IntoElement)]
struct ReadOnlyCode {
    source: String,
    offset: usize,
}

impl RenderOnce for ReadOnlyCode {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let source = self.source;
        let editor = window.use_keyed_state(("md-readonly-code", self.offset), cx, |_, cx| {
            let mut editor = MarkdownEditor::new(source.clone(), cx);
            editor.set_editable(false, cx);
            editor
        });
        editor.update(cx, |editor, cx| editor.set_source(source, cx));
        // The surrounding markdown_view already supplies this block's bottom margin.
        div().w_full().mb(px(-BLOCK_GAP)).child(editor)
    }
}

/// The text a block's field holds: its Markdown, with the lines a paragraph wraps over
/// joined. A code block, or a quote of more than one paragraph, has none.
pub fn block_edit_text(source: &str, block: &Block) -> Option<String> {
    let markdown = source.get(block.source.clone())?;
    match &block.kind {
        BlockKind::Code { .. } => return None,
        BlockKind::Quote(paragraphs) if paragraphs.len() > 1 => return None,
        _ => {}
    }
    let mut lines = markdown.lines();
    let first = lines.next().unwrap_or_default().trim_end().to_string();
    // A setext heading (`Title` over `===`) is edited as an ATX one.
    if let BlockKind::Heading(level, _) = &block.kind
        && !first.trim_start().starts_with('#')
    {
        return Some(format!("{} {}", "#".repeat(*level as usize), first.trim()));
    }
    let quote = matches!(block.kind, BlockKind::Quote(_));
    let mut text = first;
    for line in lines {
        let mut line = line.trim();
        if quote {
            line = line.strip_prefix('>').unwrap_or(line).trim_start();
        }
        if line.is_empty() {
            continue;
        }
        text.push(' ');
        text.push_str(line);
    }
    Some(text)
}

/// The marker a new item after `text` starts with, when `text` is a list item: the same
/// indent and bullet, the next number, or an open task.
pub fn continue_item(text: &str) -> Option<String> {
    let indent_len = text.len() - text.trim_start().len();
    let (indent, rest) = text.split_at(indent_len);
    let mut chars = rest.chars();
    let first = chars.next()?;
    if matches!(first, '-' | '*' | '+') && rest[1..].starts_with(' ') {
        let after = rest[1..].trim_start();
        if after.starts_with("[ ]") || after.starts_with("[x]") || after.starts_with("[X]") {
            return Some(format!("{indent}{first} [ ] "));
        }
        return Some(format!("{indent}{first} "));
    }
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() || digits.len() > 9 {
        return None;
    }
    let delimiter = rest[digits.len()..].chars().next()?;
    if !matches!(delimiter, '.' | ')') || !rest[digits.len() + 1..].starts_with(' ') {
        return None;
    }
    let next = digits.parse::<u64>().ok()? + 1;
    Some(format!("{indent}{next}{delimiter} "))
}

/// Whether a block's field holds nothing but its marker: `- `, `1. `, `## `, `> `.
fn is_blank(text: &str) -> bool {
    let text = text.trim();
    if text.chars().all(|c| c == '#' || c == '>') {
        return true;
    }
    strip_marker(text).trim().is_empty()
}

/// `text` past its list marker (`-`, `*`, `+`, or `1.`/`1)`) and task box, when it has one.
fn strip_marker(text: &str) -> &str {
    let rest = text.trim_start();
    let after = if let Some(after) = rest.strip_prefix(['-', '*', '+']) {
        after
    } else {
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        match rest[digits..].strip_prefix(['.', ')']) {
            Some(after) if digits > 0 => after,
            _ => return text,
        }
    };
    if !after.is_empty() && !after.starts_with(' ') {
        return text;
    }
    let after = after.trim_start();
    ["[ ]", "[x]", "[X]"]
        .iter()
        .find_map(|task| after.strip_prefix(task))
        .unwrap_or(after)
}

/// `source` with `range` replaced by `text`, or the block taken out with the blank lines
/// after it (before it, at the end) when `text` is blank.
pub fn replace_block(source: &str, range: Range<usize>, text: &str) -> String {
    if !is_blank(text) {
        return format!("{}{}{}", &source[..range.start], text, &source[range.end..]);
    }
    let mut start = range.start;
    let mut end = range.end;
    while source[end..].starts_with('\n') {
        end += 1;
    }
    if end == source.len() {
        start = source[..start].trim_end().len();
    }
    format!("{}{}", &source[..start], &source[end..])
}

/// `source` with a new block written at `at`, after `before` (the newlines that set it apart).
fn insert_block(source: &str, at: usize, before: &str, text: &str) -> String {
    if is_blank(text) {
        return source.to_string();
    }
    format!("{}{}{}{}", &source[..at], before, text, &source[at..])
}

/// `source` with the task at `range` checked off, or opened again.
pub fn toggle_task(source: &str, range: Range<usize>) -> String {
    let markdown = &source[range.clone()];
    for (open, done) in [("[ ]", "[x]"), ("[x]", "[ ]"), ("[X]", "[ ]")] {
        if let Some(at) = markdown.find(open) {
            let at = range.start + at;
            return format!("{}{}{}", &source[..at], done, &source[at + open.len()..]);
        }
    }
    source.to_string()
}

/// What the editor's source became.
#[derive(Clone, Debug, PartialEq)]
pub enum MarkdownEditorEvent {
    Changed(String),
}

/// Where the open field writes: over a block, or a new block at a place in the source.
#[derive(Clone, Debug, PartialEq)]
enum Target {
    Block(usize),
    Insert { at: usize, before: &'static str },
}

/// What a field opens on.
struct Opening {
    text: String,
    /// A code block's opening and closing fences: the field holds the code between them.
    fence: Option<(String, String)>,
    /// The selection to put back; without one the caret goes to the end.
    selection: Option<Range<usize>>,
}

impl Opening {
    fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            fence: None,
            selection: None,
        }
    }
}

/// A field's text as the Markdown it writes: a code block's code between its fences.
fn written_markdown(text: &str, fence: Option<&(String, String)>) -> String {
    match fence {
        Some((open, close)) => format!("{open}\n{text}\n{close}"),
        None => text.to_string(),
    }
}

/// The type a field holding `text` is set in: that of the block its Markdown makes, so a
/// paragraph that gains a `## ` turns into a heading as it is typed.
fn field_type(text: &str) -> (f32, f32, FontWeight) {
    markdown_blocks(text)
        .first()
        .map_or((TEXT_SIZE, LINE, WEIGHT), |block| block_type(&block.kind))
}

/// The newlines a new block is written after: right after a list item only another item
/// stays in the list, and anything else stands apart from it.
fn spacing(before: &'static str, markdown: &str) -> &'static str {
    if before == "\n" && !is_item(markdown) {
        "\n\n"
    } else {
        before
    }
}

/// `source` with a rule at `at`, a blank line on either side of it; and where the rule ends.
fn insert_rule(source: &str, at: usize) -> (String, usize) {
    let head = source[..at].trim_end();
    let tail = source[at..].trim_start_matches('\n');
    let before = if head.is_empty() { "" } else { "\n\n" };
    let after = if tail.is_empty() { "" } else { "\n\n" };
    (
        format!("{head}{before}---{after}{tail}"),
        head.len() + before.len() + 3,
    )
}

/// The slash menu open on the field (`SlashSession`).
#[derive(Clone, Debug, PartialEq)]
struct Slash {
    /// The `/` and the query after it in the field's text; in a block the gutter's "+" opened,
    /// what was typed into it.
    range: Range<usize>,
    query: String,
    /// Opened by the "+", on an empty block of its own.
    inserted: bool,
    /// The row Enter picks.
    row: usize,
}

struct Editing {
    target: Target,
    input: Entity<TextInput>,
    /// The Markdown the field wrote when it opened, so an untouched block keeps its spelling.
    initial: String,
    fence: Option<(String, String)>,
    /// The field's selection when the editor last looked.
    selection: Range<usize>,
    /// A code block's caret is after two empty lines at its end, where Enter leaves the block
    /// (Tiptap's `exitOnTripleEnter`).
    exit_ready: bool,
    _subscriptions: Vec<Subscription>,
}

/// Where a code block's label copies from.
#[derive(Clone)]
enum CopySource {
    Code(String),
    Field,
}

/// The document's Markdown edited one block at a time. See the module's doc.
pub struct MarkdownEditor {
    source: String,
    blocks: Vec<Block>,
    editable: bool,
    placeholder: SharedString,
    editing: Option<Editing>,
    /// The last source sent, so a keystroke that changes nothing sends nothing.
    sent: String,
    slash: Option<Slash>,
    /// Where the slash menu was closed (Escape, a row picked, a press outside), so the same
    /// `/` leaves it closed.
    dismissed_slash: Option<usize>,
    /// The selection the toolbar is drawn over: some text, outside a code block.
    toolbar: Option<Range<usize>>,
    /// The selection the toolbar was closed on, until the selection moves.
    dismissed_toolbar: Option<Range<usize>>,
    /// The block under the pointer: its hover fill and the gutter's "+".
    hovered: Option<usize>,
    /// A code block was just copied, so its label reads "Copied".
    copied: bool,
    copied_timer: Option<Task<()>>,
    slash_scroll: ScrollHandle,
}

impl EventEmitter<MarkdownEditorEvent> for MarkdownEditor {}

impl MarkdownEditor {
    pub fn new(source: impl Into<String>, cx: &mut Context<Self>) -> Self {
        let _ = cx;
        let source = source.into();
        Self {
            blocks: markdown_blocks(&source),
            sent: source.clone(),
            source,
            editable: true,
            placeholder: "Add a description…".into(),
            editing: None,
            slash: None,
            dismissed_slash: None,
            toolbar: None,
            dismissed_toolbar: None,
            hovered: None,
            copied: false,
            copied_timer: None,
            slash_scroll: ScrollHandle::new(),
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    pub fn is_editing(&self) -> bool {
        self.editing.is_some()
    }

    /// Shows another source, as when the record reloads; ignored while a block is open, so a
    /// save coming back never pulls the text from under the caret.
    pub fn set_source(&mut self, source: impl Into<String>, cx: &mut Context<Self>) {
        let source = source.into();
        if self.editing.is_some() || source == self.source {
            return;
        }
        self.blocks = markdown_blocks(&source);
        self.sent = source.clone();
        self.source = source;
        cx.notify();
    }

    pub fn set_editable(&mut self, editable: bool, cx: &mut Context<Self>) {
        if !editable {
            self.commit(cx);
            self.hovered = None;
        }
        self.editable = editable;
        cx.notify();
    }

    pub fn set_placeholder(&mut self, placeholder: impl Into<SharedString>) {
        self.placeholder = placeholder.into();
    }

    /// Whether the host must freeze the scrollers carrying this editor, as
    /// `markdown-editor.tsx` freezes its ancestors while the slash menu is open.
    pub fn slash_open(&self) -> bool {
        self.slash.is_some()
    }

    /// Opens the first block, or a new one in an empty document, as the title's Enter does.
    pub fn edit_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match (0..self.blocks.len()).find(|index| self.opening(*index).is_some()) {
            Some(index) => self.edit_block(index, window, cx),
            None => self.edit_end(window, cx),
        }
    }

    /// Opens block `index`, writing the open one first.
    pub fn edit_block(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editable {
            return;
        }
        let Some(start) = self.blocks.get(index).map(|block| block.source.start) else {
            return;
        };
        let start = shift(start, self.commit(cx));
        let Some(index) = self
            .blocks
            .iter()
            .position(|block| block.source.start == start)
        else {
            return;
        };
        if let Some(opening) = self.opening(index) {
            self.open(Target::Block(index), opening, None, window, cx);
        }
    }

    /// Selects `range` of the open block's Markdown, as a drag across it does, which raises
    /// the selection toolbar over it.
    pub fn select(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        if let Some(input) = self.editing.as_ref().map(|editing| editing.input.clone()) {
            input.update(cx, |input, cx| input.select(range, cx));
            // A field opened in this same update has not heard its own notice yet.
            if self.sync(cx) {
                cx.notify();
            }
        }
    }

    /// Opens a new block at the end.
    pub fn edit_end(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editable {
            return;
        }
        self.commit(cx);
        let at = self.source.trim_end().len();
        let before = if at == 0 { "" } else { "\n\n" };
        self.open(
            Target::Insert { at, before },
            Opening::text(""),
            None,
            window,
            cx,
        );
    }

    /// What block `index`'s field opens on: its Markdown, or a code block's code; a rule, or
    /// a quote of more than one paragraph, has none.
    fn opening(&self, index: usize) -> Option<Opening> {
        let block = self.blocks.get(index)?;
        if let BlockKind::Code { text, .. } = &block.kind {
            return Some(Opening {
                text: text.clone(),
                fence: Some(code_fences(&self.source[block.source.clone()])),
                selection: None,
            });
        }
        block_edit_text(&self.source, block).map(Opening::text)
    }

    /// Opens a field on `target`. `initial` is what the block wrote before, when the field
    /// takes over from another on the same block.
    fn open(
        &mut self,
        target: Target,
        opening: Opening,
        initial: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = cx.weak_entity();
        let on_cancel = editor.clone();
        let on_key = editor.clone();
        let placeholder = if self.source.trim().is_empty() {
            self.placeholder.clone()
        } else {
            SharedString::default()
        };
        let initial =
            initial.unwrap_or_else(|| written_markdown(&opening.text, opening.fence.as_ref()));
        let Opening {
            text,
            fence,
            selection,
        } = opening;
        let code = fence.is_some();
        let (size, line, weight) = if code {
            (CODE_SIZE, LINE, FontWeight::NORMAL)
        } else {
            field_type(&text)
        };
        let input = cx.new(|cx| {
            let input = TextInput::new(cx).prose(size, line, weight);
            let input = if code {
                input.mono().multiline().nowrap()
            } else {
                input.wrap()
            };
            let mut input = input
                .placeholder(placeholder)
                // The field calls these inside its own update, so the editor reads it after.
                .on_submit(move |_, window, cx| {
                    let editor = editor.clone();
                    window.defer(cx, move |window, cx| {
                        let _ = editor.update(cx, |this, cx| this.submit(window, cx));
                    });
                })
                .on_cancel(move |window, cx| {
                    let on_cancel = on_cancel.clone();
                    window.defer(cx, move |_, cx| {
                        let _ = on_cancel.update(cx, |this, cx| {
                            this.commit(cx);
                        });
                    });
                })
                .on_menu_key(move |key, window, cx| {
                    on_key
                        .update(cx, |this, cx| this.menu_key(key, window, cx))
                        .unwrap_or(false)
                });
            input.set_text(text, cx);
            if let Some(selection) = selection {
                input.select(selection, cx);
            }
            input
        });
        let handle = input.read(cx).focus_handle(cx);
        let id = input.entity_id();
        let blur = cx.on_blur(&handle, window, move |this, _, cx| {
            if this
                .editing
                .as_ref()
                .is_some_and(|editing| editing.input.entity_id() == id)
            {
                this.commit(cx);
            }
        });
        let typed = cx.observe(&input, |this, _, cx| {
            this.preview(cx);
            if this.sync(cx) {
                cx.notify();
            }
        });
        handle.focus(window);
        let selection = input.read(cx).selection();
        self.editing = Some(Editing {
            target,
            input,
            initial,
            fence,
            selection,
            exit_ready: false,
            _subscriptions: vec![blur, typed],
        });
        self.slash = None;
        self.toolbar = None;
        self.dismissed_toolbar = None;
        // gpui folds the field's notices from this update into the one sent before the field
        // was observed, so the editor looks at it once now.
        self.sync(cx);
        cx.notify();
    }

    /// The source with the open field written in, and where the change starts and how much
    /// it moves what follows.
    fn written(&self, cx: &App) -> Option<(String, usize, isize)> {
        let editing = self.editing.as_ref()?;
        let text = editing.input.read(cx).text().to_string();
        let markdown = written_markdown(&text, editing.fence.as_ref());
        match &editing.target {
            Target::Block(index) => {
                if markdown == editing.initial {
                    return None;
                }
                let range = self.blocks.get(*index)?.source.clone();
                let next = replace_block(&self.source, range.clone(), &markdown);
                let delta = next.len() as isize - self.source.len() as isize;
                Some((next, range.end, delta))
            }
            Target::Insert { at, before } => {
                let before = spacing(before, &markdown);
                let next = insert_block(&self.source, *at, before, &markdown);
                let delta = next.len() as isize - self.source.len() as isize;
                (delta != 0).then_some((next, *at, delta))
            }
        }
    }

    /// Sends the source as the open field would write it.
    fn preview(&mut self, cx: &mut Context<Self>) {
        let next = self
            .written(cx)
            .map(|(next, _, _)| next)
            .unwrap_or_else(|| self.source.clone());
        if next != self.sent {
            self.sent = next.clone();
            cx.emit(MarkdownEditorEvent::Changed(next));
        }
    }

    /// Looks at the open field after it changed: its type follows its Markdown, and the slash
    /// menu and the toolbar follow its text and selection, as the design system's
    /// `syncOverlays` does. Whether what hangs over the field changed.
    fn sync(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((input, code, before)) = self.editing.as_ref().map(|editing| {
            (
                editing.input.clone(),
                editing.fence.is_some(),
                editing.selection.clone(),
            )
        }) else {
            return false;
        };
        let (text, selection, caret) = {
            let input = input.read(cx);
            (input.text().to_string(), input.selection(), input.cursor())
        };
        let mut slash = None;
        if selection.is_empty() && !code {
            let inserted = self.slash.as_ref().is_some_and(|slash| slash.inserted);
            if inserted && filters(&text[..caret]) {
                slash = Some(Slash {
                    range: 0..caret,
                    query: text[..caret].to_string(),
                    inserted: true,
                    row: 0,
                });
            } else if let Some(range) = slash_query(&text, caret) {
                if self.dismissed_slash != Some(range.start) {
                    slash = Some(Slash {
                        query: text[range.start + 1..range.end].to_string(),
                        range,
                        inserted: false,
                        row: 0,
                    });
                }
            } else {
                self.dismissed_slash = None;
            }
        } else {
            self.dismissed_slash = None;
        }
        // A new session or query starts at the first row, scrolled to the top.
        match (slash.as_mut(), self.slash.as_ref()) {
            (Some(next), Some(open))
                if next.query == open.query && next.inserted == open.inserted =>
            {
                next.row = open.row;
            }
            (Some(_), _) => self.slash_scroll.set_offset(point(px(0.0), px(0.0))),
            _ => {}
        }
        let highlight = slash
            .as_ref()
            .map(|slash| slash.range.clone())
            .filter(|range| !range.is_empty());
        let toolbar = (!text[selection.clone()].trim().is_empty()).then(|| selection.clone());
        if self.dismissed_toolbar.as_ref() != Some(&selection) {
            self.dismissed_toolbar = None;
        }
        let changed = slash != self.slash
            || toolbar != self.toolbar
            || (toolbar.is_some() && before != selection);
        if let Some(editing) = self.editing.as_mut() {
            editing.exit_ready = code && caret == text.len() && text.ends_with("\n\n");
            editing.selection = selection;
        }
        self.slash = slash;
        self.toolbar = toolbar;
        let style = (!code).then(|| field_type(&text));
        input.update(cx, |input, cx| {
            input.set_highlight(highlight, cx);
            if let Some((size, line, weight)) = style {
                input.set_prose(size, line, weight, cx);
            }
        });
        changed
    }

    /// A key the slash menu or the toolbar answers before the field: ↑/↓ move the menu's row
    /// and wrap, Enter or Tab picks it, Escape closes the menu, or else the toolbar. Enter in a
    /// code block that ends in two empty lines leaves it. It runs inside the field's update, so
    /// what reads the field waits until after.
    fn menu_key(&mut self, key: MenuKey, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(slash) = self.slash.as_mut() {
            let rows = slash_rows(&slash.query);
            match key {
                MenuKey::Escape => self.close_slash(window, cx),
                MenuKey::Up | MenuKey::Down => {
                    if !rows.is_empty() {
                        let count = rows.len();
                        let row = slash.row.min(count - 1);
                        slash.row = if key == MenuKey::Down {
                            (row + 1) % count
                        } else {
                            (row + count - 1) % count
                        };
                        let row = slash.row;
                        self.reveal_row(row, &rows);
                        cx.notify();
                    }
                }
                MenuKey::Enter | MenuKey::Tab => {
                    let Some(tool) = rows.get(slash.row).copied() else {
                        // With no match, Enter goes on as it would without the menu.
                        return false;
                    };
                    cx.defer_in(window, move |this, window, cx| {
                        this.choose(tool, window, cx)
                    });
                }
            }
            return true;
        }
        match key {
            MenuKey::Escape if self.toolbar_shown() => {
                self.dismissed_toolbar = self.toolbar.clone();
                cx.notify();
                true
            }
            MenuKey::Enter
                if self
                    .editing
                    .as_ref()
                    .is_some_and(|editing| editing.exit_ready) =>
            {
                cx.defer_in(window, |this, window, cx| this.exit_code(window, cx));
                true
            }
            _ => false,
        }
    }

    fn toolbar_shown(&self) -> bool {
        self.slash.is_none() && self.toolbar.is_some() && self.dismissed_toolbar != self.toolbar
    }

    /// Scrolls the menu's list so row `row` shows, with its section's heading for a section's
    /// first row.
    fn reveal_row(&self, row: usize, rows: &[Tool]) {
        if row == 0 {
            self.slash_scroll.set_offset(point(px(0.0), px(0.0)));
            return;
        }
        let mut sections: Vec<usize> = rows.iter().map(|tool| tool.section()).collect();
        sections.dedup();
        if let Some(child) = sections
            .iter()
            .position(|section| Some(*section) == rows.get(row).map(|tool| tool.section()))
        {
            self.slash_scroll.scroll_to_item(child);
        }
    }

    /// Closes the slash menu, which stays closed for this `/`.
    fn close_slash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(slash) = self.slash.take() else {
            return;
        };
        self.dismissed_slash = Some(slash.range.start);
        if let Some(input) = self.editing.as_ref().map(|editing| editing.input.clone()) {
            // This may run inside the field's own update.
            window.defer(cx, move |_, cx| {
                input.update(cx, |input, cx| input.set_highlight(None, cx));
            });
        }
        cx.notify();
    }

    /// Picks a slash menu row: the query goes and the block becomes what the row makes it.
    fn choose(&mut self, tool: Tool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(slash) = self.slash.take() else {
            return;
        };
        self.dismissed_slash = Some(slash.range.start);
        let Some(input) = self.editing.as_ref().map(|editing| editing.input.clone()) else {
            return;
        };
        let text = input.read(cx).text().to_string();
        let end = slash.range.end.min(text.len());
        let start = slash.range.start.min(end);
        let rest = format!("{}{}", &text[..start], &text[end..]);
        input.update(cx, |input, cx| input.set_highlight(None, cx));
        self.transform(tool, rest, start..start, window, cx);
    }

    /// A toolbar button: the tool on the field's selection.
    fn apply_tool(&mut self, tool: Tool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.editing.as_ref().map(|editing| editing.input.clone()) else {
            return;
        };
        let (text, selection) = {
            let input = input.read(cx);
            (input.text().to_string(), input.selection())
        };
        self.transform(tool, text, selection, window, cx);
    }

    /// The field's block, holding `text` with `selection`, changed by `tool`.
    fn transform(
        &mut self,
        tool: Tool,
        text: String,
        selection: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let code = self
            .editing
            .as_ref()
            .is_some_and(|editing| editing.fence.is_some());
        if code {
            // Tiptap code blocks exclude marks; block tools clear the code node first.
            if matches!(tool, Tool::Bold | Tool::Italic) {
                return;
            }
            if tool != Tool::Divider {
                let editing = self.editing.take().expect("the code field is open");
                self.open(
                    editing.target,
                    Opening {
                        text: text.clone(),
                        fence: None,
                        selection: Some(selection.clone()),
                    },
                    Some(editing.initial),
                    window,
                    cx,
                );
                if tool == Tool::Code {
                    return;
                }
            }
        }
        let shape = match tool {
            Tool::Bold | Tool::Italic => {
                let mark = if tool == Tool::Bold {
                    Mark::Bold
                } else {
                    Mark::Italic
                };
                let (next, selection) = toggle_mark(&text, selection, mark);
                self.set_field(next, selection, cx);
                return;
            }
            Tool::Code => {
                let (code, selection) = body(&text, selection);
                if let Some(editing) = self.editing.take() {
                    let opening = Opening {
                        text: code,
                        fence: Some(("```".into(), "```".into())),
                        selection: Some(selection),
                    };
                    self.open(editing.target, opening, Some(editing.initial), window, cx);
                }
                return;
            }
            Tool::Divider => {
                self.divide(text, window, cx);
                return;
            }
            Tool::Paragraph => Shape::Paragraph,
            Tool::Heading(level) => Shape::Heading(level),
            Tool::Bullet => Shape::Bullet,
            Tool::Numbered => Shape::Numbered,
            Tool::Task => Shape::Task,
            Tool::Quote => Shape::Quote,
        };
        let (next, selection) = if code {
            // Code text has no Markdown block marker to remove or toggle off.
            let prefix = match shape {
                Shape::Paragraph => String::new(),
                Shape::Heading(level) => format!("{} ", "#".repeat(level as usize)),
                Shape::Bullet => "- ".into(),
                Shape::Numbered => "1. ".into(),
                Shape::Task => "- [ ] ".into(),
                Shape::Quote => "> ".into(),
            };
            let selection = selection.start + prefix.len()..selection.end + prefix.len();
            (format!("{prefix}{text}"), selection)
        } else {
            toggle_shape(&text, selection, shape)
        };
        self.set_field(next, selection, cx);
    }

    /// The field's text replaced, as an edit Undo takes back.
    fn set_field(&mut self, text: String, selection: Range<usize>, cx: &mut Context<Self>) {
        if let Some(input) = self.editing.as_ref().map(|editing| editing.input.clone()) {
            input.update(cx, |input, cx| input.edit(text, selection, cx));
        }
    }

    /// Divider: the block, `rest` without the query, is written with a rule after it (in its
    /// place when it holds nothing), and a paragraph opens after the rule.
    fn divide(&mut self, rest: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editing) = self.editing.as_ref() else {
            return;
        };
        let from = match &editing.target {
            Target::Block(index) => self.blocks.get(*index).map(|block| block.source.start),
            Target::Insert { at, .. } => Some(*at),
        };
        let Some(from) = from else {
            return;
        };
        let input = editing.input.clone();
        let blank = is_blank(&rest);
        input.update(cx, |input, cx| input.set_text(rest, cx));
        self.commit(cx);
        let end = (!blank)
            .then(|| {
                self.blocks
                    .iter()
                    .find(|block| block.source.start >= from)
                    .map(|block| block.source.end)
            })
            .flatten();
        let at = end.unwrap_or(from).min(self.source.len());
        let (next, end) = insert_rule(&self.source, at);
        self.replace_source(next, cx);
        self.open(
            Target::Insert {
                at: end,
                before: "\n\n",
            },
            Opening::text(""),
            None,
            window,
            cx,
        );
    }

    /// Enter after two empty lines at the end of a code block: they go, and a paragraph opens
    /// after the block.
    fn exit_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editing) = self.editing.as_ref() else {
            return;
        };
        let from = match &editing.target {
            Target::Block(index) => self
                .blocks
                .get(*index)
                .map_or(0, |block| block.source.start),
            Target::Insert { at, .. } => *at,
        };
        let input = editing.input.clone();
        let text = input.read(cx).text().to_string();
        let code = text.strip_suffix("\n\n").unwrap_or(&text).to_string();
        input.update(cx, |input, cx| input.set_text(code, cx));
        self.commit(cx);
        let at = self
            .blocks
            .iter()
            .find(|block| block.source.start >= from)
            .map_or(self.source.trim_end().len(), |block| block.source.end);
        self.open(
            Target::Insert { at, before: "\n\n" },
            Opening::text(""),
            None,
            window,
            cx,
        );
    }

    /// The gutter's "+" beside block `index`: an empty item after an item (and the items
    /// nested in it), or else an empty paragraph after the block with the slash menu open on
    /// it, waiting for a block type.
    pub fn insert_below(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editable {
            return;
        }
        let Some(start) = self.blocks.get(index).map(|block| block.source.start) else {
            return;
        };
        let start = shift(start, self.commit(cx));
        let Some(index) = self
            .blocks
            .iter()
            .position(|block| block.source.start == start)
        else {
            return;
        };
        self.hovered = None;
        let block = &self.blocks[index];
        if let BlockKind::Item { depth, list, .. } = &block.kind {
            let end = self.blocks[index + 1..]
                .iter()
                .take_while(|next| {
                    matches!(&next.kind, BlockKind::Item { depth: d, list: l, .. } if l == list && d > depth)
                })
                .last()
                .map_or(block.source.end, |nested| nested.source.end);
            let line = self.source[..start].rfind('\n').map_or(0, |at| at + 1);
            let indent = &self.source[line..start];
            let indent = if indent.trim().is_empty() { indent } else { "" };
            let text = block_edit_text(&self.source, block).unwrap_or_default();
            let marker = continue_item(&text).unwrap_or_else(|| "- ".into());
            let text = format!("{indent}{marker}");
            self.open(
                Target::Insert {
                    at: end,
                    before: "\n",
                },
                Opening::text(text),
                None,
                window,
                cx,
            );
            return;
        }
        let at = block.source.end;
        self.dismissed_slash = None;
        self.open(
            Target::Insert { at, before: "\n\n" },
            Opening::text(""),
            None,
            window,
            cx,
        );
        self.slash = Some(Slash {
            range: 0..0,
            query: String::new(),
            inserted: true,
            row: 0,
        });
        self.slash_scroll.set_offset(point(px(0.0), px(0.0)));
    }

    /// Copies a code block's text; its label reads "Copied" for 1.5s.
    fn copy(&mut self, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.copied = true;
        cx.notify();
        self.copied_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            let _ = this.update(cx, |this, cx| {
                this.copied = false;
                cx.notify();
            });
        }));
    }

    /// The source replaced outside a field, as a rule is put in.
    fn replace_source(&mut self, next: String, cx: &mut Context<Self>) {
        self.source = next;
        self.blocks = markdown_blocks(&self.source);
        if self.sent != self.source {
            self.sent = self.source.clone();
            cx.emit(MarkdownEditorEvent::Changed(self.source.clone()));
        }
        cx.notify();
    }

    /// Writes the open field into the source and closes it; returns where the source changed
    /// and by how much.
    fn commit(&mut self, cx: &mut Context<Self>) -> Option<(usize, isize)> {
        let written = self.written(cx);
        self.editing = None;
        self.slash = None;
        self.dismissed_slash = None;
        self.toolbar = None;
        self.dismissed_toolbar = None;
        cx.notify();
        let (next, at, delta) = written?;
        self.source = next;
        self.blocks = markdown_blocks(&self.source);
        if self.sent != self.source {
            self.sent = self.source.clone();
            cx.emit(MarkdownEditorEvent::Changed(self.source.clone()));
        }
        Some((at, delta))
    }

    /// Enter: writes the block and opens the next one after it.
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editing) = self.editing.as_ref() else {
            return;
        };
        let text = editing.input.read(cx).text().to_string();
        let target = editing.target.clone();
        if is_blank(&text) {
            // An empty item leaves the list: the field stays, as a paragraph.
            if let Target::Insert { at, before: "\n" } = target {
                self.editing = None;
                self.open(
                    Target::Insert { at, before: "\n\n" },
                    Opening::text(""),
                    None,
                    window,
                    cx,
                );
            }
            return;
        }
        let start = match &target {
            Target::Block(index) => self.blocks.get(*index).map(|block| block.source.start),
            Target::Insert { at, before } => Some(at + spacing(before, &text).len()),
        };
        let Some(start) = start else {
            return;
        };
        self.commit(cx);
        let at = start + text.len();
        let (before, text) = match continue_item(&text) {
            Some(marker) => ("\n", marker),
            None => ("\n\n", String::new()),
        };
        self.open(
            Target::Insert { at, before },
            Opening::text(text),
            None,
            window,
            cx,
        );
    }

    fn toggle_task(&mut self, index: usize, cx: &mut Context<Self>) {
        let start = self.blocks.get(index).map(|block| block.source.start);
        let Some(start) = start.map(|start| shift(start, self.commit(cx))) else {
            return;
        };
        let Some(block) = self.blocks.iter().find(|block| block.source.start == start) else {
            return;
        };
        self.source = toggle_task(&self.source, block.source.clone());
        self.blocks = markdown_blocks(&self.source);
        self.sent = self.source.clone();
        cx.emit(MarkdownEditorEvent::Changed(self.source.clone()));
        cx.notify();
    }

    /// The open field: a code block's in the code frame with its label, any other bare, with
    /// "Type to filter…" over an empty block the "+" opened.
    fn field(&self, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editing = self.editing.as_ref()?;
        if editing.fence.is_some() {
            let group = SharedString::from("md-code-field");
            return Some(
                code_frame(theme)
                    .relative()
                    .group(group.clone())
                    .child(editing.input.clone())
                    .child(self.copy_label(group, CopySource::Field, theme, cx))
                    .into_any_element(),
            );
        }
        let ghost = self
            .slash
            .as_ref()
            .is_some_and(|slash| slash.inserted && slash.range.is_empty());
        Some(
            div()
                .relative()
                .w_full()
                .rounded(px(theme.radius_control))
                .child(editing.input.clone())
                .when(ghost, |field| field.child(filter_ghost(theme)))
                .into_any_element(),
        )
    }

    /// `pre::after`: "Copy", or "Copied" for a while after a copy, 6px from the top and 10px
    /// from the end of a code block under the pointer; a press in the block's 64 by 32 corner
    /// copies it.
    fn copy_label(
        &self,
        group: SharedString,
        source: CopySource,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = if self.copied { "Copied" } else { "Copy" };
        div()
            .id(ElementId::Name(format!("{group}-copy").into()))
            .debug_selector(|| "md-copy-corner".into())
            .absolute()
            .top_0()
            .right_0()
            .w(px(COPY_CORNER.0))
            .h(px(COPY_CORNER.1))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    let text = match &source {
                        CopySource::Code(text) => text.clone(),
                        CopySource::Field => this
                            .editing
                            .as_ref()
                            .map(|editing| editing.input.read(cx).text().to_string())
                            .unwrap_or_default(),
                    };
                    this.copy(text, cx);
                }),
            )
            .child(
                div()
                    .absolute()
                    .top(px(6.0))
                    .right(px(10.0))
                    .px(px(8.0))
                    .py(px(2.0))
                    .rounded_full()
                    .bg(theme.color_surface_subtle)
                    .font_family(theme.font_sans)
                    .text_size(px(11.0))
                    .line_height(px(LINE))
                    .text_color(theme.color_text_secondary)
                    .whitespace_nowrap()
                    .opacity(0.0)
                    .group_hover(group, |style| style.opacity(1.0))
                    .debug_selector(move || {
                        if label == "Copied" {
                            "md-copied".into()
                        } else {
                            "md-copy".into()
                        }
                    })
                    .child(label),
            )
            .into_any_element()
    }

    /// The gutter's "+" (`.pw-md-editor__add`), 40px before the text and centered on the
    /// block's first line: a 28px round tool on the subtle surface in muted ink, the hover
    /// surface and primary ink under the pointer.
    fn add_button(
        &self,
        index: usize,
        top: f32,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let group = SharedString::from(format!("md-add-{index}"));
        div()
            .id(("md-add", index))
            .tooltip(crate::tooltip::tooltip(
                if self
                    .blocks
                    .get(index)
                    .is_some_and(|block| matches!(block.kind, BlockKind::Item { .. }))
                {
                    "Insert item below"
                } else {
                    "Insert block below"
                },
            ))
            .group(group.clone())
            .absolute()
            .left_0()
            .top(px(top))
            .size(px(TOOL))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(theme.color_surface_subtle)
            .hover(|style| style.bg(theme.color_surface_hover))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.insert_below(index, window, cx);
            }))
            .child(
                svg()
                    .path(Icon::Plus.asset_path())
                    .size(px(14.0))
                    .flex_none()
                    .text_color(theme.color_text_muted)
                    .group_hover(group, |style| style.text_color(theme.color_text_primary)),
            )
            .into_any_element()
    }

    /// The slash menu (`.pw-md-editor__slash`) under the caret at `anchor`: 240px wide and at
    /// most 300px tall, its rows under the Text, Lists, and Insert headings (11px medium muted)
    /// with the picked one lit, "No matching blocks" when none matches, and "Close menu" with
    /// its `esc` pinned below the list. It flips above the caret when there is no room below.
    fn slash_menu(
        &self,
        anchor: Bounds<Pixels>,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let slash = self.slash.as_ref()?;
        let rows = slash_rows(&slash.query);
        let mut list = div()
            .id("md-slash-list")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .track_scroll(&self.slash_scroll)
            .pt(px(tw(1.5)))
            .px(px(tw(1.5)))
            .flex()
            .flex_col();
        let mut first = true;
        for (section, heading) in SECTIONS.iter().enumerate() {
            let items: Vec<(usize, Tool)> = rows
                .iter()
                .copied()
                .enumerate()
                .filter(|(_, tool)| tool.section() == section)
                .collect();
            if items.is_empty() {
                continue;
            }
            let mut group = div()
                .flex()
                .flex_col()
                .when(!first, |group| {
                    group
                        .mt(px(6.0))
                        .pt(px(6.0))
                        .border_t_1()
                        .border_color(theme.color_border_subtle)
                })
                .child(
                    div()
                        .pt(px(4.0))
                        .px(px(10.0))
                        .pb(px(6.0))
                        .text_size(px(11.0))
                        .line_height(px(SLASH_LABEL_LINE))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.color_text_muted)
                        .child(*heading),
                );
            first = false;
            for (index, tool) in items {
                let lit = index == slash.row;
                let ink = if lit {
                    theme.color_text_primary
                } else {
                    theme.color_text_secondary
                };
                group = group.child(
                    div()
                        .id(("md-slash-row", index))
                        .flex()
                        .items_center()
                        .gap(px(theme.space_2))
                        .px(px(tw(2.5)))
                        .py(px(tw(2.0)))
                        .rounded(px(theme.radius_item))
                        .cursor_pointer()
                        .text_color(ink)
                        .when(lit, |row| row.bg(theme.color_surface_hover))
                        .on_mouse_move(cx.listener(move |this, _, _, cx| {
                            if let Some(slash) = this.slash.as_mut()
                                && slash.row != index
                            {
                                slash.row = index;
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.choose(tool, window, cx);
                        }))
                        .child(icon_colored(tool.icon(), 14.0, ink))
                        .child(tool.label()),
                );
            }
            list = list.child(group);
        }
        if rows.is_empty() {
            list = list.child(
                div()
                    .p(px(12.0))
                    .text_color(theme.color_text_muted)
                    .child("No matching blocks"),
            );
        }
        let close = div()
            .id("md-slash-close")
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px(px(16.0))
            .py(px(12.0))
            .border_t_1()
            .border_color(theme.color_border_subtle)
            .text_size(px(13.0))
            .line_height(px(13.0 * 1.5))
            .text_color(theme.color_text_primary)
            .cursor_pointer()
            .on_click(cx.listener(|this, _, window, cx| this.close_slash(window, cx)))
            .child("Close menu")
            .child(div().text_color(theme.color_text_muted).child("esc"));
        let panel = floating_panel(
            div()
                .relative()
                .w(px(SLASH_WIDTH))
                .max_h(px(SLASH_MAX))
                .flex()
                .flex_col()
                .overflow_hidden()
                .rounded(px(theme.radius_menu))
                .bg(theme.color_surface_subtle)
                .text_size(px(12.0))
                .line_height(px(18.0))
                .text_color(theme.color_text_primary)
                .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    if !this.in_field(event.position, cx) {
                        this.close_slash(window, cx);
                    }
                }))
                .child(list)
                .child(close),
            theme.radius_menu,
            theme,
        );
        let height = slash_height(&rows);
        let below = anchor.bottom() + px(POPUP_GAP);
        let room = window.viewport_size().height - px(POPUP_EDGE);
        let top = if below + px(height) > room
            && anchor.top() - px(POPUP_GAP + height) >= px(POPUP_EDGE)
        {
            anchor.top() - px(POPUP_GAP + height)
        } else {
            below
        };
        Some(
            deferred(
                anchored()
                    .position(point(anchor.left(), top))
                    .snap_to_window_with_margin(px(POPUP_EDGE))
                    .child(panel),
            )
            .with_priority(6)
            .into_any_element(),
        )
    }

    /// The selection toolbar (`.pw-md-editor__toolbar`) over the selection at `anchor`: Bold
    /// and Italic, H1 to H3 with the bullet list and the checklist, then the code block and
    /// the quote, each 28px round, the groups set apart by a subtle rule. A tool lights for
    /// the selection it already applies to. It sits 8px above the selection, centered on it,
    /// or below when there is no room above.
    fn toolbar_menu(
        &self,
        anchor: Bounds<Pixels>,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let editing = self.editing.as_ref()?;
        let selection = self.toolbar.clone()?;
        let text = editing.input.read(cx).text().to_string();
        let code = editing.fence.is_some();
        let shape = shape_of(&text).0;
        let mut bar = div()
            .debug_selector(|| "md-toolbar".into())
            .occlude()
            .flex()
            .items_center()
            .gap(px(TOOLBAR_GAP))
            .py(px(3.0))
            .px(px(4.0))
            .rounded(px(theme.radius_menu))
            .bg(theme.color_surface_subtle)
            .shadow(vec![material::drop(1.0, 2.0, theme.color_shadow_subtle)])
            .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                if !this.in_field(event.position, cx) {
                    this.dismissed_toolbar = this.toolbar.clone();
                    cx.notify();
                }
            }));
        for (index, tools) in TOOLBAR_GROUPS.iter().enumerate() {
            let mut group = div()
                .flex()
                .items_center()
                .gap(px(1.0))
                .when(index > 0, |group| {
                    group
                        .pl(px(6.0))
                        .ml(px(2.0))
                        .border_l_1()
                        .border_color(theme.color_border_subtle)
                });
            for tool in tools.iter().copied() {
                let active = match tool {
                    Tool::Bold => !code && has_mark(&text, selection.clone(), Mark::Bold),
                    Tool::Italic => !code && has_mark(&text, selection.clone(), Mark::Italic),
                    Tool::Heading(level) => !code && shape == Shape::Heading(level),
                    Tool::Bullet => !code && shape == Shape::Bullet,
                    Tool::Task => !code && shape == Shape::Task,
                    Tool::Quote => !code && shape == Shape::Quote,
                    Tool::Code => code,
                    _ => false,
                };
                group = group.child(tool_button(tool, active, theme, cx));
            }
            bar = bar.child(group);
        }
        let width = toolbar_width();
        let above = anchor.top() - px(POPUP_GAP + TOOLBAR_HEIGHT);
        let top = if above < px(POPUP_EDGE) {
            anchor.bottom() + px(POPUP_GAP)
        } else {
            above
        };
        let left = anchor.center().x - px(width / 2.0);
        Some(
            deferred(
                anchored()
                    .position(point(left, top))
                    .snap_to_window_with_margin(px(POPUP_EDGE))
                    .child(bar),
            )
            .with_priority(6)
            .into_any_element(),
        )
    }

    /// Whether `position` is on the open field, where a press moves the caret and leaves the
    /// menu and the toolbar to the selection.
    fn in_field(&self, position: Point<Pixels>, cx: &App) -> bool {
        self.editing
            .as_ref()
            .and_then(|editing| editing.input.read(cx).bounds())
            .is_some_and(|bounds| bounds.contains(&position))
    }

    /// The range the slash menu or the toolbar hangs from: the caret, or the selection.
    fn overlay_range(&self) -> Option<Range<usize>> {
        let editing = self.editing.as_ref()?;
        if self.slash.is_some() {
            return Some(editing.selection.end..editing.selection.end);
        }
        self.toolbar_shown().then(|| editing.selection.clone())
    }

    /// One block in its row: the row reaches into the gutter, where the pointer finds the
    /// block and its "+"; `body` fills with the hover surface while the block is under the
    /// pointer.
    fn row(
        &self,
        index: Option<usize>,
        space: f32,
        line: f32,
        body: AnyElement,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let editable = self.editable;
        let hovered = editable && index.is_some() && self.hovered == index;
        let mut row = div()
            .id(("md-row", index.unwrap_or(usize::MAX)))
            .relative()
            .ml(px(-GUTTER))
            .pl(px(GUTTER))
            .pt(px(space))
            .child(
                div()
                    .mx(px(-HOVER_BLEED))
                    .px(px(HOVER_BLEED))
                    .my(px(-HOVER_RISE))
                    .py(px(HOVER_RISE))
                    .rounded(px(theme.radius_control))
                    .when(hovered, |fill| fill.bg(theme.color_surface_hover))
                    .child(body),
            );
        if let (true, Some(index)) = (editable, index) {
            row = row.on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered && this.hovered != Some(index) {
                    this.hovered = Some(index);
                    cx.notify();
                } else if !*hovered && this.hovered == Some(index) {
                    this.hovered = None;
                    cx.notify();
                }
            }));
            if hovered && self.slash.is_none() {
                row = row.child(self.add_button(index, space + (line - TOOL) / 2.0, theme, cx));
            }
        }
        row.into_any_element()
    }
}

/// `offset` once the source changed at `at` by `delta`.
fn shift(offset: usize, change: Option<(usize, isize)>) -> usize {
    match change {
        Some((at, delta)) if offset >= at => (offset as isize + delta).max(0) as usize,
        _ => offset,
    }
}

/// The slash menu's height for `rows`, as it lays them out, up to its 300px.
fn slash_height(rows: &[Tool]) -> f32 {
    let row = 18.0 + 2.0 * tw(2.0);
    let label = 4.0 + SLASH_LABEL_LINE + 6.0;
    let mut sections: Vec<usize> = rows.iter().map(|tool| tool.section()).collect();
    sections.dedup();
    let list = if rows.is_empty() {
        24.0 + 18.0
    } else {
        sections.len() as f32 * label
            + rows.len() as f32 * row
            + sections.len().saturating_sub(1) as f32 * 13.0
    };
    let close = 1.0 + 24.0 + 13.0 * 1.5;
    (tw(1.5) + list + close).min(SLASH_MAX)
}

/// The toolbar's width: its padding, its tools, the 4px between groups, and each later
/// group's rule with the room around it.
fn toolbar_width() -> f32 {
    let tools: usize = TOOLBAR_GROUPS.iter().map(|group| group.len()).sum();
    let gaps: usize = TOOLBAR_GROUPS.iter().map(|group| group.len() - 1).sum();
    let groups = TOOLBAR_GROUPS.len() as f32;
    8.0 + tools as f32 * TOOL + gaps as f32 + (groups - 1.0) * (TOOLBAR_GAP + 9.0)
}

/// A toolbar tool (`.pw-md-editor__tool`): its icon, or its `H1`–`H3` in 11px semibold, on
/// the subtle surface in muted ink; the hover surface under the pointer and the active
/// surface while it applies, both in primary ink.
fn tool_button(
    tool: Tool,
    active: bool,
    theme: &Theme,
    cx: &mut Context<MarkdownEditor>,
) -> AnyElement {
    let group = SharedString::from(format!("md-tool-{}", tool.label()));
    let ink = if active {
        theme.color_text_primary
    } else {
        theme.color_text_muted
    };
    let primary = theme.color_text_primary;
    let face: AnyElement = match tool.text() {
        Some(text) => div()
            .text_size(px(11.0))
            .line_height(px(11.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(ink)
            .group_hover(group.clone(), move |style| style.text_color(primary))
            .child(text)
            .into_any_element(),
        None => svg()
            .path(tool.icon().asset_path())
            .size(px(14.0))
            .flex_none()
            .text_color(ink)
            .group_hover(group.clone(), move |style| style.text_color(primary))
            .into_any_element(),
    };
    div()
        .id(ElementId::Name(group.clone()))
        .group(group)
        .size(px(TOOL))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(if active {
            theme.color_surface_active
        } else {
            theme.color_surface_subtle
        })
        .hover(|style| style.bg(theme.color_surface_hover))
        .cursor_pointer()
        .tooltip(tooltip(tool.label()))
        .on_click(cx.listener(move |this, _, window, cx| {
            cx.stop_propagation();
            this.apply_tool(tool, window, cx);
        }))
        .child(face)
        .into_any_element()
}

/// `.pw-md-editor__slash-ghost`: "Type to filter…" where the block the "+" opened waits for
/// its type, on the selected surface 4px before the caret and 2px over its line.
fn filter_ghost(theme: &Theme) -> Div {
    div()
        .absolute()
        .left(px(-4.0))
        .top(px(-2.0))
        .h(px(LINE + 4.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .rounded(px(6.0))
        .bg(theme.color_surface_selected)
        .font_family(theme.font_sans)
        .text_size(px(13.0))
        .font_weight(FontWeight::NORMAL)
        .text_color(theme.color_text_muted)
        .whitespace_nowrap()
        .child("Type to filter…")
}

/// A probe that asks for one more frame when `range` of `input` was drawn somewhere other than
/// `used`, where the menu or the toolbar was hung this frame: the field lays a keystroke out
/// only as it paints, after the editor hung them from the frame before.
fn anchor_probe(
    input: Entity<TextInput>,
    range: Range<usize>,
    used: Option<Bounds<Pixels>>,
) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |_, _, window, cx| {
            if input.read(cx).range_bounds(range) != used {
                window.on_next_frame(|window, _| window.refresh());
            }
        },
    )
    .absolute()
    .size_0()
}

impl Render for MarkdownEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let editable = self.editable;
        let editing_block = match self.editing.as_ref().map(|editing| &editing.target) {
            Some(Target::Block(index)) => Some(*index),
            _ => None,
        };
        let insert_after = match self.editing.as_ref().map(|editing| &editing.target) {
            Some(Target::Insert { at, .. }) => Some(
                self.blocks
                    .iter()
                    .rposition(|block| block.source.end <= *at),
            ),
            _ => None,
        };
        let mut root = document(&theme).when(editable, |root| {
            root.min_h(px(EDITABLE_MIN)).pr(px(GUTTER_END))
        });
        let mut previous: Option<Flow> = None;
        let insert_flow = |previous: Option<Flow>| match previous {
            Some(Flow::Item(list, task))
                if self.editing.as_ref().is_some_and(|editing| {
                    matches!(editing.target, Target::Insert { before: "\n", .. })
                }) =>
            {
                Flow::Item(list, task)
            }
            _ => Flow::Text,
        };
        if insert_after == Some(None) {
            let flow = insert_flow(None);
            if let Some(field) = self.field(&theme, cx) {
                root = root.child(self.row(None, gap(None, flow), LINE, field, &theme, cx));
            }
            previous = Some(flow);
        }
        for index in 0..self.blocks.len() {
            let block = &self.blocks[index];
            let flow = Flow::of(&block.kind);
            let space = gap(previous, flow);
            let line = block_type(&block.kind).1;
            let body = if editing_block == Some(index) {
                self.field(&theme, cx)
                    .unwrap_or_else(|| div().into_any_element())
            } else {
                let on_task = (editable
                    && matches!(
                        block.kind,
                        BlockKind::Item {
                            marker: Marker::Task(_),
                            ..
                        }
                    ))
                .then(|| {
                    let editor = cx.weak_entity();
                    std::rc::Rc::new(move |_: &mut Window, cx: &mut App| {
                        let _ = editor.update(cx, |this, cx| this.toggle_task(index, cx));
                    }) as crate::Handler
                });
                let mut element = block_element(block, on_task, &theme);
                if let BlockKind::Code { text, .. } = &block.kind {
                    let group = SharedString::from(format!("md-code-{index}"));
                    let source = CopySource::Code(text.clone());
                    element = div()
                        .relative()
                        .group(group.clone())
                        .child(element)
                        .child(self.copy_label(group, source, &theme, cx))
                        .into_any_element();
                }
                if editable && self.opening(index).is_some() {
                    div()
                        .id(("md-block", index))
                        .cursor(CursorStyle::IBeam)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.edit_block(index, window, cx);
                        }))
                        .child(element)
                        .into_any_element()
                } else {
                    element
                }
            };
            root = root.child(self.row(Some(index), space, line, body, &theme, cx));
            previous = Some(flow);
            if insert_after == Some(Some(index)) {
                let flow = insert_flow(previous);
                let space = gap(previous, flow);
                if let Some(field) = self.field(&theme, cx) {
                    root = root.child(self.row(None, space, LINE, field, &theme, cx));
                }
                previous = Some(flow);
            }
        }
        if let Some(last) = previous {
            root = root.pb(px(last.bottom()));
        }
        if self.blocks.is_empty() && self.editing.is_none() && editable {
            root = root.child(
                div()
                    .text_color(theme.color_text_muted)
                    .child(self.placeholder.clone()),
            );
        }
        if editable {
            // The room under the text opens a new block at the end.
            root = root.child(
                div()
                    .id("md-end")
                    .w_full()
                    .flex_1()
                    .min_h(px(LINE))
                    .cursor(CursorStyle::IBeam)
                    .on_click(cx.listener(|this, _, window, cx| this.edit_end(window, cx))),
            );
        }
        // The slash menu or the toolbar, hung from where the field drew the caret or the
        // selection in the last frame.
        if let (Some(range), Some(input)) = (
            self.overlay_range().filter(|_| editable),
            self.editing.as_ref().map(|editing| editing.input.clone()),
        ) {
            let anchor = input.read(cx).range_bounds(range.clone());
            if let Some(anchor) = anchor {
                let menu = if self.slash.is_some() {
                    self.slash_menu(anchor, window, &theme, cx)
                } else {
                    self.toolbar_menu(anchor, &theme, cx)
                };
                root = root.children(menu);
            }
            root = root.child(anchor_probe(input, range, anchor));
        }
        root
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{Entity, TestAppContext, size};

    use super::*;
    use crate::test_support::{draw, open};

    fn kinds(source: &str) -> Vec<BlockKind> {
        markdown_blocks(source)
            .into_iter()
            .map(|block| block.kind)
            .collect()
    }

    fn plain(text: &str) -> Vec<Span> {
        vec![Span {
            text: text.into(),
            ..Span::default()
        }]
    }

    #[test]
    fn blocks_keep_their_source_and_marks() {
        let source = "# Title\n\nSome **bold** and `code` and [a link](https://x.y).\n\n---\n";
        let blocks = markdown_blocks(source);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].kind, BlockKind::Heading(1, plain("Title")));
        assert_eq!(&source[blocks[0].source.clone()], "# Title");
        let BlockKind::Paragraph(spans) = &blocks[1].kind else {
            panic!("a paragraph");
        };
        assert!(spans[1].strong && spans[1].text == "bold");
        assert!(spans[3].code && spans[3].text == "code");
        assert_eq!(spans[5].link.as_deref(), Some("https://x.y"));
        assert_eq!(blocks[2].kind, BlockKind::Rule);
        assert_eq!(&source[blocks[2].source.clone()], "---");
    }

    #[test]
    fn a_list_gives_each_item_its_own_text_number_and_depth() {
        let source = "3. three\n4. four\n   - nested\n     - deeper\n\n- [x] done\n- [ ] open\n";
        let blocks = markdown_blocks(source);
        let items: Vec<(Marker, usize, usize, String)> = blocks
            .iter()
            .map(|block| match &block.kind {
                BlockKind::Item {
                    marker,
                    depth,
                    list,
                    text,
                } => (*marker, *depth, *list, text[0].text.clone()),
                other => panic!("an item, not {other:?}"),
            })
            .collect();
        assert_eq!(
            items,
            [
                (Marker::Number(3), 0, 1, "three".into()),
                (Marker::Number(4), 0, 1, "four".into()),
                (Marker::Bullet, 1, 1, "nested".into()),
                (Marker::Bullet, 2, 1, "deeper".into()),
                (Marker::Task(true), 0, 2, "done".into()),
                (Marker::Task(false), 0, 2, "open".into()),
            ]
        );
        // The parent item's source stops where its nested list starts.
        assert_eq!(&source[blocks[1].source.clone()], "4. four");
        assert_eq!(&source[blocks[4].source.clone()], "- [x] done");
    }

    #[test]
    fn quotes_and_code_stay_whole() {
        assert_eq!(
            kinds("> one\n> two\n>\n> three\n\n```rust\nfn a() {}\n\nb\n```\n"),
            [
                BlockKind::Quote(vec![plain("one two"), plain("three")]),
                BlockKind::Code {
                    language: "rust".into(),
                    text: "fn a() {}\n\nb".into(),
                },
            ]
        );
    }

    #[test]
    fn margins_collapse_as_css_does() {
        assert_eq!(gap(None, Flow::Text), 0.0);
        assert_eq!(gap(None, Flow::Heading), HEADING_TOP);
        assert_eq!(gap(Some(Flow::Text), Flow::Heading), HEADING_TOP);
        assert_eq!(gap(Some(Flow::Heading), Flow::Text), HEADING_BOTTOM);
        assert_eq!(
            gap(Some(Flow::Item(1, true)), Flow::Item(1, true)),
            ITEM_GAP
        );
        assert_eq!(
            gap(Some(Flow::Item(1, false)), Flow::Item(1, false)),
            BLOCK_GAP
        );
        assert_eq!(
            gap(Some(Flow::Item(1, true)), Flow::Item(2, true)),
            BLOCK_GAP
        );
        assert_eq!(gap(Some(Flow::Text), Flow::Rule), RULE_MARGIN);
    }

    #[test]
    fn a_field_edits_a_block_and_the_rest_keeps_its_spelling() {
        let source = "Intro that\nwraps.\n\n* item  \n\nEnd";
        let blocks = markdown_blocks(source);
        assert_eq!(
            block_edit_text(source, &blocks[0]).as_deref(),
            Some("Intro that wraps.")
        );
        let next = replace_block(source, blocks[0].source.clone(), "Intro.");
        assert_eq!(next, "Intro.\n\n* item  \n\nEnd");
        // Emptied, the block goes with the blank lines after it, or before it at the end.
        assert_eq!(replace_block(&next, 0..6, ""), "* item  \n\nEnd");
        let last = markdown_blocks(&next).last().unwrap().source.clone();
        assert_eq!(replace_block(&next, last, " "), "Intro.\n\n* item");
        // A code block's field holds its code between the fences, not this text.
        let code = markdown_blocks("```\na\n```");
        assert_eq!(block_edit_text("```\na\n```", &code[0]), None);
        // A setext heading opens as an ATX one.
        let setext = markdown_blocks("Title\n=====");
        assert_eq!(
            block_edit_text("Title\n=====", &setext[0]).as_deref(),
            Some("# Title")
        );
    }

    #[test]
    fn enter_after_an_item_continues_the_list() {
        assert_eq!(continue_item("- a").as_deref(), Some("- "));
        assert_eq!(continue_item("  * [x] a").as_deref(), Some("  * [ ] "));
        assert_eq!(continue_item("9) a").as_deref(), Some("10) "));
        assert_eq!(continue_item("Plain text"), None);
        assert_eq!(continue_item("-not a list"), None);
        assert!(is_blank("- ") && is_blank("12. ") && is_blank("## ") && is_blank("- [ ] "));
        assert!(!is_blank("- a") && !is_blank("1. b"));
        assert_eq!(toggle_task("- [ ] a\n- [x] b", 0..7), "- [x] a\n- [x] b");
        assert_eq!(toggle_task("- [ ] a\n- [x] b", 8..15), "- [ ] a\n- [ ] b");
    }

    #[gpui::test]
    fn a_click_opens_a_block_and_enter_writes_it_and_opens_the_next(cx: &mut TestAppContext) {
        let editor = cx.update(|cx| cx.new(|cx| MarkdownEditor::new("First.\n\n- one", cx)));
        let changes = Rc::new(RefCell::new(Vec::<String>::new()));
        cx.update(|cx| {
            let changes = changes.clone();
            cx.subscribe(&editor, move |_, event: &MarkdownEditorEvent, _| {
                let MarkdownEditorEvent::Changed(source) = event;
                changes.borrow_mut().push(source.clone());
            })
            .detach();
        });
        let view = editor.clone();
        let mut cx = open(cx, size(600.0, 400.0), move |_, _| {
            view.clone().into_any_element()
        });
        cx.update(|window, cx| editor.update(cx, |editor, cx| editor.edit_block(1, window, cx)));
        draw(&mut cx);
        assert!(editor.read_with(&cx, |editor, _| editor.is_editing()));
        cx.simulate_input(" more");
        cx.simulate_keystrokes("enter");
        draw(&mut cx);
        // The item is written, and the next one opens with its marker.
        assert_eq!(
            editor.read_with(&cx, |editor, _| editor.source().to_string()),
            "First.\n\n- one more"
        );
        cx.simulate_input("two");
        cx.simulate_keystrokes("escape");
        draw(&mut cx);
        assert_eq!(
            editor.read_with(&cx, |editor, _| editor.source().to_string()),
            "First.\n\n- one more\n- two"
        );
        assert!(!editor.read_with(&cx, |editor, _| editor.is_editing()));
        assert_eq!(
            changes.borrow().last().map(String::as_str),
            Some("First.\n\n- one more\n- two")
        );
    }

    /// An editor on `source` in a window, 100px down, so a toolbar has room above it.
    fn editor_on(
        source: &str,
        cx: &mut TestAppContext,
    ) -> (Entity<MarkdownEditor>, gpui::VisualTestContext) {
        let source = source.to_string();
        let editor = cx.update(|cx| cx.new(|cx| MarkdownEditor::new(source, cx)));
        let view = editor.clone();
        let cx = open(cx, size(600.0, 400.0), move |_, _| {
            div().pt(px(100.0)).child(view.clone()).into_any_element()
        });
        (editor, cx)
    }

    fn field_text(editor: &Entity<MarkdownEditor>, cx: &gpui::VisualTestContext) -> String {
        editor.read_with(cx, |editor, cx| {
            editor
                .editing
                .as_ref()
                .map(|editing| editing.input.read(cx).text().to_string())
                .unwrap_or_default()
        })
    }

    fn source_of(editor: &Entity<MarkdownEditor>, cx: &gpui::VisualTestContext) -> String {
        editor.read_with(cx, |editor, _| editor.source().to_string())
    }

    #[gpui::test]
    fn a_slash_opens_the_menu_and_its_row_turns_the_block(cx: &mut TestAppContext) {
        let (editor, mut cx) = editor_on("First.", cx);
        cx.update(|window, cx| editor.update(cx, |editor, cx| editor.edit_end(window, cx)));
        draw(&mut cx);
        cx.simulate_input("/head");
        draw(&mut cx);
        let query = editor.read_with(&cx, |editor, _| editor.slash.clone().map(|s| s.query));
        assert_eq!(query.as_deref(), Some("head"));
        // ↓ lights Heading 2, Enter picks it, and the field turns into the heading.
        cx.simulate_keystrokes("down enter");
        draw(&mut cx);
        assert_eq!(field_text(&editor, &cx), "## ");
        assert!(editor.read_with(&cx, |editor, _| editor.slash.is_none()));
        cx.simulate_input("Plan");
        cx.simulate_keystrokes("escape");
        draw(&mut cx);
        assert_eq!(source_of(&editor, &cx), "First.\n\n## Plan");

        // Escape closes the menu for that `/` and leaves the field open.
        cx.update(|window, cx| editor.update(cx, |editor, cx| editor.edit_end(window, cx)));
        draw(&mut cx);
        cx.simulate_input("/quo");
        draw(&mut cx);
        cx.simulate_keystrokes("escape");
        cx.simulate_input("t");
        draw(&mut cx);
        assert!(editor.read_with(&cx, |editor, _| editor.slash.is_none()));
        assert_eq!(field_text(&editor, &cx), "/quot");
    }

    #[gpui::test]
    fn the_toolbar_marks_the_selection_and_turns_the_block(cx: &mut TestAppContext) {
        let (editor, mut cx) = editor_on("make it bold", cx);
        cx.update(|window, cx| editor.update(cx, |editor, cx| editor.edit_block(0, window, cx)));
        draw(&mut cx);
        let input = editor.read_with(&cx, |editor, _| {
            editor.editing.as_ref().unwrap().input.clone()
        });
        input.update(&mut cx, |input, cx| input.select(8..12, cx));
        draw(&mut cx);
        assert!(editor.read_with(&cx, |editor, _| editor.toolbar_shown()));
        draw(&mut cx);
        let bar = cx.debug_bounds("md-toolbar").expect("the toolbar draws");
        let line = input
            .read_with(&cx, |input, _| input.range_bounds(8..12))
            .expect("the field drew");
        assert_eq!(bar.bottom(), line.top() - px(POPUP_GAP));
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| editor.apply_tool(Tool::Bold, window, cx))
        });
        draw(&mut cx);
        assert_eq!(field_text(&editor, &cx), "make it **bold**");
        assert_eq!(input.read_with(&cx, |input, _| input.selection()), 10..14);
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.apply_tool(Tool::Heading(1), window, cx)
            })
        });
        draw(&mut cx);
        assert_eq!(field_text(&editor, &cx), "# make it **bold**");
        // Escape closes the toolbar first, then writes the block.
        cx.simulate_keystrokes("escape");
        draw(&mut cx);
        assert!(editor.read_with(&cx, |editor, _| editor.is_editing()
            && !editor.toolbar_shown()));
        cx.simulate_keystrokes("escape");
        draw(&mut cx);
        assert_eq!(source_of(&editor, &cx), "# make it **bold**");
    }

    #[gpui::test]
    fn code_selection_has_tools_but_excludes_marks(cx: &mut TestAppContext) {
        for (tool, expected) in [
            (Tool::Bold, "plain code"),
            (Tool::Italic, "plain code"),
            (Tool::Heading(2), "## plain code"),
            (Tool::Bullet, "- plain code"),
            (Tool::Task, "- [ ] plain code"),
            (Tool::Quote, "> plain code"),
            (Tool::Code, "plain code"),
        ] {
            let (editor, mut cx) = editor_on("```rs\nplain code\n```", cx);
            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    editor.edit_block(0, window, cx);
                    editor.select(0..5, cx);
                })
            });
            draw(&mut cx);
            assert!(editor.read_with(&cx, |editor, _| editor.toolbar_shown()));
            assert!(cx.debug_bounds("md-toolbar").is_some());
            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| editor.apply_tool(tool, window, cx))
            });
            draw(&mut cx);
            assert_eq!(field_text(&editor, &cx), expected);
            let code = editor.read_with(&cx, |editor, _| {
                editor.editing.as_ref().unwrap().fence.is_some()
            });
            assert_eq!(code, matches!(tool, Tool::Bold | Tool::Italic));
        }
    }

    #[gpui::test]
    fn readonly_code_copies_with_the_editors_feedback(cx: &mut TestAppContext) {
        let mut cx = open(cx, size(600.0, 400.0), |_, cx| {
            markdown_view("```rs\nlet answer = 42;\n```", cx).into_any_element()
        });
        draw(&mut cx);
        let corner = cx
            .debug_bounds("md-copy-corner")
            .expect("read-only code has Copy");
        cx.simulate_mouse_down(
            corner.center(),
            MouseButton::Left,
            gpui::Modifiers::default(),
        );
        draw(&mut cx);
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().unwrap().text()),
            Some("let answer = 42;".to_string())
        );
        assert!(cx.debug_bounds("md-copied").is_some());
    }

    #[gpui::test]
    fn copied_feedback_lasts_one_and_a_half_seconds(cx: &mut TestAppContext) {
        let (editor, mut cx) = editor_on("```\ncode\n```", cx);
        editor.update(&mut cx, |editor, cx| editor.copy("code".into(), cx));
        cx.run_until_parked();
        cx.executor()
            .advance_clock(COPIED_FOR - Duration::from_millis(1));
        cx.run_until_parked();
        assert!(editor.read_with(&cx, |editor, _| editor.copied));
        cx.executor().advance_clock(Duration::from_millis(11));
        cx.run_until_parked();
        assert!(!editor.read_with(&cx, |editor, _| editor.copied));
    }

    #[gpui::test]
    fn a_code_block_is_edited_between_its_fences_and_left_by_enter(cx: &mut TestAppContext) {
        let (editor, mut cx) = editor_on("Intro\n\n```go\nfunc a() {}\n```", cx);
        cx.update(|window, cx| editor.update(cx, |editor, cx| editor.edit_block(1, window, cx)));
        draw(&mut cx);
        assert_eq!(field_text(&editor, &cx), "func a() {}");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("b()");
        cx.simulate_keystrokes("escape");
        draw(&mut cx);
        assert_eq!(
            source_of(&editor, &cx),
            "Intro\n\n```go\nfunc a() {}\nb()\n```"
        );
        // Two empty lines at its end, and Enter opens a paragraph after it.
        cx.update(|window, cx| editor.update(cx, |editor, cx| editor.edit_block(1, window, cx)));
        draw(&mut cx);
        cx.simulate_keystrokes("enter enter enter");
        draw(&mut cx);
        cx.simulate_input("Next");
        cx.simulate_keystrokes("escape");
        draw(&mut cx);
        assert_eq!(
            source_of(&editor, &cx),
            "Intro\n\n```go\nfunc a() {}\nb()\n```\n\nNext"
        );
    }

    #[gpui::test]
    fn the_gutters_plus_opens_a_block_or_an_item_below(app: &mut TestAppContext) {
        let (editor, mut cx) = editor_on("One.\n\nTwo.", app);
        cx.update(|window, cx| editor.update(cx, |editor, cx| editor.insert_below(0, window, cx)));
        draw(&mut cx);
        let inserted = editor.read_with(&cx, |editor, _| editor.slash.clone().map(|s| s.inserted));
        assert_eq!(inserted, Some(true));
        cx.simulate_input("quo");
        draw(&mut cx);
        cx.simulate_keystrokes("enter");
        draw(&mut cx);
        assert_eq!(field_text(&editor, &cx), "> ");
        cx.simulate_input("said");
        cx.simulate_keystrokes("escape");
        draw(&mut cx);
        assert_eq!(source_of(&editor, &cx), "One.\n\n> said\n\nTwo.");

        let (editor, mut cx) = editor_on("- a\n  - nested\n- b", app);
        cx.update(|window, cx| editor.update(cx, |editor, cx| editor.insert_below(0, window, cx)));
        draw(&mut cx);
        assert_eq!(field_text(&editor, &cx), "- ");
        cx.simulate_input("c");
        cx.simulate_keystrokes("escape");
        draw(&mut cx);
        assert_eq!(source_of(&editor, &cx), "- a\n  - nested\n- c\n- b");
    }

    #[gpui::test]
    fn a_divider_goes_after_the_block_and_a_paragraph_after_it(cx: &mut TestAppContext) {
        let (editor, mut cx) = editor_on("Text", cx);
        cx.update(|window, cx| editor.update(cx, |editor, cx| editor.edit_end(window, cx)));
        draw(&mut cx);
        cx.simulate_input("/divider");
        draw(&mut cx);
        cx.simulate_keystrokes("enter");
        draw(&mut cx);
        assert_eq!(source_of(&editor, &cx), "Text\n\n---");
        cx.simulate_input("After");
        cx.simulate_keystrokes("escape");
        draw(&mut cx);
        assert_eq!(source_of(&editor, &cx), "Text\n\n---\n\nAfter");
        assert_eq!(insert_rule("A\n\nB", 1), ("A\n\n---\n\nB".to_string(), 6));
    }
}
