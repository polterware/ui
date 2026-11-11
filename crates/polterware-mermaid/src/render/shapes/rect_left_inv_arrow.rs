//! Port of upstream `shapes/leftinversearrowrectangle.ts` classic geometry.
use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, create_path_from_points, prepare,
    shape_style,
};
use crate::scene::{Item, PathShape, Point, Rect, Size, SourceRef};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text) = prepare(&node, ctx)?;
    // handDrawn: rough.js (later)
    let points = {
        let (px, py) = if node.look == "neo" {
            (21.0, 12.0)
        } else {
            (node.padding, node.padding)
        };
        let h = (text.height + if node.look == "neo" { py * 2.0 } else { py })
            .max(node.height.unwrap_or(0.0));
        let w = (text.width + if node.look == "neo" { px * 2.0 } else { px })
            .max(node.width.unwrap_or(0.0) - h / 4.0);
        let x = -w / 2.0;
        let y = -h / 2.0;
        let notch = y / 2.0;
        vec![
            Point::new(x + notch, y),
            Point::new(x, 0.0),
            Point::new(x + notch, -y),
            Point::new(-x, -y),
            Point::new(-x, y),
        ]
    };
    let (min_x, max_x, min_y, max_y) = points.iter().fold(
        (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ),
        |(a, b, c, d), p| (a.min(p.x), b.max(p.x), c.min(p.y), d.max(p.y)),
    );
    let width = max_x - min_x;
    let height = max_y - min_y;
    node.bounds = Rect::centered(Point::ZERO, Size::new(width, height));
    let local = points
        .iter()
        .map(|p| Point::new(p.x - (min_x + max_x) / 2.0, p.y - (min_y + max_y) / 2.0))
        .collect::<Vec<_>>();
    group.push(
        PathShape::new(create_path_from_points(&local), shape_style())
            .with_source(SourceRef::node(node.id.clone())),
    );
    node.intersect = IntersectKind::Polygon(points);
    group.items.push(Item::Group(label));
    Ok(RenderedNode { node, group })
}
