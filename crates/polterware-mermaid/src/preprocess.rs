//! What upstream does before a diagram's parser runs (`preprocess.ts`,
//! `diagram-api/frontmatter.ts`, `diagram-api/comments.ts`, `diagram-api/regexes.ts`, and the
//! directive helpers in `utils.ts`): line-ending and HTML-attribute cleanup, YAML frontmatter
//! (`title`, `config`, `displayMode`), `%%{init: ...}%%` directives, comment lines, and
//! configuration merging.
//!
//! The body keeps the input's line numbers: every removed line becomes an empty line, so a
//! parser's spans and an editor's lines agree. Upstream hands parsers the comment-stripped
//! text (`cleaned`), which shifts the lines a comment occupied; this crate leaves an empty
//! line in their place, so a comment cannot shift a line either.
//!
//! The YAML reader is [`crate::common::yaml`], the subset of `js-yaml`'s `JSON_SCHEMA` that
//! frontmatter and `@{ ... }` metadata share.
//!
//! Where `js-yaml` or `JSON.parse` would throw, [`preprocess`] records the message in
//! [`Source::error`] instead: upstream's `preprocessDiagram` throws, and this crate's
//! signature has no place to throw from, so the caller decides. Directive JSON errors are
//! swallowed like upstream's `detectDirective` catch does.

use serde_json::{Map, Value};

use crate::common::yaml;
use crate::diagrams;

/// True for the characters JavaScript's `\s` matches. The shared helper lives in
/// [`crate::common`]; the parsers still import it from here.
pub(crate) use crate::common::is_js_whitespace;

/// The source split into what the parsers read and the configuration around it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Source {
    /// The diagram text a parser reads, same line count as the input. Frontmatter,
    /// directives, and `%%` comment lines are blanked to empty lines.
    pub body: String,
    pub title: Option<String>,
    /// Frontmatter `config` and every directive merged in order, as JSON.
    pub config: Map<String, Value>,
    /// The frontmatter YAML error upstream throws, when the block is present but invalid.
    /// The body is then left untouched, as upstream never gets past the frontmatter.
    pub error: Option<String>,
}

/// Preprocesses `text` like upstream's `preprocessDiagram`, without throwing.
pub fn preprocess(text: &str) -> Source {
    let normalized = cleanup_text(text);
    match extract_front_matter(&normalized) {
        FrontMatter::Invalid(error) => Source {
            body: normalized,
            error: Some(error),
            ..Source::default()
        },
        FrontMatter::Missing => Source {
            body: cleanup_comments(&remove_directives(&normalized)),
            config: directive_config(&normalized),
            title: None,
            error: None,
        },
        FrontMatter::Found {
            length,
            title,
            display_mode,
            mut config,
        } => {
            if let Some(mode) = display_mode {
                let gantt = config
                    .entry("gantt".to_string())
                    .or_insert_with(|| Value::Object(Map::new()));
                if let Value::Object(gantt) = gantt {
                    gantt.insert("displayMode".to_string(), Value::String(mode));
                }
            }
            let rest = &normalized[length..];
            let config = clean_and_merge(config, Value::Object(directive_config(rest)));
            let mut body = "\n".repeat(normalized[..length].matches('\n').count());
            body.push_str(&cleanup_comments(&remove_directives(rest)));
            Source {
                body,
                title,
                config,
                error: None,
            }
        }
    }
}

/// The text [`crate::diagrams::detect`] runs its detectors on: frontmatter, directives, and
/// comments removed, like upstream's `detectType` does before it loops over the detectors.
pub(crate) fn detection_text(text: &str) -> String {
    let text = match match_front_matter(text) {
        Some(found) => &text[found.length..],
        None => text,
    };
    strip_comments(&remove_directives(text))
}

/// `preprocess.ts`'s `cleanupText`: normalize line endings, then single-quote HTML
/// attributes (the parsers reject double quotes).
fn cleanup_text(code: &str) -> String {
    let normalized = code.replace("\r\n", "\n").replace('\r', "\n");
    normalize_html_quotes(&normalized)
}

/// The `<(\w+)([^>]*)>` replacement in `cleanupText`, with `="..."` turned into `='...'`.
fn normalize_html_quotes(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    let mut rest = code;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let tag_len = after
            .char_indices()
            .take_while(|(_, c)| is_word_char(*c))
            .last()
            .map_or(0, |(index, c)| index + c.len_utf8());
        if tag_len == 0 {
            out.push('<');
            rest = after;
            continue;
        }
        let Some(close) = after[tag_len..].find('>') else {
            out.push('<');
            rest = after;
            continue;
        };
        out.push('<');
        out.push_str(&after[..tag_len]);
        out.push_str(&replace_double_quoted_attributes(
            &after[tag_len..tag_len + close],
        ));
        out.push('>');
        rest = &after[tag_len + close + 1..];
    }
    out.push_str(rest);
    out
}

/// `attributes.replace(/="([^"]*)"/g, "='$1'")`.
fn replace_double_quoted_attributes(attributes: &str) -> String {
    let mut out = String::with_capacity(attributes.len());
    let mut rest = attributes;
    while let Some(equals) = rest.find("=\"") {
        let value = &rest[equals + 2..];
        let Some(close) = value.find('"') else {
            break;
        };
        out.push_str(&rest[..equals]);
        out.push_str("='");
        out.push_str(&value[..close]);
        out.push('\'');
        rest = &value[close + 1..];
    }
    out.push_str(rest);
    out
}

enum FrontMatter {
    Missing,
    Invalid(String),
    Found {
        length: usize,
        title: Option<String>,
        display_mode: Option<String>,
        config: Map<String, Value>,
    },
}

/// `diagram-api/frontmatter.ts`'s `extractFrontMatter`, minus the throwing.
fn extract_front_matter(text: &str) -> FrontMatter {
    let Some(found) = match_front_matter(text) else {
        return FrontMatter::Missing;
    };
    let body = dedent(&found.body, &found.indent);
    let parsed = match yaml::parse_document(&body) {
        Ok(parsed) => parsed,
        Err(error) => return FrontMatter::Invalid(error),
    };
    let (title, display_mode, config) = frontmatter_metadata(parsed);
    FrontMatter::Found {
        length: found.length,
        title,
        display_mode,
        config,
    }
}

/// js-yaml rejects tab-indented documents, so upstream dedents the block by the fence's
/// indent before parsing.
fn dedent(body: &str, indent: &str) -> String {
    if indent.is_empty() {
        return body.to_string();
    }
    body.split('\n')
        .map(|line| line.strip_prefix(indent).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The three keys upstream copies out of the parsed block; everything else is ignored.
fn frontmatter_metadata(parsed: Value) -> (Option<String>, Option<String>, Map<String, Value>) {
    let Value::Object(mut map) = parsed else {
        return (None, None, Map::new());
    };
    let title = map
        .get("title")
        .filter(|value| truthy(value))
        .map(js_to_string);
    let display_mode = map
        .get("displayMode")
        .filter(|value| truthy(value))
        .map(js_to_string);
    let config = match map.remove("config") {
        Some(Value::Object(config)) => config,
        _ => Map::new(),
    };
    (title, display_mode, config)
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// `value.toString()` for the values `js-yaml` can put under `title` or `displayMode`.
fn js_to_string(value: &Value) -> String {
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

/// A matched Jekyll-style frontmatter block, byte lengths included.
pub(crate) struct FrontMatterMatch {
    indent: String,
    body: String,
    length: usize,
}

/// `diagram-api/regexes.ts`'s `matchFrontMatter`, ported step for step because the regex it
/// replaces backtracks into its own opening delimiter.
pub(crate) fn match_front_matter(text: &str) -> Option<FrontMatterMatch> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut cursor = 0;
    while cursor < len && is_js_whitespace(chars[cursor]) && !is_line_break(chars[cursor]) {
        cursor += 1;
    }
    let indent: String = chars[..cursor].iter().collect();
    if !starts_with_chars(&chars, cursor, "---") {
        return None;
    }

    let mut opening_ends = Vec::new();
    let mut index = cursor + 3;
    while index < len && is_js_whitespace(chars[index]) {
        if is_line_break(chars[index]) {
            opening_ends.push(index);
        }
        index += 1;
    }
    if opening_ends.is_empty() {
        return None;
    }

    let closing: Vec<char> = indent.chars().chain("---".chars()).collect();
    // A closing fence is a line break, the indent plus `---`, and at least one more line
    // break in the whitespace run after it. `closing_end` is the index of that last break.
    let closing_end = |at: usize| -> Option<usize> {
        if at >= len || !is_line_break(chars[at]) || !starts_with_seq(&chars, at + 1, &closing) {
            return None;
        }
        let mut end = None;
        let mut probe = at + 1 + closing.len();
        while probe < len && is_js_whitespace(chars[probe]) {
            if is_line_break(chars[probe]) {
                end = Some(probe);
            }
            probe += 1;
        }
        end
    };

    let earliest_body_start = opening_ends[0] + 1;
    let mut last_closing = None;
    for at in earliest_body_start..len {
        if closing_end(at).is_some() {
            last_closing = Some(at);
        }
    }
    let last_closing = last_closing?;

    // The engine prefers the longest opening that still leaves a closing fence behind.
    let mut opening_end = opening_ends[0];
    for candidate in opening_ends {
        if candidate < last_closing {
            opening_end = candidate;
        }
    }

    let body_start = opening_end + 1;
    for at in body_start..=last_closing {
        if let Some(end) = closing_end(at) {
            return Some(FrontMatterMatch {
                indent,
                body: chars[body_start..at].iter().collect(),
                length: chars[..=end].iter().map(|c| c.len_utf8()).sum(),
            });
        }
    }
    None
}

fn is_line_break(c: char) -> bool {
    c == '\n' || c == '\r'
}

fn starts_with_chars(chars: &[char], at: usize, prefix: &str) -> bool {
    prefix
        .chars()
        .enumerate()
        .all(|(offset, expected)| chars.get(at + offset) == Some(&expected))
}

fn starts_with_seq(chars: &[char], at: usize, prefix: &[char]) -> bool {
    prefix
        .iter()
        .enumerate()
        .all(|(offset, expected)| chars.get(at + offset) == Some(expected))
}

/// `utils.ts`'s `removeDirectives`, except that a removed multi-line directive keeps its
/// line breaks instead of collapsing them.
pub(crate) fn remove_directives(text: &str) -> String {
    let directives = scan_directives(text);
    if directives.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for directive in directives {
        out.push_str(&text[cursor..directive.start]);
        let removed = &text[directive.start..directive.end];
        for _ in 0..removed.matches('\n').count() {
            out.push('\n');
        }
        cursor = directive.end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// `diagram-api/comments.ts`'s `cleanupComments`: a line whose first non-whitespace
/// characters are `%%`, and whose next character is not `{` (a directive), is a comment.
///
/// Upstream deletes those lines and trims the blank lines before the diagram. [`Source::body`]
/// keeps the input's line numbers, so a comment line becomes an empty line here, and the
/// parsers read a body they already skip.
pub(crate) fn cleanup_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        if !is_comment_line(line) {
            out.push_str(line);
        }
    }
    out
}

/// `^\s*%%(?!{)[^\n]+`, matched within one line.
fn is_comment_line(line: &str) -> bool {
    let rest = line.trim_start_matches(is_js_whitespace);
    match rest.strip_prefix("%%") {
        Some(after) => !after.is_empty() && !after.starts_with('{'),
        None => false,
    }
}

/// `diagram-api/regexes.ts`'s `stripAnyComments`, the linear replacement for
/// `text.replace(/\s*%%.*\n/gm, '\n')`.
pub(crate) fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut consumed = 0;
    let mut search_from = 0;
    while search_from < text.len() {
        let Some(offset) = text[search_from..].find("%%") else {
            break;
        };
        let marker = search_from + offset;
        let Some(line_offset) = text[marker..].find('\n') else {
            break;
        };
        let line_end = marker + line_offset;
        let mut start = marker;
        while start > consumed {
            let previous = text[..start].chars().next_back().expect("start > consumed");
            if !is_js_whitespace(previous) {
                break;
            }
            start -= previous.len_utf8();
        }
        out.push_str(&text[consumed..start]);
        out.push('\n');
        consumed = line_end + 1;
        search_from = consumed;
    }
    out.push_str(&text[consumed..]);
    out
}

#[derive(Clone, Debug, PartialEq)]
enum DirectiveArg {
    Word(String),
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
struct RawDirective {
    start: usize,
    end: usize,
    kind: String,
    arg: Option<DirectiveArg>,
}

/// `directiveRegex` from `regexes.ts`, scanned by hand: `%{2}{\s*(?:(\w+)\s*:|(\w+))\s*
/// (?:(\w+)|((?:(?!}%{2}).|\r?\n)*))?\s*(?:}%{2})?` with the `gi` flags.
fn scan_directives(text: &str) -> Vec<RawDirective> {
    let mut directives = Vec::new();
    let mut index = 0;
    while index + 2 < text.len() {
        if text[index..].starts_with("%%{")
            && let Some(directive) = scan_directive_at(text, index)
        {
            index = directive.end;
            directives.push(directive);
            continue;
        }
        index += char_at(text, index).map_or(1, char::len_utf8);
    }
    directives
}

fn scan_directive_at(text: &str, start: usize) -> Option<RawDirective> {
    let mut pos = start + 3;
    pos = skip_whitespace(text, pos);
    let kind_start = pos;
    while char_at(text, pos).is_some_and(is_word_char) {
        pos += char_at(text, pos).map_or(1, char::len_utf8);
    }
    if pos == kind_start {
        return None;
    }
    let kind = text[kind_start..pos].to_string();

    let after_word = skip_whitespace(text, pos);
    if char_at(text, after_word) == Some(':') {
        pos = after_word + 1;
    } else {
        pos = after_word;
    }
    pos = skip_whitespace(text, pos);

    let arg = if char_at(text, pos).is_some_and(is_word_char) {
        let word_start = pos;
        while char_at(text, pos).is_some_and(is_word_char) {
            pos += char_at(text, pos).map_or(1, char::len_utf8);
        }
        Some(DirectiveArg::Word(text[word_start..pos].to_string()))
    } else {
        let body_start = pos;
        while pos < text.len() && !text[pos..].starts_with("}%%") {
            pos += char_at(text, pos).map_or(1, char::len_utf8);
        }
        // An empty group is falsy in upstream's `match[4] ? ... : null`.
        if pos == body_start {
            None
        } else {
            Some(DirectiveArg::Text(text[body_start..pos].to_string()))
        }
    };

    pos = skip_whitespace(text, pos);
    if text[pos..].starts_with("}%%") {
        pos += 3;
    }
    Some(RawDirective {
        start,
        end: pos,
        kind,
        arg,
    })
}

fn skip_whitespace(text: &str, mut pos: usize) -> usize {
    while let Some(c) = char_at(text, pos) {
        if !is_js_whitespace(c) {
            break;
        }
        pos += c.len_utf8();
    }
    pos
}

fn char_at(text: &str, pos: usize) -> Option<char> {
    if pos < text.len() {
        text[pos..].chars().next()
    } else {
        None
    }
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// One directive whose type passed the filter, with its parsed arguments.
struct Directive {
    kind: String,
    args: Option<Value>,
}

/// `utils.ts`'s `detectDirective`. `None` is upstream's catch: one directive with
/// unparsable JSON drops the whole result.
fn detect_directives(text: &str, wanted: impl Fn(&str) -> bool) -> Option<Vec<Directive>> {
    let prepared = directive_text(text);
    let mut found = Vec::new();
    for raw in scan_directives(&prepared) {
        if !wanted(&raw.kind) {
            continue;
        }
        let args = match raw.arg {
            None => None,
            Some(DirectiveArg::Word(word)) => Some(Value::String(word)),
            Some(DirectiveArg::Text(body)) => Some(serde_json::from_str(body.trim()).ok()?),
        };
        found.push(Directive {
            kind: raw.kind,
            args,
        });
    }
    Some(found)
}

/// The preparation `detectDirective` does before scanning: trim, drop comment-form
/// directives, and turn single quotes into double quotes so `JSON.parse` accepts them.
fn directive_text(text: &str) -> String {
    let trimmed = text.trim_matches(is_js_whitespace);
    strip_directive_comment_lines(trimmed).replace('\'', "\"")
}

/// The `[%]{2}(?![{]...(?=[}][%]{2}).*\n` removal in `detectDirective`, line by line.
fn strip_directive_comment_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        match comment_directive_start(line) {
            Some(start) => out.push_str(&line[..start]),
            None => out.push_str(line),
        }
    }
    out
}

fn comment_directive_start(line: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(offset) = line[from..].find("%%") {
        let at = from + offset;
        let rest = &line[at + 2..];
        if !rest.starts_with('{') {
            let rest = rest.trim_end();
            if !rest.is_empty() && rest.contains("}%%") {
                return Some(at);
            }
        }
        from = at + 2;
    }
    None
}

/// `utils.ts`'s `detectInit`: every `init`/`initialize` directive's arguments merged, with
/// a `config` key moved under the detected diagram's configuration key.
fn detect_init(text: &str) -> Option<Value> {
    let directives = detect_directives(text, is_init_directive)?;
    if directives.is_empty() {
        return None;
    }
    let mut value = if directives.len() == 1 {
        directives
            .into_iter()
            .next()
            .and_then(|directive| directive.args)
            .unwrap_or(Value::Null)
    } else {
        let mut merged = Value::Object(Map::new());
        for directive in directives {
            assign_with_depth(&mut merged, &directive.args.unwrap_or(Value::Null), 2);
        }
        merged
    };
    if !truthy(&value) {
        return None;
    }
    if let Value::Object(object) = &mut value
        && let Some(config) = object.remove("config")
    {
        let key = match diagrams::detect_tag(text) {
            // Upstream rewrites only `flowchart-v2`; `flowchart-elk` keeps its own id.
            Some(("flowchart-v2", _)) => "flowchart".to_string(),
            Some((id, _)) => id.to_string(),
            None => "config".to_string(),
        };
        object.insert(key, config);
    }
    Some(value)
}

/// Upstream matches `/(?:init\b)|(?:initialize\b)/` against the directive's name.
fn is_init_directive(kind: &str) -> bool {
    contains_with_boundary(kind, "init") || contains_with_boundary(kind, "initialize")
}

fn contains_with_boundary(haystack: &str, needle: &str) -> bool {
    let mut from = 0;
    while let Some(offset) = haystack[from..].find(needle) {
        let end = from + offset + needle.len();
        if haystack[end..]
            .chars()
            .next()
            .is_none_or(|c| !is_word_char(c))
        {
            return true;
        }
        from = from + offset + 1;
    }
    false
}

/// `processDirectives`: the init configuration plus the `wrap` directive's boolean.
fn directive_config(text: &str) -> Map<String, Value> {
    let mut config = match detect_init(text) {
        Some(Value::Object(config)) => config,
        _ => Map::new(),
    };
    let mut wrapping = detect_directives(text, |kind| kind.contains("wrap")).unwrap_or_default();
    match wrapping.len() {
        0 => {}
        1 => {
            if wrapping.remove(0).kind == "wrap" {
                config.insert("wrap".to_string(), Value::Bool(true));
            }
        }
        // With several matches upstream assigns the `some` result, false included.
        _ => {
            let wrap = wrapping.iter().any(|directive| directive.kind == "wrap");
            config.insert("wrap".to_string(), Value::Bool(wrap));
        }
    }
    config
}

/// `cleanAndMerge(defaultData, data)` = lodash `merge({}, defaultData, data)`.
fn clean_and_merge(base: Map<String, Value>, data: Value) -> Map<String, Value> {
    let mut merged = Value::Object(Map::new());
    merge_value(&mut merged, &Value::Object(base));
    merge_value(&mut merged, &data);
    match merged {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// lodash `merge`: deep for maps, index-wise for arrays, and the source wins last.
fn merge_value(destination: &mut Value, source: &Value) {
    match (destination, source) {
        (Value::Object(destination), Value::Object(source)) => {
            for (key, value) in source {
                match destination.get_mut(key) {
                    Some(existing) => merge_value(existing, value),
                    None => {
                        destination.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (Value::Array(destination), Value::Array(source)) => {
            for (index, value) in source.iter().enumerate() {
                if index < destination.len() {
                    merge_value(&mut destination[index], value);
                } else {
                    destination.push(value.clone());
                }
            }
        }
        (destination, source) => *destination = source.clone(),
    }
}

/// `assignWithDepth.ts` at its default depth of 2: objects merge, arrays append unique
/// items, and dissimilar types do not clobber each other.
fn assign_with_depth(destination: &mut Value, source: &Value, depth: i32) {
    if let Value::Array(items) = source {
        if let Value::Array(destination) = destination {
            for item in items {
                if !destination.contains(item) {
                    destination.push(item.clone());
                }
            }
        } else {
            for item in items {
                assign_with_depth(destination, item, depth);
            }
        }
        return;
    }
    if source.is_null() {
        // A null argument is skipped; upstream's return value is only used by nested
        // merges, and those skip null values before they get here.
        return;
    }
    if depth <= 0 {
        match (&mut *destination, source) {
            (Value::Object(destination), Value::Object(source)) => {
                for (key, value) in source {
                    destination.insert(key.clone(), value.clone());
                }
            }
            (Value::Array(destination), Value::Array(source)) => {
                for (index, value) in source.iter().enumerate() {
                    if index < destination.len() {
                        destination[index] = value.clone();
                    } else {
                        destination.push(value.clone());
                    }
                }
            }
            (destination, source) => *destination = source.clone(),
        }
        return;
    }
    if let (Value::Object(destination), Value::Object(source)) = (&mut *destination, source) {
        for (key, value) in source {
            if value.is_object() {
                if !destination.contains_key(key) {
                    destination.insert(key.clone(), Value::Null);
                }
                if destination.get(key).is_none_or(Value::is_null) {
                    destination.insert(
                        key.clone(),
                        if value.is_array() {
                            Value::Array(Vec::new())
                        } else {
                            Value::Object(Map::new())
                        },
                    );
                }
                if destination.get(key).is_some_and(Value::is_object) {
                    let child = destination.get_mut(key).expect("just checked");
                    assign_with_depth(child, value, depth - 1);
                }
            } else if !destination.get(key).is_some_and(Value::is_object) {
                destination.insert(key.clone(), value.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Upstream's frontmatter spec compares the text that follows the block. The body here
    /// keeps the block's lines as empty lines, so drop those leading breaks to compare.
    fn after_frontmatter(source: &Source) -> &str {
        source.body.trim_start_matches('\n')
    }

    fn config(source: &Source) -> Value {
        Value::Object(source.config.clone())
    }

    fn line_count(text: &str) -> usize {
        text.split('\n').count()
    }

    #[test]
    fn returns_text_unchanged_if_no_frontmatter() {
        let source = preprocess("diagram");
        assert_eq!(source.body, "diagram");
        assert_eq!(source.title, None);
        assert!(source.config.is_empty());
        assert_eq!(source.error, None);
    }

    #[test]
    fn returns_text_unchanged_if_frontmatter_lacks_closing_delimiter() {
        let text = "---\ntitle: foo\ndiagram";
        let source = preprocess(text);
        assert_eq!(source.body, text);
        assert_eq!(source.title, None);
    }

    #[test]
    fn handles_empty_frontmatter() {
        let source = preprocess("---\n\n---\ndiagram");
        assert_eq!(after_frontmatter(&source), "diagram");
        assert_eq!(source.title, None);
        assert_eq!(line_count(&source.body), 4);
    }

    #[test]
    fn handles_frontmatter_without_mappings() {
        for text in [
            "---\n1\n---\ndiagram",
            "---\n-1\n-2\n---\ndiagram",
            "---\nnull\n---\ndiagram",
        ] {
            let source = preprocess(text);
            assert_eq!(after_frontmatter(&source), "diagram", "text: {text:?}");
            assert_eq!(source.title, None, "text: {text:?}");
            assert!(source.config.is_empty(), "text: {text:?}");
        }
    }

    #[test]
    fn does_not_try_to_parse_frontmatter_at_the_end() {
        let text = "diagram\n---\ntitle: foo\n---\n";
        let source = preprocess(text);
        assert_eq!(source.body, text);
        assert_eq!(source.title, None);
    }

    #[test]
    fn handles_frontmatter_with_multiple_delimiters() {
        let source = preprocess("---\ntitle: foo---bar\n---\ndiagram\n---\ntest");
        assert_eq!(source.title.as_deref(), Some("foo---bar"));
        assert_eq!(after_frontmatter(&source), "diagram\n---\ntest");
    }

    #[test]
    fn handles_frontmatter_with_multi_line_string_and_multiple_delimiters() {
        let source = preprocess("---\ntitle: |\n   multi-line string\n   ---\n---\ndiagram");
        assert_eq!(source.title.as_deref(), Some("multi-line string\n---\n"));
        assert_eq!(after_frontmatter(&source), "diagram");
    }

    #[test]
    fn handles_frontmatter_with_title() {
        let source = preprocess("---\ntitle: foo\n---\ndiagram");
        assert_eq!(source.title.as_deref(), Some("foo"));
        assert_eq!(after_frontmatter(&source), "diagram");
    }

    #[test]
    fn handles_booleans_in_frontmatter_properly() {
        let source = preprocess("---\ntitle: true\n---\ndiagram");
        assert_eq!(source.title.as_deref(), Some("true"));
    }

    #[test]
    fn ignores_unspecified_frontmatter_keys() {
        let source = preprocess("---\ninvalid: true\ntitle: foo\ntest: bar\n---\ndiagram");
        assert_eq!(source.title.as_deref(), Some("foo"));
        assert!(source.config.is_empty());
    }

    #[test]
    fn throws_exception_for_invalid_yaml_syntax() {
        let source = preprocess("---\n!!!\n---\ndiagram");
        assert_eq!(
            source.error.as_deref(),
            Some("tag suffix cannot contain exclamation marks")
        );
        assert_eq!(source.body, "---\n!!!\n---\ndiagram");
    }

    #[test]
    fn handles_leading_horizontal_whitespace_before_delimiters() {
        let source = preprocess("   ---\n   title: foo\n   ---\n   diagram");
        assert_eq!(source.title.as_deref(), Some("foo"));
        assert_eq!(after_frontmatter(&source), "   diagram");
    }

    #[test]
    fn handles_tab_indentation_before_delimiters() {
        let source = preprocess("\t---\n\ttitle: foo\n\t---\n\tdiagram");
        assert_eq!(source.title.as_deref(), Some("foo"));
        assert_eq!(after_frontmatter(&source), "\tdiagram");
    }

    #[test]
    fn handles_indented_frontmatter_with_config() {
        let source = preprocess(
            "   ---\n   config:\n     sankey:\n       showValues: true\n   ---\n   sankey\n   a,b,8",
        );
        assert_eq!(config(&source), json!({ "sankey": { "showValues": true } }));
        assert_eq!(after_frontmatter(&source), "   sankey\n   a,b,8");
    }

    #[test]
    fn does_not_match_an_indented_closing_delimiter_when_opening_is_at_column_zero() {
        let source = preprocess("---\ntitle: foo\n   ---\n   diagram");
        assert_eq!(source.title, None);
    }

    #[test]
    fn handles_frontmatter_with_config() {
        let text = "---\ntitle: hello\nconfig:\n  graph:\n    string: hello\n    number: 14\n    boolean: false\n    array: [1, 2, 3]\n---\ndiagram";
        let source = preprocess(text);
        assert_eq!(source.title.as_deref(), Some("hello"));
        assert_eq!(
            config(&source),
            json!({
                "graph": {
                    "string": "hello",
                    "number": 14,
                    "boolean": false,
                    "array": [1, 2, 3],
                }
            })
        );
        assert_eq!(after_frontmatter(&source), "diagram");
    }

    #[test]
    fn display_mode_lands_in_the_gantt_config() {
        let source = preprocess("---\ndisplayMode: compact\n---\ngantt\nA: 1");
        assert_eq!(
            config(&source),
            json!({ "gantt": { "displayMode": "compact" } })
        );
    }

    #[test]
    fn display_mode_keeps_other_gantt_keys() {
        let source = preprocess(
            "---\ndisplayMode: compact\nconfig:\n  gantt:\n    useMaxWidth: true\n---\ngantt",
        );
        assert_eq!(
            config(&source),
            json!({ "gantt": { "useMaxWidth": true, "displayMode": "compact" } })
        );
    }

    #[test]
    fn body_keeps_the_input_line_count() {
        for text in [
            "---\ntitle: foo\n---\ngraph LR\n  A --> B",
            "graph LR\n  A --> B",
            "%%{init: {\"theme\": \"dark\"}}%%\ngraph LR\n  A --> B",
        ] {
            assert_eq!(
                line_count(&preprocess(text).body),
                line_count(text),
                "text: {text:?}"
            );
        }
    }

    #[test]
    fn single_line_html_attributes_use_single_quotes() {
        let source = preprocess("graph LR\nA[\"<div class=\"x\">hi</div>\"]");
        assert!(source.body.contains("class='x'"));
    }

    #[test]
    fn normalize_line_endings() {
        let source = preprocess("graph LR\r\nA --> B\rC");
        assert_eq!(source.body, "graph LR\nA --> B\nC");
    }

    #[test]
    fn handles_an_initialize_definition() {
        let text = "\n%%{initialize: { 'logLevel': 0, 'theme': 'dark' }}%%\nsequenceDiagram\nAlice->Bob: hi";
        let source = preprocess(text);
        assert_eq!(config(&source), json!({ "logLevel": 0, "theme": "dark" }));
        assert_eq!(source.body, "\n\nsequenceDiagram\nAlice->Bob: hi");
    }

    #[test]
    fn handles_an_init_definition() {
        let text =
            "\n%%{init: { 'logLevel': 0, 'theme': 'dark' }}%%\nsequenceDiagram\nAlice->Bob: hi";
        assert_eq!(
            config(&preprocess(text)),
            json!({ "logLevel": 0, "theme": "dark" })
        );
    }

    #[test]
    fn moves_the_init_config_under_the_diagram_key() {
        let text = "\n%%{init: { 'logLevel': 0, 'theme': 'dark', 'config': {'wrap': true} } }%%\nsequenceDiagram\nAlice->Bob: hi";
        assert_eq!(
            config(&preprocess(text)),
            json!({ "logLevel": 0, "theme": "dark", "sequence": { "wrap": true } })
        );
    }

    #[test]
    fn moves_the_init_config_under_flowchart_for_a_flowchart() {
        let text = "%%{init: { 'config': { 'curve': 'basis' } }}%%\ngraph LR\nA --> B";
        assert_eq!(
            config(&preprocess(text)),
            json!({ "flowchart": { "curve": "basis" } })
        );
    }

    #[test]
    fn keeps_the_flowchart_elk_id_for_the_init_config() {
        // Upstream rewrites only `flowchart-v2`; `flowchart-elk` keeps its own detector id.
        let text = "%%{init: { 'config': { 'curve': 'basis' } }}%%\nflowchart-elk LR\nA --> B";
        assert_eq!(
            config(&preprocess(text)),
            json!({ "flowchart-elk": { "curve": "basis" } })
        );
    }

    #[test]
    fn handles_a_multiline_init_definition() {
        let text = "\n%%{\n  init: {\n    'logLevel': 0,\n    'theme': 'dark'\n  }\n}%%\nsequenceDiagram\nAlice->Bob: hi";
        let source = preprocess(text);
        assert_eq!(config(&source), json!({ "logLevel": 0, "theme": "dark" }));
        assert_eq!(line_count(&source.body), line_count(text));
    }

    #[test]
    fn handles_multiple_init_directives() {
        let text = "\n%%{ init: { 'logLevel': 0, 'theme': 'forest' } }%%\n%%{\n  init: {\n    'theme': 'dark'\n  }\n}%%\nsequenceDiagram\nAlice->Bob: hi";
        assert_eq!(
            config(&preprocess(text)),
            json!({ "logLevel": 0, "theme": "dark" })
        );
    }

    #[test]
    fn directive_config_wins_over_frontmatter_config() {
        let text = "---\nconfig:\n  theme: forest\n  logLevel: 1\n---\n%%{init: { 'theme': 'dark' }}%%\nflowchart LR\nA --> B";
        assert_eq!(
            config(&preprocess(text)),
            json!({ "theme": "dark", "logLevel": 1 })
        );
    }

    #[test]
    fn merges_nested_directive_config_into_frontmatter_config() {
        let text = "---\nconfig:\n  flowchart:\n    curve: basis\n    htmlLabels: false\n---\n%%{init: { 'flowchart': { 'htmlLabels': true } }}%%\nflowchart LR\nA --> B";
        assert_eq!(
            config(&preprocess(text)),
            json!({ "flowchart": { "curve": "basis", "htmlLabels": true } })
        );
    }

    #[test]
    fn handles_the_wrap_directive() {
        let source = preprocess("%%{wrap}%%\ngraph LR\nA --> B");
        assert_eq!(config(&source), json!({ "wrap": true }));
        assert_eq!(source.body, "\ngraph LR\nA --> B");
    }

    #[test]
    fn wrap_directive_needs_the_exact_name() {
        let source = preprocess("%%{wrappers}%%\ngraph LR\nA --> B");
        assert!(source.config.is_empty());
    }

    #[test]
    fn several_wrap_like_directives_set_wrap_to_the_exact_match() {
        let source = preprocess("%%{wrappers}%%\n%%{wrappers}%%\ngraph LR\nA --> B");
        assert_eq!(config(&source), json!({ "wrap": false }));
    }

    #[test]
    fn handles_the_wrap_directive_like_upstream() {
        assert!(detect_wrap("%%{wrap}%%"));
        assert!(detect_wrap("%%{\n      wrap\n    }%%"));
        assert!(detect_wrap("%%{\n\n          wrap\n\n    }%%"));
        assert!(detect_wrap("%%{wrap:}%%"));
        assert!(detect_wrap("%%{wrap: }%%"));
        assert!(!detect_wrap("graph"));
    }

    fn detect_wrap(text: &str) -> bool {
        detect_directives(text, |kind| kind.contains("wrap"))
            .is_some_and(|directives| directives.iter().any(|d| d.kind == "wrap"))
    }

    #[test]
    fn a_comment_that_looks_like_a_directive_is_ignored() {
        let source = preprocess("%% init: { 'theme': 'dark' } }%%\ngraph LR\nA --> B");
        assert!(source.config.is_empty());
    }

    #[test]
    fn invalid_directive_json_is_ignored() {
        let source = preprocess("%%{init: {}}%%\ngraph LR\nA --> B");
        assert!(source.config.is_empty());
        assert_eq!(source.body, "\ngraph LR\nA --> B");
    }

    #[test]
    fn a_directive_without_a_colon_has_word_arguments() {
        let text = "%%{init: dark}%%\ngraph LR\nA --> B";
        assert_eq!(config(&preprocess(text)), json!({}));
    }

    #[test]
    fn init_config_without_a_detected_diagram_keeps_its_key() {
        let text = "%%{init: { 'config': { 'wrap': true } }}%%\nnot a diagram";
        assert_eq!(
            config(&preprocess(text)),
            json!({ "config": { "wrap": true } })
        );
    }

    #[test]
    fn clean_and_merge_merges_objects() {
        assert_eq!(
            clean_and_merge(
                serde_json::from_value(json!({ "a": 1, "b": 2 })).unwrap(),
                json!({ "b": 3 })
            ),
            serde_json::from_value(json!({ "a": 1, "b": 3 })).unwrap()
        );
        assert_eq!(
            clean_and_merge(
                serde_json::from_value(json!({ "a": 1 })).unwrap(),
                json!({ "a": 2 })
            ),
            serde_json::from_value(json!({ "a": 2 })).unwrap()
        );
    }

    #[test]
    fn clean_and_merge_keeps_destination_keys_missing_from_the_source() {
        assert_eq!(
            clean_and_merge(
                serde_json::from_value(json!({ "a": 1, "b": { "c": 2 } })).unwrap(),
                json!({ "a": 2 })
            ),
            serde_json::from_value(json!({ "a": 2, "b": { "c": 2 } })).unwrap()
        );
    }

    #[test]
    fn assign_with_depth_does_not_clobber_dissimilar_types() {
        let mut destination = json!({ "foo": "bar", "bar": { "foo": "bar" } });
        assign_with_depth(&mut destination, &json!({ "foo": "foo", "bar": "nope" }), 2);
        assert_eq!(
            destination,
            json!({ "foo": "foo", "bar": { "foo": "bar" } })
        );
    }

    #[test]
    fn assign_with_depth_merges_source_orphans() {
        let mut destination = json!({ "foo": "bar" });
        let source = json!({ "foo": "bar", "bar": { "foo": "bar" } });
        assign_with_depth(&mut destination, &source, 2);
        assert_eq!(destination, source);
    }

    #[test]
    fn assign_with_depth_merges_at_depth_two() {
        let mut destination = json!({ "foo": "bar", "bar": { "foo": "bar" }, "boofar": 1 });
        let source = json!({ "foo": "foo", "bar": { "bar": 0 }, "foobar": "foobar" });
        assign_with_depth(&mut destination, &source, 2);
        assert_eq!(
            destination,
            json!({
                "foo": "foo",
                "bar": { "foo": "bar", "bar": 0 },
                "foobar": "foobar",
                "boofar": 1,
            })
        );
    }

    #[test]
    fn assign_with_depth_ignores_null_values() {
        let mut destination = json!({ "foo": "bar", "bar": { "foo": "bar" }, "boofar": 1 });
        let source = json!({ "foo": "foo", "bar": null, "foobar": "foobar" });
        assign_with_depth(&mut destination, &source, 2);
        assert_eq!(
            destination,
            json!({ "foo": "foo", "bar": { "foo": "bar" }, "foobar": "foobar", "boofar": 1 })
        );
    }

    #[test]
    fn assign_with_depth_does_not_replace_an_object_with_null() {
        let mut destination = json!({ "a": 1 });
        assign_with_depth(&mut destination, &Value::Null, 2);
        assert_eq!(destination, json!({ "a": 1 }));
    }

    #[test]
    fn assign_with_depth_clobbers_below_the_depth_limit() {
        let mut destination = json!({
            "bar": { "bar": { "foo": { "message": "this", "willbe": "clobbered" } } }
        });
        let source = json!({
            "bar": { "bar": { "foo": { "message": "clobbered other foo" } } }
        });
        assign_with_depth(&mut destination, &source, 2);
        assert_eq!(
            destination,
            json!({ "bar": { "bar": { "foo": { "message": "clobbered other foo" } } } })
        );
    }

    #[test]
    fn rejects_deeply_nested_flow_collections_instead_of_overflowing() {
        let nested = format!("value: {}{}", "[".repeat(200), "]".repeat(200));
        let text = format!("---\n{nested}\n---\ndiagram");
        assert!(preprocess(&text).error.is_some());
    }

    #[test]
    fn rejects_excessively_indented_frontmatter_instead_of_overflowing() {
        let mut body = String::new();
        for level in 0..1100 {
            body.push_str(&" ".repeat(level));
            body.push_str(&format!("k{level}:\n"));
        }
        let text = format!("---\n{body}---\ndiagram");
        assert!(preprocess(&text).error.is_some());
    }

    #[test]
    fn match_front_matter_requires_a_trailing_line_break() {
        assert!(match_front_matter("---\ntitle: foo\n---").is_none());
        assert!(match_front_matter("---\ntitle: foo\n---\n").is_some());
    }

    #[test]
    fn cleanup_comments_blanks_a_comment_line_and_keeps_its_line() {
        let source = preprocess("%% a comment\ngraph LR\n  %% another\nA --> B");
        assert_eq!(source.body, "\ngraph LR\n\nA --> B");
        assert_eq!(line_count(&source.body), 4);
    }

    #[test]
    fn cleanup_comments_leaves_a_line_that_only_starts_with_the_marker() {
        // Upstream's pattern needs a character after `%%`.
        assert_eq!(preprocess("%%\ngraph LR").body, "%%\ngraph LR");
    }

    #[test]
    fn cleanup_comments_leaves_a_directive_like_line_to_the_directive_rule() {
        let source = preprocess("%%{init: {'theme': 'dark'}}%%\ngraph LR");
        assert_eq!(source.body, "\ngraph LR");
        assert_eq!(config(&source), json!({ "theme": "dark" }));
    }

    #[test]
    fn strip_comments_removes_comment_lines() {
        assert_eq!(strip_comments("%% a\ngraph LR"), "\ngraph LR");
        assert_eq!(strip_comments("  %% a\ngraph LR"), "\ngraph LR");
        assert_eq!(
            strip_comments("graph LR\n%% a\nA --> B"),
            "graph LR\nA --> B"
        );
        assert_eq!(
            strip_comments("graph LR\nA --> B %% not a comment"),
            "graph LR\nA --> B %% not a comment"
        );
        assert_eq!(strip_comments("%% no newline"), "%% no newline");
    }
}
