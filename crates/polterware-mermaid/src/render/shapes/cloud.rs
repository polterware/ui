//! Upstream `shapes/cloud.ts`; classic `basic label-container` path and rectangular intersection.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Item, Path, PathShape, Point, Rect, Size, SourceRef};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text) = prepare(&node, ctx)?;
    let pad = if node.look == "neo" {
        12.0
    } else {
        node.padding
    };
    let w = text.width + 2.0 * pad;
    let h = text.height + 2.0 * pad;
    if !w.is_finite() || !h.is_finite() {
        return Err(ShapeError {
            message: "invalid cloud label dimensions".into(),
        });
    }
    let r1 = 0.15 * w;
    let r2 = 0.25 * w;
    let r3 = 0.35 * w;
    let r4 = 0.2 * w;
    // Classic SVG path from upstream; handDrawn: rough.js (later)
    let mut path = Path::new();
    path.move_to(Point::new(0.0, 0.0));
    let arcs = [
        (r1, r1, 0.25 * w, -0.1 * w),
        (r3, r3, 0.4 * w, -0.1 * w),
        (r2, r2, 0.35 * w, 0.2 * w),
        (r1, r1, 0.15 * w, 0.35 * h),
        (r4, r4, -0.15 * w, 0.65 * h),
        (r2, r1, -0.25 * w, 0.15 * w),
        (r3, r3, -0.5 * w, 0.0),
        (r1, r1, -0.25 * w, -0.15 * w),
        (r1, r1, -0.1 * w, -0.35 * h),
        (r4, r4, 0.1 * w, -0.65 * h),
    ];
    let mut p = Point::ZERO;
    for (rx, ry, dx, dy) in arcs {
        p = Point::new(p.x + dx, p.y + dy);
        path.arc_to(rx, ry, 0.0, true, true, p);
    }
    path.line_to(Point::new(0.0, 0.0));
    path.close();
    group.push(PathShape::new(path, shape_style()).with_source(SourceRef::node(node.id.clone())));
    node.bounds = Rect::centered(Point::ZERO, Size::new(w, h));
    node.intersect = IntersectKind::Rect;
    group.items.push(Item::Group(label));
    Ok(RenderedNode { node, group })
}
