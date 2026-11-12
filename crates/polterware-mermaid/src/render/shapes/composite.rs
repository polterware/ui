//! Classic port of upstream `shapes/composite.ts`.
//!
//! The unpositioned branch (during measurement) hugs the label plus `node.padding`. The
//! positioned branch, used by the block renderer's second insertion after layout, takes its
//! box from `node.width`/`node.height`; the label stays centred inside it.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Group, Item, Point, Rect, RectShape, Size, SourceRef, Transform};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (group, label, text_size) = prepare(&node, ctx)?;
    let total_width = if node.positioned {
        node.width.unwrap_or(0.0)
    } else {
        text_size.width + node.padding
    };
    let total_height = if node.positioned {
        node.height.unwrap_or(0.0)
    } else {
        text_size.height + node.padding
    };
    let bounds = Rect::centered(Point::ZERO, Size::new(total_width, total_height));
    node.bounds = bounds;

    let mut group = group;
    let mut shape = Group::new().with_class("basic cluster composite label-container");
    shape.push(
        RectShape::new(bounds, node.rx, shape_style())
            .with_source(SourceRef::node(node.id.clone())),
    );
    group.push(shape);

    let mut label = label;
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

    #[test]
    fn matches_upstream_composite_fixtures() {
        let fixture: Value =
            serde_json::from_str(include_str!("../testdata/shapes/composite.json")).unwrap();
        let ctx = ShapeContext {
            measure: &FallbackMeasure,
            theme: &crate::theme::ThemeVariables::default(),
        };
        for case in fixture["cases"].as_array().unwrap() {
            let label = case["label"].as_str().unwrap();
            let mut node = Node::new("n1", label, "composite");
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
            let rect = first_rect(&rendered.group.items).expect("rect");
            close(rect.rect.width, rendered.node.bounds.width);
            close(rect.rect.height, rendered.node.bounds.height);
            for (index, query) in [(0, Point::new(1000.0, 0.0)), (1, Point::new(0.0, -1000.0))] {
                let hit = rendered.node.intersect(query);
                close(hit.x, case["intersects"][index]["x"].as_f64().unwrap());
                close(hit.y, case["intersects"][index]["y"].as_f64().unwrap());
            }
        }
    }
}
