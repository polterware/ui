//! What upstream keeps in `diagrams/common/` and `config.ts` and that more than one diagram
//! needs: the plain-text sanitizer, the line-break helpers, the YAML reader, and the text
//! helpers the generated lexers share.
//!
//! Porting these once keeps every diagram's database agreeing with the others, and keeps a
//! fix to one of upstream's helpers a single edit here.

pub mod sanitize;
pub mod text;
pub mod yaml;

/// True for the characters JavaScript's `\s` matches, which every upstream token pattern
/// uses. `char::is_whitespace` differs on `\u{85}` and the BOM, so the set is spelled out.
///
/// [`crate::preprocess`] re-exports it, because that is where the parsers used to import it.
pub fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'..='\u{000d}'
            | '\u{0020}'
            | '\u{00a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    )
}

/// `String.prototype.trim`: like [`str::trim`], but with JavaScript's set, which also strips
/// a byte-order mark and does not strip `\u{85}`.
pub fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_whitespace)
}
