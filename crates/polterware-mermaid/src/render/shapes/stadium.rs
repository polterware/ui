//! Upstream `shapes/stadium.ts`.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Item, Path, PathShape, Point, Rect, Size, SourceRef};

const ARC_POINT_COUNT: usize = 50;

fn stadium_dimensions(bbox_w: f64, bbox_h: f64, padding_x: f64, padding_y: f64) -> (f64, f64) {
    let h = bbox_h + padding_y;
    let inscribed_diameter =
        h * (std::f64::consts::PI / (2.0 * (ARC_POINT_COUNT as f64 - 1.0))).cos();
    let cap_width_at_label = (inscribed_diameter.powi(2) - bbox_h.powi(2))
        .max(0.0)
        .sqrt();
    let w = (bbox_w + h / 4.0 + padding_x)
        .max(1.5 * h + (h - inscribed_diameter))
        .max(bbox_w + padding_x + h - cap_width_at_label);
    (w, h)
}

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text_size) = prepare(&node, ctx)?;
    let padding_x = if node.look == "neo" {
        40.0
    } else {
        node.padding
    };
    let padding_y = if node.look == "neo" {
        24.0
    } else {
        node.padding
    };
    let (width, height) =
        stadium_dimensions(text_size.width, text_size.height, padding_x, padding_y);
    let width = width.max(node.width.unwrap_or(0.0));
    let height = height.max(node.height.unwrap_or(0.0));
    let radius = height / 2.0;

    let mut points = vec![
        Point::new(-width / 2.0 + radius, -height / 2.0),
        Point::new(width / 2.0 - radius, -height / 2.0),
    ];
    points.extend(super::generate_circle_points(
        -width / 2.0 + radius,
        0.0,
        radius,
        ARC_POINT_COUNT,
        90.0,
        270.0,
    ));
    points.push(Point::new(width / 2.0 - radius, height / 2.0));
    points.extend(super::generate_circle_points(
        width / 2.0 - radius,
        0.0,
        radius,
        ARC_POINT_COUNT,
        270.0,
        450.0,
    ));

    let (min_x, max_x, min_y, max_y) = points.iter().fold(
        (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ),
        |(min_x, max_x, min_y, max_y), pt| {
            (
                min_x.min(pt.x),
                max_x.max(pt.x),
                min_y.min(pt.y),
                max_y.max(pt.y),
            )
        },
    );
    node.bounds = Rect::centered(Point::ZERO, Size::new(max_x - min_x, max_y - min_y));

    let mut path = Path::new();
    let r = radius;
    let x = -width / 2.0;
    let y = -height / 2.0;
    let right = width / 2.0;
    path.move_to(Point::new(x + r, y))
        .line_to(Point::new(right - r, y))
        .arc_to(r, r, 0.0, false, true, Point::new(right - r, y + height))
        .line_to(Point::new(x + r, y + height))
        .arc_to(r, r, 0.0, false, true, Point::new(x + r, y))
        .close();
    group.push(PathShape::new(path, shape_style()).with_source(SourceRef::node(node.id.clone())));

    node.intersect = IntersectKind::Polygon(points);
    group.items.push(Item::Group(label));
    Ok(RenderedNode { node, group })
}
