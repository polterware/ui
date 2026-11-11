//! The `@{ ... }` metadata attached to nodes, edges, and subgraphs.
//!
//! Upstream feeds the lexed `SHAPE_DATA` text to js-yaml with `JSON_SCHEMA`, then removes the
//! keys only a pool/lane diagram may set (`flowDb.ts` `INTERNAL_METADATA_KEYS`), and validates
//! `shape` against the shape registry. This module ports the subset of YAML the flowchart
//! metadata syntax produces: one-level block mappings, flow mappings and sequences, quoted and
//! plain scalars, and `|`/`>` block scalars. It also holds the shape names and the plain-text
//! sanitizer the labels go through (`common.ts` `sanitizeText` with `htmlLabels` on).

use serde_json::{Map, Number, Value};

/// Keys set by a diagram that models pools and lanes itself, never by the author of a `@{ }`.
const INTERNAL_METADATA_KEYS: [&str; 3] = ["laneRole", "laneIndex", "drawnExtent"];

/// Parses the metadata text the lexer assembled from a `@{ ... }` block.
///
/// Upstream wraps text without a newline in `{ ... }` and parses everything else as a block
/// mapping. The same split happens here so both spellings of the same block agree.
pub fn parse(text: &str) -> Result<Value, String> {
    if text.contains('\n') {
        parse_block_map(&format!("{text}\n"))
    } else {
        Flow::new(&format!("{{\n{text}\n}}")).parse_map()
    }
}

/// Drops the keys a `@{ }` block may not set on a node or subgraph.
pub fn without_internal_keys(doc: Value) -> Value {
    match doc {
        Value::Object(mut map) => {
            for key in INTERNAL_METADATA_KEYS {
                map.remove(key);
            }
            Value::Object(map)
        }
        other => other,
    }
}

/// Returns the object a parsed `@{ }` block holds, or `None` when it is not a mapping.
pub fn as_map(doc: &Value) -> Option<&Map<String, Value>> {
    doc.as_object()
}

/// The shape names upstream accepts in `@{ shape: ... }` (the shape registry's short names,
/// aliases, and internal aliases).
pub fn is_valid_shape(shape: &str) -> bool {
    SHAPES.contains(&shape)
}

/// The names and aliases of `rendering-util/rendering-elements/shapes.ts`.
const SHAPES: &[&str] = &[
    "bang",
    "bolt",
    "bow-rect",
    "bow-tie-rectangle",
    "brace",
    "brace-l",
    "brace-r",
    "braces",
    "browser",
    "bucket",
    "card",
    "circ",
    "circle",
    "cloud",
    "collate",
    "com-link",
    "comment",
    "console",
    "cross-circ",
    "crossed-circle",
    "curv-trap",
    "curved-trapezoid",
    "cyl",
    "cylinder",
    "das",
    "data-store",
    "database",
    "datastore",
    "db",
    "dbl-circ",
    "decision",
    "delay",
    "diam",
    "diamond",
    "directory",
    "disk",
    "display",
    "div-proc",
    "div-rect",
    "divided-process",
    "divided-rectangle",
    "doc",
    "docs",
    "document",
    "documents",
    "double-circle",
    "doublecircle",
    "event",
    "extract",
    "f-circ",
    "filled-circle",
    "flag",
    "flip-tri",
    "flipped-triangle",
    "folder",
    "fork",
    "forkJoin",
    "fr-circ",
    "fr-rect",
    "framed-circle",
    "framed-rectangle",
    "h-cyl",
    "half-rounded-rectangle",
    "hex",
    "hexagon",
    "horizontal-cylinder",
    "hourglass",
    "in-out",
    "internal-storage",
    "inv-trapezoid",
    "inv_trapezoid",
    "join",
    "junction",
    "lean-l",
    "lean-left",
    "lean-r",
    "lean-right",
    "lean_left",
    "lean_right",
    "lightning-bolt",
    "lin-cyl",
    "lin-doc",
    "lin-proc",
    "lin-rect",
    "lined-cylinder",
    "lined-document",
    "lined-process",
    "lined-rectangle",
    "loop-limit",
    "manual",
    "manual-file",
    "manual-input",
    "notch-pent",
    "notch-rect",
    "notched-pentagon",
    "notched-rectangle",
    "odd",
    "out-in",
    "paper-tape",
    "person",
    "pill",
    "prepare",
    "priority",
    "proc",
    "process",
    "processes",
    "procs",
    "question",
    "rect",
    "rect_left_inv_arrow",
    "rectangle",
    "rounded",
    "roundedRect",
    "shaded-process",
    "sl-rect",
    "sloped-rectangle",
    "sm-circ",
    "small-circle",
    "squareRect",
    "st-doc",
    "st-rect",
    "stacked-document",
    "stacked-rectangle",
    "stadium",
    "start",
    "stateEnd",
    "stateStart",
    "stop",
    "stored-data",
    "subproc",
    "subprocess",
    "subroutine",
    "summary",
    "tag-doc",
    "tag-proc",
    "tag-rect",
    "tagged-document",
    "tagged-process",
    "tagged-rectangle",
    "terminal",
    "text",
    "trap-b",
    "trap-t",
    "trapezoid",
    "trapezoid-bottom",
    "trapezoid-top",
    "tri",
    "triangle",
    "win-pane",
    "window-pane",
];

/// Approximates `common.ts` `sanitizeText` with `htmlLabels` on: DOMPurify drops `script` and
/// `style` elements, escapes a `<` that opens no tag, and keeps the rest of the markup.
pub fn sanitize_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        if let Some(consumed) = strip_element(rest) {
            i += consumed;
            continue;
        }
        if rest.starts_with('<') {
            if let Some(end) = tag_end(rest) {
                out.push_str(&rest[..end]);
                i += end;
                continue;
            }
            out.push_str("&lt;");
            i += 1;
            continue;
        }
        let ch = rest.chars().next().expect("non-empty remainder");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Length of a `script`/`style` element with its content, when `text` starts with one.
fn strip_element(text: &str) -> Option<usize> {
    let (name, open_end) = opening_tag(text)?;
    let lower = name.to_ascii_lowercase();
    if lower != "script" && lower != "style" {
        return None;
    }
    let closing = format!("</{lower}");
    let rest = &text[open_end..];
    let lower_rest = rest.to_ascii_lowercase();
    if let Some(found) = lower_rest.find(&closing) {
        let after = open_end + found;
        return Some(tag_end(&text[after..]).map_or(text.len(), |end| after + end));
    }
    Some(text.len())
}

/// The tag name and the byte length of the opening tag when `text` starts with an element.
fn opening_tag(text: &str) -> Option<(String, usize)> {
    let mut chars = text.char_indices();
    let (_, first) = chars.next()?;
    if first != '<' {
        return None;
    }
    let mut name = String::new();
    for (index, ch) in chars {
        if ch.is_ascii_alphabetic() {
            name.push(ch);
        } else if ch.is_ascii_whitespace() || ch == '>' || ch == '/' {
            if name.is_empty() {
                return None;
            }
            let end = text[index..].find('>')? + index + 1;
            return Some((name, end));
        } else {
            return None;
        }
    }
    if name.is_empty() {
        None
    } else {
        Some((name, text.len()))
    }
}

/// The byte length of the tag starting at `<`, or `None` when it is not a tag.
fn tag_end(text: &str) -> Option<usize> {
    if !text.starts_with('<') {
        return None;
    }
    let next = text[1..].chars().next()?;
    if !(next.is_ascii_alphabetic() || next == '/' || next == '!' || next == '?') {
        return None;
    }
    if text.starts_with("<!--") {
        let end = text.find("-->")?;
        return Some(end + 3);
    }
    text.find('>').map(|index| index + 1)
}

/// A flow-mapping/sequence parser for the single-line spelling of `@{ ... }`.
struct Flow<'a> {
    text: &'a str,
    index: usize,
}

impl<'a> Flow<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, index: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.text[self.index..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.index += ch.len_utf8();
        Some(ch)
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.bump();
            } else {
                break;
            }
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), String> {
        self.skip_whitespace();
        match self.bump() {
            Some(ch) if ch == expected => Ok(()),
            other => Err(format!("expected '{expected}', found {other:?}")),
        }
    }

    fn parse_map(&mut self) -> Result<Value, String> {
        self.expect('{')?;
        let mut map = Map::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some('}') => {
                    self.bump();
                    return Ok(Value::Object(map));
                }
                None => return Err("unterminated flow mapping".to_string()),
                _ => {}
            }
            let key = self.parse_key()?;
            self.expect(':')?;
            let value = self.parse_value()?;
            map.insert(key, value);
            self.skip_whitespace();
            match self.peek() {
                Some(',') => {
                    self.bump();
                }
                Some('}') => {
                    self.bump();
                    return Ok(Value::Object(map));
                }
                other => return Err(format!("expected ',', found {other:?}")),
            }
        }
    }

    fn parse_sequence(&mut self) -> Result<Value, String> {
        self.expect('[')?;
        let mut items = Vec::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(']') => {
                    self.bump();
                    return Ok(Value::Array(items));
                }
                None => return Err("unterminated flow sequence".to_string()),
                _ => {}
            }
            items.push(self.parse_value()?);
            self.skip_whitespace();
            match self.peek() {
                Some(',') => {
                    self.bump();
                }
                Some(']') => {
                    self.bump();
                    return Ok(Value::Array(items));
                }
                other => return Err(format!("expected ',', found {other:?}")),
            }
        }
    }

    fn parse_key(&mut self) -> Result<String, String> {
        self.skip_whitespace();
        match self.peek() {
            Some('"') => match self.parse_double_quoted()? {
                Value::String(key) => Ok(key),
                other => Ok(other.to_string()),
            },
            Some('\'') => match self.parse_single_quoted()? {
                Value::String(key) => Ok(key),
                other => Ok(other.to_string()),
            },
            _ => {
                let start = self.index;
                while let Some(ch) = self.peek() {
                    if ch == ':' || ch == '}' || ch == ',' {
                        break;
                    }
                    self.bump();
                }
                let key = self.text[start..self.index].trim().to_string();
                if key.is_empty() {
                    Err("empty metadata key".to_string())
                } else {
                    Ok(key)
                }
            }
        }
    }

    fn parse_value(&mut self) -> Result<Value, String> {
        self.skip_whitespace();
        match self.peek() {
            Some('{') => self.parse_map(),
            Some('[') => self.parse_sequence(),
            Some('"') => self.parse_double_quoted(),
            Some('\'') => self.parse_single_quoted(),
            _ => Ok(plain_scalar(self.parse_plain(&[',', '}', ']', '\n']))),
        }
    }

    fn parse_plain(&mut self, stop: &[char]) -> &'a str {
        let start = self.index;
        while let Some(ch) = self.peek() {
            if stop.contains(&ch) {
                break;
            }
            self.bump();
        }
        self.text[start..self.index].trim_end()
    }

    fn parse_double_quoted(&mut self) -> Result<Value, String> {
        self.bump();
        let mut out = String::new();
        loop {
            match self.bump() {
                Some('"') => return Ok(Value::String(out)),
                Some('\\') => match self.bump() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some('0') => out.push('\0'),
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some('/') => out.push('/'),
                    Some('u') => {
                        let mut code = 0u32;
                        for _ in 0..4 {
                            let digit = self.bump().and_then(|ch| ch.to_digit(16));
                            code = code * 16 + digit.ok_or("bad \\u escape")?;
                        }
                        out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                    }
                    Some(other) => out.push(other),
                    None => return Err("unterminated string".to_string()),
                },
                Some(ch) => out.push(ch),
                None => return Err("unterminated string".to_string()),
            }
        }
    }

    fn parse_single_quoted(&mut self) -> Result<Value, String> {
        self.bump();
        let mut out = String::new();
        loop {
            match self.bump() {
                Some('\'') => {
                    if self.peek() == Some('\'') {
                        self.bump();
                        out.push('\'');
                    } else {
                        return Ok(Value::String(out));
                    }
                }
                Some(ch) => out.push(ch),
                None => return Err("unterminated string".to_string()),
            }
        }
    }
}

/// Parses the block-mapping spelling of `@{ ... }`.
fn parse_block_map(text: &str) -> Result<Value, String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut index = 0;
    while index < lines.len() && lines[index].trim().is_empty() {
        index += 1;
    }
    if index == lines.len() {
        return Ok(Value::Object(Map::new()));
    }
    let indent = indent_of(lines[index]);
    parse_map_lines(&lines, &mut index, indent)
}

fn parse_map_lines(lines: &[&str], index: &mut usize, indent: usize) -> Result<Value, String> {
    let mut map = Map::new();
    while *index < lines.len() {
        let line = lines[*index];
        if line.trim().is_empty() {
            *index += 1;
            continue;
        }
        let line_indent = indent_of(line);
        if line_indent < indent {
            break;
        }
        if line_indent > indent {
            return Err("unexpected indentation in metadata".to_string());
        }
        let rest = &line[indent..];
        let colon = key_colon(rest)?;
        let key = unquote(rest[..colon].trim())?;
        let after = &rest[colon + 1..];
        let after = after.trim_start();
        *index += 1;
        let value = if after.is_empty() {
            match next_content_line(lines, *index) {
                Some((next, next_indent)) if next_indent > indent => {
                    parse_map_lines(lines, index, next_indent)?
                }
                _ => Value::Null,
            }
        } else if after.starts_with('|') || after.starts_with('>') {
            block_scalar(lines, index, indent, after)
        } else if after.starts_with('{') || after.starts_with('[') {
            let collected = collect_flow(lines, index, after)?;
            let mut flow = Flow::new(&collected);
            let value = flow.parse_value()?;
            flow.skip_whitespace();
            if flow.peek().is_some() {
                return Err("trailing text after flow value".to_string());
            }
            value
        } else if after.starts_with('"') || after.starts_with('\'') {
            let mut flow = Flow::new(after);
            flow.parse_value()?
        } else {
            plain_scalar(after)
        };
        map.insert(key, value);
    }
    Ok(Value::Object(map))
}

fn next_content_line<'a>(lines: &[&'a str], mut index: usize) -> Option<(&'a str, usize)> {
    while index < lines.len() {
        if !lines[index].trim().is_empty() {
            return Some((lines[index], indent_of(lines[index])));
        }
        index += 1;
    }
    None
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn key_colon(text: &str) -> Result<usize, String> {
    let mut quote: Option<char> = None;
    for (index, ch) in text.char_indices() {
        match quote {
            Some(expected) if ch == expected => quote = None,
            Some(_) => {}
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch == ':' => return Ok(index),
            None => {}
        }
    }
    Err("metadata line has no ':'".to_string())
}

fn unquote(text: &str) -> Result<String, String> {
    let mut flow = Flow::new(text);
    match flow.parse_value()? {
        Value::String(key) => Ok(key),
        other => Ok(other.to_string()),
    }
}

fn collect_flow(lines: &[&str], index: &mut usize, first: &str) -> Result<String, String> {
    let mut text = first.to_string();
    let mut depth = depth_delta(first);
    while depth > 0 && *index < lines.len() {
        text.push('\n');
        text.push_str(lines[*index]);
        depth += depth_delta(lines[*index]);
        *index += 1;
    }
    if depth > 0 {
        return Err("unterminated flow collection".to_string());
    }
    Ok(text)
}

fn depth_delta(text: &str) -> i32 {
    let mut delta = 0;
    let mut quote: Option<char> = None;
    for ch in text.chars() {
        match quote {
            Some(expected) if ch == expected => quote = None,
            Some(_) => {}
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch == '{' || ch == '[' => delta += 1,
            None if ch == '}' || ch == ']' => delta -= 1,
            None => {}
        }
    }
    delta
}

fn block_scalar(lines: &[&str], index: &mut usize, parent_indent: usize, indicator: &str) -> Value {
    let folded = indicator.starts_with('>');
    let strip = indicator.contains('-');
    let keep = indicator.contains('+');
    let mut content: Vec<String> = Vec::new();
    let mut block_indent: Option<usize> = None;
    while *index < lines.len() {
        let line = lines[*index];
        if line.trim().is_empty() {
            content.push(String::new());
            *index += 1;
            continue;
        }
        let line_indent = indent_of(line);
        if line_indent <= parent_indent {
            break;
        }
        let indent = *block_indent.get_or_insert(line_indent);
        if line_indent < indent {
            break;
        }
        content.push(line[indent..].to_string());
        *index += 1;
    }
    while content.last().is_some_and(|line| line.is_empty()) && !keep {
        content.pop();
    }
    let mut text = if folded {
        folded_scalar(&content)
    } else {
        content.join("\n")
    };
    if strip {
        text.truncate(text.trim_end_matches('\n').len());
    } else if !content.is_empty() {
        text.push('\n');
    }
    Value::String(text)
}

fn folded_scalar(lines: &[String]) -> String {
    let mut out = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            if line.is_empty() || lines[index - 1].is_empty() {
                out.push('\n');
            } else {
                out.push(' ');
            }
        }
        out.push_str(line);
    }
    out
}

fn plain_scalar(text: &str) -> Value {
    let text = text.trim();
    match text {
        "" | "~" | "null" | "Null" | "NULL" => Value::Null,
        "true" | "True" | "TRUE" => Value::Bool(true),
        "false" | "False" | "FALSE" => Value::Bool(false),
        _ => {
            if let Ok(integer) = text.parse::<i64>() {
                return Value::Number(Number::from(integer));
            }
            if let Ok(float) = text.parse::<f64>()
                && let Some(number) = Number::from_f64(float)
            {
                return Value::Number(number);
            }
            Value::String(text.to_string())
        }
    }
}
