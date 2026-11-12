//! Port of upstream `shapes/bpmnIcons.ts` and the BPMN glyph half of `getIconSVG`.
//!
//! `bpmnIcons` is an Iconify pack: every icon is a body of SVG `<path>`/`<circle>` elements in a
//! 24 × 24 viewBox, drawn at `size` by the viewport scale. This module keeps the bodies verbatim
//! (so the raw pack is available through [`icon_svg`]) and turns them into scene items
//! ([`glyph_group`]) that a painter can draw without parsing SVG.
//!
//! The caller owns placement: upstream `appendGlyph` wraps the glyph in
//! `<g class="bpmn-glyph" transform="translate(-size/2, -size/2)">` around the sized viewport,
//! while an activity's corner icon and its marker row place the same glyph uncentred. A caller
//! therefore sets the returned group's transform to `scale(size / 24)` followed by its own
//! translation; [`size_glyph`] does the centred case.

use std::collections::HashMap;

use super::{Node, RenderedNode, ShapeContext, ShapeError};
use crate::scene::{
    Color, EllipseShape, Group, LineCap, LineJoin, Path, PathShape, PathStyle, Point,
};

/// The icon pack's prefix, upstream `bpmnIcons.prefix`.
pub const PREFIX: &str = "bpmn";

/// The icon pack's native viewBox, upstream `width`/`height`.
pub const GLYPH_VIEWBOX: f64 = 24.0;

/// Every icon of upstream `bpmnIcons.ts`, in source order.
pub const ICONS: &[(&str, &str)] = &[
    (
        "message",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.6" d="M3 6h18v12H3z"/><path class="bpmn-glyph-fold" fill="none" stroke="currentColor" stroke-width="1.6" d="m3 6 9 7 9-7"/>"#,
    ),
    (
        "timer",
        r#"<circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="1.6"/><path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" d="M12 7v5l3.5 2.5"/><path stroke="currentColor" stroke-width="1.2" d="M12 3v1.5M21 12h-1.5M12 21v-1.5M3 12h1.5"/>"#,
    ),
    (
        "error",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" d="M4 21 9.5 6 13 12.5 20 4 14.5 19 11 12.5Z"/>"#,
    ),
    (
        "escalation",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" d="m12 3 8 18-8-7-8 7z"/>"#,
    ),
    (
        "signal",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" d="M12 4 22 20H2z"/>"#,
    ),
    (
        "conditional",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.6" d="M4 4h16v16H4z"/><path stroke="currentColor" stroke-width="1.6" stroke-linecap="round" d="M7 9h10M7 12.5h10M7 16h10"/>"#,
    ),
    (
        "link",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" d="M3 9h10V5l8 7-8 7v-4H3z"/>"#,
    ),
    (
        "compensation",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" d="M11 6 3 12l8 6zM21 6l-8 6 8 6z"/>"#,
    ),
    (
        "cancel",
        r#"<path stroke="currentColor" stroke-width="2.2" stroke-linecap="round" d="m5 5 14 14M19 5 5 19"/>"#,
    ),
    (
        "terminate",
        r#"<circle cx="12" cy="12" r="9" fill="currentColor"/>"#,
    ),
    (
        "multiple",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round" d="m12 3 8.5 6.2-3.2 10H6.7l-3.2-10z"/>"#,
    ),
    (
        "user",
        r#"<circle cx="12" cy="8" r="4" fill="none" stroke="currentColor" stroke-width="1.7"/><path fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" d="M4.5 21a7.5 7.5 0 0 1 15 0"/>"#,
    ),
    (
        "service",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" d="m20.2 14.3-2 .6a6.6 6.6 0 0 1-1 1.7l.6 2-2.4 1.4-1.5-1.5a6.6 6.6 0 0 1-2 0l-1.5 1.5-2.4-1.4.6-2a6.6 6.6 0 0 1-1-1.7l-2-.6v-2.8l2-.6a6.6 6.6 0 0 1 1-1.7l-.6-2 2.4-1.4 1.5 1.5a6.6 6.6 0 0 1 2 0l1.5-1.5 2.4 1.4-.6 2a6.6 6.6 0 0 1 1 1.7l2 .6z"/><circle cx="12" cy="12.9" r="2.6" fill="none" stroke="currentColor" stroke-width="1.5"/>"#,
    ),
    (
        "script",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" d="M8 3h9a2.5 2.5 0 0 0-2.5 2.5v13A2.5 2.5 0 0 1 12 21H3a2.5 2.5 0 0 0 2.5-2.5v-13A2.5 2.5 0 0 1 8 3Z"/><path stroke="currentColor" stroke-width="1.4" stroke-linecap="round" d="M6.5 8h6M6.5 11.5h6M6.5 15h4"/>"#,
    ),
    (
        "manual",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" d="M8 11V5.5a1.8 1.8 0 0 1 3.5 0V10m0-.5a1.8 1.8 0 0 1 3.5 0V11m0-.5a1.8 1.8 0 0 1 3.5 0V16a5 5 0 0 1-5 5h-2a5 5 0 0 1-5-5v-4.5a1.8 1.8 0 0 1 1.5-1.8z"/>"#,
    ),
    (
        "receive",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.6" d="M3 6h18v12H3z"/><path fill="none" stroke="currentColor" stroke-width="1.6" d="m3 6 9 7 9-7"/>"#,
    ),
    (
        "send",
        r#"<path fill="currentColor" d="M3 6h18v12H3z" opacity=".18"/><path fill="none" stroke="currentColor" stroke-width="1.6" d="M3 6h18v12H3z"/><path fill="none" stroke="currentColor" stroke-width="1.6" d="m3 6 9 7 9-7"/>"#,
    ),
    (
        "rule",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.6" d="M3 5h18v14H3z"/><path stroke="currentColor" stroke-width="1.4" d="M3 9h18M3 13h18M8 9v10"/>"#,
    ),
    (
        "loop",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" d="M20 12a8 8 0 1 1-2.7-6"/><path fill="currentColor" d="M20 3.2v5.4h-5.4z"/>"#,
    ),
    (
        "parallel",
        r#"<path stroke="currentColor" stroke-width="2.4" stroke-linecap="round" d="M6 4v16M12 4v16M18 4v16"/>"#,
    ),
    (
        "sequential",
        r#"<path stroke="currentColor" stroke-width="2.4" stroke-linecap="round" d="M4 6h16M4 12h16M4 18h16"/>"#,
    ),
    (
        "ad-hoc",
        r#"<path fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" d="M3 15c2.5-6 6-6 8.5 0S18 21 21 15"/>"#,
    ),
    (
        "subprocess",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.6" d="M3 3h18v18H3z"/><path stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M12 7v10M7 12h10"/>"#,
    ),
    (
        "exclusive",
        r#"<path stroke="currentColor" stroke-width="2.6" stroke-linecap="round" d="m6 6 12 12M18 6 6 18"/>"#,
    ),
    (
        "parallel-multiple",
        r#"<path fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" d="M12 5v14M5 12h14"/>"#,
    ),
    (
        "inclusive",
        r#"<circle cx="12" cy="12" r="7.5" fill="none" stroke="currentColor" stroke-width="2.4"/>"#,
    ),
    (
        "parallel-gateway",
        r#"<path stroke="currentColor" stroke-width="2.6" stroke-linecap="round" d="M12 4v16M4 12h16"/>"#,
    ),
    (
        "event-based",
        r#"<circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="1.3"/><circle cx="12" cy="12" r="6.8" fill="none" stroke="currentColor" stroke-width="1.3"/><path fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" d="m12 7.4 4 2.9-1.5 4.7h-5L8 10.3z"/>"#,
    ),
    (
        "complex",
        r#"<path stroke="currentColor" stroke-width="2.2" stroke-linecap="round" d="M12 3v18M3 12h18M5.6 5.6l12.8 12.8M18.4 5.6 5.6 18.4"/>"#,
    ),
];

/// The raw SVG body of one icon pack entry.
pub fn icon_body(name: &str) -> Option<&'static str> {
    let key = name.strip_prefix("bpmn:").unwrap_or(name);
    ICONS
        .iter()
        .find(|(icon, _)| *icon == key)
        .map(|(_, body)| *body)
}

/// The raw SVG body of one icon, the pack's `getIconSVG` data half.
pub fn icon_svg(name: &str) -> Option<&'static str> {
    icon_body(name)
}

/// A glyph as scene items in its 24 × 24 viewBox, wrapped in `<g class="bpmn-glyph">`.
///
/// The group has no transform: scale it by `size / 24` and translate it where the diagram
/// needs it, or call [`size_glyph`] for the centred placement `appendGlyph` uses.
pub fn glyph_group(name: &str) -> Option<Group> {
    let body = icon_body(name)?;
    let mut glyph = Group::new().with_class("bpmn-glyph");
    for element in elements(body) {
        let attrs = attributes(&element);
        let mut item = Group::new();
        if let Some(class) = attrs.get("class") {
            item = item.with_class(*class);
        }
        match element.tag {
            "path" => {
                if let Some(d) = attrs.get("d") {
                    let path = parse_path_data(d);
                    if !path.is_empty() {
                        item.push(PathShape::new(path, style_of(&attrs)));
                    }
                }
            }
            "circle" => {
                let cx = number(&attrs, "cx");
                let cy = number(&attrs, "cy");
                let r = number(&attrs, "r");
                item.push(EllipseShape::new(
                    Point::new(cx, cy),
                    r,
                    r,
                    style_of(&attrs),
                ));
            }
            _ => {}
        }
        if !item.items.is_empty() {
            glyph.push(item);
        }
    }
    Some(glyph)
}

/// `appendGlyph`'s placement: scale the 24-unit glyph to `size` and centre it on the origin.
pub fn size_glyph(name: &str, size: f64) -> Option<Group> {
    let mut glyph = glyph_group(name)?;
    glyph.transform = crate::scene::Transform::scale(size / GLYPH_VIEWBOX, size / GLYPH_VIEWBOX)
        .then(crate::scene::Transform::translate(-size / 2.0, -size / 2.0));
    Some(glyph)
}

struct RawElement<'a> {
    tag: &'a str,
    body: &'a str,
}

fn elements(body: &str) -> Vec<RawElement<'_>> {
    let mut out = Vec::new();
    let mut index = 0;
    while let Some(offset) = body[index..].find('<') {
        let start = index + offset + 1;
        let Some(end_offset) = body[start..].find('>') else {
            break;
        };
        let end = start + end_offset;
        let raw = body[start..end].trim();
        index = end + 1;
        if raw.starts_with('/') || raw.starts_with('!') || raw.starts_with('?') {
            continue;
        }
        let raw = raw.trim_end_matches('/').trim();
        let name_end = raw.find(|c: char| c.is_whitespace()).unwrap_or(raw.len());
        let tag = &raw[..name_end];
        out.push(RawElement {
            tag,
            body: &raw[name_end..],
        });
    }
    out
}

fn attributes<'a>(element: &RawElement<'a>) -> HashMap<&'a str, &'a str> {
    let raw = element.body;
    let bytes = raw.as_bytes();
    let mut attrs = HashMap::new();
    let mut index = 0;
    while index < bytes.len() {
        while index < bytes.len() && (bytes[index].is_ascii_whitespace() || bytes[index] == b'/') {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        let name_start = index;
        while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b'=' {
            index += 1;
        }
        let name = &raw[name_start..index];
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let mut value = "";
        if index < bytes.len() && bytes[index] == b'=' {
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            if index < bytes.len() && (bytes[index] == b'"' || bytes[index] == b'\'') {
                let quote = bytes[index];
                index += 1;
                let value_start = index;
                while index < bytes.len() && bytes[index] != quote {
                    index += 1;
                }
                value = &raw[value_start..index];
                if index < bytes.len() {
                    index += 1;
                }
            } else {
                let value_start = index;
                while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                    index += 1;
                }
                value = &raw[value_start..index];
            }
        }
        attrs.insert(name, value);
    }
    attrs
}

fn number(attrs: &HashMap<&str, &str>, name: &str) -> f64 {
    attrs
        .get(name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0.0)
}

fn style_of(attrs: &HashMap<&str, &str>) -> PathStyle {
    let mut style = PathStyle {
        fill: match attrs.get("fill") {
            Some(&"none") => None,
            Some(&value) => Some(Color::new(value)),
            None => Some(Color::new("#000000")),
        },
        stroke: attrs.get("stroke").map(|value| Color::new(*value)),
        stroke_width: number_opt(attrs, "stroke-width").unwrap_or(1.0),
        line_cap: match attrs.get("stroke-linecap") {
            Some(&"round") => LineCap::Round,
            Some(&"square") => LineCap::Square,
            _ => LineCap::Butt,
        },
        line_join: match attrs.get("stroke-linejoin") {
            Some(&"round") => LineJoin::Round,
            Some(&"bevel") => LineJoin::Bevel,
            _ => LineJoin::Miter,
        },
        opacity: number_opt(attrs, "opacity").unwrap_or(1.0),
        ..PathStyle::default()
    };
    if style.stroke.is_none() {
        style.stroke_width = 1.0;
    }
    style
}

fn number_opt(attrs: &HashMap<&str, &str>, name: &str) -> Option<f64> {
    attrs.get(name).and_then(|value| value.parse().ok())
}

/// Parses an SVG `d` attribute into the scene's path data. Handles the command grammar the
/// BPMN glyphs and shape bodies use: `M m L l H h V v C c S s Q q T t A a Z z`, implicit
/// repeated coordinate sets, and implicit `lineTo` after `moveTo`.
pub fn parse_path_data(data: &str) -> Path {
    let chars: Vec<char> = data.chars().collect();
    let mut index = 0;
    let mut path = Path::new();
    let mut command: Option<char> = None;
    let mut current = Point::ZERO;
    let mut subpath = Point::ZERO;
    let mut cubic_control: Option<Point> = None;
    let mut quad_control: Option<Point> = None;
    while let Some(symbol) = next_non_separator(&chars, &mut index) {
        if symbol.is_ascii_alphabetic() {
            command = Some(symbol);
            index += 1;
        } else if command.is_none() {
            break;
        }
        let Some(active) = command else { break };
        let relative = active.is_ascii_lowercase();
        match active.to_ascii_uppercase() {
            'Z' => {
                path.close();
                current = subpath;
                cubic_control = None;
                quad_control = None;
                command = None;
            }
            'M' => {
                let Some(x) = read_number(&chars, &mut index) else {
                    break;
                };
                let Some(y) = read_number(&chars, &mut index) else {
                    break;
                };
                let point = if relative {
                    Point::new(current.x + x, current.y + y)
                } else {
                    Point::new(x, y)
                };
                path.move_to(point);
                current = point;
                subpath = point;
                cubic_control = None;
                quad_control = None;
                command = Some(if relative { 'l' } else { 'L' });
            }
            'L' => {
                let Some(point) = read_point(&chars, &mut index, current, relative) else {
                    break;
                };
                path.line_to(point);
                current = point;
                cubic_control = None;
                quad_control = None;
            }
            'H' => {
                let Some(x) = read_number(&chars, &mut index) else {
                    break;
                };
                let point = Point::new(if relative { current.x + x } else { x }, current.y);
                path.line_to(point);
                current = point;
                cubic_control = None;
                quad_control = None;
            }
            'V' => {
                let Some(y) = read_number(&chars, &mut index) else {
                    break;
                };
                let point = Point::new(current.x, if relative { current.y + y } else { y });
                path.line_to(point);
                current = point;
                cubic_control = None;
                quad_control = None;
            }
            'C' => {
                let Some(c1) = read_point(&chars, &mut index, current, relative) else {
                    break;
                };
                let Some(c2) = read_point(&chars, &mut index, current, relative) else {
                    break;
                };
                let Some(point) = read_point(&chars, &mut index, current, relative) else {
                    break;
                };
                path.cubic_to(c1, c2, point);
                current = point;
                cubic_control = Some(c2);
                quad_control = None;
            }
            'S' => {
                let c1 = cubic_control
                    .map(|control| {
                        Point::new(2.0 * current.x - control.x, 2.0 * current.y - control.y)
                    })
                    .unwrap_or(current);
                let Some(c2) = read_point(&chars, &mut index, current, relative) else {
                    break;
                };
                let Some(point) = read_point(&chars, &mut index, current, relative) else {
                    break;
                };
                path.cubic_to(c1, c2, point);
                current = point;
                cubic_control = Some(c2);
                quad_control = None;
            }
            'Q' => {
                let Some(control) = read_point(&chars, &mut index, current, relative) else {
                    break;
                };
                let Some(point) = read_point(&chars, &mut index, current, relative) else {
                    break;
                };
                path.quad_to(control, point);
                current = point;
                cubic_control = None;
                quad_control = Some(control);
            }
            'T' => {
                let control = quad_control
                    .map(|previous| {
                        Point::new(2.0 * current.x - previous.x, 2.0 * current.y - previous.y)
                    })
                    .unwrap_or(current);
                let Some(point) = read_point(&chars, &mut index, current, relative) else {
                    break;
                };
                path.quad_to(control, point);
                current = point;
                cubic_control = None;
                quad_control = Some(control);
            }
            'A' => {
                let Some(rx) = read_number(&chars, &mut index) else {
                    break;
                };
                let Some(ry) = read_number(&chars, &mut index) else {
                    break;
                };
                let Some(rotation) = read_number(&chars, &mut index) else {
                    break;
                };
                let Some(large_arc) = read_flag(&chars, &mut index) else {
                    break;
                };
                let Some(sweep) = read_flag(&chars, &mut index) else {
                    break;
                };
                let Some(point) = read_point(&chars, &mut index, current, relative) else {
                    break;
                };
                path.arc_to(rx, ry, rotation, large_arc, sweep, point);
                current = point;
                cubic_control = None;
                quad_control = None;
            }
            _ => break,
        }
    }
    path
}

fn next_non_separator(chars: &[char], index: &mut usize) -> Option<char> {
    while *index < chars.len() && (chars[*index].is_whitespace() || chars[*index] == ',') {
        *index += 1;
    }
    chars.get(*index).copied()
}

fn read_number(chars: &[char], index: &mut usize) -> Option<f64> {
    while *index < chars.len() && (chars[*index].is_whitespace() || chars[*index] == ',') {
        *index += 1;
    }
    let start = *index;
    if *index < chars.len() && (chars[*index] == '-' || chars[*index] == '+') {
        *index += 1;
    }
    let mut digits = false;
    while *index < chars.len() && chars[*index].is_ascii_digit() {
        *index += 1;
        digits = true;
    }
    if *index < chars.len() && chars[*index] == '.' {
        *index += 1;
        while *index < chars.len() && chars[*index].is_ascii_digit() {
            *index += 1;
            digits = true;
        }
    }
    if digits && *index < chars.len() && (chars[*index] == 'e' || chars[*index] == 'E') {
        let exponent = *index;
        *index += 1;
        if *index < chars.len() && (chars[*index] == '-' || chars[*index] == '+') {
            *index += 1;
        }
        let mut exponent_digits = false;
        while *index < chars.len() && chars[*index].is_ascii_digit() {
            *index += 1;
            exponent_digits = true;
        }
        if !exponent_digits {
            *index = exponent;
        }
    }
    if !digits {
        *index = start;
        return None;
    }
    chars[start..*index].iter().collect::<String>().parse().ok()
}

fn read_flag(chars: &[char], index: &mut usize) -> Option<bool> {
    let value = read_number(chars, index)?;
    Some(value != 0.0)
}

fn read_point(chars: &[char], index: &mut usize, current: Point, relative: bool) -> Option<Point> {
    let x = read_number(chars, index)?;
    let y = read_number(chars, index)?;
    Some(if relative {
        Point::new(current.x + x, current.y + y)
    } else {
        Point::new(x, y)
    })
}

// Compatibility entry for the current registry; this upstream module is a helper, not a shape.
pub fn draw(node: &Node, _ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    Err(ShapeError {
        message: format!("{} is a rendering helper, not a shape", node.shape),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_upstream_icon_is_present_and_renders_items() {
        assert_eq!(ICONS.len(), 29);
        for (name, _) in ICONS {
            let glyph = glyph_group(name).unwrap_or_else(|| panic!("{name} parses"));
            assert!(!glyph.items.is_empty(), "{name} has no scene items");
        }
    }

    #[test]
    fn icon_names_accept_the_bpmn_prefix() {
        assert_eq!(icon_body("message"), icon_body("bpmn:message"));
        assert!(icon_body("missing").is_none());
    }

    #[test]
    fn paths_cover_relative_commands_and_arcs() {
        let path = parse_path_data("M-25,-19 a25,6 0 0 1 50,0 V19 a25,6 0 0 1 -50,0 Z");
        assert_eq!(
            path.commands,
            vec![
                crate::scene::PathCommand::move_to(Point::new(-25.0, -19.0)),
                crate::scene::PathCommand::arc_to(
                    25.0,
                    6.0,
                    0.0,
                    false,
                    true,
                    Point::new(25.0, -19.0)
                ),
                crate::scene::PathCommand::line_to(Point::new(25.0, 19.0)),
                crate::scene::PathCommand::arc_to(
                    25.0,
                    6.0,
                    0.0,
                    false,
                    true,
                    Point::new(-25.0, 19.0)
                ),
                crate::scene::PathCommand::Close,
            ]
        );
    }

    #[test]
    fn smooth_cubics_reflect_the_previous_control_point() {
        let path = parse_path_data("M0 0 C1 1 2 2 3 3 S5 5 6 6");
        assert_eq!(
            path.commands,
            vec![
                crate::scene::PathCommand::move_to(Point::ZERO),
                crate::scene::PathCommand::cubic_to(
                    Point::new(1.0, 1.0),
                    Point::new(2.0, 2.0),
                    Point::new(3.0, 3.0)
                ),
                crate::scene::PathCommand::cubic_to(
                    Point::new(4.0, 4.0),
                    Point::new(5.0, 5.0),
                    Point::new(6.0, 6.0)
                ),
            ]
        );
    }

    #[test]
    fn glyphs_keep_their_stroke_style() {
        use crate::scene::Item;
        let glyph = glyph_group("timer").expect("timer glyph");
        let first = glyph
            .items
            .iter()
            .find_map(|item| match item {
                Item::Group(group) => Some(group),
                _ => None,
            })
            .expect("timer has a circle group");
        assert_eq!(first.class.as_deref(), None);
        assert!(matches!(
            first.items.first(),
            Some(Item::Ellipse(ellipse)) if ellipse.radius_x == 9.0
        ));
    }
}
