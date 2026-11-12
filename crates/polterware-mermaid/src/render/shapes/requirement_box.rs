//! Classic port of upstream `shapes/requirementBox.ts`.
//!
//! `Node` carries no requirement fields, so `'verifyMethod' in node` is false and upstream
//! takes its Element branch: a `<<Element>>` stereotype above the (bold) name. The remaining
//! Element fields (`type`, `docRef`) are absent, so no body rows or divider are produced.
use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, get_node_classes, shape_style,
};
use crate::measure::Font;
use crate::render::labels::{LabelOptions, create_label};
use crate::scene::{Group, Item, Point, Rect, RectShape, Size, SourceRef, Transform};

/// `const padding = 20` in `requirementBox`.
const PADDING: f64 = 20.0;
/// Non-HTML labels get `bbox.height += 6` in requirementBox's `addText`.
const SVG_LABEL_EXTRA_HEIGHT: f64 = 6.0;

fn text_size(label: &Group) -> Size {
    match label.items.first() {
        Some(Item::Text(text)) => text.size,
        _ => Size::ZERO,
    }
}

fn add_text(
    text: &str,
    font: Font,
    node: &Node,
    ctx: &ShapeContext<'_>,
) -> Result<(Group, Size), ShapeError> {
    let label = create_label(
        &LabelOptions {
            text: text.to_owned(),
            font,
            markdown: false,
            width: f64::INFINITY,
            centered: false,
            class: "label".into(),
            source: Some(SourceRef::node(node.id.clone())),
        },
        ctx.measure,
    )
    .map_err(|error| ShapeError {
        message: error.to_string(),
    })?;
    let size = text_size(&label);
    Ok((label, size))
}

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let mut group = Group::new()
        .with_class(get_node_classes(&node, None))
        .with_source(SourceRef::node(node.id.clone()));

    let font_size = ctx
        .theme
        .color("fontSize")
        .and_then(|s| s.trim_end_matches("px").parse::<f64>().ok())
        .or_else(|| ctx.theme.number("fontSize"))
        .unwrap_or(Font::default().size);
    let font_family = ctx
        .theme
        .color("fontFamily")
        .map(|s| s.to_string())
        .unwrap_or_else(|| Font::default().family);
    let is_monospace = font_family.to_ascii_lowercase().contains("mono");
    let type_font = Font {
        family: font_family.clone(),
        size: font_size,
        weight: crate::scene::NORMAL_WEIGHT,
        italic: false,
    };
    let name_font = Font {
        family: font_family,
        size: font_size,
        weight: if is_monospace {
            crate::scene::NORMAL_WEIGHT
        } else {
            crate::scene::BOLD_WEIGHT
        },
        italic: false,
    };

    let (mut type_label, type_size) = add_text("&lt;&lt;Element&gt;&gt;", type_font, &node, ctx)?;
    let (mut name_label, name_size) = add_text(node.label.as_str(), name_font, &node, ctx)?;

    let type_height = type_size.height + SVG_LABEL_EXTRA_HEIGHT;
    let name_height = name_size.height + SVG_LABEL_EXTRA_HEIGHT;
    let type_trans_y = -type_height / 2.0;
    let name_trans_y = -name_height / 2.0 + type_height;

    let y_min = type_trans_y;
    let y_max = name_trans_y + name_size.height;
    let total_height = (y_max - y_min) + PADDING;
    let total_width = type_size.width.max(name_size.width) + PADDING;
    let rect = Rect::centered(Point::ZERO, Size::new(total_width, total_height));
    node.bounds = rect;

    let mut shape = Group::new().with_class("basic label-container outer-path");
    // handDrawn: rough.js (later)
    shape.push(
        RectShape::new(rect, 0.0, shape_style()).with_source(SourceRef::node(node.id.clone())),
    );
    group.push(shape);

    // Re-translate the labels once the rectangle is centred, then undo the SVG label's
    // centred bbox: scene text runs start at their position.
    let final_type_y = type_trans_y - total_height / 2.0 + PADDING;
    let final_name_y = name_trans_y - total_height / 2.0 + PADDING;
    type_label.transform = Transform::translate(-type_size.width, final_type_y);
    name_label.transform = Transform::translate(-name_size.width, final_name_y);
    group.push(Item::Group(type_label));
    group.push(Item::Group(name_label));

    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::render::intersect::Intersectable;
    use crate::scene::Item;
    use serde_json::Value;

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() <= 0.01, "{actual} != {expected}");
    }

    fn label_groups(items: &[Item]) -> Vec<&Group> {
        items
            .iter()
            .filter_map(|item| match item {
                Item::Group(group) if group.class.as_deref() == Some("label") => Some(group),
                _ => None,
            })
            .collect()
    }

    fn label_width(label: &Group) -> f64 {
        match label.items.first() {
            Some(Item::Text(text)) => text.size.width,
            _ => 0.0,
        }
    }

    fn label_text(label: &Group) -> String {
        match label.items.first() {
            Some(Item::Text(text)) => text.lines.iter().map(|line| line.text()).collect(),
            _ => String::new(),
        }
    }

    fn translate(transform: &str) -> Point {
        let inner = transform
            .trim_start_matches("translate(")
            .trim_end_matches(')');
        let mut parts = inner.split(',');
        Point::new(
            parts.next().unwrap().trim().parse().unwrap(),
            parts.next().unwrap().trim().parse().unwrap(),
        )
    }

    #[test]
    fn matches_upstream_requirement_box_fixtures() {
        let fixture: Value =
            serde_json::from_str(include_str!("../testdata/shapes/requirement_box.json")).unwrap();
        let ctx = ShapeContext {
            measure: &FallbackMeasure,
            theme: &crate::theme::ThemeVariables::default(),
        };
        for case in fixture["cases"].as_array().unwrap() {
            let label = case["label"].as_str().unwrap();
            let mut node = Node::new("n1", label, "requirementBox");
            node.padding = 15.0;
            node.look = "classic".into();
            let rendered = draw(&node, &ctx).unwrap();
            close(
                rendered.node.bounds.width,
                case["bbox"]["width"].as_f64().unwrap(),
            );
            close(
                rendered.node.bounds.height,
                case["bbox"]["height"].as_f64().unwrap(),
            );
            for (index, query) in [(0, Point::new(1000.0, 0.0)), (1, Point::new(0.0, -1000.0))] {
                let hit = rendered.node.intersect(query);
                close(hit.x, case["intersects"][index]["x"].as_f64().unwrap());
                close(hit.y, case["intersects"][index]["y"].as_f64().unwrap());
            }

            let labels = label_groups(&rendered.group.items);
            let expected = case["labels"].as_array().unwrap();
            assert_eq!(labels.len(), expected.len());
            for (label, expected) in labels.iter().zip(expected) {
                assert_eq!(label_text(label), expected["text"].as_str().unwrap());
                let upstream = translate(expected["transform"].as_str().unwrap());
                close(label.transform.e, upstream.x - label_width(label) / 2.0);
                close(label.transform.f, upstream.y);
            }
        }
    }
}
