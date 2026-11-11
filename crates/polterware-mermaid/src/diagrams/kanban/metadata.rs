//! The `@{ ... }` item metadata (`addNode`'s `shapeData`).
//!
//! Upstream hands the lexed text to js-yaml with `JSON_SCHEMA`: text without a newline is
//! wrapped in `{ ... }` and read as a flow mapping, text with a newline is read as a block
//! mapping. This module ports the subset of that reader the kanban metadata syntax produces:
//! block mappings with their indentation, flow mappings and sequences, quoted and plain
//! scalars, and `|`/`>` block scalars. Exotic YAML (anchors, aliases, tags, multi-line plain
//! scalars, multi-document streams) is rejected with a message; upstream would carry it.
//!
//! Scalars resolve the way `JSON_SCHEMA` does: `null`/`true`/`false` variants, integers and
//! floats, and every other plain scalar is a string, so `icon: star` is the string `star`.

use serde_json::{Map, Number, Value};

/// Parses the metadata text the lexer assembled from a `@{ ... }` block.
pub(crate) fn parse(text: &str) -> Result<Map<String, Value>, String> {
    let value = if text.contains('\n') {
        parse_block_map(&format!("{text}\n"))?
    } else {
        Flow::new(&format!("{{\n{text}\n}}")).parse_value()?
    };
    match value {
        Value::Object(map) => Ok(map),
        Value::Null => Ok(Map::new()),
        _ => Err("item metadata is not a mapping".to_string()),
    }
}

/// JavaScript truthiness, for the `if (doc?.key)` guards in `addNode`.
pub(crate) fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// `value.toString()` for the metadata values `addNode` stringifies.
pub(crate) fn js_to_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                integer.to_string()
            } else if let Some(integer) = number.as_u64() {
                integer.to_string()
            } else {
                let float = number.as_f64().unwrap_or_default();
                format!("{float}")
            }
        }
        Value::String(text) => text.clone(),
        Value::Array(items) => items.iter().map(js_to_string).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
}

fn parse_block_map(text: &str) -> Result<Value, String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut index = 0;
    skip_blank_lines(&lines, &mut index);
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
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            *index += 1;
            continue;
        }
        let line_indent = indent_of(line);
        if line_indent < indent {
            break;
        }
        if line_indent > indent {
            return Err("unexpected indentation in item metadata".to_string());
        }
        let rest = &line[indent..];
        let colon = key_colon(rest)?;
        let key = unquote(rest[..colon].trim())?;
        let after = rest[colon + 1..].trim_start();
        *index += 1;
        let value = if after.is_empty() {
            match next_content_line(lines, *index) {
                Some((next, next_indent)) if next_indent > indent => {
                    if next.trim_start().starts_with('-') {
                        parse_sequence_lines(lines, index, next_indent)?
                    } else {
                        parse_map_lines(lines, index, next_indent)?
                    }
                }
                _ => Value::Null,
            }
        } else if after.starts_with('|') || after.starts_with('>') {
            parse_block_scalar(lines, index, indent, after)?
        } else if after.starts_with('{') || after.starts_with('[') {
            let collected = collect_flow(lines, index, after)?;
            let mut flow = Flow::new(&collected);
            let value = flow.parse_value()?;
            flow.skip_whitespace();
            if flow.peek().is_some() {
                return Err("trailing text after a flow value".to_string());
            }
            value
        } else if after.starts_with('"') || after.starts_with('\'') {
            Flow::new(after).parse_value()?
        } else {
            plain_scalar(after)
        };
        map.insert(key, value);
    }
    Ok(Value::Object(map))
}

fn parse_sequence_lines(lines: &[&str], index: &mut usize, indent: usize) -> Result<Value, String> {
    let mut items = Vec::new();
    while *index < lines.len() {
        let line = lines[*index];
        if line.trim().is_empty() {
            *index += 1;
            continue;
        }
        if indent_of(line) != indent {
            break;
        }
        let Some(rest) = line[indent..].strip_prefix('-') else {
            break;
        };
        if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
            break;
        }
        *index += 1;
        let rest = rest.trim_start();
        if rest.is_empty() {
            match next_content_line(lines, *index) {
                Some((_, next_indent)) if next_indent > indent => {
                    items.push(parse_map_lines(lines, index, next_indent)?);
                }
                _ => items.push(Value::Null),
            }
        } else {
            items.push(plain_scalar(rest));
        }
    }
    Ok(Value::Array(items))
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

fn skip_blank_lines(lines: &[&str], index: &mut usize) {
    while *index < lines.len()
        && (lines[*index].trim().is_empty() || lines[*index].trim_start().starts_with('#'))
    {
        *index += 1;
    }
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

/// The first `:` outside a quoted key.
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
    Err("item metadata line has no ':'".to_string())
}

fn unquote(text: &str) -> Result<String, String> {
    match Flow::new(text).parse_value()? {
        Value::String(key) => Ok(key),
        other => Ok(js_to_string(&other)),
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

#[derive(Clone, Copy)]
enum Chomp {
    Strip,
    Clip,
    Keep,
}

fn parse_block_scalar(
    lines: &[&str],
    index: &mut usize,
    parent_indent: usize,
    header: &str,
) -> Result<Value, String> {
    let literal = header.starts_with('|');
    let chomp = if header.contains('-') {
        Chomp::Strip
    } else if header.contains('+') {
        Chomp::Keep
    } else {
        Chomp::Clip
    };
    let start = *index;
    let mut end = start;
    let mut content_indent = None;
    while end < lines.len() {
        let line = lines[end];
        if line.trim().is_empty() {
            end += 1;
            continue;
        }
        let indent = indent_of(line);
        if indent <= parent_indent {
            break;
        }
        content_indent.get_or_insert(indent);
        end += 1;
    }
    let indent = content_indent.unwrap_or(parent_indent + 1);
    let collected: Vec<&str> = lines[start..end]
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                ""
            } else {
                line.get(indent..).unwrap_or("").trim_end()
            }
        })
        .collect();
    *index = end;
    let mut text = if literal {
        collected.join("\n")
    } else {
        fold_lines(&collected)
    };
    text = match chomp {
        Chomp::Keep => text,
        Chomp::Strip => text.trim_end_matches('\n').to_string(),
        Chomp::Clip => {
            let trimmed = text.trim_end_matches('\n');
            if trimmed.is_empty() {
                String::new()
            } else {
                format!("{trimmed}\n")
            }
        }
    };
    Ok(Value::String(text))
}

/// Folded (`>`) block scalars: single breaks become spaces, blank lines become breaks.
fn fold_lines(lines: &[&str]) -> String {
    let mut out = String::new();
    let mut first = true;
    let mut blanks = 0usize;
    for line in lines {
        if line.is_empty() {
            blanks += 1;
            continue;
        }
        if !first {
            if blanks > 0 {
                out.push_str(&"\n".repeat(blanks));
            } else {
                out.push(' ');
            }
        }
        out.push_str(line);
        blanks = 0;
        first = false;
    }
    out
}

/// A plain scalar in block context: `#` after whitespace starts a comment.
fn plain_scalar(raw: &str) -> Value {
    let raw = strip_inline_comment(raw);
    resolve_scalar(raw.trim())
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

fn resolve_scalar(raw: &str) -> Value {
    if raw.is_empty() {
        return Value::Null;
    }
    match raw {
        "null" | "Null" | "NULL" | "~" => Value::Null,
        "true" | "True" | "TRUE" => Value::Bool(true),
        "false" | "False" | "FALSE" => Value::Bool(false),
        _ => match parse_number(raw) {
            Some(number) => Value::Number(number),
            None => Value::String(raw.to_string()),
        },
    }
}

fn parse_number(raw: &str) -> Option<Number> {
    if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("-0x")) {
        let negative = raw.starts_with('-');
        let value = i64::from_str_radix(hex, 16).ok()?;
        return Some((if negative { -value } else { value }).into());
    }
    if let Ok(integer) = raw.parse::<i64>() {
        return Some(integer.into());
    }
    if raw.contains(['.', 'e', 'E'])
        && let Ok(float) = raw.parse::<f64>()
    {
        return Number::from_f64(float);
    }
    None
}

/// The flow-mapping and flow-sequence reader of the single-line spelling.
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
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn parse_value(&mut self) -> Result<Value, String> {
        self.skip_whitespace();
        match self.peek() {
            Some('{') => self.parse_map(),
            Some('[') => self.parse_sequence(),
            Some('"') => self.parse_double_quoted(),
            Some('\'') => self.parse_single_quoted(),
            Some(_) => Ok(resolve_scalar(
                self.parse_plain(&[',', '}', ']', '\n']).trim(),
            )),
            None => Err("expected a value".to_string()),
        }
    }

    fn parse_map(&mut self) -> Result<Value, String> {
        self.bump();
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
            let key = match self.peek() {
                Some('"') => match self.parse_double_quoted()? {
                    Value::String(key) => key,
                    _ => unreachable!("a quoted scalar is a string"),
                },
                Some('\'') => match self.parse_single_quoted()? {
                    Value::String(key) => key,
                    _ => unreachable!("a quoted scalar is a string"),
                },
                _ => self.parse_plain(&[':']).trim().to_string(),
            };
            self.skip_whitespace();
            if self.peek() != Some(':') {
                return Err("expected ':' in item metadata".to_string());
            }
            self.bump();
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
                other => return Err(format!("expected ',' or '}}', found {other:?}")),
            }
        }
    }

    fn parse_sequence(&mut self) -> Result<Value, String> {
        self.bump();
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
                other => return Err(format!("expected ',' or ']', found {other:?}")),
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
        &self.text[start..self.index]
    }

    fn parse_double_quoted(&mut self) -> Result<Value, String> {
        self.bump();
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return Err("unterminated quoted scalar".to_string()),
                Some('"') => return Ok(Value::String(out)),
                Some('\\') => match self.bump() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some('b') => out.push('\u{8}'),
                    Some('f') => out.push('\u{c}'),
                    Some('0') => out.push('\0'),
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some('/') => out.push('/'),
                    Some('u') => {
                        let mut hex = String::new();
                        for _ in 0..4 {
                            hex.push(
                                self.bump()
                                    .ok_or_else(|| "unterminated quoted scalar".to_string())?,
                            );
                        }
                        let code = u32::from_str_radix(&hex, 16)
                            .map_err(|_| "invalid unicode escape".to_string())?;
                        out.push(
                            char::from_u32(code)
                                .ok_or_else(|| "invalid unicode escape".to_string())?,
                        );
                    }
                    Some(other) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => return Err("unterminated quoted scalar".to_string()),
                },
                Some(ch) => out.push(ch),
            }
        }
    }

    fn parse_single_quoted(&mut self) -> Result<Value, String> {
        self.bump();
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return Err("unterminated quoted scalar".to_string()),
                Some('\'') => {
                    if self.peek() == Some('\'') {
                        self.bump();
                        out.push('\'');
                    } else {
                        return Ok(Value::String(out));
                    }
                }
                Some(ch) => out.push(ch),
            }
        }
    }
}
