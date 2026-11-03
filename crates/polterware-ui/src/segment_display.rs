//! `SegmentDisplay` (`segment-display.tsx`): a seven-segment readout of digits, minus, colon,
//! dot, percent, hash, and space; other characters draw every segment unlit. Each glyph is the
//! design system's SVG (a 10×18 view box, 4×18 for the narrow `:`, `.`, and space), its paths
//! stroked 1.5 wide with round caps, lit in `--color-text-primary` and unlit in
//! `--color-surface-selected`, scaled to the readout's height, `height × 0.14` apart.
//!
//! GPUI tints a whole SVG one color, so a glyph is two SVGs on top of each other, its unlit
//! paths and its lit ones, which [`glyph_svg`] writes for the asset loader.

use gpui::{App, IntoElement, SharedString, div, prelude::*, px, svg};

use crate::theme::Theme;

/// Segments a to g, clockwise from the top bar, with g in the middle.
const SEGMENTS: [&str; 7] = [
    "M2.5 2H7.5",
    "M8.5 3V8",
    "M8.5 10V15",
    "M2.5 16H7.5",
    "M1.5 10V15",
    "M1.5 3V8",
    "M2.5 9H7.5",
];

/// What one character of a segment display draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlyphKind {
    /// Segments a, b, c, d, e, f, g. `g` is the middle bar.
    Segments([bool; 7]),
    Colon,
    Dot,
    Percent,
    Hash,
    Space,
    Blank,
}

impl GlyphKind {
    fn narrow(self) -> bool {
        matches!(self, GlyphKind::Colon | GlyphKind::Dot | GlyphKind::Space)
    }

    /// The asset key: the character's kind and, for segments, which are lit.
    fn key(self) -> String {
        match self {
            GlyphKind::Segments(flags) => {
                let mask = flags
                    .iter()
                    .enumerate()
                    .fold(0u8, |mask, (index, lit)| mask | (u8::from(*lit) << index));
                format!("s{mask}")
            }
            GlyphKind::Blank => "s0".to_string(),
            GlyphKind::Colon => "colon".to_string(),
            GlyphKind::Dot => "dot".to_string(),
            GlyphKind::Percent => "percent".to_string(),
            GlyphKind::Hash => "hash".to_string(),
            GlyphKind::Space => "space".to_string(),
        }
    }
}

pub fn segment_glyph(ch: char) -> GlyphKind {
    let mask = |segments: &str| {
        let mut flags = [false; 7];
        for segment in segments.chars() {
            let index = match segment {
                'a' => 0,
                'b' => 1,
                'c' => 2,
                'd' => 3,
                'e' => 4,
                'f' => 5,
                'g' => 6,
                _ => continue,
            };
            flags[index] = true;
        }
        GlyphKind::Segments(flags)
    };
    match ch {
        '0' => mask("abcdef"),
        '1' => mask("bc"),
        '2' => mask("abged"),
        '3' => mask("abgcd"),
        '4' => mask("fgbc"),
        '5' => mask("afgcd"),
        '6' => mask("afgecd"),
        '7' => mask("abc"),
        '8' => mask("abcdefg"),
        '9' => mask("abcdfg"),
        '-' => mask("g"),
        ':' => GlyphKind::Colon,
        '.' => GlyphKind::Dot,
        '%' => GlyphKind::Percent,
        '#' => GlyphKind::Hash,
        ' ' => GlyphKind::Space,
        _ => GlyphKind::Blank,
    }
}

/// The paths a glyph strokes unlit and lit.
fn glyph_paths(kind: GlyphKind) -> (Vec<&'static str>, Vec<&'static str>) {
    match kind {
        GlyphKind::Segments(flags) => {
            let mut unlit = Vec::new();
            let mut lit = Vec::new();
            for (path, on) in SEGMENTS.iter().zip(flags) {
                if on { &mut lit } else { &mut unlit }.push(*path);
            }
            (unlit, lit)
        }
        GlyphKind::Blank => (SEGMENTS.to_vec(), Vec::new()),
        GlyphKind::Percent => (
            vec!["M2.5 5H3.5", "M6.5 13H7.5"],
            vec![
                "M2 2H4M4.5 2.5V4.5M4.5 5.5V7.5M2 8H4M1.5 5.5V7.5M1.5 2.5V4.5",
                "M6 10H8M8.5 10.5V12.5M8.5 13.5V15.5M6 16H8M5.5 13.5V15.5M5.5 10.5V12.5",
                "M8.5 3L1.5 15",
            ],
        ),
        GlyphKind::Hash => (
            Vec::new(),
            vec!["M4 4L3 14M7.5 4L6.5 14M1.5 7.5H8.5M1 11H8"],
        ),
        GlyphKind::Colon | GlyphKind::Dot | GlyphKind::Space => (Vec::new(), Vec::new()),
    }
}

/// The SVG for an asset path `segments/<key>-<lit|unlit>.svg`, painted black for GPUI to tint.
pub fn glyph_svg(path: &str) -> Option<String> {
    let name = path.strip_prefix("segments/")?.strip_suffix(".svg")?;
    let (key, layer) = name.rsplit_once('-')?;
    let lit = match layer {
        "lit" => true,
        "unlit" => false,
        _ => return None,
    };
    let kind = [
        GlyphKind::Colon,
        GlyphKind::Dot,
        GlyphKind::Percent,
        GlyphKind::Hash,
        GlyphKind::Space,
    ]
    .into_iter()
    .find(|kind| kind.key() == key)
    .or_else(|| {
        let mask: u8 = key.strip_prefix('s')?.parse().ok()?;
        let mut flags = [false; 7];
        for (index, flag) in flags.iter_mut().enumerate() {
            *flag = mask & (1 << index) != 0;
        }
        Some(GlyphKind::Segments(flags))
    })?;
    let width = if kind.narrow() { 4 } else { 10 };
    let mut body = String::new();
    if lit {
        match kind {
            GlyphKind::Colon => {
                body.push_str(r#"<circle cx="2" cy="15" r="1"/><circle cx="2" cy="5.5" r="1"/>"#)
            }
            GlyphKind::Dot => body.push_str(r#"<circle cx="2" cy="15" r="1"/>"#),
            _ => {}
        }
    }
    let (unlit, lit_paths) = glyph_paths(kind);
    for path in if lit { lit_paths } else { unlit } {
        body.push_str(&format!(
            r#"<path d="{path}" fill="none" stroke="black" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#
        ));
    }
    Some(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="18" viewBox="0 0 {width} 18" fill="black">{body}</svg>"#
    ))
}

pub fn segment_display(value: impl AsRef<str>, height_px: f32, cx: &App) -> impl IntoElement {
    let theme = *Theme::global(cx);
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(height_px * 0.14))
        .h(px(height_px));
    for ch in value.as_ref().chars() {
        row = row.child(glyph(segment_glyph(ch), height_px, &theme));
    }
    row
}

fn glyph(kind: GlyphKind, height: f32, theme: &Theme) -> impl IntoElement + use<> {
    let scale = height / 18.0;
    let width = if kind.narrow() { 4.0 } else { 10.0 } * scale;
    let layer = |name: &str, color| {
        svg()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .w(px(width))
            .h(px(height))
            .path(SharedString::from(format!(
                "segments/{}-{name}.svg",
                kind.key()
            )))
            .text_color(color)
    };
    let (unlit, lit) = glyph_paths(kind);
    let dots = matches!(kind, GlyphKind::Colon | GlyphKind::Dot);
    div()
        .relative()
        .flex_none()
        .w(px(width))
        .h(px(height))
        .when(!unlit.is_empty(), |cell| {
            cell.child(layer("unlit", theme.color_surface_selected))
        })
        .when(!lit.is_empty() || dots, |cell| {
            cell.child(layer("lit", theme.color_text_primary))
        })
}

#[cfg(test)]
mod tests {
    use super::{GlyphKind, glyph_svg, segment_glyph};

    #[test]
    fn digits_match_the_design_system_map() {
        assert_eq!(
            segment_glyph('8'),
            GlyphKind::Segments([true, true, true, true, true, true, true])
        );
        assert_eq!(
            segment_glyph('1'),
            GlyphKind::Segments([false, true, true, false, false, false, false])
        );
        assert_eq!(
            segment_glyph('-'),
            GlyphKind::Segments([false, false, false, false, false, false, true])
        );
        assert_eq!(segment_glyph(':'), GlyphKind::Colon);
        assert_eq!(segment_glyph('.'), GlyphKind::Dot);
        assert_eq!(segment_glyph('%'), GlyphKind::Percent);
        assert_eq!(segment_glyph('#'), GlyphKind::Hash);
        assert_eq!(segment_glyph(' '), GlyphKind::Space);
        assert_eq!(segment_glyph('A'), GlyphKind::Blank);
    }

    #[test]
    fn each_glyph_layer_is_an_svg_of_its_paths() {
        // "1" lights b and c (mask 6): the lit layer strokes those two, the unlit one five.
        let lit = glyph_svg("segments/s6-lit.svg").unwrap();
        assert_eq!(lit.matches("<path").count(), 2);
        assert!(lit.contains(r#"d="M8.5 3V8""#));
        let unlit = glyph_svg("segments/s6-unlit.svg").unwrap();
        assert_eq!(unlit.matches("<path").count(), 5);
        let percent = glyph_svg("segments/percent-lit.svg").unwrap();
        assert!(percent.contains("M8.5 3L1.5 15"));
        let colon = glyph_svg("segments/colon-lit.svg").unwrap();
        assert!(colon.contains("<circle"));
        assert!(colon.contains(r#"width="4""#));
        assert!(glyph_svg("icons/bot.svg").is_none());
    }
}
