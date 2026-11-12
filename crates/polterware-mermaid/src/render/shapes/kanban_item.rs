//! Classic port of upstream `shapes/kanbanItem.ts`.
//!
//! `Node` carries no ticket, assignee, or priority, so the ticket and assignee labels measure
//! empty and the priority stripe is not drawn. The title keeps upstream's 10px horizontal
//! and vertical label padding; the card width is the node width (200 upstream default).
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Group, Item, Point, Rect, RectShape, Size, SourceRef, Transform};

/// `labelPaddingY` in kanbanItem.
const LABEL_PADDING_Y: f64 = 10.0;
/// Default card width when the node has none.
const DEFAULT_WIDTH: f64 = 200.0;
/// Default corner radius (`rx ?? 5`).
const DEFAULT_RADIUS: f64 = 5.0;

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text_size) = prepare(&node, ctx)?;
    let padding = if node.padding != 0.0 {
        node.padding
    } else {
        LABEL_PADDING_Y
    };
    let total_width = node.width.unwrap_or(DEFAULT_WIDTH);
    let total_height = (text_size.height + LABEL_PADDING_Y * 2.0).max(node.height.unwrap_or(0.0));
    let rect = Rect::centered(Point::ZERO, Size::new(total_width, total_height));
    node.bounds = rect;
    node.height = Some(total_height);

    let rx = if node.rx > 0.0 {
        node.rx
    } else {
        DEFAULT_RADIUS
    };
    let ry = if node.ry > 0.0 {
        node.ry
    } else {
        DEFAULT_RADIUS
    };
    let mut shape = Group::new().with_class("basic label-container __APA__");
    // handDrawn: rough.js (later)
    let mut rect_shape =
        RectShape::new(rect, rx, shape_style()).with_source(SourceRef::node(node.id.clone()));
    rect_shape.ry = ry;
    shape.push(rect_shape);
    group.push(shape);

    let mut label = label;
    // Title: left-aligned with `padding`, vertically centred on the card's title line.
    label.transform = Transform::translate(
        padding - total_width / 2.0 - text_size.width / 2.0,
        -text_size.height / 2.0,
    );
    group.push(Item::Group(label));

    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::render::intersect::Intersectable;
    use crate::scene::{Item, RectShape};
    use serde_json::Value;

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() <= 0.01, "{actual} != {expected}");
    }

    fn first_rect(items: &[Item]) -> Option<&RectShape> {
        for item in items {
            match item {
                Item::Rect(rect) => return Some(rect),
                Item::Group(group) => {
                    if let Some(rect) = first_rect(&group.items) {
                        return Some(rect);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn label_group(items: &[Item]) -> Option<&Group> {
        for item in items {
            if let Item::Group(group) = item {
                if group.class.as_deref() == Some("label") {
                    return Some(group);
                }
                if let Some(found) = label_group(&group.items) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn translate_x(transform: &str) -> f64 {
        transform
            .trim_start_matches("translate(")
            .split(',')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    }

    #[test]
    fn matches_upstream_kanban_item_fixtures() {
        let fixture: Value =
            serde_json::from_str(include_str!("../testdata/shapes/kanban_item.json")).unwrap();
        let ctx = ShapeContext {
            measure: &FallbackMeasure,
            theme: &crate::theme::ThemeVariables::default(),
        };
        for case in fixture["cases"].as_array().unwrap() {
            let label = case["label"].as_str().unwrap();
            let mut node = Node::new("n1", label, "kanbanItem");
            node.padding = 10.0;
            node.width = Some(200.0);
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
            let rect = first_rect(&rendered.group.items).expect("rect");
            close(rect.rx, 5.0);
            close(rect.ry, 5.0);
            let label_group = label_group(&rendered.group.items).expect("label");
            let upstream_x = translate_x(
                case["labels"][0]["transform"]
                    .as_str()
                    .expect("label transform"),
            );
            close(
                label_group.transform.e,
                upstream_x - case["label_size"]["width"].as_f64().unwrap() / 2.0,
            );
            for (index, query) in [(0, Point::new(1000.0, 0.0)), (1, Point::new(0.0, -1000.0))] {
                let hit = rendered.node.intersect(query);
                close(hit.x, case["intersects"][index]["x"].as_f64().unwrap());
                close(hit.y, case["intersects"][index]["y"].as_f64().unwrap());
            }
        }
    }
}
