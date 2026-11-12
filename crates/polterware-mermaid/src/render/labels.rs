//! Port of `rendering-elements/createLabel.js`, `labelTransform.ts`, and the label-building
//! portion of `createText.ts`. Labels become a positioned scene group; HTML `foreignObject`
//! labels are not represented by the scene model.

use crate::measure::{Font, TextMeasure, TextOptions, shape_text};
use crate::scene::{Group, Point, Size, SourceRef, TextLine, TextRun, Transform};

/// Text and placement options for an upstream label group.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelOptions {
    pub text: String,
    pub font: Font,
    pub markdown: bool,
    pub width: f64,
    pub centered: bool,
    pub class: String,
    pub source: Option<SourceRef>,
}

impl LabelOptions {
    /// `createLabel`'s non-markdown, unwrapped text defaults.
    pub fn plain(text: impl Into<String>, font: Font) -> Self {
        Self {
            text: text.into(),
            font,
            markdown: false,
            width: f64::INFINITY,
            centered: false,
            class: "label".to_string(),
            source: None,
        }
    }
}

/// Build the SVG-equivalent `<g class="label">` and text content using `shape_text`.
pub fn create_label(
    options: &LabelOptions,
    measure: &dyn TextMeasure,
) -> Result<Group, crate::measure::TextError> {
    let block = shape_text(
        &options.text,
        &TextOptions {
            width: options.width,
            markdown: options.markdown,
            min_width: 0.0,
        },
        &options.font,
        measure,
    )?;
    let x = if options.centered {
        -block.size.width / 2.0
    } else {
        0.0
    };
    let mut group =
        Group::translated(x, -block.size.height / 2.0).with_class(options.class.clone());
    group.source = options.source.clone();
    group.push(TextRun::new(Point::ZERO, options.font.clone(), block));
    Ok(group)
}

/// The box `getBBox()` reports for `createFormattedText`'s `<text>`: each wrapped line is a
/// `<tspan>` and only a line's first word has no leading space, so the text content runs the
/// lines together. That box is the joined lines' width and a single line's height, which is
/// what a flowchart node's `labelHelper` sizes the shape from; the label group keeps the
/// wrapped block's own box for painting. Empty text has no ink.
pub(crate) fn svg_text_box(lines: &[TextLine], font: &Font, measure: &dyn TextMeasure) -> Size {
    let text: String = lines.iter().map(TextLine::text).collect();
    if text.is_empty() {
        return Size::ZERO;
    }
    Size::new(measure.width(&text, font), measure.line_height(font))
}

/// `labelTransform.ts`: SVG labels account for bbox origin; HTML labels only use dimensions.
pub fn label_transform(
    x: Option<f64>,
    y: Option<f64>,
    width: f64,
    height: f64,
    html: bool,
) -> Transform {
    let (x, y) = if html {
        (0.0, 0.0)
    } else {
        (x.unwrap_or(0.0), y.unwrap_or(0.0))
    };
    Transform::translate(-(x + width / 2.0), -(y + height / 2.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::scene::Item;

    #[test]
    fn creates_measured_centered_label_group() {
        let label = create_label(
            &LabelOptions {
                centered: true,
                ..LabelOptions::plain("label", Font::default())
            },
            &FallbackMeasure,
        )
        .unwrap();
        assert_eq!(label.class.as_deref(), Some("label"));
        assert_eq!(label.items.len(), 1);
        assert!(matches!(&label.items[0], Item::Text(text) if text.position == Point::ZERO));
        assert_eq!(
            label.transform.e,
            -match &label.items[0] {
                Item::Text(text) => text.size.width / 2.0,
                _ => unreachable!(),
            }
        ); // finite translation
    }

    #[test]
    fn svg_text_box_runs_wrapped_lines_together() {
        let font = Font::new("monospace", 12.0);
        let block = shape_text(
            "aa bb cc",
            &TextOptions::default().with_width(30.0),
            &font,
            &FallbackMeasure,
        )
        .expect("text shapes");
        assert_eq!(block.lines.len(), 3);
        let text_box = svg_text_box(&block.lines, &font, &FallbackMeasure);
        // "aabbcc" run together is six characters of 0.6 em at 12 px, one line tall.
        assert!((text_box.width - 43.2).abs() < 1e-9);
        assert!((text_box.height - 13.8).abs() < 1e-9);
    }

    #[test]
    fn label_transform_matches_svg_and_html_rules() {
        assert_eq!(
            label_transform(Some(-2.0), Some(4.0), 20.0, 10.0, false),
            Transform::translate(-8.0, -9.0)
        );
        assert_eq!(
            label_transform(Some(100.0), Some(100.0), 20.0, 10.0, true),
            Transform::translate(-10.0, -5.0)
        );
    }
}
