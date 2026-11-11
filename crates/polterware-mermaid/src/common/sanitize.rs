//! `diagrams/common/common.ts` without a DOM: the line-break helpers, `getRows`, and the
//! `sanitizeText` family, plus the two accessibility setters every `*Db.ts` copies from
//! `commonDb.ts`.
//!
//! Upstream's `sanitizeText` is DOMPurify. There is no DOM here, so the port approximates
//! what DOMPurify does to the markup labels carry: `script` (and, at the default security
//! level, `style`) elements are dropped with their content, a `<` that opens no tag is
//! escaped as `&lt;`, and a break tag is serialized as `<br>`. The rest of the markup is
//! kept; full sanitizing belongs to the painter. Diagram configuration (`securityLevel`,
//! `htmlLabels`, `dompurifyConfig`) is not owned by this crate, so the default is the only
//! level ported: `sanitizeMore` is folded into [`remove_script`] and [`sanitize_text`].

use serde_json::Value;

use super::is_js_whitespace;

/// Upstream's `lineBreakRegex`, `/<\/?br\s*\/?>/gi`, as text: every break spelling a label
/// may carry, the optional leading slash accepting the malformed `</br>`.
pub const LINE_BREAK_REGEX: &str = r"<\/?br\s*\/?>";

/// The byte length of a `lineBreakRegex` match at the start of `text`.
fn break_tag_len(text: &str) -> Option<usize> {
    let rest = text.strip_prefix('<')?;
    let mut end = 1;
    let rest = match rest.strip_prefix('/') {
        Some(after) => {
            end += 1;
            after
        }
        None => rest,
    };
    if !rest.get(..2)?.eq_ignore_ascii_case("br") {
        return None;
    }
    end += 2;
    let rest = &rest[2..];
    let spaces = rest.len() - rest.trim_start_matches(is_js_whitespace).len();
    end += spaces;
    let rest = &rest[spaces..];
    let rest = match rest.strip_prefix('/') {
        Some(after) => {
            end += 1;
            after
        }
        None => rest,
    };
    rest.strip_prefix('>')?;
    Some(end + 1)
}

/// `hasBreaks`: whether the text carries a break tag. Upstream tests the regex without the
/// `g` flag, because `.test()` on a global regex advances `lastIndex`.
pub fn has_breaks(text: &str) -> bool {
    text.match_indices('<')
        .any(|(index, _)| break_tag_len(&text[index..]).is_some())
}

/// `splitBreaks`: the lines of a label, split at its break tags.
pub fn split_breaks(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut search = 0;
    while let Some(offset) = text[search..].find('<') {
        let at = search + offset;
        match break_tag_len(&text[at..]) {
            Some(length) => {
                parts.push(&text[start..at]);
                start = at + length;
                search = start;
            }
            None => search = at + 1,
        }
    }
    parts.push(&text[start..]);
    parts
}

/// `breakToPlaceholder`: every break tag becomes the `#br#` placeholder.
pub fn break_to_placeholder(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start = 0;
    let mut search = 0;
    while let Some(offset) = text[search..].find('<') {
        let at = search + offset;
        match break_tag_len(&text[at..]) {
            Some(length) => {
                out.push_str(&text[start..at]);
                out.push_str("#br#");
                start = at + length;
                search = start;
            }
            None => search = at + 1,
        }
    }
    out.push_str(&text[start..]);
    out
}

/// `placeholderToBreak`: every `#br#` placeholder becomes `<br/>`.
pub fn placeholder_to_break(text: &str) -> String {
    text.replace("#br#", "<br/>")
}

/// `getRows`: the rows of a label, with its break tags and its literal `\n` as separators.
pub fn get_rows(text: Option<&str>) -> Vec<String> {
    let Some(text) = text.filter(|text| !text.is_empty()) else {
        return vec![String::new()];
    };
    break_to_placeholder(text)
        .replace("\\n", "#br#")
        .split("#br#")
        .map(str::to_string)
        .collect()
}

/// `config.ts` `evaluate`: `false` and the strings `false`, `null`, and `0` are falsy.
pub fn evaluate(value: Option<&Value>) -> bool {
    let text = match value {
        None => return true,
        Some(Value::Bool(false)) => return false,
        Some(Value::Bool(true)) => return true,
        Some(Value::Null) => "null".to_string(),
        Some(Value::Number(number)) => {
            if number.as_f64() == Some(0.0) {
                "0".to_string()
            } else {
                number.to_string()
            }
        }
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(_)) | Some(Value::Object(_)) => return true,
    };
    !matches!(
        text.trim().to_ascii_lowercase().as_str(),
        "false" | "null" | "0"
    )
}

/// `common.ts` `sanitizeText` at the default security level: DOMPurify with
/// `FORBID_TAGS: ['style']` after `sanitizeMore`'s `removeScript`.
pub fn sanitize_text(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    sanitize_markup(text, true, true)
}

/// `common.ts` `sanitizeText` for a label the painter draws as HTML (`htmlLabels` on). The
/// flowchart and agentflow ports leave a break tag as the author wrote it, because their
/// painter splits the label on break tags itself.
pub fn sanitize_html_label(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    sanitize_markup(text, true, false)
}

/// `common.ts` `removeScript`: `DOMPurify.sanitize(txt)` with no configuration.
pub fn remove_script(text: &str) -> String {
    sanitize_markup(text, false, true)
}

/// `commonDb.ts` `setAccDescription`: `sanitizeText(txt).replace(/\n\s+/g, '\n')`.
pub fn set_acc_description(text: &str) -> String {
    collapse_description_indent(&sanitize_text(text))
}

/// `setAccDescription`'s `replace(/\n\s+/g, '\n')`: the whitespace that follows a newline is
/// dropped, so a description written as an indented block reads as plain lines.
pub fn collapse_description_indent(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        out.push(ch);
        if ch == '\n' {
            while chars.peek().is_some_and(|next| is_js_whitespace(*next)) {
                chars.next();
            }
        }
    }
    out
}

/// `commonDb.ts` `setAccTitle`: `sanitizeText(txt).replace(/^\s+/g, '')`.
pub fn set_acc_title(text: &str) -> String {
    sanitize_text(text)
        .trim_start_matches(is_js_whitespace)
        .to_string()
}

/// What DOMPurify leaves of `text`: `script` (and `style` when `forbid_style`) elements are
/// dropped with their content, a `<` that opens no tag is escaped, and a break tag is
/// serialized as `<br>` unless `normalize_breaks` is off.
fn sanitize_markup(text: &str, forbid_style: bool, normalize_breaks: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        if let Some(consumed) = strip_element(rest, forbid_style) {
            i += consumed;
            continue;
        }
        if rest.starts_with('<') {
            if let Some(end) = tag_end(rest) {
                if normalize_breaks && break_tag_len(rest) == Some(end) {
                    out.push_str("<br>");
                } else {
                    out.push_str(&rest[..end]);
                }
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
fn strip_element(text: &str, forbid_style: bool) -> Option<usize> {
    let (name, open_end) = opening_tag(text)?;
    let lower = name.to_ascii_lowercase();
    if lower != "script" && !(forbid_style && lower == "style") {
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn sanitize_text_drops_script_and_style_with_their_content() {
        assert_eq!(
            sanitize_text("<script>alert(1)</script>A<style>b{}</style>B"),
            "AB"
        );
    }

    #[test]
    fn sanitize_text_escapes_a_stray_angle_bracket() {
        assert_eq!(sanitize_text("a < b"), "a &lt; b");
    }

    #[test]
    fn sanitize_text_serializes_every_break_spelling_as_br() {
        assert_eq!(
            sanitize_text("a<br>b<br/>c<br />d</br />e<br\t/>f"),
            "a<br>b<br>c<br>d<br>e<br>f"
        );
    }

    #[test]
    fn sanitize_html_label_keeps_a_break_as_written() {
        assert_eq!(sanitize_html_label("a<br/>b"), "a<br/>b");
    }

    #[test]
    fn remove_script_keeps_style() {
        assert_eq!(remove_script("<style>b{}</style>A"), "<style>b{}</style>A");
        assert_eq!(remove_script("<script>x</script>A"), "A");
    }

    #[test]
    fn has_breaks_matches_the_regex_ignoring_case() {
        assert!(has_breaks("a<br/>b"));
        assert!(has_breaks("a</BR />b"));
        assert!(!has_breaks("a < b"));
    }

    #[test]
    fn split_breaks_keeps_the_text_around_each_break() {
        assert_eq!(split_breaks("a<br>b<br/>c"), vec!["a", "b", "c"]);
        assert_eq!(split_breaks("no break"), vec!["no break"]);
    }

    #[test]
    fn get_rows_splits_on_breaks_and_literal_newline_escapes() {
        assert_eq!(get_rows(None), vec![String::new()]);
        assert_eq!(get_rows(Some("")), vec![String::new()]);
        assert_eq!(get_rows(Some("a<br>b\\nc")), vec!["a", "b", "c"]);
    }

    #[test]
    fn placeholder_round_trip() {
        assert_eq!(
            placeholder_to_break(&break_to_placeholder("a<br/>b")),
            "a<br/>b"
        );
    }

    #[test]
    fn evaluate_follows_upstream_falsy_values() {
        assert!(!evaluate(Some(&json!(false))));
        assert!(!evaluate(Some(&json!(" false "))));
        assert!(!evaluate(Some(&json!("NULL"))));
        assert!(!evaluate(Some(&json!(0))));
        assert!(!evaluate(Some(&Value::Null)));
        assert!(evaluate(None));
        assert!(evaluate(Some(&json!(true))));
        assert!(evaluate(Some(&json!(1))));
        assert!(evaluate(Some(&json!("yes"))));
        assert!(evaluate(Some(&json!([] as [u8; 0]))));
    }

    #[test]
    fn set_acc_title_strips_leading_whitespace() {
        assert_eq!(set_acc_title("  title  "), "title  ");
    }

    #[test]
    fn set_acc_description_collapses_the_indent_after_a_newline() {
        assert_eq!(set_acc_description("a\n   b"), "a\nb");
        assert_eq!(set_acc_description("a\n\n b"), "a\nb");
        assert_eq!(set_acc_description("a\n"), "a\n");
    }

    #[test]
    fn line_break_regex_is_the_upstream_pattern() {
        assert_eq!(LINE_BREAK_REGEX, r"<\/?br\s*\/?>");
    }
}
