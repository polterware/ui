//! Classic port of upstream `shapes/crossedCircle.ts`.
//!
//! A circle of the larger of 30px and the node width, crossed by the two 45-degree diagonals
//! `createLine` builds, all inside the `outer-path` group classic rendering emits.
use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, get_node_classes, shape_style,
};
use crate::scene::{
    Color, EllipseShape, Group, Item, Path, PathShape, Point, Rect, Size, SourceRef,
};

fn create_line(radius: f64) -> Path {
    let axis = std::f64::consts::FRAC_PI_4.cos();
    let length = radius * 2.0;
    let q1 = Point::new((length / 2.0) * axis, (length / 2.0) * axis);
    let q2 = Point::new(-(length / 2.0) * axis, (length / 2.0) * axis);
    let q3 = Point::new(-(length / 2.0) * axis, -(length / 2.0) * axis);
    let q4 = Point::new((length / 2.0) * axis, -(length / 2.0) * axis);
    let mut path = Path::new();
    path.move_to(q2).line_to(q4).move_to(q1).line_to(q3);
    path
}

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    node.label.clear();

    // handDrawn: rough.js (later)
    let radius = 30.0_f64.max(node.width.unwrap_or(0.0));
    node.bounds = Rect::centered(Point::ZERO, Size::new(radius * 2.0, radius * 2.0));

    let border = ctx.theme.color("nodeBorder").unwrap_or("#9370DB");
    let background = ctx.theme.color("mainBkg").unwrap_or("#ECECFF");
    let mut style = shape_style();
    style.fill = Some(Color::new(background));
    style.stroke = Some(Color::new(border));
    style.stroke_width = 1.3;
    let mut line_style = style.clone();
    line_style.fill = None;

    let mut outer = Group::new()
        .with_class("outer-path")
        .with_source(SourceRef::node(node.id.clone()));
    outer.push(
        EllipseShape::new(Point::ZERO, radius, radius, style)
            .with_source(SourceRef::node(node.id.clone())),
    );
    outer.push(
        PathShape::new(create_line(radius), line_style)
            .with_source(SourceRef::node(node.id.clone())),
    );

    let mut group = Group::new()
        .with_class(get_node_classes(&node, None))
        .with_source(SourceRef::node(node.id.clone()));
    group.items.push(Item::Group(outer));

    node.intersect = IntersectKind::Circle;
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_upstream_classic_fixture() {
        crate::render::shapes::double_circle::fixture::check(
            include_str!("../testdata/shapes/crossed_circle.json"),
            draw,
        );
    }
}
