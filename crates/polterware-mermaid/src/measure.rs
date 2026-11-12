//! Text measurement and the text shaping upstream does in `rendering-util/`.
//!
//! Three upstream files live here, because they only make sense together, plus
//! `calculateTextDimensions` from `utils.ts`:
//!
//! - [`TextMeasure`] is the seam between this crate and the painter. The GPUI side implements
//!   it with its own text system; [`FallbackMeasure`] is the deterministic stand-in tests use.
//!   Its [`TextMeasure::dimensions`] follows `utils.ts` `calculateTextDimensions`: the width is
//!   the widest line rounded, the height is the sum of the rounded line heights, the line height
//!   is the tallest rounded line, the two candidate families (`sans-serif` and the requested
//!   one) are measured and the larger wins, and empty text is `0 × 0`.
//! - [`split_text_to_chars`], [`split_line_to_words`], [`split_word_to_fit_width`] and
//!   [`split_line_to_fit_width`] port `splitText.ts`. Upstream asks `Intl.Segmenter` for
//!   graphemes and words; this port segments graphemes itself (combining marks, variation
//!   selectors, emoji modifiers, ZWJ sequences, and regional-indicator pairs) and words as runs
//!   of whitespace and runs of non-whitespace, which is what the segmenter's word granularity
//!   yields for label text.
//! - [`markdown_to_lines`] and [`non_markdown_to_lines`] port `handle-markdown-text.ts`, and
//!   [`shape_text`] ports the non-HTML half of `createText.ts`: preprocess, lex, wrap to the
//!   requested width, and return the resolved [`TextBlock`] the scene paints.
//!
//! Not ported, on purpose: `markdownToHTML` and `createText`'s `foreignObject`/HTML-label path
//! (the painter draws text with its own text system, so labels are spans, not HTML),
//! `replaceIconSubstring` (icons are named [`crate::scene::ImageRef`]s in the scene), and
//! `calculateTextDimensions`' `svg element not in render tree` throw (there is no DOM to be
//! detached from). The markdown lexer covers the tokens those functions use: paragraphs,
//! `strong`, `em`, inline HTML, and top-level blocks they fall back to raw. Code spans and
//! links are dropped, as `handle-markdown-text.ts`'s `processNode` drops them.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::scene::{BOLD_WEIGHT, NORMAL_WEIGHT, Size, TextBlock, TextLine, TextSpan};

/// A font request: the family, size in px, weight, and slant a text run is drawn with.
/// Defaults are `calculateTextDimensions`' defaults (`fontSize: 12`, `fontFamily: 'Arial'`,
/// `fontWeight: 400`, upright).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Font {
    pub family: String,
    pub size: f64,
    pub weight: u16,
    pub italic: bool,
}

impl Font {
    pub fn new(family: impl Into<String>, size: f64) -> Self {
        Self {
            family: family.into(),
            size,
            ..Self::default()
        }
    }

    pub fn with_weight(mut self, weight: u16) -> Self {
        self.weight = weight;
        self
    }

    pub fn bold(mut self) -> Self {
        self.weight = self.weight.max(BOLD_WEIGHT);
        self
    }

    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub fn is_bold(&self) -> bool {
        self.weight >= BOLD_WEIGHT
    }
}

impl Default for Font {
    fn default() -> Self {
        Self {
            family: "Arial".to_string(),
            size: 12.0,
            weight: NORMAL_WEIGHT,
            italic: false,
        }
    }
}

/// A measured text box, the shape of `TextDimensions` in `utils.ts`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TextDimensions {
    pub width: f64,
    pub height: f64,
    pub line_height: f64,
}

impl TextDimensions {
    pub const ZERO: TextDimensions = TextDimensions {
        width: 0.0,
        height: 0.0,
        line_height: 0.0,
    };

    pub const fn new(width: f64, height: f64, line_height: f64) -> Self {
        Self {
            width,
            height,
            line_height,
        }
    }
}

/// Distance between baselines inside a wrapped label, `createFormattedText`'s `lineHeight`.
pub const LINE_HEIGHT_EM: f64 = 1.1;

/// `createFormattedText`'s background padding: a label with `addSvgBackground` (edge labels,
/// icon labels) draws its rect 2px larger than the text box on every side.
pub const LABEL_BACKGROUND_PADDING: f64 = 2.0;

/// What the painter must answer about text. The GPUI implementation forwards to its text
/// system; [`FallbackMeasure`] answers deterministically for tests.
pub trait TextMeasure {
    /// The advance width of `text` in px, for one line in `font`.
    fn width(&self, text: &str, font: &Font) -> f64;

    /// The ink box height of one line in `font` (`getBBox().height` upstream: ascent plus
    /// descent, not the line advance).
    fn line_height(&self, font: &Font) -> f64;

    /// The font a span is drawn with when it decorates the base font. `code` spans use the
    /// painter's monospace family; the scene carries the flag, not a family name.
    fn span_font(&self, base: &Font, span: &TextSpan) -> Font {
        let mut font = base.clone();
        if span.bold {
            font.weight = font.weight.max(BOLD_WEIGHT);
        }
        if span.italic {
            font.italic = true;
        }
        if span.code {
            font.family = "monospace".to_string();
        }
        font
    }

    /// The width of a resolved span, styling included.
    fn span_width(&self, span: &TextSpan, base: &Font) -> f64 {
        self.width(&span.text, &self.span_font(base, span))
    }

    /// The width of a resolved line: the spans already contain their joining spaces, so this
    /// is their sum (GPUI lays spans out without kerning across a style change, like SVG).
    fn line_width(&self, line: &TextLine, base: &Font) -> f64 {
        line.spans
            .iter()
            .map(|span| self.span_width(span, base))
            .sum()
    }

    /// `calculateTextDimensions`: `text` split on `<br>` tags, measured line by line. The
    /// requested family and `sans-serif` are both measured — a browser may not have the
    /// requested font — and the larger result wins.
    fn dimensions(&self, text: &str, font: &Font) -> TextDimensions {
        if text.is_empty() {
            return TextDimensions::ZERO;
        }
        let lines = split_breaks(text);
        let mut requested = TextDimensions::ZERO;
        for line in &lines {
            let height = self.line_height(font).round();
            let width = if line.is_empty() {
                0.0
            } else {
                self.width(line, font).round()
            };
            requested.width = requested.width.max(width);
            requested.height += height;
            requested.line_height = requested.line_height.max(height);
        }
        let mut sans_serif = TextDimensions::ZERO;
        let sans_serif_font = Font {
            family: "sans-serif".to_string(),
            ..font.clone()
        };
        for line in &lines {
            let height = self.line_height(&sans_serif_font).round();
            let width = if line.is_empty() {
                0.0
            } else {
                self.width(line, &sans_serif_font).round()
            };
            sans_serif.width = sans_serif.width.max(width);
            sans_serif.height += height;
            sans_serif.line_height = sans_serif.line_height.max(height);
        }
        if sans_serif.width > requested.width
            && sans_serif.height > requested.height
            && sans_serif.line_height > requested.line_height
        {
            sans_serif
        } else {
            requested
        }
    }
}

/// `splitBreaks` (`common.ts`): split on `<br>`, `</br>`, `<br/>`, `<br />`, any case.
pub fn split_breaks(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut i = 0;
    while i < chars.len() {
        if let Some(length) = break_tag_length(&chars[i..]) {
            lines.push(std::mem::take(&mut current));
            i += length;
        } else {
            current.push(chars[i]);
            i += 1;
        }
    }
    lines.push(current);
    lines
}

/// The length of a `<br>`-family tag starting at the slice, if one starts there.
fn break_tag_length(chars: &[char]) -> Option<usize> {
    let mut i = 0;
    if chars.first() != Some(&'<') {
        return None;
    }
    i += 1;
    if chars.get(i) == Some(&'/') {
        i += 1;
    }
    if i + 1 >= chars.len()
        || !chars[i].eq_ignore_ascii_case(&'b')
        || !chars[i + 1].eq_ignore_ascii_case(&'r')
    {
        return None;
    }
    i += 2;
    while chars.get(i).is_some_and(|ch| ch.is_whitespace()) {
        i += 1;
    }
    if chars.get(i) == Some(&'/') {
        i += 1;
    }
    if chars.get(i) == Some(&'>') {
        Some(i + 1)
    } else {
        None
    }
}

/// A deterministic [`TextMeasure`] that needs no text system: every grapheme advances a fixed
/// fraction of the font size by its shape class, and a line is `1.15 × size` tall. It stands in
/// for `sans-serif` as well as the requested family (the table is the font), which keeps
/// [`TextMeasure::dimensions`]' max-of-two-families structure while making it reproducible.
/// It is for tests and headless validation; an app passes its real text system.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FallbackMeasure;

impl FallbackMeasure {
    pub fn new() -> Self {
        Self
    }
}

impl TextMeasure for FallbackMeasure {
    fn width(&self, text: &str, font: &Font) -> f64 {
        let per_em = if is_monospace(&font.family) {
            MONOSPACE_ADVANCE * split_text_to_chars(text).len() as f64
        } else {
            split_text_to_chars(text)
                .iter()
                .map(|grapheme| grapheme_advance(grapheme))
                .sum()
        };
        let weight = if font.is_bold() { 1.02 } else { 1.0 };
        per_em * font.size * weight
    }

    fn line_height(&self, font: &Font) -> f64 {
        font.size * FALLBACK_LINE_HEIGHT
    }
}

/// Advance of a full-width grapheme (CJK, emoji) in em.
const FULL_WIDTH_ADVANCE: f64 = 1.0;
/// Advance of one character in the fallback monospace family, in em.
const MONOSPACE_ADVANCE: f64 = 0.6;
/// Ink height of a fallback line, as a multiple of the font size.
const FALLBACK_LINE_HEIGHT: f64 = 1.15;

fn is_monospace(family: &str) -> bool {
    family.to_ascii_lowercase().contains("mono")
}

/// The advance of one grapheme in em. A rough Arial-like table: narrow letters and
/// punctuation, wide letters, upper and lower case, digits, and full-width scripts.
fn grapheme_advance(grapheme: &str) -> f64 {
    let Some(ch) = grapheme.chars().next() else {
        return 0.0;
    };
    match ch {
        '\t' => 1.12,
        ' ' => 0.28,
        'i' | 'l' | 'j' | 't' | 'f' | 'r' | 'I' | '.' | ',' | ':' | ';' | '\'' | '!' | '|' => 0.28,
        'm' | 'w' | 'M' | 'W' | '@' | '%' => 0.86,
        '0'..='9' => 0.55,
        'a'..='z' => 0.5,
        'A'..='Z' => 0.66,
        _ if (ch as u32) >= 0x2E80 => FULL_WIDTH_ADVANCE,
        _ => 0.5,
    }
}

/// An error from text shaping. The message is upstream's, word for word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextError {
    /// `splitLineToFitWidth does not support newlines in the line`.
    NewlineInLine,
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextError::NewlineInLine => {
                f.write_str("splitLineToFitWidth does not support newlines in the line")
            }
        }
    }
}

impl std::error::Error for TextError {}

/// What kind of markdown run a word is. `handle-markdown-text.ts` only produces these three.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkdownWordKind {
    #[default]
    Normal,
    Strong,
    Em,
}

/// One whitespace-free word of a label, `MarkdownWord` upstream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MarkdownWord {
    pub content: String,
    pub kind: MarkdownWordKind,
}

impl MarkdownWord {
    pub fn new(content: impl Into<String>, kind: MarkdownWordKind) -> Self {
        Self {
            content: content.into(),
            kind,
        }
    }
}

impl Default for MarkdownWord {
    fn default() -> Self {
        Self {
            content: String::new(),
            kind: MarkdownWordKind::Normal,
        }
    }
}

/// A line of words, `MarkdownLine` upstream.
pub type MarkdownLine = Vec<MarkdownWord>;

/// Splits a string into grapheme clusters, `splitTextToChars` when `Intl.Segmenter` exists:
/// combining marks, variation selectors, emoji modifiers and tags, and joined (ZWJ) sequences
/// stay with their base, and regional-indicator flags pair up.
pub fn split_text_to_chars(text: &str) -> Vec<String> {
    let mut clusters = Vec::new();
    let mut current = String::new();
    let mut join_next = false;
    let mut regionals = 0usize;
    for ch in text.chars() {
        let attach = !current.is_empty()
            && (join_next
                || is_grapheme_extender(ch)
                || (is_regional_indicator(ch) && regionals % 2 == 1));
        if !attach && !current.is_empty() {
            clusters.push(std::mem::take(&mut current));
            regionals = 0;
        }
        join_next = ch == '\u{200d}';
        if is_regional_indicator(ch) {
            regionals += 1;
        } else if !is_grapheme_extender(ch) {
            regionals = 0;
        }
        current.push(ch);
    }
    if !current.is_empty() {
        clusters.push(current);
    }
    clusters
}

fn is_regional_indicator(ch: char) -> bool {
    matches!(ch as u32, 0x1F1E6..=0x1F1FF)
}

/// Characters that never start a new grapheme: combining marks and modifiers, variation
/// selectors, the zero-width joiners, emoji skin tones, keycap, and tag characters.
fn is_grapheme_extender(ch: char) -> bool {
    matches!(
        ch as u32,
        0x0300..=0x036F
            | 0x0483..=0x0489
            | 0x0591..=0x05BD
            | 0x0610..=0x061A
            | 0x064B..=0x065F
            | 0x0670
            | 0x06D6..=0x06ED
            | 0x0E31
            | 0x0E34..=0x0E3A
            | 0x0E47..=0x0E4E
            | 0x200C..=0x200D
            | 0x20D0..=0x20FF
            | 0xFE00..=0xFE0F
            | 0xFE20..=0xFE2F
            | 0x1F3FB..=0x1F3FF
            | 0xE0020..=0xE007F
            | 0xE0100..=0xE01EF
    )
}

/// Splits a string into words, `splitLineToWords`: runs of whitespace and runs of
/// non-whitespace, the word granularity `Intl.Segmenter` gives label text.
pub fn split_line_to_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_space: Option<bool> = None;
    for cluster in split_text_to_chars(text) {
        let space = cluster.chars().all(char::is_whitespace);
        if in_space.is_some_and(|previous| previous != space) {
            words.push(std::mem::take(&mut current));
        }
        in_space = Some(space);
        current.push_str(&cluster);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Splits a word into the part that fits and the rest, `splitWordToFitWidth`. A first character
/// that does not fit is kept anyway, so the result always progresses.
pub fn split_word_to_fit_width(
    check_fit: &dyn Fn(&[MarkdownWord]) -> bool,
    word: &MarkdownWord,
) -> (MarkdownWord, MarkdownWord) {
    let characters = split_text_to_chars(&word.content);
    let mut used: Vec<String> = Vec::new();
    for (index, character) in characters.iter().enumerate() {
        let mut candidate = used.clone();
        candidate.push(character.clone());
        let line = vec![MarkdownWord::new(candidate.join(""), word.kind)];
        if check_fit(&line) {
            used = candidate;
        } else if used.is_empty() {
            return (
                MarkdownWord::new(character.clone(), word.kind),
                MarkdownWord::new(characters[index + 1..].join(""), word.kind),
            );
        } else {
            return (
                MarkdownWord::new(used.join(""), word.kind),
                MarkdownWord::new(characters[index..].join(""), word.kind),
            );
        }
    }
    (
        MarkdownWord::new(used.join(""), word.kind),
        MarkdownWord::new(String::new(), word.kind),
    )
}

/// Splits a line into the lines that fit, `splitLineToFitWidth`. A line containing a newline is
/// an error, exactly as upstream throws.
pub fn split_line_to_fit_width(
    line: &[MarkdownWord],
    check_fit: &dyn Fn(&[MarkdownWord]) -> bool,
) -> Result<Vec<MarkdownLine>, TextError> {
    if line.iter().any(|word| word.content.contains('\n')) {
        return Err(TextError::NewlineInLine);
    }
    let mut words: std::collections::VecDeque<MarkdownWord> = line.iter().cloned().collect();
    let mut lines: Vec<MarkdownLine> = Vec::new();
    let mut new_line: MarkdownLine = Vec::new();
    loop {
        if words.is_empty() {
            if !new_line.is_empty() {
                lines.push(new_line);
            }
            return Ok(lines);
        }
        let mut joiner = String::new();
        if words.front().is_some_and(|word| word.content == " ") {
            joiner.push(' ');
            words.pop_front();
        }
        let next_word = words.pop_front().unwrap_or_default();
        let mut line_with_next = new_line.clone();
        if !joiner.is_empty() {
            line_with_next.push(MarkdownWord::new(joiner, MarkdownWordKind::Normal));
        }
        line_with_next.push(next_word.clone());
        if check_fit(&line_with_next) {
            new_line = line_with_next;
            continue;
        }
        if !new_line.is_empty() {
            lines.push(std::mem::take(&mut new_line));
            words.push_front(next_word);
        } else if !next_word.content.is_empty() {
            let (fitted, rest) = split_word_to_fit_width(check_fit, &next_word);
            lines.push(vec![fitted]);
            if !rest.content.is_empty() {
                words.push_front(rest);
            }
        }
    }
}

/// `createText.ts` `decodeHTMLEntities`: the three entities `sanitizeText` encodes, decoded in
/// one pass for SVG text (`&amp;lt;` stays `&lt;`).
pub fn decode_html_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find('&') {
        out.push_str(&rest[..index]);
        let after = &rest[index..];
        if let Some(decoded) = after.strip_prefix("&amp;") {
            out.push('&');
            rest = decoded;
        } else if let Some(decoded) = after.strip_prefix("&lt;") {
            out.push('<');
            rest = decoded;
        } else if let Some(decoded) = after.strip_prefix("&gt;") {
            out.push('>');
            rest = decoded;
        } else {
            out.push('&');
            rest = &after[1..];
        }
    }
    out.push_str(rest);
    out
}

/// `handle-markdown-text.ts` `preprocessMarkdown`, minus the `markdownAutoWrap` branch (it is
/// a no-op upstream): `<br/>` becomes a newline, runs of newlines collapse to one, and the
/// common indentation is removed.
fn preprocess_markdown(markdown: &str) -> String {
    let without_break = markdown.replace("<br/>", "\n");
    let without_multiple_newlines = collapse_newlines(&without_break);
    dedent(&without_multiple_newlines)
}

fn collapse_newlines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            out.push('\n');
            while bytes.get(i) == Some(&b'\n') {
                i += 1;
            }
        } else {
            let ch = text[i..].chars().next().expect("non-empty remainder");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// `ts-dedent`'s `dedent`: drop the leading newline, then the smallest indentation shared by
/// the non-blank lines.
fn dedent(text: &str) -> String {
    let text = text.strip_prefix('\n').unwrap_or(text);
    let text = text.strip_suffix('\r').unwrap_or(text);
    let lines: Vec<&str> = text.split('\n').collect();
    let min_indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.chars().take_while(|ch| ch.is_whitespace()).count())
        .min()
        .unwrap_or(0);
    if min_indent == 0 {
        return lines.join("\n");
    }
    lines
        .iter()
        .map(|line| line.chars().skip(min_indent).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// `handle-markdown-text.ts` `nonMarkdownToLines`: split on newlines, `\n`, and `<br>` tags,
/// trim, and keep SVG tags as words.
pub fn non_markdown_to_lines(non_markdown_text: &str) -> Vec<MarkdownLine> {
    split_non_markdown_lines(non_markdown_text)
        .into_iter()
        .map(|line| {
            split_non_markdown_words(line.trim())
                .into_iter()
                .map(|word| MarkdownWord::new(word, MarkdownWordKind::Normal))
                .collect()
        })
        .collect()
}

fn split_non_markdown_lines(text: &str) -> Vec<&str> {
    let chars: Vec<char> = text.chars().collect();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && chars.get(i + 1) == Some(&'n') {
            lines.push(&text[byte_index(&chars, start)..byte_index(&chars, i)]);
            i += 2;
            start = i;
        } else if chars[i] == '\n' {
            lines.push(&text[byte_index(&chars, start)..byte_index(&chars, i)]);
            i += 1;
            start = i;
        } else if let Some(length) = break_tag_length(&chars[i..]) {
            lines.push(&text[byte_index(&chars, start)..byte_index(&chars, i)]);
            i += length;
            start = i;
        } else {
            i += 1;
        }
    }
    lines.push(&text[byte_index(&chars, start)..]);
    lines
}

fn byte_index(chars: &[char], char_index: usize) -> usize {
    chars.iter().take(char_index).map(|ch| ch.len_utf8()).sum()
}

/// `/<[^>]+>|[^\s<>]+/g`: a whole tag, or a run that stops at whitespace and angle brackets.
fn split_non_markdown_words(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut words = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' {
            if let Some(end) = chars[i + 1..].iter().position(|ch| *ch == '>') {
                let end = i + 1 + end;
                words.push(chars[i..=end].iter().collect());
                i = end + 1;
            } else {
                i += 1;
            }
        } else if chars[i].is_whitespace() || chars[i] == '>' {
            i += 1;
        } else {
            let start = i;
            while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '<' && chars[i] != '>'
            {
                i += 1;
            }
            words.push(chars[start..i].iter().collect());
        }
    }
    words
}

/// `handle-markdown-text.ts` `markdownToLines`: markdown into lines of typed words. Paragraphs
/// keep their line breaks; `strong` and `em` set the word type; inline HTML is a normal word;
/// blocks the upstream function does not understand are one raw normal word.
pub fn markdown_to_lines(markdown: &str) -> Vec<MarkdownLine> {
    let preprocessed = preprocess_markdown(markdown);
    let mut lines: Vec<MarkdownLine> = vec![Vec::new()];
    let mut current = 0usize;
    for block in lex_blocks(&preprocessed) {
        match block {
            Block::Paragraph(text) => {
                let inline = parse_inline(&text);
                push_inline(&inline, MarkdownWordKind::Normal, &mut lines, &mut current);
            }
            Block::Html(raw) | Block::Raw(raw) => {
                lines[current].push(MarkdownWord::new(raw, MarkdownWordKind::Normal));
            }
        }
    }
    lines
}

enum Block {
    Paragraph(String),
    Html(String),
    Raw(String),
}

fn lex_blocks(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let lines: Vec<&str> = text.split('\n').collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            flush_paragraph(&mut paragraph, &mut blocks);
            i += 1;
            continue;
        }
        if is_fence(line) {
            flush_paragraph(&mut paragraph, &mut blocks);
            let mut raw = Vec::new();
            i += 1;
            while i < lines.len() {
                let inner = lines[i];
                raw.push(inner);
                i += 1;
                if is_fence(inner) {
                    break;
                }
            }
            blocks.push(Block::Raw(raw.join("\n")));
            continue;
        }
        if is_list_item(line) {
            flush_paragraph(&mut paragraph, &mut blocks);
            let mut raw = Vec::new();
            while i < lines.len() && (is_list_item(lines[i]) || is_list_continuation(lines[i])) {
                raw.push(lines[i]);
                i += 1;
            }
            blocks.push(Block::Raw(raw.join("\n")));
            continue;
        }
        if is_heading(line) || is_thematic_break(line) || is_blockquote(line) {
            flush_paragraph(&mut paragraph, &mut blocks);
            blocks.push(Block::Raw(line.to_string()));
            i += 1;
            continue;
        }
        if is_html_block_start(line) {
            flush_paragraph(&mut paragraph, &mut blocks);
            let mut raw = Vec::new();
            while i < lines.len() && !lines[i].trim().is_empty() {
                raw.push(lines[i]);
                i += 1;
            }
            blocks.push(Block::Html(raw.join("\n")));
            continue;
        }
        paragraph.push(line);
        i += 1;
    }
    flush_paragraph(&mut paragraph, &mut blocks);
    blocks
}

fn flush_paragraph(paragraph: &mut Vec<&str>, blocks: &mut Vec<Block>) {
    if !paragraph.is_empty() {
        blocks.push(Block::Paragraph(paragraph.join("\n")));
        paragraph.clear();
    }
}

fn is_fence(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

fn is_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    let mut chars = trimmed.chars();
    match chars.next() {
        Some('-' | '*' | '+') => chars.next() == Some(' '),
        Some(ch) if ch.is_ascii_digit() => {
            let rest: String = trimmed
                .chars()
                .skip_while(|ch| ch.is_ascii_digit())
                .collect();
            rest.starts_with(". ") || rest.starts_with(") ")
        }
        _ => false,
    }
}

/// An indented line under a list item, which marked keeps inside the list block.
fn is_list_continuation(line: &str) -> bool {
    line.starts_with("  ") && !line.trim().is_empty()
}

fn is_heading(line: &str) -> bool {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|ch| *ch == '#').count();
    (1..=6).contains(&hashes) && trimmed.chars().nth(hashes) == Some(' ')
}

fn is_thematic_break(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.len() >= 3
        && (trimmed.chars().all(|ch| ch == '-')
            || trimmed.chars().all(|ch| ch == '*')
            || trimmed.chars().all(|ch| ch == '_'))
}

fn is_blockquote(line: &str) -> bool {
    line.trim_start().starts_with('>')
}

fn is_html_block_start(line: &str) -> bool {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('<') {
        return false;
    }
    let name: String = trimmed
        .trim_start_matches('<')
        .trim_start_matches('/')
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "address"
            | "article"
            | "aside"
            | "base"
            | "basefont"
            | "blockquote"
            | "body"
            | "caption"
            | "center"
            | "col"
            | "colgroup"
            | "dd"
            | "details"
            | "dialog"
            | "dir"
            | "div"
            | "dl"
            | "dt"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "frame"
            | "frameset"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "head"
            | "header"
            | "hr"
            | "html"
            | "iframe"
            | "legend"
            | "li"
            | "link"
            | "main"
            | "menu"
            | "menuitem"
            | "nav"
            | "noframes"
            | "ol"
            | "optgroup"
            | "option"
            | "p"
            | "param"
            | "pre"
            | "script"
            | "search"
            | "section"
            | "style"
            | "summary"
            | "table"
            | "tbody"
            | "td"
            | "tfoot"
            | "th"
            | "thead"
            | "title"
            | "tr"
            | "track"
            | "ul"
    )
}

enum Inline {
    Text(String),
    Strong(Vec<Inline>),
    Em(Vec<Inline>),
    Html(String),
}

fn parse_inline(text: &str) -> Vec<Inline> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut buffer = String::new();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        match ch {
            '\\' if i + 1 < chars.len() => {
                buffer.push(chars[i + 1]);
                i += 2;
            }
            '`' => {
                let run = run_length(&chars, i);
                if let Some(end) = find_run(&chars, i + run, '`', run) {
                    push_text(&mut out, &mut buffer);
                    i = end + run;
                } else {
                    for _ in 0..run {
                        buffer.push('`');
                    }
                    i += run;
                }
            }
            '[' => {
                if let Some(end) = link_end(&chars, i) {
                    push_text(&mut out, &mut buffer);
                    i = end;
                } else {
                    buffer.push(ch);
                    i += 1;
                }
            }
            '<' => {
                // marked's inline HTML only starts a tag with a name character, `/`, `!`, or
                // `?`. A `<<Stereotype>>` label is text, not markup, so both angle brackets
                // stay in the word.
                let tag_start = chars.get(i + 1).is_some_and(|next| {
                    next.is_ascii_alphanumeric() || matches!(next, '/' | '!' | '?')
                });
                if tag_start && let Some(end) = chars[i + 1..].iter().position(|ch| *ch == '>') {
                    let end = i + 1 + end;
                    push_text(&mut out, &mut buffer);
                    out.push(Inline::Html(chars[i..=end].iter().collect()));
                    i = end + 1;
                } else {
                    buffer.push(ch);
                    i += 1;
                }
            }
            '*' | '_' => {
                let run = run_length(&chars, i);
                let delimiter = if run >= 3 { 3 } else { run };
                // Underscore emphasis does not open inside a word (`snake_case_name`).
                let can_open = ch != '_'
                    || (i == 0 || !chars[i - 1].is_alphanumeric())
                        && chars
                            .get(i + delimiter)
                            .is_some_and(|next| !next.is_whitespace());
                if delimiter > 0
                    && can_open
                    && let Some(end) = find_emphasis_end(&chars, i + delimiter, ch, delimiter)
                {
                    let inner: String = chars[i + delimiter..end].iter().collect();
                    push_text(&mut out, &mut buffer);
                    let parsed = parse_inline(&inner);
                    match delimiter {
                        3 => out.push(Inline::Strong(vec![Inline::Em(parsed)])),
                        2 => out.push(Inline::Strong(parsed)),
                        _ => out.push(Inline::Em(parsed)),
                    }
                    i = end + delimiter;
                    continue;
                }
                for _ in 0..run {
                    buffer.push(ch);
                }
                i += run;
            }
            _ => {
                buffer.push(ch);
                i += 1;
            }
        }
    }
    push_text(&mut out, &mut buffer);
    out
}

fn push_text(out: &mut Vec<Inline>, buffer: &mut String) {
    if !buffer.is_empty() {
        out.push(Inline::Text(std::mem::take(buffer)));
    }
}

fn run_length(chars: &[char], start: usize) -> usize {
    let ch = chars[start];
    chars[start..]
        .iter()
        .take_while(|other| **other == ch)
        .count()
}

fn find_run(chars: &[char], from: usize, ch: char, length: usize) -> Option<usize> {
    let mut i = from;
    while i < chars.len() {
        if chars[i] == ch {
            let run = run_length(chars, i);
            if run >= length {
                return Some(i);
            }
            i += run;
        } else {
            i += 1;
        }
    }
    None
}

/// The closing delimiter of an emphasis run. A closer may not follow whitespace, and an
/// underscore closer may not be followed by a word character (CommonMark's flanking rules).
fn find_emphasis_end(chars: &[char], from: usize, ch: char, length: usize) -> Option<usize> {
    let mut i = from;
    while i < chars.len() {
        if chars[i] == ch {
            let run = run_length(chars, i);
            if run >= length {
                let not_after_space = i == 0 || !chars[i - 1].is_whitespace();
                let not_before_word = ch != '_'
                    || chars
                        .get(i + length)
                        .is_none_or(|next| !next.is_alphanumeric());
                if not_after_space && not_before_word {
                    return Some(i);
                }
            }
            i += run;
        } else {
            i += 1;
        }
    }
    None
}

/// `[label](url)`: upstream's `processNode` has no branch for a link token, so the whole link
/// is dropped. The index after the closing parenthesis is returned.
fn link_end(chars: &[char], start: usize) -> Option<usize> {
    if chars.get(start + 1) == Some(&'[') {
        return None;
    }
    let label_end = chars[start + 1..].iter().position(|ch| *ch == ']')? + start + 1;
    if chars.get(label_end + 1) != Some(&'(') {
        return None;
    }
    let url_end = chars[label_end + 2..].iter().position(|ch| *ch == ')')? + label_end + 2;
    Some(url_end + 1)
}

fn push_inline(
    inline: &[Inline],
    kind: MarkdownWordKind,
    lines: &mut Vec<MarkdownLine>,
    current: &mut usize,
) {
    for token in inline {
        match token {
            Inline::Text(text) => {
                for (index, text_line) in text.split('\n').enumerate() {
                    if index != 0 {
                        *current += 1;
                        lines.push(Vec::new());
                    }
                    for word in text_line.split(' ').filter(|word| !word.is_empty()) {
                        lines[*current].push(MarkdownWord::new(word.replace("&#39;", "'"), kind));
                    }
                }
            }
            Inline::Strong(children) => {
                push_inline(children, MarkdownWordKind::Strong, lines, current);
            }
            Inline::Em(children) => {
                push_inline(children, MarkdownWordKind::Em, lines, current);
            }
            Inline::Html(raw) => {
                lines[*current].push(MarkdownWord::new(raw.clone(), MarkdownWordKind::Normal));
            }
        }
    }
}

/// Options for [`shape_text`], `createText`'s non-HTML options that change the result.
#[derive(Clone, Debug, PartialEq)]
pub struct TextOptions {
    /// The wrap width: `Number.POSITIVE_INFINITY` disables wrapping. `200` is upstream's
    /// default, which callers override with the node or flowchart wrapping width.
    pub width: f64,
    /// Parse the label as markdown; otherwise newlines and `<br>` are the only breaks.
    pub markdown: bool,
    /// Widen the measured box to at least this width (`createText`'s `minWidth`, which only
    /// HTML labels honour upstream; a shape can still use it).
    pub min_width: f64,
}

impl Default for TextOptions {
    fn default() -> Self {
        Self {
            width: 200.0,
            markdown: true,
            min_width: 0.0,
        }
    }
}

impl TextOptions {
    /// No wrapping at any width: the whole label is one line per explicit break.
    pub fn unwrapped() -> Self {
        Self {
            width: f64::INFINITY,
            ..Self::default()
        }
    }

    pub fn with_width(mut self, width: f64) -> Self {
        self.width = width;
        self
    }

    pub fn with_markdown(mut self, markdown: bool) -> Self {
        self.markdown = markdown;
        self
    }

    pub fn with_min_width(mut self, min_width: f64) -> Self {
        self.min_width = min_width;
        self
    }
}

/// `createText.ts`'s non-HTML path: split the label into structured lines, wrap each to
/// `options.width`, and hand back the lines of styled spans and their measured box.
///
/// The line height is [`LINE_HEIGHT_EM`] × the font size, the distance `createFormattedText`
/// spaces tspans by. An empty label measures `0 × 0`.
pub fn shape_text(
    text: &str,
    options: &TextOptions,
    font: &Font,
    measure: &dyn TextMeasure,
) -> Result<TextBlock, TextError> {
    let sanitized = sanitize_breaks(text);
    let structured = if options.markdown {
        markdown_to_lines(&sanitized)
    } else {
        non_markdown_to_lines(&sanitized)
    };
    let check_fit = |line: &[MarkdownWord]| {
        if options.width.is_infinite() {
            return true;
        }
        width_of_words(line, font, measure) <= options.width
    };
    let mut resolved: Vec<TextLine> = Vec::new();
    for line in &structured {
        if check_fit(line) {
            resolved.push(line_to_spans(line));
        } else {
            for fitted in split_line_to_fit_width(line, &check_fit)? {
                resolved.push(line_to_spans(&fitted));
            }
        }
    }
    let line_height = LINE_HEIGHT_EM * font.size;
    let measured_width = resolved
        .iter()
        .map(|line| measure.line_width(line, font))
        .fold(0.0, f64::max);
    // The bbox of a `<text>` ends at its last non-empty line; trailing empty tspans add no ink.
    let height = match resolved.iter().rposition(|line| !line.text().is_empty()) {
        Some(last) => measure.line_height(font) + line_height * last as f64,
        None => 0.0,
    };
    Ok(TextBlock::new(
        resolved,
        Size::new(measured_width.max(options.min_width), height),
        line_height,
    ))
}

/// A literal text block: each newline-separated line is one span with no markdown or HTML
/// tokenization. Requirement labels are literal `<<Stereotype>>` and `Text: ...` strings;
/// `shape_text`'s markdown/HTML splitters would treat the angle brackets as markup and lose
/// one of the closing `>`. Width and line height are `TextMeasure`'s, so the block matches
/// what `shape_text` reports for the same single-line text.
pub fn plain_text_block(
    text: &str,
    font: &Font,
    measure: &dyn TextMeasure,
) -> crate::scene::TextBlock {
    use crate::scene::{TextBlock, TextLine, TextSpan};
    let lines: Vec<TextLine> = text
        .split('\n')
        .map(|line| TextLine::new(vec![TextSpan::plain(line)]))
        .collect();
    let width = lines
        .iter()
        .map(|line| measure.width(&line.text(), font))
        .fold(0.0, f64::max);
    let last = lines.iter().rposition(|line| !line.is_empty()).unwrap_or(0);
    let line_advance = LINE_HEIGHT_EM * font.size;
    let height = measure.line_height(font) + line_advance * last as f64;
    TextBlock::new(lines, crate::scene::Size::new(width, height), line_advance)
}

/// `createText.ts`'s `sanitizeBR`: every `<br>`-family tag becomes the exact `<br/>` the
/// markdown preprocessor turns into a newline.
fn sanitize_breaks(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if let Some(length) = break_tag_length(&chars[i..]) {
            out.push_str("<br/>");
            i += length;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn width_of_words(words: &[MarkdownWord], font: &Font, measure: &dyn TextMeasure) -> f64 {
    measure.line_width(&line_to_spans(words), font)
}

/// `createFormattedText`'s `updateTextContentAndStyles`: the first word is drawn as is, every
/// later word gets a joining space, consecutive words of the same type merge into one span,
/// and `decodeHTMLEntities` runs on the way in.
fn line_to_spans(words: &[MarkdownWord]) -> TextLine {
    let mut spans: Vec<TextSpan> = Vec::new();
    for (index, word) in words.iter().enumerate() {
        let decoded = decode_html_entities(&word.content);
        let text = if index == 0 {
            decoded
        } else {
            format!(" {decoded}")
        };
        let styled = styled_span(&text, word.kind);
        match spans.last_mut() {
            Some(previous) if previous.is_styled_like(&styled) => previous.text.push_str(&text),
            _ => spans.push(styled),
        }
    }
    TextLine::new(spans)
}

fn styled_span(text: &str, kind: MarkdownWordKind) -> TextSpan {
    match kind {
        MarkdownWordKind::Normal => TextSpan::plain(text),
        MarkdownWordKind::Strong => TextSpan::bold(text),
        MarkdownWordKind::Em => TextSpan::italic(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn word(content: &str, kind: MarkdownWordKind) -> MarkdownWord {
        MarkdownWord::new(content, kind)
    }

    fn line(words: &[(&str, MarkdownWordKind)]) -> MarkdownLine {
        words
            .iter()
            .map(|(content, kind)| word(content, *kind))
            .collect()
    }

    fn normal(words: &[(&str, MarkdownWordKind)]) -> MarkdownLine {
        line(words)
    }

    /// `createCheckFn`: fit when the joined line has at most `width` graphemes.
    fn char_fit(width: usize) -> impl Fn(&[MarkdownWord]) -> bool {
        move |text: &[MarkdownWord]| {
            let joined: String = text.iter().map(|word| word.content.as_str()).collect();
            split_text_to_chars(&joined).len() <= width
        }
    }

    fn spec_line(text: &str) -> MarkdownLine {
        split_line_to_words(text)
            .into_iter()
            .map(|content| word(&content, MarkdownWordKind::Normal))
            .collect()
    }

    // splitText.spec.ts: "should split $str into graphemes".
    #[test]
    fn split_text_to_chars_matches_intl_segmenter_graphemes() {
        assert_eq!(split_text_to_chars(""), Vec::<String>::new());
        assert_eq!(split_text_to_chars("🏳️‍⚧️🏳️‍🌈👩🏾‍❤️‍👨🏻"), vec!["🏳️‍⚧️", "🏳️‍🌈", "👩🏾‍❤️‍👨🏻"]);
        assert_eq!(split_text_to_chars("ok"), vec!["o", "k"]);
        assert_eq!(split_text_to_chars("abc"), vec!["a", "b", "c"]);
        assert_eq!(split_text_to_chars("中文中"), vec!["中", "文", "中"]);
    }

    // splitText.spec.ts: "should create valid checkFit function".
    #[test]
    fn check_fit_counts_graphemes() {
        let check_fit_five = char_fit(5);
        assert!(check_fit_five(&[word("hello", MarkdownWordKind::Normal)]));
        assert!(!check_fit_five(&[
            word("hello", MarkdownWordKind::Normal),
            word("world", MarkdownWordKind::Normal),
        ]));
        let check_fit_one = char_fit(1);
        assert!(check_fit_one(&[word("A", MarkdownWordKind::Normal)]));
        assert!(check_fit_one(&[word("🏳️‍⚧️", MarkdownWordKind::Normal)]));
        assert!(!check_fit_one(&[word("🏳️‍⚧️🏳️‍⚧️", MarkdownWordKind::Normal)]));
    }

    // splitText.spec.ts: the Intl.Segmenter split-line table.
    #[test]
    fn split_line_to_fit_width_matches_the_segmenter_table() {
        let cases: &[(&str, usize, &[&str])] = &[
            ("hello world", 7, &["hello", "world"]),
            ("hello world", 20, &["hello world"]),
            ("hello world", 3, &["hel", "lo", "wor", "ld"]),
            ("hello 12 world", 4, &["hell", "o 12", "worl", "d"]),
            ("hello  1 2 world", 4, &["hell", "o  1", "2", "worl", "d"]),
            ("hello  1 2 world", 6, &["hello", "  1 2", "world"]),
            ("🏳️‍⚧️🏳️‍🌈👩🏾‍❤️‍👨🏻", 0, &["🏳️‍⚧️", "🏳️‍🌈", "👩🏾‍❤️‍👨🏻"]),
            ("🏳️‍⚧️🏳️‍🌈👩🏾‍❤️‍👨🏻", 1, &["🏳️‍⚧️", "🏳️‍🌈", "👩🏾‍❤️‍👨🏻"]),
            ("🏳️‍⚧️🏳️‍🌈👩🏾‍❤️‍👨🏻", 2, &["🏳️‍⚧️🏳️‍🌈", "👩🏾‍❤️‍👨🏻"]),
            ("🏳️‍⚧️🏳️‍🌈👩🏾‍❤️‍👨🏻", 3, &["🏳️‍⚧️🏳️‍🌈👩🏾‍❤️‍👨🏻"]),
            ("中文中", 1, &["中", "文", "中"]),
            ("中文中", 2, &["中文", "中"]),
            ("中文中", 3, &["中文中"]),
            ("Flag 🏳️‍⚧️ this 🏳️‍🌈", 6, &["Flag 🏳️‍⚧️", "this 🏳️‍🌈"]),
        ];
        for (text, width, expected) in cases {
            let check_fit = char_fit(*width);
            let actual = split_line_to_fit_width(&spec_line(text), &check_fit)
                .unwrap_or_else(|error| panic!("{text:?} failed: {error}"));
            let expected: Vec<MarkdownLine> = expected.iter().map(|line| spec_line(line)).collect();
            assert_eq!(actual, expected, "splitting {text:?} at width {width}");
        }
    }

    // splitText.spec.ts: "should handle strings with newlines".
    #[test]
    fn split_line_to_fit_width_rejects_newlines() {
        let error = split_line_to_fit_width(&spec_line("Flag\n  🏳️‍⚧️ this 🏳️‍🌈"), &char_fit(6))
            .expect_err("a newline in the line is an error");
        assert_eq!(error, TextError::NewlineInLine);
        assert_eq!(
            error.to_string(),
            "splitLineToFitWidth does not support newlines in the line"
        );
    }

    // handle-markdown-text.spec.ts: "markdownToLines - Basic test".
    #[test]
    fn markdown_to_lines_parses_paragraphs_and_mixed_formatting() {
        let input = "This is regular text
Here is a new line
There is some words **with a bold** section
Here is a line *with an italic* section";
        let expected = vec![
            normal(&[
                ("This", MarkdownWordKind::Normal),
                ("is", MarkdownWordKind::Normal),
                ("regular", MarkdownWordKind::Normal),
                ("text", MarkdownWordKind::Normal),
            ]),
            normal(&[
                ("Here", MarkdownWordKind::Normal),
                ("is", MarkdownWordKind::Normal),
                ("a", MarkdownWordKind::Normal),
                ("new", MarkdownWordKind::Normal),
                ("line", MarkdownWordKind::Normal),
            ]),
            normal(&[
                ("There", MarkdownWordKind::Normal),
                ("is", MarkdownWordKind::Normal),
                ("some", MarkdownWordKind::Normal),
                ("words", MarkdownWordKind::Normal),
                ("with", MarkdownWordKind::Strong),
                ("a", MarkdownWordKind::Strong),
                ("bold", MarkdownWordKind::Strong),
                ("section", MarkdownWordKind::Normal),
            ]),
            normal(&[
                ("Here", MarkdownWordKind::Normal),
                ("is", MarkdownWordKind::Normal),
                ("a", MarkdownWordKind::Normal),
                ("line", MarkdownWordKind::Normal),
                ("with", MarkdownWordKind::Em),
                ("an", MarkdownWordKind::Em),
                ("italic", MarkdownWordKind::Em),
                ("section", MarkdownWordKind::Normal),
            ]),
        ];
        assert_eq!(markdown_to_lines(input), expected);
    }

    // handle-markdown-text.spec.ts: "markdownToLines - Empty input".
    #[test]
    fn markdown_to_lines_keeps_one_empty_line_for_empty_input() {
        assert_eq!(markdown_to_lines(""), vec![Vec::new()]);
    }

    // handle-markdown-text.spec.ts: "markdownToLines - No formatting".
    #[test]
    fn markdown_to_lines_splits_unformatted_paragraphs() {
        let expected = vec![
            normal(&[
                ("This", MarkdownWordKind::Normal),
                ("is", MarkdownWordKind::Normal),
                ("a", MarkdownWordKind::Normal),
                ("simple", MarkdownWordKind::Normal),
                ("test", MarkdownWordKind::Normal),
            ]),
            normal(&[
                ("with", MarkdownWordKind::Normal),
                ("no", MarkdownWordKind::Normal),
                ("formatting", MarkdownWordKind::Normal),
            ]),
        ];
        assert_eq!(
            markdown_to_lines("This is a simple test\nwith no formatting"),
            expected
        );
    }

    // handle-markdown-text.spec.ts: "markdownToLines - Only bold formatting".
    #[test]
    fn markdown_to_lines_reads_one_bold_run() {
        assert_eq!(
            markdown_to_lines("This is a **bold** test"),
            vec![normal(&[
                ("This", MarkdownWordKind::Normal),
                ("is", MarkdownWordKind::Normal),
                ("a", MarkdownWordKind::Normal),
                ("bold", MarkdownWordKind::Strong),
                ("test", MarkdownWordKind::Normal),
            ])]
        );
    }

    // handle-markdown-text.spec.ts: "markdownToLines - paragraph 1".
    #[test]
    fn markdown_to_lines_dedents_a_continued_paragraph() {
        let expected = vec![
            normal(&[
                ("Start", MarkdownWordKind::Strong),
                ("with", MarkdownWordKind::Normal),
            ]),
            normal(&[
                ("a", MarkdownWordKind::Normal),
                ("second", MarkdownWordKind::Normal),
                ("line", MarkdownWordKind::Normal),
            ]),
        ];
        assert_eq!(
            markdown_to_lines("**Start** with\n    a second line"),
            expected
        );
    }

    // handle-markdown-text.spec.ts: "markdownToLines - Only italic formatting".
    #[test]
    fn markdown_to_lines_reads_one_italic_run() {
        assert_eq!(
            markdown_to_lines("This is an *italic* test"),
            vec![normal(&[
                ("This", MarkdownWordKind::Normal),
                ("is", MarkdownWordKind::Normal),
                ("an", MarkdownWordKind::Normal),
                ("italic", MarkdownWordKind::Em),
                ("test", MarkdownWordKind::Normal),
            ])]
        );
    }

    // handle-markdown-text.spec.ts: the two "markdownToLines - Mixed formatting" tests.
    #[test]
    fn markdown_to_lines_reads_italic_then_bold() {
        assert_eq!(
            markdown_to_lines("*Italic* and **bold** formatting"),
            vec![normal(&[
                ("Italic", MarkdownWordKind::Em),
                ("and", MarkdownWordKind::Normal),
                ("bold", MarkdownWordKind::Strong),
                ("formatting", MarkdownWordKind::Normal),
            ])]
        );
        assert_eq!(
            markdown_to_lines("*Italic with space* and **bold ws** formatting"),
            vec![normal(&[
                ("Italic", MarkdownWordKind::Em),
                ("with", MarkdownWordKind::Em),
                ("space", MarkdownWordKind::Em),
                ("and", MarkdownWordKind::Normal),
                ("bold", MarkdownWordKind::Strong),
                ("ws", MarkdownWordKind::Strong),
                ("formatting", MarkdownWordKind::Normal),
            ])]
        );
    }

    // handle-markdown-text.spec.ts: the "The dog in **the** hog..." mixed test.
    #[test]
    fn markdown_to_lines_keeps_emphasis_inside_a_paragraph() {
        let input = "The dog in **the** hog... a *very long text* about it\nWord!";
        let expected = vec![
            normal(&[
                ("The", MarkdownWordKind::Normal),
                ("dog", MarkdownWordKind::Normal),
                ("in", MarkdownWordKind::Normal),
                ("the", MarkdownWordKind::Strong),
                ("hog...", MarkdownWordKind::Normal),
                ("a", MarkdownWordKind::Normal),
                ("very", MarkdownWordKind::Em),
                ("long", MarkdownWordKind::Em),
                ("text", MarkdownWordKind::Em),
                ("about", MarkdownWordKind::Normal),
                ("it", MarkdownWordKind::Normal),
            ]),
            normal(&[("Word!", MarkdownWordKind::Normal)]),
        ];
        assert_eq!(markdown_to_lines(input), expected);
    }

    // handle-markdown-text.spec.ts: "nonMarkdownToLines - splits text by newlines...".
    #[test]
    fn non_markdown_to_lines_splits_breaks_and_keeps_tags() {
        assert_eq!(
            non_markdown_to_lines("Line 1\\nLine 2<br/>Text with <strong>tag</strong>"),
            vec![
                normal(&[
                    ("Line", MarkdownWordKind::Normal),
                    ("1", MarkdownWordKind::Normal),
                ]),
                normal(&[
                    ("Line", MarkdownWordKind::Normal),
                    ("2", MarkdownWordKind::Normal),
                ]),
                normal(&[
                    ("Text", MarkdownWordKind::Normal),
                    ("with", MarkdownWordKind::Normal),
                    ("<strong>", MarkdownWordKind::Normal),
                    ("tag", MarkdownWordKind::Normal),
                    ("</strong>", MarkdownWordKind::Normal),
                ]),
            ]
        );
        assert_eq!(
            non_markdown_to_lines("**bold** and *italic*"),
            vec![normal(&[
                ("**bold**", MarkdownWordKind::Normal),
                ("and", MarkdownWordKind::Normal),
                ("*italic*", MarkdownWordKind::Normal),
            ])]
        );
    }

    #[test]
    fn markdown_to_lines_turns_br_into_a_line_break() {
        assert_eq!(
            markdown_to_lines("one<br/>two"),
            vec![
                normal(&[("one", MarkdownWordKind::Normal)]),
                normal(&[("two", MarkdownWordKind::Normal)]),
            ]
        );
    }

    #[test]
    fn markdown_to_lines_keeps_inline_html_as_a_word() {
        assert_eq!(
            markdown_to_lines("press <b>now</b>"),
            vec![normal(&[
                ("press", MarkdownWordKind::Normal),
                ("<b>", MarkdownWordKind::Normal),
                ("now", MarkdownWordKind::Normal),
                ("</b>", MarkdownWordKind::Normal),
            ])]
        );
    }

    #[test]
    fn markdown_to_lines_keeps_unsupported_blocks_as_raw_words() {
        let lines = markdown_to_lines("Hello\n  - l1\n  - l2");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0][0], word("Hello", MarkdownWordKind::Normal));
        assert_eq!(
            lines[0][1],
            word("  - l1\n  - l2", MarkdownWordKind::Normal)
        );
    }

    #[test]
    fn markdown_to_lines_decodes_the_apostrophe_entity() {
        assert_eq!(
            markdown_to_lines("it&#39;s"),
            vec![normal(&[("it's", MarkdownWordKind::Normal)])]
        );
    }

    #[test]
    fn split_breaks_handles_every_br_spelling() {
        assert_eq!(split_breaks("a<br>b"), vec!["a", "b"]);
        assert_eq!(split_breaks("a<br/>b"), vec!["a", "b"]);
        assert_eq!(split_breaks("a<BR />b"), vec!["a", "b"]);
        assert_eq!(split_breaks("a</br>b"), vec!["a", "b"]);
        assert_eq!(split_breaks("plain"), vec!["plain"]);
    }

    // createText.spec.ts: "decodes HTML entities in text".
    #[test]
    fn shape_text_decodes_html_entities() {
        for markdown in [true, false] {
            let block = shape_text(
                "5 &gt; 3 &amp;&amp; 2 &lt; 4",
                &TextOptions::default().with_markdown(markdown),
                &Font::default(),
                &FallbackMeasure,
            )
            .expect("text shapes");
            assert_eq!(block.lines.len(), 1);
            assert_eq!(block.lines[0].text(), "5 > 3 && 2 < 4");
        }
    }

    #[test]
    fn shape_text_decodes_only_one_entity_pass() {
        assert_eq!(decode_html_entities("&amp;lt;"), "&lt;");
        assert_eq!(decode_html_entities("&amp;"), "&");
        assert_eq!(decode_html_entities("a & b"), "a & b");
    }

    #[test]
    fn shape_text_marks_bold_and_italic_spans() {
        let block = shape_text(
            "plain **bold** *italic*",
            &TextOptions::unwrapped(),
            &Font::default(),
            &FallbackMeasure,
        )
        .expect("text shapes");
        let spans = &block.lines[0].spans;
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0], TextSpan::plain("plain"));
        assert_eq!(spans[1], TextSpan::bold(" bold"));
        assert_eq!(spans[2], TextSpan::italic(" italic"));
        assert_eq!(block.lines[0].text(), "plain bold italic");
    }

    #[test]
    fn shape_text_keeps_font_defaults_and_line_height() {
        let font = Font::default();
        let block = shape_text(
            "one\ntwo",
            &TextOptions::unwrapped(),
            &font,
            &FallbackMeasure,
        )
        .expect("text shapes");
        assert_eq!(block.lines.len(), 2);
        assert!((block.line_height - LINE_HEIGHT_EM * 12.0).abs() < 1e-9);
        let glyph = FallbackMeasure.line_height(&font);
        let expected_height = glyph + block.line_height;
        assert!((block.size.height - expected_height).abs() < 1e-9);
        assert!(block.size.width > 0.0);
    }

    #[test]
    fn shape_text_empty_label_measures_zero() {
        let block = shape_text(
            "",
            &TextOptions::default(),
            &Font::default(),
            &FallbackMeasure,
        )
        .expect("empty text shapes");
        assert_eq!(block.lines, vec![TextLine::default()]);
        assert_eq!(block.size, Size::ZERO);
    }

    #[test]
    fn shape_text_wraps_every_line_to_the_width() {
        let font = Font::default();
        let width = 40.0;
        let block = shape_text(
            "the quick brown fox jumps over the lazy dog",
            &TextOptions::default().with_width(width),
            &font,
            &FallbackMeasure,
        )
        .expect("text shapes");
        assert!(block.lines.len() > 1);
        for line in &block.lines {
            let line_width = FallbackMeasure.line_width(line, &font);
            assert!(
                line_width <= width,
                "line {:?} is {line_width} wide",
                line.text()
            );
        }
        let joined: String = block
            .lines
            .iter()
            .map(|line| line.text())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            joined.split_whitespace().collect::<Vec<_>>(),
            "the quick brown fox jumps over the lazy dog"
                .split_whitespace()
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn shape_text_with_zero_width_breaks_into_single_graphemes() {
        let block = shape_text(
            "abc",
            &TextOptions::default().with_width(0.0),
            &Font::default(),
            &FallbackMeasure,
        )
        .expect("text shapes");
        let lines: Vec<String> = block.lines.iter().map(TextLine::text).collect();
        assert_eq!(lines, vec!["a", "b", "c"]);
    }

    #[test]
    fn shape_text_infinite_width_does_not_wrap() {
        let block = shape_text(
            "a very long label that would wrap at two hundred pixels",
            &TextOptions::unwrapped(),
            &Font::default(),
            &FallbackMeasure,
        )
        .expect("text shapes");
        assert_eq!(block.lines.len(), 1);
        assert!(block.size.width > 200.0);
    }

    #[test]
    fn shape_text_honours_min_width() {
        let block = shape_text(
            "ab",
            &TextOptions::unwrapped().with_min_width(100.0),
            &Font::default(),
            &FallbackMeasure,
        )
        .expect("text shapes");
        assert_eq!(block.size.width, 100.0);
    }

    #[test]
    fn shape_text_without_markdown_keeps_markers() {
        let block = shape_text(
            "**bold** and *italic*",
            &TextOptions::unwrapped().with_markdown(false),
            &Font::default(),
            &FallbackMeasure,
        )
        .expect("text shapes");
        assert_eq!(block.lines[0].text(), "**bold** and *italic*");
        assert!(
            block.lines[0]
                .spans
                .iter()
                .all(|span| !span.bold && !span.italic)
        );
    }

    #[test]
    fn shape_text_breaks_on_br_before_wrapping() {
        let block = shape_text(
            "one<br/>two",
            &TextOptions::unwrapped(),
            &Font::default(),
            &FallbackMeasure,
        )
        .expect("text shapes");
        assert_eq!(
            block.lines.iter().map(TextLine::text).collect::<Vec<_>>(),
            vec!["one", "two"]
        );
    }

    #[test]
    fn fallback_dimensions_follow_calculate_text_dimensions() {
        struct Fixed;
        impl TextMeasure for Fixed {
            fn width(&self, text: &str, _font: &Font) -> f64 {
                text.chars().count() as f64 * 10.0
            }
            fn line_height(&self, _font: &Font) -> f64 {
                12.0
            }
        }
        assert_eq!(Fixed.dimensions("", &Font::default()), TextDimensions::ZERO);
        // `calculateTextDimensions` splits on `<br>` tags only, not on newlines.
        assert_eq!(
            Fixed.dimensions("ab\nc", &Font::default()),
            TextDimensions::new(40.0, 12.0, 12.0)
        );
        assert_eq!(
            Fixed.dimensions("ab<br>cdef", &Font::default()),
            TextDimensions::new(40.0, 24.0, 12.0)
        );
    }

    #[test]
    fn fallback_widths_are_deterministic_and_shape_aware() {
        let font = Font::default();
        assert_eq!(
            FallbackMeasure.width("iiii", &font),
            FallbackMeasure.width("iiii", &font)
        );
        assert!(FallbackMeasure.width("iiii", &font) < FallbackMeasure.width("MMMM", &font));
        assert!(
            FallbackMeasure.width("abc", &font.clone().bold())
                > FallbackMeasure.width("abc", &font)
        );
        let mono = Font::new("monospace", 12.0);
        assert!((FallbackMeasure.width("abc", &mono) - 3.0 * 12.0 * 0.6).abs() < 1e-9);
        assert_eq!(FallbackMeasure.line_height(&font), 12.0 * 1.15);
    }

    #[test]
    fn fallback_span_font_applies_span_styles() {
        let base = Font::default();
        assert_eq!(
            FallbackMeasure.span_font(&base, &TextSpan::plain("x")),
            base
        );
        assert_eq!(
            FallbackMeasure
                .span_font(&base, &TextSpan::bold("x"))
                .weight,
            BOLD_WEIGHT
        );
        assert!(
            FallbackMeasure
                .span_font(&base, &TextSpan::italic("x"))
                .italic
        );
        assert_eq!(
            FallbackMeasure
                .span_font(&base, &TextSpan::code("x"))
                .family,
            "monospace"
        );
    }

    #[test]
    fn split_line_to_words_keeps_whitespace_runs() {
        assert_eq!(split_line_to_words(""), Vec::<String>::new());
        assert_eq!(
            split_line_to_words("hello world"),
            vec!["hello", " ", "world"]
        );
        assert_eq!(split_line_to_words("hello  1"), vec!["hello", "  ", "1"]);
    }

    #[test]
    fn markdown_to_lines_reads_underscore_emphasis_but_not_inside_words() {
        assert_eq!(
            markdown_to_lines("_one_ and __two__"),
            vec![normal(&[
                ("one", MarkdownWordKind::Em),
                ("and", MarkdownWordKind::Normal),
                ("two", MarkdownWordKind::Strong),
            ])]
        );
        assert_eq!(
            markdown_to_lines("snake_case_name"),
            vec![normal(&[("snake_case_name", MarkdownWordKind::Normal)])]
        );
    }

    #[test]
    fn markdown_to_lines_dedents_a_leading_blank_line() {
        assert_eq!(
            markdown_to_lines("\n\tfirst\n\tsecond"),
            vec![
                normal(&[("first", MarkdownWordKind::Normal)]),
                normal(&[("second", MarkdownWordKind::Normal)]),
            ]
        );
    }

    #[test]
    fn shape_text_ignores_a_trailing_break_for_its_height() {
        let font = Font::default();
        let block = shape_text(
            "one<br/>",
            &TextOptions::unwrapped().with_markdown(false),
            &font,
            &FallbackMeasure,
        )
        .expect("text shapes");
        assert_eq!(
            block.lines.iter().map(TextLine::text).collect::<Vec<_>>(),
            vec!["one", ""]
        );
        assert!((block.size.height - FallbackMeasure.line_height(&font)).abs() < 1e-9);
    }
}
