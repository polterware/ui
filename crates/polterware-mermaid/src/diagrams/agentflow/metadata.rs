//! `@{ ... }` metadata and the shape vocabulary of agentflow.
//!
//! Upstream feeds the lexed `SHAPE_DATA` text to js-yaml with `JSON_SCHEMA`
//! (`agentflowDb.ts` `addVertex`): a body without a newline is wrapped in `{ ... }` and read as
//! a flow mapping, a body with one is read as block YAML. When block YAML fails, upstream
//! retries once with line-trailing commas stripped (`stripLineTrailingCommas`). This module
//! ports that reader for the subset the syntax produces: block and flow mappings and
//! sequences, quoted and plain scalars (including a quoted scalar folded across lines), block
//! scalars, and comments. Anchors, aliases, and tags are rejected, as `JSON_SCHEMA` would.
//!
//! The shape vocabulary is `shapes.ts`: the v0.8.1 aliases, the removed shapes, and the
//! allowed shapes. `sanitizeText` is approximated the way the flowchart port does it.

use serde_json::{Map, Number, Value};

/// The v0.8.1 shape aliases (§4.3.2), from `shapes.ts` `SHAPE_ALIASES`.
pub const SHAPE_ALIASES: [(&str, &str); 8] = [
    ("task", "roundedRect"),
    ("tool", "subroutine"),
    ("input", "lean-right"),
    ("lean_right", "lean-right"),
    ("decision", "diamond"),
    ("refdoc", "lin-doc"),
    ("action", "hexagon"),
    ("round", "rect"),
];

/// Shapes that mark a vertex as a tool definition (`SUBROUTINE_ALIASES`).
pub const SUBROUTINE_ALIASES: [&str; 5] = [
    "subroutine",
    "subprocess",
    "subproc",
    "framed-rectangle",
    "tool",
];

/// Shapes removed in v0.8.1 (`REMOVED_SHAPES`).
pub const REMOVED_SHAPES: [&str; 27] = [
    "doc",
    "stadium",
    "terminal",
    "circle",
    "trapezoid",
    "inv_trapezoid",
    "inv-trapezoid",
    "doublecircle",
    "double-circle",
    "typeDeclaration",
    "procs",
    "lean_left",
    "lean-left",
    "in-out",
    "cylinder",
    "ellipse",
    "odd",
    "tag-rect",
    "tagged-rectangle",
    "delay",
    "half-rounded-rectangle",
    "lin-rect",
    "lined-rectangle",
    "win-pane",
    "window-pane",
    "curv-trap",
    "curved-trapezoid",
];

/// Shapes the v0.8.1 spec allows after alias resolution (`ALLOWED_SHAPES`).
pub const ALLOWED_SHAPES: [&str; 13] = [
    "roundedRect",
    "subroutine",
    "subprocess",
    "subproc",
    "framed-rectangle",
    "lean-right",
    "diamond",
    "lin-doc",
    "lined-document",
    "hexagon",
    "hex",
    "connector",
    "collapsedGroup",
];

/// `DEFAULT_SHAPE`.
pub const DEFAULT_SHAPE: &str = "roundedRect";

/// `resolveShapeAlias`: the canonical id for an alias, or the input unchanged.
pub fn resolve_shape_alias(shape: Option<&str>) -> Option<&str> {
    let shape = shape?;
    if let Some((_, resolved)) = SHAPE_ALIASES.iter().find(|(alias, _)| *alias == shape) {
        return Some(resolved);
    }
    Some(shape)
}

/// The shape registry `isValidShape` consults: the names and aliases of
/// `rendering-util/rendering-elements/shapes.ts`.
pub fn is_valid_shape(shape: &str) -> bool {
    SHAPES.contains(&shape)
}

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

/// A YAML failure from a `@{ ... }` body, with a 0-based line and column in that body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl MetadataError {
    fn new(message: impl Into<String>, line: usize, column: usize) -> Self {
        Self {
            message: message.into(),
            line,
            column,
        }
    }
}

/// Parses the metadata text the lexer assembled from a `@{ ... }` block.
pub fn parse(text: &str) -> Result<Value, MetadataError> {
    if text.contains('\n') {
        parse_block(&format!("{text}\n"))
    } else {
        let wrapped = format!("{{\n{text}\n}}");
        let mut flow = Flow::new(&wrapped);
        flow.parse_value()
    }
}

/// Upstream's retry input: the block body with line-trailing commas removed.
///
/// Commas inside quoted scalars, flow collections, and block scalar content survive.
pub fn strip_line_trailing_commas(body: &str) -> String {
    let mut in_single = false;
    let mut in_double = false;
    let mut flow_depth = 0i32;
    let mut block_scalar_indent: Option<usize> = None;
    let mut out = Vec::new();
    for line in body.split('\n') {
        if let Some(parent) = block_scalar_indent {
            let indent = indent_of(line);
            if line.trim().is_empty() || indent > parent {
                out.push(line.to_string());
                continue;
            }
            block_scalar_indent = None;
        }
        let mut comment_start = None;
        let mut index = 0;
        while index < line.len() {
            let ch = line[index..]
                .chars()
                .next()
                .expect("index is on a char boundary");
            if in_double {
                if ch == '\\' {
                    index += ch.len_utf8();
                    if let Some(next) = line[index..].chars().next() {
                        index += next.len_utf8();
                    }
                    continue;
                }
                if ch == '"' {
                    in_double = false;
                }
            } else if in_single {
                if ch == '\'' {
                    in_single = false;
                }
            } else if ch == '"' {
                in_double = true;
            } else if ch == '\'' {
                in_single = true;
            } else if ch == '#'
                && (index == 0 || matches!(line.as_bytes()[index - 1], b' ' | b'\t'))
            {
                comment_start = Some(index);
                break;
            } else if ch == '[' || ch == '{' {
                flow_depth += 1;
            } else if ch == ']' || ch == '}' {
                flow_depth = (flow_depth - 1).max(0);
            }
            index += ch.len_utf8();
        }
        if in_single || in_double || flow_depth > 0 {
            out.push(line.to_string());
            continue;
        }
        let (code, comment) = match comment_start {
            Some(start) => (&line[..start], &line[start..]),
            None => (line, ""),
        };
        if opens_block_scalar(code) {
            block_scalar_indent = Some(indent_of(line));
            out.push(line.to_string());
            continue;
        }
        out.push(format!("{}{comment}", strip_one_trailing_comma(code)));
    }
    out.join("\n")
}

fn opens_block_scalar(code: &str) -> bool {
    let code = code.trim_end();
    let Some(colon) = code.rfind(':') else {
        return false;
    };
    let value = code[colon + 1..].trim();
    let mut chars = value.chars();
    matches!(chars.next(), Some('|') | Some('>'))
        && chars.all(|ch| ch.is_ascii_digit() || matches!(ch, '+' | '-'))
}

fn strip_one_trailing_comma(code: &str) -> &str {
    let trimmed = code.trim_end();
    match trimmed.strip_suffix(',') {
        Some(before) => before.trim_end(),
        None => code,
    }
}

/// Removes `__proto__`, `constructor`, and `prototype` keys, recursively (`stripPrototypeKeys`).
pub fn strip_prototype_keys(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(strip_prototype_keys).collect()),
        Value::Object(map) => {
            let mut clean = Map::new();
            for (key, value) in map {
                if key == "__proto__" || key == "constructor" || key == "prototype" {
                    continue;
                }
                clean.insert(key, strip_prototype_keys(value));
            }
            Value::Object(clean)
        }
        other => other,
    }
}

/// JavaScript truthiness for the values js-yaml can produce.
pub fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(values) => !values.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

/// Approximates `common.ts` `sanitizeText`: DOMPurify drops `script` and `style` elements,
/// escapes a `<` that opens no tag, and keeps the rest of the markup.
pub fn sanitize_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        let rest = &text[index..];
        if let Some(consumed) = strip_element(rest) {
            index += consumed;
            continue;
        }
        if rest.starts_with('<') {
            if let Some(end) = tag_end(rest) {
                out.push_str(&rest[..end]);
                index += end;
                continue;
            }
            out.push_str("&lt;");
            index += 1;
            continue;
        }
        let ch = rest.chars().next().expect("non-empty remainder");
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

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

/// A flow-mapping/sequence parser, shared by the single-line body and flow values.
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

    fn expect(&mut self, expected: char) -> Result<(), MetadataError> {
        self.skip_whitespace();
        match self.bump() {
            Some(ch) if ch == expected => Ok(()),
            other => Err(self.error(format!("expected '{expected}', found {other:?}"))),
        }
    }

    fn error(&self, message: impl Into<String>) -> MetadataError {
        let before = &self.text[..self.index.min(self.text.len())];
        let line = before.matches('\n').count();
        let column = before
            .rsplit_once('\n')
            .map_or(before.chars().count(), |(_, tail)| tail.chars().count());
        MetadataError::new(message, line, column)
    }

    fn parse_value(&mut self) -> Result<Value, MetadataError> {
        self.skip_whitespace();
        match self.peek() {
            Some('{') => self.parse_map(),
            Some('[') => self.parse_sequence(),
            Some('"') => self.parse_double_quoted(),
            Some('\'') => self.parse_single_quoted(),
            Some('*') => Err(self.error("alias nodes are not supported")),
            _ => Ok(plain_scalar(self.parse_plain(&[',', '}', ']', '\n']))),
        }
    }

    fn parse_map(&mut self) -> Result<Value, MetadataError> {
        self.expect('{')?;
        let mut map = Map::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some('}') => {
                    self.bump();
                    return Ok(Value::Object(map));
                }
                None => return Err(self.error("unterminated flow mapping")),
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
                other => return Err(self.error(format!("expected ',', found {other:?}"))),
            }
        }
    }

    fn parse_sequence(&mut self) -> Result<Value, MetadataError> {
        self.expect('[')?;
        let mut items = Vec::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(']') => {
                    self.bump();
                    return Ok(Value::Array(items));
                }
                None => return Err(self.error("unterminated flow sequence")),
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
                other => return Err(self.error(format!("expected ',', found {other:?}"))),
            }
        }
    }

    fn parse_key(&mut self) -> Result<String, MetadataError> {
        self.skip_whitespace();
        match self.peek() {
            Some('*') | Some('&') | Some('!') => {
                Err(self.error("alias and tag nodes are not supported"))
            }
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
                    Err(self.error("empty metadata key"))
                } else {
                    Ok(key)
                }
            }
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

    fn parse_double_quoted(&mut self) -> Result<Value, MetadataError> {
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
                            code = code * 16 + digit.ok_or_else(|| self.error("bad \\u escape"))?;
                        }
                        out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                    }
                    Some(other) => out.push(other),
                    None => return Err(self.error("unterminated string")),
                },
                Some(ch) => out.push(ch),
                None => return Err(self.error("unterminated string")),
            }
        }
    }

    fn parse_single_quoted(&mut self) -> Result<Value, MetadataError> {
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
                None => return Err(self.error("unterminated string")),
            }
        }
    }
}

/// Parses a block-mapping body.
fn parse_block(text: &str) -> Result<Value, MetadataError> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut index = 0;
    skip_blank_lines(&lines, &mut index);
    if index == lines.len() {
        return Ok(Value::Null);
    }
    parse_block_node(&lines, &mut index)
}

fn skip_blank_lines(lines: &[&str], index: &mut usize) {
    while *index < lines.len() {
        let trimmed = lines[*index].trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            *index += 1;
        } else {
            break;
        }
    }
}

fn parse_block_node(lines: &[&str], index: &mut usize) -> Result<Value, MetadataError> {
    skip_blank_lines(lines, index);
    if *index >= lines.len() {
        return Ok(Value::Null);
    }
    reject_tab_indent(lines[*index], *index)?;
    let indent = indent_of(lines[*index]);
    if is_sequence_entry(lines[*index].trim_start()) {
        parse_sequence_lines(lines, index, indent)
    } else {
        parse_map_lines(lines, index, indent)
    }
}

fn parse_map_lines(
    lines: &[&str],
    index: &mut usize,
    indent: usize,
) -> Result<Value, MetadataError> {
    let mut map = Map::new();
    loop {
        skip_blank_lines(lines, index);
        if *index >= lines.len() {
            break;
        }
        let line = lines[*index];
        reject_tab_indent(line, *index)?;
        let line_indent = indent_of(line);
        if line_indent < indent {
            break;
        }
        if line_indent > indent {
            return Err(error_at(
                line,
                *index,
                line_indent,
                "unexpected indentation",
            ));
        }
        let content = &line[indent..];
        if is_sequence_entry(content) {
            break;
        }
        let Some((key, rest)) = mapping_split(content) else {
            return Err(error_at(
                line,
                *index,
                line_indent,
                "could not find expected ':'",
            ));
        };
        *index += 1;
        let value = parse_block_value(lines, index, indent, rest)?;
        map.insert(key, value);
    }
    Ok(Value::Object(map))
}

fn parse_block_value(
    lines: &[&str],
    index: &mut usize,
    parent_indent: usize,
    rest: &str,
) -> Result<Value, MetadataError> {
    let rest = rest.trim();
    if rest.is_empty() {
        skip_blank_lines(lines, index);
        if *index < lines.len() {
            let next_indent = indent_of(lines[*index]);
            if next_indent > parent_indent {
                return parse_block_node(lines, index);
            }
        }
        return Ok(Value::Null);
    }
    if rest.starts_with('|') || rest.starts_with('>') {
        return Ok(Value::String(block_scalar(
            lines,
            index,
            parent_indent,
            rest,
        )));
    }
    if rest.starts_with('[') || rest.starts_with('{') {
        let collected = collect_flow(lines, index, rest)?;
        let mut flow = Flow::new(&collected);
        return flow.parse_value();
    }
    if rest.starts_with('"') || rest.starts_with('\'') {
        let quote = rest.chars().next().expect("checked above");
        let collected = collect_quoted(lines, index, rest, quote)?;
        let mut flow = Flow::new(&collected);
        return flow.parse_value();
    }
    if rest.starts_with('*') || rest.starts_with('&') || rest.starts_with('!') {
        return Err(MetadataError::new(
            "alias and tag nodes are not supported",
            0,
            0,
        ));
    }
    let plain = strip_inline_comment(rest);
    if plain.trim_end().ends_with(',') {
        return Err(MetadataError::new(
            "block scalar values cannot end with ','",
            0,
            0,
        ));
    }
    Ok(plain_scalar(plain))
}

fn parse_sequence_lines(
    lines: &[&str],
    index: &mut usize,
    indent: usize,
) -> Result<Value, MetadataError> {
    let mut items = Vec::new();
    loop {
        skip_blank_lines(lines, index);
        if *index >= lines.len() {
            break;
        }
        let line = lines[*index];
        reject_tab_indent(line, *index)?;
        if indent_of(line) != indent {
            break;
        }
        let content = &line[indent..];
        if !is_sequence_entry(content) {
            break;
        }
        let rest = content[1..].trim_start();
        *index += 1;
        if rest.is_empty() {
            skip_blank_lines(lines, index);
            if *index < lines.len() && indent_of(lines[*index]) > indent {
                items.push(parse_block_node(lines, index)?);
            } else {
                items.push(Value::Null);
            }
            continue;
        }
        if let Some((key, value)) = mapping_split(rest) {
            items.push(parse_compact_mapping(lines, index, indent, key, value)?);
            continue;
        }
        items.push(parse_inline_value(lines, index, indent, rest)?);
    }
    Ok(Value::Array(items))
}

fn parse_compact_mapping(
    lines: &[&str],
    index: &mut usize,
    sequence_indent: usize,
    first_key: String,
    first_rest: &str,
) -> Result<Value, MetadataError> {
    let mut map = Map::new();
    let value = parse_block_value(lines, index, sequence_indent + 1, first_rest)?;
    map.insert(first_key, value);
    loop {
        skip_blank_lines(lines, index);
        if *index >= lines.len() {
            break;
        }
        let line = lines[*index];
        let line_indent = indent_of(line);
        if line_indent <= sequence_indent {
            break;
        }
        let content = &line[line_indent..];
        let Some((key, rest)) = mapping_split(content) else {
            break;
        };
        *index += 1;
        let value = parse_block_value(lines, index, line_indent, rest)?;
        map.insert(key, value);
    }
    Ok(Value::Object(map))
}

fn parse_inline_value(
    lines: &[&str],
    index: &mut usize,
    parent_indent: usize,
    rest: &str,
) -> Result<Value, MetadataError> {
    parse_block_value(lines, index, parent_indent, rest)
}

fn collect_flow(lines: &[&str], index: &mut usize, first: &str) -> Result<String, MetadataError> {
    let mut text = first.to_string();
    let mut depth = depth_delta(first);
    while depth > 0 && *index < lines.len() {
        text.push('\n');
        text.push_str(lines[*index]);
        depth += depth_delta(lines[*index]);
        *index += 1;
    }
    if depth > 0 {
        return Err(MetadataError::new("unterminated flow collection", 0, 0));
    }
    Ok(text)
}

fn collect_quoted(
    lines: &[&str],
    index: &mut usize,
    first: &str,
    quote: char,
) -> Result<String, MetadataError> {
    if quoted_closes(first, quote) {
        return Ok(first.to_string());
    }
    let mut text = first.to_string();
    while *index < lines.len() {
        let line = lines[*index].trim_start();
        *index += 1;
        text.push(' ');
        text.push_str(line);
        if quoted_closes(&text, quote) {
            return Ok(text);
        }
    }
    Err(MetadataError::new("unterminated quoted scalar", 0, 0))
}

fn quoted_closes(text: &str, quote: char) -> bool {
    let mut escaped = false;
    let mut chars = text.char_indices().skip(1).peekable();
    while let Some((_, ch)) = chars.next() {
        if quote == '"' && escaped {
            escaped = false;
            continue;
        }
        if quote == '"' && ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            // A doubled single quote escapes itself.
            if quote == '\'' && chars.peek().is_some_and(|(_, next)| *next == '\'') {
                chars.next();
                continue;
            }
            return true;
        }
    }
    false
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

fn block_scalar(
    lines: &[&str],
    index: &mut usize,
    parent_indent: usize,
    indicator: &str,
) -> String {
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
    text
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

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn is_sequence_entry(content: &str) -> bool {
    content == "-" || content.starts_with("- ") || content.starts_with("-\t")
}

fn mapping_split(content: &str) -> Option<(String, &str)> {
    let mut quote: Option<char> = None;
    let mut depth = 0i32;
    for (index, ch) in content.char_indices() {
        match quote {
            Some(expected) => {
                if ch == expected {
                    quote = None;
                }
            }
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch == '[' || ch == '{' => depth += 1,
            None if ch == ']' || ch == '}' => depth -= 1,
            None if ch == ':' && depth == 0 => {
                let key = content[..index].trim();
                if key.is_empty() {
                    return None;
                }
                return Some((
                    unquote_key(key),
                    content[index + 1..].trim_start_matches([' ', '\t']),
                ));
            }
            None => {}
        }
    }
    None
}

fn unquote_key(key: &str) -> String {
    if (key.starts_with('"') && key.ends_with('"') && key.len() >= 2)
        || (key.starts_with('\'') && key.ends_with('\'') && key.len() >= 2)
    {
        let mut flow = Flow::new(key);
        if let Ok(Value::String(key)) = flow.parse_value() {
            return key;
        }
    }
    key.to_string()
}

fn strip_inline_comment(raw: &str) -> &str {
    let bytes = raw.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'#' && (index == 0 || matches!(bytes[index - 1], b' ' | b'\t')) {
            return raw[..index].trim_end_matches([' ', '\t']);
        }
    }
    raw.trim_end_matches([' ', '\t'])
}

fn plain_scalar(raw: &str) -> Value {
    let raw = raw.trim();
    match raw {
        "" | "~" | "null" | "Null" | "NULL" => Value::Null,
        "true" | "True" | "TRUE" => Value::Bool(true),
        "false" | "False" | "FALSE" => Value::Bool(false),
        _ => {
            if let Ok(integer) = raw.parse::<i64>() {
                return Value::Number(Number::from(integer));
            }
            if let Ok(float) = raw.parse::<f64>()
                && let Some(number) = Number::from_f64(float)
            {
                return Value::Number(number);
            }
            Value::String(raw.to_string())
        }
    }
}

fn error_at(line: &str, index: usize, column: usize, message: &str) -> MetadataError {
    let _ = line;
    MetadataError::new(message, index, column)
}

fn reject_tab_indent(line: &str, index: usize) -> Result<(), MetadataError> {
    if line.trim_start_matches(' ').starts_with('\t') {
        return Err(MetadataError::new(
            "tabs are not allowed for indentation",
            index,
            indent_of(line),
        ));
    }
    Ok(())
}
