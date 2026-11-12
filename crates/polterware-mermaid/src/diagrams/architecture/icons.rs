//! The architecture icon pack, port of `diagrams/architecture/architectureIcons.ts` and the
//! fallback icon of `rendering-util/icons.ts`.
//!
//! `architectureIcons` is an Iconify pack in an 80 × 80 viewBox: every body is an SVG fragment
//! whose `<g>` is a solid blue tile (`#087ebf`) with white strokes, except `unknown`, whose text
//! glyph is the placeholder for an icon name the pack cannot resolve. The bodies are kept
//! verbatim, and [`icon_group`] turns one into scene items in the 80-unit coordinate system.
//! The caller scales the group to the size `getIconSVG` gives the nested `<svg>`
//! (`iconSize` for a service, `padding * 0.75` for a group).
//!
//! Not ported: Iconify's `iconToHTML` wrapper (a nested `<svg>` with the scaled viewport), which
//! becomes a [`Group`] transform, and `replaceIDs`, which only rewrites the ids inside the
//! markup. Both are invisible.

use std::collections::HashMap;

use crate::render::shapes::bpmn_icons::parse_path_data;
use crate::scene::{
    Color, EllipseShape, Group, Item, Path, PathShape, PathStyle, Point, Rect, RectShape, Size,
    TextBlock, TextLine, TextRun, TextSpan, Transform,
};

/// The viewBox side every architecture icon is drawn in, `architectureIcons.width`/`height`.
pub const ICON_VIEWBOX: f64 = 80.0;

/// The pack's prefix, `architectureIcons.prefix`.
pub const PREFIX: &str = "mermaid-architecture";

/// `unknownIcon` of `rendering-util/icons.ts`, the fallback for a name the pack cannot resolve.
pub const UNKNOWN: &str = r#"<g><rect width="80" height="80" style="fill: #087ebf; stroke-width: 0px;"/><text transform="translate(21.16 64.67)" style="fill: #fff; font-family: ArialMT, Arial; font-size: 67.75px;"><tspan x="0" y="0">?</tspan></text></g>"#;

/// Every icon of `architectureIcons.icons`, in source order, as `wrapIcon(body)`.
pub const ICONS: &[(&str, &str)] = &[
    (
        "database",
        r#"<g><rect width="80" height="80" style="fill: #087ebf; stroke-width: 0px;"/><path id="b" data-name="4" d="m20,57.86c0,3.94,8.95,7.14,20,7.14s20-3.2,20-7.14" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><path id="c" data-name="3" d="m20,45.95c0,3.94,8.95,7.14,20,7.14s20-3.2,20-7.14" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><path id="d" data-name="2" d="m20,34.05c0,3.94,8.95,7.14,20,7.14s20-3.2,20-7.14" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><ellipse id="e" data-name="1" cx="40" cy="22.14" rx="20" ry="7.14" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><line x1="20" y1="57.86" x2="20" y2="22.14" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><line x1="60" y1="57.86" x2="60" y2="22.14" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/></g>"#,
    ),
    (
        "server",
        r#"<g><rect width="80" height="80" style="fill: #087ebf; stroke-width: 0px;"/><rect x="17.5" y="17.5" width="45" height="45" rx="2" ry="2" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><line x1="17.5" y1="32.5" x2="62.5" y2="32.5" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><line x1="17.5" y1="47.5" x2="62.5" y2="47.5" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><g><path d="m56.25,25c0,.27-.45.5-1,.5h-10.5c-.55,0-1-.23-1-.5s.45-.5,1-.5h10.5c.55,0,1,.23,1,.5Z" style="fill: #fff; stroke-width: 0px;"/><path d="m56.25,25c0,.27-.45.5-1,.5h-10.5c-.55,0-1-.23-1-.5s.45-.5,1-.5h10.5c.55,0,1,.23,1,.5Z" style="fill: none; stroke: #fff; stroke-miterlimit: 10;"/></g><g><path d="m56.25,40c0,.27-.45.5-1,.5h-10.5c-.55,0-1-.23-1-.5s.45-.5,1-.5h10.5c.55,0,1,.23,1,.5Z" style="fill: #fff; stroke-width: 0px;"/><path d="m56.25,40c0,.27-.45.5-1,.5h-10.5c-.55,0-1-.23-1-.5s.45-.5,1-.5h10.5c.55,0,1,.23,1,.5Z" style="fill: none; stroke: #fff; stroke-miterlimit: 10;"/></g><g><path d="m56.25,55c0,.27-.45.5-1,.5h-10.5c-.55,0-1-.23-1-.5s.45-.5,1-.5h10.5c.55,0,1,.23,1,.5Z" style="fill: #fff; stroke-width: 0px;"/><path d="m56.25,55c0,.27-.45.5-1,.5h-10.5c-.55,0-1-.23-1-.5s.45-.5,1-.5h10.5c.55,0,1,.23,1,.5Z" style="fill: none; stroke: #fff; stroke-miterlimit: 10;"/></g><g><circle cx="32.5" cy="25" r=".75" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10;"/><circle cx="27.5" cy="25" r=".75" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10;"/><circle cx="22.5" cy="25" r=".75" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10;"/></g><g><circle cx="32.5" cy="40" r=".75" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10;"/><circle cx="27.5" cy="40" r=".75" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10;"/><circle cx="22.5" cy="40" r=".75" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10;"/></g><g><circle cx="32.5" cy="55" r=".75" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10;"/><circle cx="27.5" cy="55" r=".75" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10;"/><circle cx="22.5" cy="55" r=".75" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10;"/></g></g>"#,
    ),
    (
        "disk",
        r#"<g><rect width="80" height="80" style="fill: #087ebf; stroke-width: 0px;"/><rect x="20" y="15" width="40" height="50" rx="1" ry="1" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><ellipse cx="24" cy="19.17" rx=".8" ry=".83" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><ellipse cx="56" cy="19.17" rx=".8" ry=".83" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><ellipse cx="24" cy="60.83" rx=".8" ry=".83" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><ellipse cx="56" cy="60.83" rx=".8" ry=".83" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><ellipse cx="40" cy="33.75" rx="14" ry="14.58" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><ellipse cx="40" cy="33.75" rx="4" ry="4.17" style="fill: #fff; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><path d="m37.51,42.52l-4.83,13.22c-.26.71-1.1,1.02-1.76.64l-4.18-2.42c-.66-.38-.81-1.26-.33-1.84l9.01-10.8c.88-1.05,2.56-.08,2.09,1.2Z" style="fill: #fff; stroke-width: 0px;"/></g>"#,
    ),
    (
        "internet",
        r#"<g><rect width="80" height="80" style="fill: #087ebf; stroke-width: 0px;"/><circle cx="40" cy="40" r="22.5" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><line x1="40" y1="17.5" x2="40" y2="62.5" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><line x1="17.5" y1="40" x2="62.5" y2="40" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><path d="m39.99,17.51c-15.28,11.1-15.28,33.88,0,44.98" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><path d="m40.01,17.51c15.28,11.1,15.28,33.88,0,44.98" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><line x1="19.75" y1="30.1" x2="60.25" y2="30.1" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/><line x1="19.75" y1="49.9" x2="60.25" y2="49.9" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/></g>"#,
    ),
    (
        "cloud",
        r#"<g><rect width="80" height="80" style="fill: #087ebf; stroke-width: 0px;"/><path d="m65,47.5c0,2.76-2.24,5-5,5H20c-2.76,0-5-2.24-5-5,0-1.87,1.03-3.51,2.56-4.36-.04-.21-.06-.42-.06-.64,0-2.6,2.48-4.74,5.65-4.97,1.65-4.51,6.34-7.76,11.85-7.76.86,0,1.69.08,2.5.23,2.09-1.57,4.69-2.5,7.5-2.5,6.1,0,11.19,4.38,12.28,10.17,2.14.56,3.72,2.51,3.72,4.83,0,.03,0,.07-.01.1,2.29.46,4.01,2.48,4.01,4.9Z" style="fill: none; stroke: #fff; stroke-miterlimit: 10; stroke-width: 2px;"/></g>"#,
    ),
    (
        "blank",
        r#"<g><rect width="80" height="80" style="fill: #087ebf; stroke-width: 0px;"/></g>"#,
    ),
];

/// `getRegisteredIconData`'s resolution: a bare name takes the pack prefix, and a name with
/// another prefix is not in the pack.
fn resolve(name: &str) -> Option<&'static str> {
    if name == "unknown" {
        return Some(UNKNOWN);
    }
    let (prefix, icon) = match name.split_once(':') {
        Some((prefix, icon)) => (Some(prefix), icon),
        None => (None, name),
    };
    if prefix.is_some_and(|prefix| prefix != PREFIX) {
        return None;
    }
    ICONS
        .iter()
        .find(|(candidate, _)| *candidate == icon)
        .map(|(_, body)| *body)
}

/// The body `getIconSVG` draws for `name`, with `unknownIcon` for a name the pack cannot
/// resolve, exactly as `getRegisteredIconData`'s catch falls back.
pub fn icon_body(name: &str) -> &'static str {
    resolve(name).unwrap_or(UNKNOWN)
}

/// An icon as scene items in the 80-unit viewBox, scaled to `width` × `height`.
///
/// `getIconSVG` returns a nested `<svg width height viewBox="0 0 80 80">`; the same scale is a
/// [`Transform`] on this group.
pub fn icon_group(name: &str, width: f64, height: f64) -> Group {
    let mut group = Group::new()
        .with_class("architecture-icon")
        .with_transform(Transform::scale(
            width / ICON_VIEWBOX,
            height / ICON_VIEWBOX,
        ));
    for element in elements(icon_body(name)) {
        let attrs = attributes(element.attrs);
        if element.tag == "text" {
            if let Some(run) = text_run(&attrs, &element.text) {
                group.push(run);
            }
            continue;
        }
        let style = style_of(&attrs);
        match element.tag {
            "path" => {
                if let Some(d) = attrs.get("d") {
                    let path = parse_path_data(d);
                    if !path.is_empty() {
                        group.push(PathShape::new(path, style));
                    }
                }
            }
            "rect" => {
                let rect = Rect::new(
                    number(&attrs, "x"),
                    number(&attrs, "y"),
                    number(&attrs, "width"),
                    number(&attrs, "height"),
                );
                group.push(RectShape::new(rect, number(&attrs, "rx"), style));
            }
            "circle" => {
                let r = number(&attrs, "r");
                group.push(EllipseShape::new(
                    Point::new(number(&attrs, "cx"), number(&attrs, "cy")),
                    r,
                    r,
                    style,
                ));
            }
            "ellipse" => {
                group.push(EllipseShape::new(
                    Point::new(number(&attrs, "cx"), number(&attrs, "cy")),
                    number(&attrs, "rx"),
                    number(&attrs, "ry"),
                    style,
                ));
            }
            "line" => {
                let mut path = Path::new();
                path.move_to(Point::new(number(&attrs, "x1"), number(&attrs, "y1")))
                    .line_to(Point::new(number(&attrs, "x2"), number(&attrs, "y2")));
                group.push(PathShape::new(path, style));
            }
            _ => {}
        }
    }
    group
}

/// A raw element of an icon body: the tag, its attribute text, and (for `<text>`) its content.
struct Element<'a> {
    tag: &'a str,
    attrs: &'a str,
    text: String,
}

/// Splits an icon body into its drawable elements, flattening `<g>` and reading `<text>`
/// content as one element.
fn elements(body: &str) -> Vec<Element<'_>> {
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
        let attrs = &raw[name_end..];
        if tag == "text" {
            let close = body[index..].find("</text>").map(|offset| index + offset);
            let inner_end = close.unwrap_or(body.len());
            let inner = &body[index..inner_end];
            out.push(Element {
                tag,
                attrs,
                text: strip_tags(inner),
            });
            if let Some(close) = close {
                index = close + "</text>".len();
            }
            continue;
        }
        out.push(Element {
            tag,
            attrs,
            text: String::new(),
        });
    }
    out
}

fn strip_tags(inner: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in inner.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn attributes(raw: &str) -> HashMap<&str, &str> {
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

/// One declaration of a `style` attribute: `name: value;` without the trailing `;`.
fn style_declaration<'a>(style: &'a str, name: &str) -> Option<&'a str> {
    style.split(';').find_map(|declaration| {
        let (key, value) = declaration.split_once(':')?;
        (key.trim() == name).then(|| value.trim())
    })
}

fn paint(attrs: &HashMap<&str, &str>, key: &str) -> Option<String> {
    let style = attrs.get("style").copied().unwrap_or_default();
    let value = attrs
        .get(key)
        .copied()
        .or_else(|| style_declaration(style, key))?;
    (value != "none").then(|| value.to_string())
}

fn length(value: Option<&str>) -> Option<f64> {
    value.and_then(|value| value.trim_end_matches("px").parse::<f64>().ok())
}

fn style_of(attrs: &HashMap<&str, &str>) -> PathStyle {
    let fill = paint(attrs, "fill").map(Color::new);
    let stroke = paint(attrs, "stroke").map(Color::new);
    let stroke_width = length(attrs.get("stroke-width").copied().or_else(|| {
        attrs
            .get("style")
            .and_then(|style| style_declaration(style, "stroke-width"))
    }))
    .unwrap_or(1.0);
    PathStyle {
        fill,
        stroke,
        stroke_width,
        ..PathStyle::default()
    }
}

fn text_run(attrs: &HashMap<&str, &str>, text: &str) -> Option<Item> {
    if text.trim().is_empty() {
        return None;
    }
    let style = attrs.get("style").copied().unwrap_or_default();
    let font_size = attrs
        .get("font-size")
        .and_then(|value| length(Some(value)))
        .or_else(|| style_declaration(style, "font-size").and_then(|value| length(Some(value))))
        .unwrap_or(16.0);
    let family = style_declaration(style, "font-family")
        .or_else(|| attrs.get("font-family").copied())
        .unwrap_or("Arial")
        .split(',')
        .next()
        .unwrap_or("Arial")
        .trim()
        .to_string();
    let fill = paint(attrs, "fill").unwrap_or_else(|| "#000000".to_string());
    let (mut x, mut y) = (0.0, 0.0);
    if let Some(inner) = attrs
        .get("transform")
        .and_then(|transform| transform.strip_prefix("translate("))
        .and_then(|rest| rest.strip_suffix(')'))
    {
        let mut parts = inner.split([' ', ',']).filter(|part| !part.is_empty());
        x = parts
            .next()
            .and_then(|part| part.parse().ok())
            .unwrap_or(0.0);
        y = parts
            .next()
            .and_then(|part| part.parse().ok())
            .unwrap_or(0.0);
    }
    let font = crate::measure::Font::new(family, font_size);
    let width = text.chars().count() as f64 * 0.6 * font_size;
    let block = TextBlock::new(
        vec![TextLine::new(vec![TextSpan::plain(text)])],
        Size::new(width, font_size * 1.15),
        font_size * 1.1,
    );
    Some(Item::Text(
        TextRun::new(Point::new(x, y), font, block).with_fill(Color::new(fill)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_body_resolves_and_parses() {
        for (name, _) in ICONS {
            let group = icon_group(name, 80.0, 80.0);
            assert!(!group.items.is_empty(), "{name} has no items");
        }
        assert!(!icon_group("unknown", 80.0, 80.0).items.is_empty());
    }

    #[test]
    fn an_unknown_name_falls_back_to_the_placeholder() {
        assert_eq!(icon_body("not-an-icon"), UNKNOWN);
        assert_eq!(icon_body("logos:aws"), UNKNOWN);
        assert_eq!(
            icon_body("mermaid-architecture:server"),
            resolve("server").unwrap()
        );
    }

    #[test]
    fn the_blue_tile_is_the_first_item() {
        let group = icon_group("database", 40.0, 40.0);
        match &group.items[0] {
            Item::Rect(rect) => {
                assert_eq!(rect.rect, Rect::new(0.0, 0.0, 80.0, 80.0));
                assert_eq!(rect.style.fill, Some(Color::new("#087ebf")));
            }
            other => panic!("expected the tile, got {other:?}"),
        }
        assert_eq!(
            group.transform,
            Transform::scale(0.5, 0.5),
            "the icon is scaled to the requested size"
        );
    }
}
