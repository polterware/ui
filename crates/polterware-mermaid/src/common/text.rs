//! The small text helpers that upstream's generated lexers and grammars share: the
//! JavaScript line-terminator set, `\r?\n`, the horizontal-space trim, Langium's `STRING`
//! conversion, and JavaScript truthiness for the values `js-yaml` can produce.
//!
//! Every diagram's lexer or parser used to spell these out; they are identical ports of the
//! same upstream helpers, so they live here once.

use serde_json::Value;

/// JavaScript's `lineTerminator` set: `\n`, `\r`, `\u2028`, and `\u2029`.
pub fn is_line_terminator(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `[ \t]*`, the horizontal whitespace a grammar trims around a token.
pub fn trim_horizontal(text: &str) -> &str {
    text.trim_start_matches([' ', '\t'])
}

/// `NEWLINE: /\r?\n/`: the byte length of a line break at the start of `rest`.
pub fn newline(rest: &str) -> Option<usize> {
    if rest.starts_with("\r\n") {
        Some(2)
    } else if rest.starts_with('\n') {
        Some(1)
    } else {
        None
    }
}

/// Langium's default `STRING` conversion: drop the quotes and read `\` escapes.
pub fn convert_string(raw: &str) -> String {
    let inner = &raw[1..raw.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('v') => out.push('\u{b}'),
            Some('0') => out.push('\0'),
            Some(other) => out.push(other),
            // The lexer only produces a string when every backslash has a character.
            None => {}
        }
    }
    out
}

/// JavaScript truthiness for the values `js-yaml` can produce: an empty array and an empty
/// object are truthy, `0`, `""`, `null`, `false`, and `NaN` are not.
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn line_terminators_follow_javascript() {
        assert!(is_line_terminator('\n'));
        assert!(is_line_terminator('\r'));
        assert!(is_line_terminator('\u{2028}'));
        assert!(is_line_terminator('\u{2029}'));
        assert!(!is_line_terminator(' '));
    }

    #[test]
    fn trim_horizontal_stops_at_a_line_break() {
        assert_eq!(trim_horizontal("  \tvalue\n "), "value\n ");
    }

    #[test]
    fn newline_reads_both_spellings() {
        assert_eq!(newline("\r\nrest"), Some(2));
        assert_eq!(newline("\nrest"), Some(1));
        assert_eq!(newline(" rest"), None);
        assert_eq!(newline("\r rest"), None);
    }

    #[test]
    fn convert_string_drops_the_quotes_and_reads_escapes() {
        assert_eq!(convert_string("\"a\\nb\\\"c\""), "a\nb\"c");
        assert_eq!(convert_string("'plain'"), "plain");
        assert_eq!(
            convert_string("\"\\t\\r\\b\\f\\v\\0\""),
            "\t\r\u{8}\u{c}\u{b}\0"
        );
    }

    #[test]
    fn is_truthy_follows_javascript() {
        assert!(!is_truthy(&Value::Null));
        assert!(!is_truthy(&json!(false)));
        assert!(!is_truthy(&json!(0)));
        assert!(!is_truthy(&json!("")));
        assert!(is_truthy(&json!(true)));
        assert!(is_truthy(&json!("0")));
        assert!(is_truthy(&json!([1])));
        assert!(is_truthy(&json!({ "a": 1 })));
    }
}
