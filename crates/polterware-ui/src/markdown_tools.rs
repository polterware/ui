//! The Markdown editor's block and mark tools as edits of one block's Markdown: the slash
//! menu's rows and the selection toolbar's buttons (`BLOCK_ACTIONS` and `SELECTION_ACTIONS` in
//! the design system's `markdown-editor.tsx`), the shape a field's text gives its block, and
//! the slash query typed into it. Bold writes `**` and italic `_`, as Turndown does.

use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag};

use crate::icons::Icon;

/// A tool of the slash menu or the selection toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Bold,
    Italic,
    Paragraph,
    Heading(u8),
    Bullet,
    Numbered,
    Task,
    Code,
    Quote,
    Divider,
}

/// The slash menu's rows, in order (`BLOCK_ACTIONS`).
pub const BLOCK_TOOLS: [Tool; 10] = [
    Tool::Paragraph,
    Tool::Heading(1),
    Tool::Heading(2),
    Tool::Heading(3),
    Tool::Bullet,
    Tool::Numbered,
    Tool::Task,
    Tool::Code,
    Tool::Quote,
    Tool::Divider,
];

/// The selection toolbar's groups (`format`, `structure`, `insert`), without the numbered list
/// and the divider (`SELECTION_ACTIONS`).
pub const TOOLBAR_GROUPS: [&[Tool]; 3] = [
    &[Tool::Bold, Tool::Italic],
    &[
        Tool::Heading(1),
        Tool::Heading(2),
        Tool::Heading(3),
        Tool::Bullet,
        Tool::Task,
    ],
    &[Tool::Code, Tool::Quote],
];

/// The slash menu's section headings, each with the rows under it.
pub const SECTIONS: [&str; 3] = ["Text", "Lists", "Insert"];

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Tool::Bold => "Bold",
            Tool::Italic => "Italic",
            Tool::Paragraph => "Paragraph",
            Tool::Heading(1) => "Heading 1",
            Tool::Heading(2) => "Heading 2",
            Tool::Heading(_) => "Heading 3",
            Tool::Bullet => "Bullet list",
            Tool::Numbered => "Numbered list",
            Tool::Task => "Checklist",
            Tool::Code => "Code block",
            Tool::Quote => "Quote",
            Tool::Divider => "Divider",
        }
    }

    /// The action's id, which the slash query matches as well as its label.
    fn id(self) -> &'static str {
        match self {
            Tool::Bold => "bold",
            Tool::Italic => "italic",
            Tool::Paragraph => "paragraph",
            Tool::Heading(1) => "h1",
            Tool::Heading(2) => "h2",
            Tool::Heading(_) => "h3",
            Tool::Bullet => "bullet",
            Tool::Numbered => "ordered",
            Tool::Task => "task",
            Tool::Code => "code",
            Tool::Quote => "quote",
            Tool::Divider => "hr",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Tool::Bold => Icon::TextBoldIcon,
            Tool::Italic => Icon::TextItalicIcon,
            Tool::Paragraph => Icon::FileText,
            Tool::Heading(_) => Icon::Heading02Icon,
            Tool::Bullet => Icon::LeftToRightListBulletIcon,
            Tool::Numbered => Icon::LeftToRightListNumberIcon,
            Tool::Task => Icon::ListChecks,
            Tool::Code => Icon::SourceCodeIcon,
            Tool::Quote => Icon::QuoteDownIcon,
            Tool::Divider => Icon::MinusSignIcon,
        }
    }

    /// The toolbar's text in place of an icon: `H1` to `H3`.
    pub fn text(self) -> Option<&'static str> {
        match self {
            Tool::Heading(1) => Some("H1"),
            Tool::Heading(2) => Some("H2"),
            Tool::Heading(3) => Some("H3"),
            _ => None,
        }
    }

    /// The slash menu's section the row sits in: Text, Lists, or Insert.
    pub fn section(self) -> usize {
        match self {
            Tool::Bullet | Tool::Numbered | Tool::Task => 1,
            Tool::Code | Tool::Quote | Tool::Divider => 2,
            _ => 0,
        }
    }
}

/// The slash menu's rows for `query`: those whose label or id holds it, ignoring case.
pub fn slash_rows(query: &str) -> Vec<Tool> {
    let query = query.to_lowercase();
    BLOCK_TOOLS
        .into_iter()
        .filter(|tool| {
            format!("{} {}", tool.label(), tool.id())
                .to_lowercase()
                .contains(&query)
        })
        .collect()
}

/// What a block's Markdown makes it, as far as the tools change it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Paragraph,
    Heading(u8),
    Bullet,
    Numbered,
    Task,
    Quote,
}

/// The shape `text` gives its block and the length of its marker with the space after it (and,
/// for a nested item, the indent before it).
pub fn shape_of(text: &str) -> (Shape, usize) {
    let indent = indent_of(text).len();
    match marker_of(&text[indent..]) {
        (Shape::Paragraph, _) => (Shape::Paragraph, 0),
        (shape, marker) => (shape, indent + marker),
    }
}

/// The spaces a nested item's marker stands after.
fn indent_of(text: &str) -> &str {
    &text[..text.len() - text.trim_start_matches(' ').len()]
}

/// Whether a block of `shape` is a list item.
fn is_list(shape: Shape) -> bool {
    matches!(shape, Shape::Bullet | Shape::Numbered | Shape::Task)
}

/// Whether `text` is a list item's Markdown.
pub fn is_item(text: &str) -> bool {
    is_list(shape_of(text).0)
}

/// The shape of `text`, which starts with its marker.
fn marker_of(text: &str) -> (Shape, usize) {
    let hashes = text.len() - text.trim_start_matches('#').len();
    if (1..=6).contains(&hashes) {
        let rest = &text[hashes..];
        if rest.is_empty() || rest.starts_with(' ') {
            let level = hashes.min(3) as u8;
            return (Shape::Heading(level), text.len() - rest.trim_start().len());
        }
    }
    if let Some(rest) = text.strip_prefix('>') {
        return (
            Shape::Quote,
            text.len() - rest.strip_prefix(' ').unwrap_or(rest).len(),
        );
    }
    if let Some(rest) = text.strip_prefix(['-', '*', '+'])
        && (rest.is_empty() || rest.starts_with(' '))
    {
        let after = rest.trim_start();
        for task in ["[ ]", "[x]", "[X]"] {
            if let Some(body) = after.strip_prefix(task)
                && (body.is_empty() || body.starts_with(' '))
            {
                return (Shape::Task, text.len() - body.trim_start().len());
            }
        }
        return (Shape::Bullet, text.len() - after.len());
    }
    let digits = text.len() - text.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if (1..=9).contains(&digits)
        && let Some(rest) = text[digits..].strip_prefix(['.', ')'])
        && (rest.is_empty() || rest.starts_with(' '))
    {
        return (Shape::Numbered, text.len() - rest.trim_start().len());
    }
    (Shape::Paragraph, 0)
}

/// The marker a block of `shape` starts with.
fn marker(shape: Shape) -> String {
    match shape {
        Shape::Paragraph => String::new(),
        Shape::Heading(level) => format!("{} ", "#".repeat(level as usize)),
        Shape::Bullet => "- ".into(),
        Shape::Numbered => "1. ".into(),
        Shape::Task => "- [ ] ".into(),
        Shape::Quote => "> ".into(),
    }
}

/// `text` turned into a block of `shape`, or back into a paragraph when it already is one, as
/// Tiptap's `toggleHeading` and `toggleBulletList` do; `selection` follows the text it was on.
pub fn toggle_shape(text: &str, selection: Range<usize>, shape: Shape) -> (String, Range<usize>) {
    let (current, prefix) = shape_of(text);
    let next = if current == shape {
        Shape::Paragraph
    } else {
        shape
    };
    // An item that stays an item keeps its depth.
    let indent = if is_list(current) && is_list(next) {
        indent_of(text)
    } else {
        ""
    };
    let marker = format!("{indent}{}", marker(next));
    let at = |offset: usize| offset.saturating_sub(prefix) + marker.len();
    (
        format!("{marker}{}", &text[prefix..]),
        at(selection.start)..at(selection.end),
    )
}

/// `text` past its block's marker, and `selection` in it: what a code block made of the block
/// holds.
pub fn body(text: &str, selection: Range<usize>) -> (String, Range<usize>) {
    let (_, prefix) = shape_of(text);
    let at = |offset: usize| offset.saturating_sub(prefix);
    (
        text[prefix..].to_string(),
        at(selection.start)..at(selection.end),
    )
}

/// The slash query `text` holds before `caret`: a `/` that starts the block's text (past its
/// marker) and the word after it, up to the caret. The `/(?:^|\n)\/([^\s/]*)$/` of the
/// design system's editor, on one block.
pub fn slash_query(text: &str, caret: usize) -> Option<Range<usize>> {
    let (_, prefix) = shape_of(text);
    let typed = text.get(prefix..caret)?;
    let query = typed.strip_prefix('/')?;
    if query.contains(|c: char| c.is_whitespace() || c == '/') {
        return None;
    }
    Some(prefix..caret)
}

/// Whether `typed`, all of a block the gutter's "+" just opened, still filters its menu: a
/// word with no `/`.
pub fn filters(typed: &str) -> bool {
    !typed.contains(|c: char| c.is_whitespace() || c == '/')
}

/// A mark the toolbar sets on the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Bold,
    Italic,
}

impl Mark {
    /// What the toolbar writes around the text.
    fn delimiter(self) -> &'static str {
        match self {
            Mark::Bold => "**",
            Mark::Italic => "_",
        }
    }
}

/// The spans of `text` set in `mark`: each one's source with its delimiters.
fn mark_spans(text: &str, mark: Mark) -> Vec<Range<usize>> {
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    Parser::new_ext(text, options)
        .into_offset_iter()
        .filter_map(|(event, range)| match (event, mark) {
            (Event::Start(Tag::Strong), Mark::Bold) => Some(range),
            (Event::Start(Tag::Emphasis), Mark::Italic) => Some(range),
            _ => None,
        })
        .collect()
}

/// `selection`, kept inside `text`, without the whitespace at its ends.
fn trimmed(text: &str, selection: Range<usize>) -> Range<usize> {
    let end = selection.end.min(text.len());
    let selection = selection.start.min(end)..end;
    let selected = &text[selection.clone()];
    let start = selection.start + (selected.len() - selected.trim_start().len());
    let end = (selection.start + selected.trim_end().len()).max(start);
    start..end
}

/// The span set in `mark` that holds the selection, when one does.
fn span_holding(text: &str, selection: &Range<usize>, mark: Mark) -> Option<Range<usize>> {
    let selection = trimmed(text, selection.clone());
    let width = match mark {
        Mark::Bold => 2,
        Mark::Italic => 1,
    };
    mark_spans(text, mark).into_iter().find(|span| {
        let inner = span.start + width..span.end.saturating_sub(width);
        (inner.start <= selection.start && selection.end <= inner.end)
            || (span.start == selection.start && span.end == selection.end)
    })
}

/// Whether the selection is set in `mark`, which lights its button.
pub fn has_mark(text: &str, selection: Range<usize>, mark: Mark) -> bool {
    span_holding(text, &selection, mark).is_some()
}

/// `text` with the selection set in `mark`, or out of it when a span of it holds the
/// selection (the whole span, which a part of it cannot leave alone in Markdown);
/// `selection` follows the text it was on.
pub fn toggle_mark(text: &str, selection: Range<usize>, mark: Mark) -> (String, Range<usize>) {
    let width = match mark {
        Mark::Bold => 2,
        Mark::Italic => 1,
    };
    if let Some(span) = span_holding(text, &selection, mark) {
        let inner = span.start + width..span.end - width;
        let next = format!(
            "{}{}{}",
            &text[..span.start],
            &text[inner.clone()],
            &text[span.end..]
        );
        let at = |offset: usize| {
            offset
                .clamp(inner.start, inner.end)
                .saturating_sub(width)
                .max(span.start)
        };
        return (next, at(selection.start)..at(selection.end));
    }
    let range = trimmed(text, selection.clone());
    if range.is_empty() {
        return (text.to_string(), selection);
    }
    let delimiter = mark.delimiter();
    let next = format!(
        "{}{delimiter}{}{delimiter}{}",
        &text[..range.start],
        &text[range.clone()],
        &text[range.end..]
    );
    (next, range.start + width..range.end + width)
}

/// A fenced code block's opening and closing fences, from its Markdown; an indented block is
/// given back fenced.
pub fn code_fences(markdown: &str) -> (String, String) {
    let mut lines = markdown.lines();
    let first = lines.next().unwrap_or_default().trim_end();
    let opening = first.trim_start();
    let Some(fence) = opening.chars().next().filter(|c| *c == '`' || *c == '~') else {
        return ("```".into(), "```".into());
    };
    let run: String = opening.chars().take_while(|c| *c == fence).collect();
    if run.len() < 3 {
        return ("```".into(), "```".into());
    }
    let last = lines.last().map(str::trim).unwrap_or_default();
    let close = if last.starts_with(&run) && last.chars().all(|c| c == fence) {
        last.to_string()
    } else {
        run
    };
    (first.to_string(), close)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blocks_marker_gives_its_shape() {
        assert_eq!(shape_of("## Title"), (Shape::Heading(2), 3));
        assert_eq!(shape_of("#hashtag"), (Shape::Paragraph, 0));
        assert_eq!(shape_of("> quoted"), (Shape::Quote, 2));
        assert_eq!(shape_of("- [x] done"), (Shape::Task, 6));
        assert_eq!(shape_of("* item"), (Shape::Bullet, 2));
        assert_eq!(shape_of("12) twelve"), (Shape::Numbered, 4));
        assert_eq!(shape_of("-not"), (Shape::Paragraph, 0));
        assert_eq!(shape_of("- "), (Shape::Bullet, 2));
        assert_eq!(shape_of("   - nested"), (Shape::Bullet, 5));
        assert_eq!(shape_of("  indented"), (Shape::Paragraph, 0));
        assert!(is_item("1. one") && !is_item("one"));
    }

    #[test]
    fn a_shape_toggles_on_and_back_off() {
        assert_eq!(
            toggle_shape("Plan", 4..4, Shape::Heading(1)),
            ("# Plan".to_string(), 6..6)
        );
        assert_eq!(
            toggle_shape("# Plan", 2..6, Shape::Heading(1)),
            ("Plan".to_string(), 0..4)
        );
        assert_eq!(
            toggle_shape("- [ ] a", 6..7, Shape::Bullet),
            ("- a".to_string(), 2..3)
        );
        assert_eq!(
            toggle_shape("> said", 2..2, Shape::Paragraph),
            ("said".to_string(), 0..0)
        );
        assert_eq!(
            toggle_shape("  - a", 4..5, Shape::Task),
            ("  - [ ] a".to_string(), 8..9)
        );
    }

    #[test]
    fn the_slash_query_starts_the_blocks_text() {
        assert_eq!(slash_query("/hea", 4), Some(0..4));
        assert_eq!(slash_query("- /bul", 6), Some(2..6));
        assert_eq!(slash_query("/", 1), Some(0..1));
        assert_eq!(slash_query("a /b", 4), None);
        assert_eq!(slash_query("/two words", 10), None);
        assert_eq!(slash_query("/a/b", 4), None);
        assert!(filters("hea") && !filters("two words"));
    }

    #[test]
    fn rows_match_a_label_or_an_id() {
        assert_eq!(
            slash_rows("head"),
            [Tool::Heading(1), Tool::Heading(2), Tool::Heading(3)]
        );
        assert_eq!(slash_rows("HR"), [Tool::Divider]);
        assert_eq!(
            slash_rows("list"),
            [Tool::Bullet, Tool::Numbered, Tool::Task]
        );
        assert_eq!(slash_rows("").len(), BLOCK_TOOLS.len());
        assert!(slash_rows("zzz").is_empty());
    }

    #[test]
    fn a_mark_wraps_the_selection_and_comes_off_its_span() {
        let (bold, selection) = toggle_mark("make it bold", 8..12, Mark::Bold);
        assert_eq!(bold, "make it **bold**");
        assert_eq!(selection, 10..14);
        assert!(has_mark(&bold, selection.clone(), Mark::Bold));
        assert!(!has_mark(&bold, selection.clone(), Mark::Italic));
        assert_eq!(
            toggle_mark(&bold, selection, Mark::Bold),
            ("make it bold".to_string(), 8..12)
        );
        // The whole span comes off, even for a part of it.
        assert_eq!(
            toggle_mark("**two words**", 2..5, Mark::Bold).0,
            "two words"
        );
        assert_eq!(
            toggle_mark("say  it ", 3..8, Mark::Italic),
            ("say  _it_ ".to_string(), 6..8)
        );
        assert!(has_mark("an *old* one", 4..7, Mark::Italic));
        assert_eq!(toggle_mark("a  b", 1..3, Mark::Bold).0, "a  b");
    }

    #[test]
    fn fences_keep_their_spelling() {
        assert_eq!(
            code_fences("```go\nfunc a() {}\n```"),
            ("```go".to_string(), "```".to_string())
        );
        assert_eq!(
            code_fences("~~~~\na\n~~~~~"),
            ("~~~~".to_string(), "~~~~~".to_string())
        );
        assert_eq!(
            code_fences("```\nunclosed"),
            ("```".to_string(), "```".to_string())
        );
        assert_eq!(
            code_fences("    indented"),
            ("```".to_string(), "```".to_string())
        );
    }
}
