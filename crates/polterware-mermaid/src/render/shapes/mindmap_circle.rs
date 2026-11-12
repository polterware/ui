//! Classic port of upstream `shapes/mindmapCircle.ts`, which delegates to `shapes/circle.ts`
//! with `padding: node.padding`. The classic radius is half the label box's diagonal plus that
//! padding; the intersection is circular.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{EllipseShape, Group, Item, Point, Rect, Size, SourceRef, Transform};

/// `labelPadding` in circle.ts.
const LABEL_PADDING: f64 = 16.0;

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (group, label, text_size) = prepare(&node, ctx)?;
    let label_radius = (text_size.width.powi(2) + text_size.height.powi(2)).sqrt() / 2.0;
    let radius = if node.look == "neo" {
        label_radius + LABEL_PADDING
    } else {
        label_radius + node.padding
    };
    let bounds = Rect::centered(Point::ZERO, Size::new(radius * 2.0, radius * 2.0));
    node.bounds = bounds;

    let mut group = group;
    let mut shape = Group::new().with_class("basic label-container");
    // handDrawn: rough.js (later)
    shape.push(
        EllipseShape::new(Point::ZERO, radius, radius, shape_style())
            .with_source(SourceRef::node(node.id.clone())),
    );
    group.push(shape);

    let mut label = label;
    label.transform = Transform::translate(-text_size.width / 2.0, -text_size.height / 2.0);
    group.push(Item::Group(label));

    node.intersect = IntersectKind::Circle;
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::render::intersect::Intersectable;
    use crate::scene::{EllipseShape, Item};
    use serde_json::Value;

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() <= 0.01, "{actual} != {expected}");
    }

    fn first_ellipse(items: &[Item]) -> Option<&EllipseShape> {
        for item in items {
            match item {
                Item::Ellipse(ellipse) => return Some(ellipse),
                Item::Group(group) => {
                    if let Some(ellipse) = first_ellipse(&group.items) {
                        return Some(ellipse);
                    }
                }
                _ => {}
            }
        }
        None
    }

    #[test]
    fn matches_upstream_mindmap_circle_fixtures() {
        let fixture: Value =
            serde_json::from_str(include_str!("../testdata/shapes/mindmap_circle.json")).unwrap();
        let ctx = ShapeContext {
            measure: &FallbackMeasure,
            theme: &crate::theme::ThemeVariables::default(),
        };
        for case in fixture["cases"].as_array().unwrap() {
            let label = case["label"].as_str().unwrap();
            let mut node = Node::new("n1", label, "mindmapCircle");
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
            let circle = first_ellipse(&rendered.group.items).expect("circle");
            close(circle.radius_x, case["elements"][0]["r"].as_f64().unwrap());
            close(circle.radius_y, case["elements"][0]["r"].as_f64().unwrap());
            for (index, query) in [(0, Point::new(1000.0, 0.0)), (1, Point::new(0.0, -1000.0))] {
                let hit = rendered.node.intersect(query);
                close(hit.x, case["intersects"][index]["x"].as_f64().unwrap());
                close(hit.y, case["intersects"][index]["y"].as_f64().unwrap());
            }
        }
    }
}
