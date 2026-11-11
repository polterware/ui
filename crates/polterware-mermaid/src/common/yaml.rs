//! The subset of `js-yaml`'s `JSON_SCHEMA` that this crate reads: frontmatter, and the
//! `@{ ... }` metadata of flowchart, sequence, and agentflow.
//!
//! Mappings (nested), sequences, flow collections, block scalars, quoted and plain scalars,
//! comments, and JSON's booleans, numbers, and null. Anchors, aliases, tags, and
//! multi-document streams are not read; upstream's own frontmatter and metadata tests do not
//! need them.
//!
//! Where `js-yaml` throws, the readers answer `Err(message)`; each caller decides whether to
//! surface it (frontmatter records it in [`crate::preprocess::Source::error`]) or to wrap it
//! in a [`crate::error::ParseError`].
//!
//! Block nesting and flow nesting are bounded ([`MAX_BLOCK_DEPTH`], [`MAX_FLOW_DEPTH`]) so
//! hostile indentation is rejected with a message instead of overflowing the stack; js-yaml
//! overflows the JavaScript stack on the same text.

use serde_json::{Map, Value};

use super::is_js_whitespace;

/// Parses a YAML document: a block mapping, a block sequence, or a plain scalar, with flow
/// collections anywhere a value may stand.
pub fn parse_document(text: &str) -> Result<Value, String> {
    let mut parser = YamlParser {
        lines: text.split('\n').collect(),
        index: 0,
        depth: 0,
    };
    parser.parse_document()
}

/// Parses one flow collection or scalar and rejects anything after it but a comment.
pub fn parse_flow(text: &str) -> Result<Value, String> {
    let mut parser = FlowParser {
        text,
        pos: 0,
        depth: 0,
    };
    let value = parser.value()?;
    parser.skip_whitespace();
    let rest = &text[parser.pos..];
    if !rest.is_empty() && !rest.starts_with('#') {
        return Err("unexpected content after a flow collection".to_string());
    }
    Ok(value)
}

/// Parses the single-line spelling of a `@{ ... }` block: upstream wraps it in braces and
/// reads it as a flow mapping.
pub fn parse_flow_mapping(text: &str) -> Result<Value, String> {
    parse_flow(&format!("{{\n{text}\n}}"))
}

/// A block indented deeper than this is hostile input that would overflow the stack of a
/// recursive reader (js-yaml overflows the JavaScript stack on the same text).
const MAX_INDENT: usize = 1024;

/// The same bound for the number of nested block levels: a reader that recurses once per
/// level must fail on a stack it can still finish on.
const MAX_BLOCK_DEPTH: usize = 64;

/// The bound for flow collections, which nest without indentation.
const MAX_FLOW_DEPTH: usize = 64;

struct YamlParser<'a> {
    lines: Vec<&'a str>,
    index: usize,
    depth: usize,
}

impl<'a> YamlParser<'a> {
    fn parse_document(&mut self) -> Result<Value, String> {
        self.skip_ignorable();
        if self.at_end() {
            return Ok(Value::Null);
        }
        let indent = indent_of(self.current());
        let value = self.parse_block(indent)?;
        self.skip_ignorable();
        if !self.at_end() {
            return Err(format!("unexpected content on line {}", self.index + 1));
        }
        Ok(value)
    }

    fn at_end(&self) -> bool {
        self.index >= self.lines.len()
    }

    fn current(&self) -> &'a str {
        self.lines[self.index]
    }

    fn skip_ignorable(&mut self) {
        while !self.at_end() {
            let content = content_of(self.current());
            if content.is_empty() || content.starts_with('#') {
                self.index += 1;
            } else {
                break;
            }
        }
    }

    fn parse_block(&mut self, indent: usize) -> Result<Value, String> {
        self.depth += 1;
        let value = if self.depth > MAX_BLOCK_DEPTH || indent > MAX_INDENT {
            Err("the YAML is nested too deeply".to_string())
        } else {
            let content = content_of(self.current());
            if is_sequence_entry(content) {
                self.parse_sequence(indent)
            } else if mapping_split(content).is_some() {
                self.parse_mapping(indent)
            } else {
                self.parse_plain_block(indent)
            }
        };
        self.depth -= 1;
        value
    }

    fn parse_mapping(&mut self, indent: usize) -> Result<Value, String> {
        let mut map = Map::new();
        loop {
            self.skip_ignorable();
            if self.at_end() {
                break;
            }
            let line_indent = indent_of(self.current());
            if line_indent < indent {
                break;
            }
            let content = content_of(self.current());
            if line_indent > indent {
                return Err(format!("bad indentation on line {}", self.index + 1));
            }
            if is_sequence_entry(content) {
                break;
            }
            let Some((key, rest)) = mapping_split(content) else {
                return Err(format!(
                    "could not find expected ':' on line {}",
                    self.index + 1
                ));
            };
            self.index += 1;
            let value = self.parse_mapping_value(rest, indent)?;
            map.insert(key, value);
        }
        Ok(Value::Object(map))
    }

    fn parse_mapping_value(&mut self, rest: &str, indent: usize) -> Result<Value, String> {
        let rest = rest.trim_end();
        if let Some(header) = block_scalar_header(rest) {
            return Ok(Value::String(self.parse_block_scalar(header, indent)?));
        }
        if rest.starts_with('[') || rest.starts_with('{') {
            return parse_flow(rest);
        }
        if !rest.is_empty() {
            if rest.starts_with('!') {
                return Err(tag_error());
            }
            return Ok(resolve_scalar(strip_inline_comment(rest)));
        }
        self.skip_ignorable();
        if self.at_end() {
            return Ok(Value::Null);
        }
        let line_indent = indent_of(self.current());
        let content = content_of(self.current());
        if line_indent > indent {
            self.parse_block(line_indent)
        } else if line_indent == indent && is_sequence_entry(content) {
            self.parse_sequence(indent)
        } else {
            Ok(Value::Null)
        }
    }

    fn parse_sequence(&mut self, indent: usize) -> Result<Value, String> {
        let mut items = Vec::new();
        loop {
            self.skip_ignorable();
            if self.at_end() {
                break;
            }
            if indent_of(self.current()) != indent {
                break;
            }
            let content = content_of(self.current());
            let Some(rest) = content.strip_prefix('-') else {
                break;
            };
            if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
                break;
            }
            let rest = rest.trim_start_matches([' ', '\t']);
            self.index += 1;
            if rest.is_empty() {
                self.skip_ignorable();
                if !self.at_end() && indent_of(self.current()) > indent {
                    let nested = indent_of(self.current());
                    items.push(self.parse_block(nested)?);
                } else {
                    items.push(Value::Null);
                }
                continue;
            }
            if let Some(header) = block_scalar_header(rest) {
                items.push(Value::String(self.parse_block_scalar(header, indent)?));
                continue;
            }
            if rest.starts_with('[') || rest.starts_with('{') {
                items.push(parse_flow(rest)?);
                continue;
            }
            if let Some((key, value_rest)) = mapping_split(rest) {
                items.push(self.parse_compact_mapping(indent, key, value_rest)?);
                continue;
            }
            items.push(resolve_scalar(strip_inline_comment(rest)));
        }
        Ok(Value::Array(items))
    }

    /// `- key: value` followed by more keys indented under the dash.
    fn parse_compact_mapping(
        &mut self,
        sequence_indent: usize,
        key: String,
        rest: &str,
    ) -> Result<Value, String> {
        let mut map = Map::new();
        let value = self.parse_mapping_value(rest, sequence_indent + 1)?;
        map.insert(key, value);
        loop {
            self.skip_ignorable();
            if self.at_end() {
                break;
            }
            let line_indent = indent_of(self.current());
            if line_indent <= sequence_indent {
                break;
            }
            let content = content_of(self.current());
            let Some((key, rest)) = mapping_split(content) else {
                break;
            };
            self.index += 1;
            let value = self.parse_mapping_value(rest, line_indent)?;
            map.insert(key, value);
        }
        Ok(Value::Object(map))
    }

    fn parse_plain_block(&mut self, indent: usize) -> Result<Value, String> {
        let first = content_of(self.current());
        if first.starts_with('!') {
            return Err(tag_error());
        }
        let mut parts = vec![strip_inline_comment(first).to_string()];
        self.index += 1;
        while !self.at_end() {
            let line = self.current();
            let trimmed = line.trim_start_matches([' ', '\t']);
            let line_indent = line.len() - trimmed.len();
            let content = trimmed.trim_end();
            if content.is_empty() || content.starts_with('#') || line_indent < indent {
                break;
            }
            parts.push(strip_inline_comment(content).to_string());
            self.index += 1;
        }
        Ok(resolve_scalar(&parts.join(" ")))
    }

    fn parse_block_scalar(
        &mut self,
        header: BlockScalarHeader,
        parent_indent: usize,
    ) -> Result<String, String> {
        let start = self.index;
        let mut content_indent = header.explicit_indent.map(|extra| parent_indent + extra);
        let mut end = start;
        while end < self.lines.len() {
            let line = self.lines[end];
            let trimmed = line.trim_start_matches([' ', '\t']);
            if trimmed.trim_end().is_empty() {
                end += 1;
                continue;
            }
            let indent = line.len() - trimmed.len();
            if indent <= parent_indent {
                break;
            }
            content_indent.get_or_insert(indent);
            end += 1;
        }
        let content_indent = content_indent.unwrap_or(parent_indent + 1);
        let lines: Vec<&str> = self.lines[start..end]
            .iter()
            .map(|line| {
                if line.trim().is_empty() {
                    ""
                } else {
                    line.get(content_indent..).unwrap_or("").trim_end()
                }
            })
            .collect();
        self.index = end;
        let text = if header.literal {
            lines.join("\n")
        } else {
            fold_lines(&lines)
        };
        Ok(chomp(text, header.chomp))
    }
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

fn content_of(line: &str) -> &str {
    line.trim_start_matches([' ', '\t']).trim_end()
}

fn is_sequence_entry(content: &str) -> bool {
    content == "-" || content.starts_with("- ") || content.starts_with("-\t")
}

/// Split `key: value`; the colon must be followed by whitespace or end the line, as YAML
/// requires for block mappings.
fn mapping_split(content: &str) -> Option<(String, &str)> {
    let first = content.chars().next()?;
    if first == '"' || first == '\'' {
        let (key, used) = parse_quoted(content)?;
        let rest = content[used..].trim_start_matches([' ', '\t']);
        let rest = rest.strip_prefix(':')?;
        if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
            return None;
        }
        return Some((key, rest.trim_start_matches([' ', '\t'])));
    }
    let bytes = content.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'#' && index > 0 && matches!(bytes[index - 1], b' ' | b'\t') {
            return None;
        }
        if *byte != b':' {
            continue;
        }
        let next = content[index + 1..].chars().next();
        if next.is_some_and(|c| c != ' ' && c != '\t') {
            continue;
        }
        let key = content[..index].trim_end();
        if key.is_empty() {
            return None;
        }
        return Some((
            key.to_string(),
            content[index + 1..].trim_start_matches([' ', '\t']),
        ));
    }
    None
}

/// Strip a trailing `#` comment from a plain scalar and trailing whitespace with it.
pub fn strip_inline_comment(raw: &str) -> &str {
    let bytes = raw.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'#' && (index == 0 || matches!(bytes[index - 1], b' ' | b'\t')) {
            return raw[..index].trim_end_matches([' ', '\t']);
        }
    }
    raw.trim_end_matches([' ', '\t'])
}

fn tag_error() -> String {
    // js-yaml's message for the `!!!` document upstream's frontmatter spec rejects.
    "tag suffix cannot contain exclamation marks".to_string()
}

fn resolve_scalar(raw: &str) -> Value {
    let raw = raw.trim();
    if raw.is_empty() {
        return Value::Null;
    }
    if (raw.starts_with('"') || raw.starts_with('\''))
        && let Some((text, used)) = parse_quoted(raw)
        && used == raw.len()
    {
        return Value::String(text);
    }
    if raw.starts_with('"') || raw.starts_with('\'') {
        return Value::String(raw.to_string());
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

fn parse_number(raw: &str) -> Option<serde_json::Number> {
    if let Ok(integer) = raw.parse::<i64>() {
        return Some(integer.into());
    }
    if raw.contains(['.', 'e', 'E'])
        && let Ok(float) = raw.parse::<f64>()
    {
        return serde_json::Number::from_f64(float);
    }
    None
}

/// Read one quoted YAML scalar, returning its value and how many bytes it used.
fn parse_quoted(input: &str) -> Option<(String, usize)> {
    let quote = input.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let mut out = String::new();
    let mut chars = input.char_indices().skip(1);
    while let Some((index, c)) = chars.next() {
        if c == quote {
            if quote == '\''
                && let Some((_, '\'')) = chars.clone().next()
            {
                out.push('\'');
                chars.next();
                continue;
            }
            return Some((out, index + c.len_utf8()));
        }
        if quote == '"' && c == '\\' {
            let (_, escaped) = chars.next()?;
            match escaped {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'b' => out.push('\u{0008}'),
                'f' => out.push('\u{000c}'),
                '0' => out.push('\0'),
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).map(|(_, c)| c).collect();
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                other => {
                    out.push('\\');
                    out.push(other);
                }
            }
            continue;
        }
        out.push(c);
    }
    None
}

struct FlowParser<'a> {
    text: &'a str,
    pos: usize,
    depth: usize,
}

impl FlowParser<'_> {
    fn skip_whitespace(&mut self) {
        while let Some(ch) = next_char(self.text, self.pos) {
            if !is_js_whitespace(ch) {
                break;
            }
            self.pos += ch.len_utf8();
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        self.depth += 1;
        let value = if self.depth > MAX_FLOW_DEPTH {
            Err("excessive nesting in a flow collection".to_string())
        } else {
            self.value_inner()
        };
        self.depth -= 1;
        value
    }

    fn value_inner(&mut self) -> Result<Value, String> {
        self.skip_whitespace();
        match next_char(self.text, self.pos) {
            Some('[') => self.sequence(),
            Some('{') => self.mapping(),
            Some('"') | Some('\'') => {
                let (value, used) = parse_quoted(&self.text[self.pos..])
                    .ok_or_else(|| "unterminated quoted scalar".to_string())?;
                self.pos += used;
                Ok(Value::String(value))
            }
            _ => Ok(self.scalar()),
        }
    }

    fn scalar(&mut self) -> Value {
        let start = self.pos;
        while let Some(ch) = next_char(self.text, self.pos) {
            if matches!(ch, ',' | ']' | '}' | '\n') {
                break;
            }
            self.pos += ch.len_utf8();
        }
        resolve_scalar(&self.text[start..self.pos])
    }

    fn sequence(&mut self) -> Result<Value, String> {
        self.pos += 1;
        let mut items = Vec::new();
        self.skip_whitespace();
        if next_char(self.text, self.pos) == Some(']') {
            self.pos += 1;
            return Ok(Value::Array(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_whitespace();
            match next_char(self.text, self.pos) {
                Some(',') => {
                    self.pos += 1;
                    self.skip_whitespace();
                    if next_char(self.text, self.pos) == Some(']') {
                        self.pos += 1;
                        break;
                    }
                }
                Some(']') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err("unterminated flow sequence".to_string()),
            }
        }
        Ok(Value::Array(items))
    }

    fn mapping(&mut self) -> Result<Value, String> {
        self.pos += 1;
        let mut map = Map::new();
        self.skip_whitespace();
        if next_char(self.text, self.pos) == Some('}') {
            self.pos += 1;
            return Ok(Value::Object(map));
        }
        loop {
            self.skip_whitespace();
            let key = match next_char(self.text, self.pos) {
                Some('"') | Some('\'') => {
                    let (key, used) = parse_quoted(&self.text[self.pos..])
                        .ok_or_else(|| "unterminated quoted scalar".to_string())?;
                    self.pos += used;
                    key
                }
                _ => {
                    let start = self.pos;
                    while let Some(c) = next_char(self.text, self.pos) {
                        if c == ':' || c == ',' || c == '}' {
                            break;
                        }
                        self.pos += c.len_utf8();
                    }
                    self.text[start..self.pos].trim().to_string()
                }
            };
            self.skip_whitespace();
            if next_char(self.text, self.pos) != Some(':') {
                return Err("expected ':' in a flow mapping".to_string());
            }
            self.pos += 1;
            let value = self.value()?;
            map.insert(key, value);
            self.skip_whitespace();
            match next_char(self.text, self.pos) {
                Some(',') => {
                    self.pos += 1;
                    self.skip_whitespace();
                    if next_char(self.text, self.pos) == Some('}') {
                        self.pos += 1;
                        break;
                    }
                }
                Some('}') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err("unterminated flow mapping".to_string()),
            }
        }
        Ok(Value::Object(map))
    }
}

fn next_char(text: &str, pos: usize) -> Option<char> {
    if pos < text.len() {
        text[pos..].chars().next()
    } else {
        None
    }
}

struct BlockScalarHeader {
    literal: bool,
    chomp: Chomp,
    explicit_indent: Option<usize>,
}

#[derive(Clone, Copy, PartialEq)]
enum Chomp {
    Strip,
    Clip,
    Keep,
}

fn block_scalar_header(raw: &str) -> Option<BlockScalarHeader> {
    let mut chars = raw.chars();
    let literal = match chars.next()? {
        '|' => true,
        '>' => false,
        _ => return None,
    };
    let mut chomp = Chomp::Clip;
    let mut explicit_indent = None;
    for c in chars {
        match c {
            '+' => chomp = Chomp::Keep,
            '-' => chomp = Chomp::Strip,
            '1'..='9' => explicit_indent = Some(c.to_digit(10)? as usize),
            ' ' | '\t' => break,
            _ => return None,
        }
    }
    Some(BlockScalarHeader {
        literal,
        chomp,
        explicit_indent,
    })
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

fn chomp(text: String, chomp: Chomp) -> String {
    match chomp {
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
    }
}

/// How many flow collections (`{`/`[`) `text` opens more than it closes, outside quotes: the
/// balance a reader accumulates while it collects a flow value spread over several lines.
pub fn depth_delta(text: &str) -> i32 {
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reads_mappings_sequences_and_flow_collections() {
        let value =
            parse_document("a: [1, 2]\nb: {c: true, d: 'x'}\nlist:\n  - a\n  - 'b''c'").unwrap();
        assert_eq!(
            value,
            json!({ "a": [1, 2], "b": { "c": true, "d": "x" }, "list": ["a", "b'c"] })
        );
    }

    #[test]
    fn allows_a_comment_after_a_flow_collection() {
        assert_eq!(
            parse_document("a: [1, 2] # numbers").unwrap(),
            json!({ "a": [1, 2] })
        );
    }

    #[test]
    fn reads_block_scalars() {
        assert_eq!(
            parse_document("title: >\n  one\n  two\n").unwrap(),
            json!({ "title": "one two\n" })
        );
        assert_eq!(
            parse_document("title: |\n  one\n  two\n").unwrap(),
            json!({ "title": "one\ntwo\n" })
        );
    }

    #[test]
    fn resolves_json_scalars() {
        assert_eq!(parse_document("null").unwrap(), Value::Null);
        assert_eq!(parse_document("14").unwrap(), json!(14));
        assert_eq!(parse_document("1.5").unwrap(), json!(1.5));
        assert_eq!(parse_document("FALSE").unwrap(), json!(false));
        assert_eq!(parse_document("value").unwrap(), json!("value"));
    }

    #[test]
    fn rejects_a_tag() {
        assert_eq!(
            parse_document("!!!").unwrap_err(),
            "tag suffix cannot contain exclamation marks"
        );
    }

    #[test]
    fn rejects_content_after_a_flow_collection() {
        assert!(parse_flow("{a: 1} b").is_err());
        assert!(parse_flow("{a: 1} # comment").is_ok());
    }

    #[test]
    fn reads_the_single_line_metadata_spelling() {
        assert_eq!(
            parse_flow_mapping("shape: circle, label: bla").unwrap(),
            json!({ "shape": "circle", "label": "bla" })
        );
    }

    #[test]
    fn rejects_deeply_nested_flow_collections() {
        let text = format!("value: {}{}", "[".repeat(200), "]".repeat(200));
        assert!(parse_document(&text).is_err());
    }

    #[test]
    fn rejects_excessively_indented_blocks() {
        let mut body = String::new();
        for level in 0..1100 {
            body.push_str(&" ".repeat(level));
            body.push_str(&format!("k{level}:\n"));
        }
        assert!(parse_document(&body).is_err());
    }

    #[test]
    fn the_block_depth_bound_holds_on_a_small_stack() {
        let mut body = String::new();
        for level in 0..1100 {
            body.push_str(&" ".repeat(level));
            body.push_str(&format!("k{level}:\n"));
        }
        // A quarter of the default test thread's stack (2 MiB).
        let nested = std::thread::Builder::new()
            .stack_size(512 * 1024)
            .spawn(move || parse_document(&body).is_err())
            .expect("spawn the bounded reader")
            .join()
            .expect("the reader returns instead of overflowing");
        assert!(nested);
    }
}
