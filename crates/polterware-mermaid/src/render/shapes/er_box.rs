//! Classic port of upstream `shapes/erBox.ts`.
//!
//! Upstream draws an ER entity table when `attributes` is not empty. `Node` carries no ER
//! attributes, so this follows the label-only branch: `drawRect` with `rx`/`ry` 0, the ER
//! diagram padding (20), entity padding (15) and the default `minEntityWidth` (100), with a
//! rectangular intersection.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Group, Item, Point, Rect, RectShape, Size, SourceRef, Transform};

/// `config.er.diagramPadding`'s default, used as `PADDING`.
const DIAGRAM_PADDING: f64 = 20.0;
/// `config.er.minEntityWidth`'s default.
const MIN_ENTITY_WIDTH: f64 = 100.0;

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (group, label, text_size) = prepare(&node, ctx)?;
    let padding_x = DIAGRAM_PADDING;
    let padding_y = DIAGRAM_PADDING * 1.5;
    let padded_width = text_size.width + padding_x * 2.0;
    // Upstream replaces an explicit width with `minEntityWidth` when the padded label is narrow.
    let width = if padded_width < MIN_ENTITY_WIDTH {
        MIN_ENTITY_WIDTH
    } else {
        padded_width.max(node.width.unwrap_or(0.0))
    };
    let height = (text_size.height + padding_y * 2.0).max(node.height.unwrap_or(0.0));
    let bounds = Rect::centered(Point::ZERO, Size::new(width, height));
    node.bounds = bounds;

    let mut group = group;
    let mut shape = Group::new().with_class("basic label-container");
    // handDrawn: rough.js (later)
    shape.push(
        RectShape::new(bounds, 0.0, shape_style()).with_source(SourceRef::node(node.id.clone())),
    );
    group.push(shape);

    let mut label = label;
    // labelHelper centres SVG text on x = 0; the scene's runs start at their position.
    label.transform = Transform::translate(-text_size.width / 2.0, -text_size.height / 2.0);
    group.push(Item::Group(label));

    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::render::intersect::Intersectable;
    use serde_json::Value;

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() <= 0.01, "{actual} != {expected}");
    }

    #[test]
    fn matches_upstream_er_box_fixtures() {
        let fixture: Value =
            serde_json::from_str(include_str!("../testdata/shapes/er_box.json")).unwrap();
        let ctx = ShapeContext {
            measure: &FallbackMeasure,
            theme: &crate::theme::ThemeVariables::default(),
        };
        for case in fixture["cases"].as_array().unwrap() {
            let label = case["label"].as_str().unwrap();
            let mut node = Node::new("n1", label, "erBox");
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
        }
    }
}
