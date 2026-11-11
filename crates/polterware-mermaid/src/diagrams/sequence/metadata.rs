//! Participant metadata (`participant Alice@{ "type": "database", "alias": "Public API" }`).
//!
//! Upstream feeds the text between `@{` and `}` to `js-yaml` with `JSON_SCHEMA` (wrapping
//! it in braces when it has no newline), and reads `type` and `alias` back. This module
//! parses the same subset with no new dependency: a flow mapping (possibly spread over
//! lines), or a block mapping when the text has a newline, with quoted or plain keys and
//! scalar / nested flow values. Anything else is an error, as `js-yaml` throws for
//! malformed metadata.

use serde_json::{Map, Value};

/// The fields `addActor` reads from participant metadata.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Metadata {
    pub type_: Option<String>,
    pub alias: Option<String>,
}

pub(crate) fn parse(raw: &str) -> Result<Metadata, String> {
    let trimmed = raw.trim();
    let map = if trimmed.starts_with('{') || trimmed.starts_with('[') {
        flow_object(trimmed)?
    } else if raw.contains('\n') {
        parse_block(raw)?
    } else {
        let wrapped = format!("{{{raw}}}");
        flow_object(&wrapped)?
    };
    Ok(Metadata {
        type_: map.get("type").and_then(Value::as_str).map(str::to_string),
        alias: map.get("alias").and_then(Value::as_str).map(str::to_string),
    })
}

fn flow_object(text: &str) -> Result<Map<String, Value>, String> {
    match Flow::new(text).whole_value()? {
        Value::Object(map) => Ok(map),
        _ => Err("participant metadata is not a mapping".into()),
    }
}

fn parse_block(raw: &str) -> Result<Map<String, Value>, String> {
    let mut map = Map::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (key_text, value_text) = split_key_value(trimmed)?;
        let key = Flow::new(key_text).whole_scalar()?;
        let value = parse_block_value(value_text)?;
        map.insert(key, value);
    }
    Ok(map)
}

fn split_key_value(line: &str) -> Result<(&str, &str), String> {
    let mut quote: Option<char> = None;
    for (index, ch) in line.char_indices() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                }
            }
            None => match ch {
                '"' | '\'' => quote = Some(ch),
                ':' => return Ok((&line[..index], &line[index + 1..])),
                _ => {}
            },
        }
    }
    Err(format!(
        "expected a ':' in participant metadata line '{line}'"
    ))
}

fn parse_block_value(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Value::Null);
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') || trimmed.starts_with('"') {
        return Flow::new(trimmed).whole_value();
    }
    let plain = match trimmed.find(" #") {
        Some(index) => trimmed[..index].trim(),
        None => trimmed,
    };
    if plain.starts_with('\'') {
        return Flow::new(plain).whole_value();
    }
    Ok(plain_value(plain))
}

struct Flow {
    chars: Vec<char>,
    pos: usize,
}

impl Flow {
    fn new(source: &str) -> Self {
        Self {
            chars: source.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn skip_spaces(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            self.pos += 1;
        }
    }

    fn whole_value(mut self) -> Result<Value, String> {
        self.skip_spaces();
        let value = self.value()?;
        self.skip_spaces();
        if self.pos < self.chars.len() {
            return Err(self.unexpected());
        }
        Ok(value)
    }

    fn whole_scalar(mut self) -> Result<String, String> {
        self.skip_spaces();
        let scalar = self.scalar()?;
        self.skip_spaces();
        if self.pos < self.chars.len() {
            return Err(self.unexpected());
        }
        Ok(scalar)
    }

    fn mapping(&mut self) -> Result<Value, String> {
        self.expect('{')?;
        let mut map = Map::new();
        loop {
            self.skip_spaces();
            match self.peek() {
                Some('}') => {
                    self.bump();
                    return Ok(Value::Object(map));
                }
                None => return Err("unexpected end of participant metadata".into()),
                _ => {}
            }
            let key = self.scalar()?;
            self.skip_spaces();
            let value = if self.peek() == Some(':') {
                self.bump();
                self.skip_spaces();
                self.value()?
            } else {
                Value::Null
            };
            map.insert(key, value);
            self.skip_spaces();
            match self.peek() {
                Some(',') => {
                    self.bump();
                }
                Some('}') => {
                    self.bump();
                    return Ok(Value::Object(map));
                }
                _ => return Err(self.unexpected()),
            }
        }
    }

    fn sequence(&mut self) -> Result<Value, String> {
        self.expect('[')?;
        let mut items = Vec::new();
        loop {
            self.skip_spaces();
            match self.peek() {
                Some(']') => {
                    self.bump();
                    return Ok(Value::Array(items));
                }
                None => return Err("unexpected end of participant metadata".into()),
                _ => {}
            }
            items.push(self.value()?);
            self.skip_spaces();
            match self.peek() {
                Some(',') => {
                    self.bump();
                }
                Some(']') => {
                    self.bump();
                    return Ok(Value::Array(items));
                }
                _ => return Err(self.unexpected()),
            }
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        match self.peek() {
            Some('{') => self.mapping(),
            Some('[') => self.sequence(),
            Some('"') | Some('\'') => Ok(Value::String(self.quoted()?)),
            _ => {
                let start = self.pos;
                while let Some(ch) = self.peek() {
                    if matches!(ch, ',' | '}' | ']' | '\n') {
                        break;
                    }
                    self.pos += 1;
                }
                let text: String = self.chars[start..self.pos].iter().collect();
                Ok(plain_value(text.trim()))
            }
        }
    }

    fn scalar(&mut self) -> Result<String, String> {
        match self.peek() {
            Some('"') | Some('\'') => self.quoted(),
            _ => {
                let start = self.pos;
                while let Some(ch) = self.peek() {
                    if matches!(ch, ',' | '}' | ']' | ':' | '\n') {
                        break;
                    }
                    self.pos += 1;
                }
                let text: String = self.chars[start..self.pos].iter().collect();
                let text = text.trim();
                if text.is_empty() {
                    return Err(self.unexpected());
                }
                Ok(text.to_string())
            }
        }
    }

    fn quoted(&mut self) -> Result<String, String> {
        let quote = match self.bump() {
            Some(q @ ('"' | '\'')) => q,
            _ => return Err(self.unexpected()),
        };
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return Err("unterminated quoted string in participant metadata".into()),
                Some(ch) if ch == quote => {
                    if quote == '\'' && self.peek() == Some('\'') {
                        self.bump();
                        out.push('\'');
                    } else {
                        return Ok(out);
                    }
                }
                Some('\\') if quote == '"' => match self.bump() {
                    None => return Err("unterminated escape in participant metadata".into()),
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some(other) => out.push(other),
                },
                Some(ch) => out.push(ch),
            }
        }
    }

    fn expect(&mut self, wanted: char) -> Result<(), String> {
        match self.bump() {
            Some(ch) if ch == wanted => Ok(()),
            _ => Err(self.unexpected()),
        }
    }

    fn unexpected(&self) -> String {
        match self.peek() {
            Some(ch) => format!("unexpected '{ch}' in participant metadata"),
            None => "unexpected end of participant metadata".into(),
        }
    }
}

fn plain_value(text: &str) -> Value {
    if text.is_empty() || text == "~" || text.eq_ignore_ascii_case("null") {
        return Value::Null;
    }
    if text.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if text.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }
    if let Ok(number) = text.parse::<serde_json::Number>() {
        return Value::Number(number);
    }
    Value::String(text.to_string())
}
