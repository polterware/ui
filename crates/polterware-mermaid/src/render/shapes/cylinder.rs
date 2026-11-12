//! Port of upstream `shapes/cylinder.ts`; classic path matches its non-handDrawn `d`.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Group, Item, Path, PathShape, Point, Rect, Size, SourceRef};

// handDrawn: rough.js (later)
pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, mut label, text) = prepare(&node, ctx)?;
    let pad_x = if node.look == "neo" {
        24.0
    } else {
        node.padding
    };
    let pad_y = if node.look == "neo" {
        24.0
    } else {
        node.padding
    };
    if let Some(width) = node.width {
        node.width = Some((width - pad_y).max(8.0));
    }
    let mut rx;
    let mut ry;
    if let Some(height) = node.height {
        let original_width = node.width.unwrap_or(0.0) + pad_y;
        rx = original_width / 2.0;
        ry = rx / (2.5 + original_width / 50.0);
        node.height = Some((height - pad_x - ry * 3.0).max(8.0));
    }
    let w = node.width.unwrap_or(0.0).max(text.width) + pad_y;
    rx = w / 2.0;
    ry = rx / (2.5 + w / 50.0);
    let h = node.height.unwrap_or(0.0).max(text.height) + pad_x + ry;
    let bounds = Rect::centered(Point::ZERO, Size::new(w, h + 2.0 * ry));
    node.bounds = bounds;
    let x = -w / 2.0;
    let top = -h / 2.0;
    let mut path = Path::new();
    path.move_to(Point::new(x, top));
    path.arc_to(rx, ry, 0.0, false, false, Point::new(x + w, top));
    path.arc_to(rx, ry, 0.0, false, false, Point::new(x, top));
    path.line_to(Point::new(x, top + h));
    path.arc_to(rx, ry, 0.0, false, false, Point::new(x + w, top + h));
    path.line_to(Point::new(x + w, top));
    let mut shape = Group::new().with_class("basic label-container outer-path");
    shape.push(PathShape::new(path, shape_style()).with_source(SourceRef::node(node.id.clone())));
    group.push(shape);
    label.transform = crate::scene::Transform::translate(
        -text.width / 2.0,
        -text.height / 2.0 + node.padding / 1.5,
    );
    group.push(Item::Group(label));
    node.intersect = IntersectKind::Cylinder { rx, ry };
    Ok(RenderedNode { node, group })
}

pub(super) fn cylinder_outline(width: f64, height: f64, ry: f64, steps: usize) -> Vec<Point> {
    let rx = width / 2.0;
    let mut points = Vec::with_capacity(steps * 2 + 2);
    for i in 0..=steps {
        let a = std::f64::consts::PI - std::f64::consts::PI * i as f64 / steps as f64;
        points.push(Point::new(rx * a.cos(), -height / 2.0 - ry * a.sin()));
    }
    points.push(Point::new(rx, height / 2.0));
    for i in 0..=steps {
        let a = std::f64::consts::PI * i as f64 / steps as f64;
        points.push(Point::new(rx * a.cos(), height / 2.0 + ry * a.sin()));
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::render::intersect::Intersectable;

    #[test]
    fn classic_cylinder_has_two_curved_ends_and_elliptic_intersection() {
        let rendered = draw(
            &Node::new("n", "DB", "cylinder"),
            &ShapeContext {
                measure: &FallbackMeasure,
                theme: &crate::theme::ThemeVariables::default(),
            },
        )
        .unwrap();
        let Item::Group(shape) = &rendered.group.items[0] else {
            panic!("shape group")
        };
        let Item::Path(path) = &shape.items[0] else {
            panic!("path")
        };
        assert_eq!(path.path.commands.len(), 6);
        let hit = rendered.node.intersect(Point::new(100.0, 0.0));
        assert!(hit.x > 0.0 && hit.x <= rendered.node.bounds.right());
    }
}
