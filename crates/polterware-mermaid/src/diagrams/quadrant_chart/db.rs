//! The quadrant chart database: a port of `quadrantDb.ts` and of the parts of
//! `quadrantBuilder.ts` the database calls (`setData`, `addPoints`, `addClass`).
//!
//! `parseStyles` keeps upstream's validation and its exact error messages: a style name other
//! than `radius`, `color`, `stroke-color`, or `stroke-width` is rejected, and so is a value that
//! does not match the key's pattern. `InvalidStyleError` becomes a [`ParseError`] at the style's
//! position instead of a JavaScript exception.
//!
//! The parser hands the database raw text, so the trimming and sanitizing of
//! `textSanitizer` (`text.trim()` then `sanitizeText`) happen here, as do `commonDb.ts`'s
//! `setDiagramTitle`, `setAccTitle`, and `setAccDescription`. `addPoints` prepends and
//! `addClass` replaces in place, as the builder does; [`super::ast::Ast`] documents both.

use super::ast::{Ast, ClassStyle, Point, Styles};
use super::parser::{ParsedClass, ParsedDiagram, ParsedPoint};
use super::sanitize;
use crate::error::{ParseError, Span};
use crate::preprocess::is_js_whitespace;

/// One `style` from `stylesOpt`, with the position a validation error points at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StyleText {
    pub(crate) text: String,
    pub(crate) span: Span,
}

impl StyleText {
    /// A style without a position, for the database tests that call [`parse_styles`] directly.
    #[cfg(test)]
    pub(crate) fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            span: Span::default(),
        }
    }
}

/// The two database operations a `points` or `classDefStatement` action performs and that can
/// fail, as upstream's `yy` receives them.
///
/// The parser calls them as it parses, so a style error on an earlier line is reported before a
/// parse error on a later one, exactly as upstream's actions do. A parser test that wants the
/// grammar without the database supplies its own recorder.
pub(crate) trait Sink {
    fn add_point(&mut self, point: ParsedPoint) -> Result<(), ParseError>;
    fn add_class(&mut self, class: ParsedClass) -> Result<(), ParseError>;
}

/// The quadrant chart database as the parser populates it.
#[derive(Default)]
pub(crate) struct Db {
    points: Vec<Point>,
    classes: Vec<ClassStyle>,
}

impl Sink for Db {
    /// `addPoint`: validate the styles, then prepend the point (`addPoints`).
    fn add_point(&mut self, point: ParsedPoint) -> Result<(), ParseError> {
        let styles = parse_styles(&point.styles)?;
        self.points.insert(
            0,
            Point {
                x: point.x,
                y: point.y,
                text: sanitize::sanitize_text(trim_js(&point.text)),
                class_name: point.class_name,
                styles,
            },
        );
        Ok(())
    }

    /// `addClass`: validate the styles, then write to the map (`Map.set` keeps the position).
    fn add_class(&mut self, class: ParsedClass) -> Result<(), ParseError> {
        let styles = parse_styles(&class.styles)?;
        if let Some(existing) = self
            .classes
            .iter_mut()
            .find(|existing| existing.name == class.name)
        {
            existing.styles = styles;
        } else {
            self.classes.push(ClassStyle {
                name: class.name,
                styles,
            });
        }
        Ok(())
    }
}

impl Db {
    /// Applies the single-valued setters the grammar made and answers the final [`Ast`].
    pub(crate) fn build(self, parsed: ParsedDiagram) -> Ast {
        let mut ast = Ast {
            points: self.points,
            classes: self.classes,
            ..Ast::default()
        };
        if let Some(title) = parsed.title {
            ast.diagram_title = sanitize::sanitize_text(&title);
        }
        if let Some(acc_title) = parsed.acc_title {
            ast.acc_title = sanitize::set_acc_title(&acc_title);
        }
        if let Some(acc_description) = parsed.acc_description {
            ast.acc_description = sanitize::set_acc_description(&acc_description);
        }
        ast.quadrant1_text = text(&parsed.quadrant_texts[0]);
        ast.quadrant2_text = text(&parsed.quadrant_texts[1]);
        ast.quadrant3_text = text(&parsed.quadrant_texts[2]);
        ast.quadrant4_text = text(&parsed.quadrant_texts[3]);
        ast.x_axis_left_text = text(&parsed.x_axis_left);
        ast.x_axis_right_text = text(&parsed.x_axis_right);
        ast.y_axis_bottom_text = text(&parsed.y_axis_bottom);
        ast.y_axis_top_text = text(&parsed.y_axis_top);
        ast
    }
}

/// `textSanitizer`: `sanitizeText(text.trim(), getConfig())`.
fn text(raw: &Option<String>) -> String {
    raw.as_deref()
        .map_or_else(String::new, |text| sanitize::sanitize_text(trim_js(text)))
}

/// `parseStyles`: the four supported style keys, with upstream's error messages.
pub(crate) fn parse_styles(styles: &[StyleText]) -> Result<Styles, ParseError> {
    let mut styles_object = Styles::default();
    for style in styles {
        let (key, value) = split_style(&style.text);
        match key.as_str() {
            "radius" => {
                let value = value.as_deref().unwrap_or("undefined");
                if !is_number(value) {
                    return Err(invalid_style(&style.span, "radius", value, "number"));
                }
                let radius = value
                    .parse::<f64>()
                    .map_err(|_| invalid_style(&style.span, "radius", value, "number"))?;
                styles_object.radius = Some(radius);
            }
            "color" => {
                let value = value.as_deref().unwrap_or("undefined");
                if !is_hex_code(value) {
                    return Err(invalid_style(&style.span, "color", value, "hex code"));
                }
                styles_object.color = Some(value.to_string());
            }
            "stroke-color" => {
                let value = value.as_deref().unwrap_or("undefined");
                if !is_hex_code(value) {
                    return Err(invalid_style(
                        &style.span,
                        "stroke-color",
                        value,
                        "hex code",
                    ));
                }
                styles_object.stroke_color = Some(value.to_string());
            }
            "stroke-width" => {
                let value = value.as_deref().unwrap_or("undefined");
                if !is_size_in_pixels(value) {
                    return Err(invalid_style(
                        &style.span,
                        "stroke-width",
                        value,
                        "number of pixels (eg. 10px)",
                    ));
                }
                styles_object.stroke_width = Some(value.to_string());
            }
            _ => {
                return Err(ParseError::at(
                    format!("style named {key} is not supported."),
                    style.span,
                ));
            }
        }
    }
    Ok(styles_object)
}

/// `style.trim().split(/\s*:\s*/)`, of which only the first two parts are read.
///
/// Each separator is a `:` with the whitespace characters around it; the split is on every
/// occurrence, so `radius: 10:20` has the value `10`, not `10:20`. A style without a `:` has no
/// value, as the destructuring of a one-element array leaves `value` undefined.
fn split_style(style: &str) -> (String, Option<String>) {
    let style = trim_js(style);
    let Some((start, end)) = separator(style, 0) else {
        return (style.to_string(), None);
    };
    let value_end = separator(style, end).map_or(style.len(), |(next, _)| next);
    (
        style[..start].to_string(),
        Some(style[end..value_end].to_string()),
    )
}

/// The first `\s*:\s*` match at or after `from`, as its byte range.
fn separator(style: &str, from: usize) -> Option<(usize, usize)> {
    let colon = from + style[from..].find(':')?;
    let mut start = colon;
    while start > from {
        let Some(ch) = style[..start].chars().next_back() else {
            break;
        };
        if !is_js_whitespace(ch) {
            break;
        }
        start -= ch.len_utf8();
    }
    let mut end = colon + 1;
    while let Some(ch) = style[end..].chars().next() {
        if !is_js_whitespace(ch) {
            break;
        }
        end += ch.len_utf8();
    }
    Some((start, end))
}

fn invalid_style(span: &Span, style: &str, value: &str, kind: &str) -> ParseError {
    ParseError::at(
        format!("value for {style} {value} is invalid, please use a valid {kind}"),
        *span,
    )
}

/// `validateNumber`: `^\d+$`.
fn is_number(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

/// `validateHexCode`: `^#?([\dA-Fa-f]{6}|[\dA-Fa-f]{3})$`.
fn is_hex_code(value: &str) -> bool {
    let digits = value.strip_prefix('#').unwrap_or(value);
    (digits.len() == 3 || digits.len() == 6) && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// `validateSizeInPixels`: `^\d+px$`.
fn is_size_in_pixels(value: &str) -> bool {
    value.strip_suffix("px").is_some_and(is_number)
}

/// `String.prototype.trim`. `char::is_whitespace` differs from JavaScript's `\s` on U+FEFF, so
/// the JavaScript set is used.
pub(crate) fn trim_js(text: &str) -> &str {
    text.trim_matches(is_js_whitespace)
}
