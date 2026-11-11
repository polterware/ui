//! An approximation of upstream `common.ts` `sanitizeText` (DOMPurify) for the three strings
//! the pie database sanitizes: the diagram title and the two accessibility fields.
//!
//! `script` and `style` elements are dropped with their content, a `<` that opens no tag is
//! escaped, and `<br/>`/`<br />` are serialized as `<br>` the way DOMPurify does. The rest of
//! the markup is kept: there is no DOM here, and full sanitizing belongs to the painter.
//! `setAccDescription` then collapses the indentation that follows newlines and
//! `setAccTitle` strips leading whitespace, exactly as `commonDb.ts` does.

/// `common.ts` `sanitizeText`.
pub(crate) fn sanitize_text(text: &str) -> String {
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
                if is_break_tag(rest) {
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

/// `commonDb.ts` `setAccDescription`: `sanitizeText(txt).replace(/\n\s+/g, '\n')`.
pub(crate) fn set_acc_description(text: &str) -> String {
    let sanitized = sanitize_text(text);
    let mut out = String::with_capacity(sanitized.len());
    let mut i = 0;
    while i < sanitized.len() {
        let rest = &sanitized[i..];
        if rest.starts_with('\n') {
            let mut j = i + 1;
            while j < sanitized.len() {
                let ch = sanitized[j..].chars().next().expect("non-empty remainder");
                if ch.is_whitespace() {
                    j += ch.len_utf8();
                } else {
                    break;
                }
            }
            // `\n\s+` needs at least one whitespace character after the newline.
            if j > i + 1 {
                out.push('\n');
                i = j;
                continue;
            }
        }
        let ch = rest.chars().next().expect("non-empty remainder");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// `commonDb.ts` `setAccTitle`: `sanitizeText(txt).replace(/^\s+/g, '')`.
pub(crate) fn set_acc_title(text: &str) -> String {
    sanitize_text(text).trim_start().to_string()
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

/// Whether the tag starting at `<` is a break element, which DOMPurify serializes as `<br>`.
fn is_break_tag(text: &str) -> bool {
    match opening_tag(text) {
        Some((name, _)) => name.eq_ignore_ascii_case("br"),
        None => false,
    }
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
