//! Upstream `shapes/question.ts`.
//!
//! Upstream translates the diamond polygon half a point right of its label
//! (`adjustment = 0.5`). The landing box stays centred — `updateNodeBounds` reads the size,
//! not the offset, and `intersect` answers from the label's centre — while the painted
//! polygon carries the shift.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Item, Path, PathShape, Point, Rect, Size, SourceRef};

/// `question.ts`' `adjustment`: the polygon's rightward offset from the label's centre.
const ADJUSTMENT: f64 = 0.5;

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text_size) = prepare(&node, ctx)?;
    let w = text_size.width + node.padding;
    let h = text_size.height + node.padding;
    let s = w + h;
    node.bounds = Rect::centered(Point::ZERO, Size::new(s, s));
    let points = vec![
        Point::new(s / 2.0, 0.0),
        Point::new(s, -s / 2.0),
        Point::new(s / 2.0, -s),
        Point::new(0.0, -s / 2.0),
    ];
    let local: Vec<Point> = points
        .iter()
        .map(|p| Point::new(p.x - s / 2.0 + ADJUSTMENT, p.y + s / 2.0))
        .collect();
    group.push(
        PathShape::new(Path::polygon(local), shape_style())
            .with_source(SourceRef::node(node.id.clone())),
    );
    node.intersect = IntersectKind::Polygon(points);
    group.items.push(Item::Group(label));
    Ok(RenderedNode { node, group })
}
