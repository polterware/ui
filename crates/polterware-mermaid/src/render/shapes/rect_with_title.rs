//! Classic port of upstream `shapes/rectWithTitle.ts`.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Item, Path, PathShape, Point, Rect, SourceRef, Transform};

// TODO(dedupe): this shape-local renderer mirrors updateNodeBounds/createPathFromPoints.
fn box_points(w: f64, h: f64) -> Vec<Point> {
    vec![
        Point::new(-w / 2.0, -h / 2.0),
        Point::new(w / 2.0, -h / 2.0),
        Point::new(w / 2.0, h / 2.0),
        Point::new(-w / 2.0, h / 2.0),
    ]
}

fn finish(
    mut node: Node,
    mut group: crate::scene::Group,
    label: crate::scene::Group,
    points: Vec<Point>,
    label_offset: Point,
) -> Result<RenderedNode, ShapeError> {
    let min_x = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let max_x = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let min_y = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let max_y = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    node.bounds = Rect::new(min_x, min_y, max_x - min_x, max_y - min_y);
    let path = Path::polygon(points.iter().copied());
    group.push(PathShape::new(path, shape_style()).with_source(SourceRef::node(node.id.clone())));
    let mut label = label;
    label.transform = Transform::translate(label_offset.x, label_offset.y);
    node.intersect = IntersectKind::Polygon(points);
    group.items.push(Item::Group(label));
    Ok(RenderedNode { node, group })
}

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let node = node.clone();
    // `rectWithTitle` builds its title through `createLabel`, which passes
    // `width: Number.POSITIVE_INFINITY` to `createText`: the title never wraps, ignoring the
    // diagram's `wrappingWidth`.
    let mut title_node = node.clone();
    title_node.wrapping_width = None;
    // `rectWithTitle` builds title and description through `createLabel`, not `labelHelper`,
    // so the state's minimum node width does not apply.
    title_node.min_width = None;
    let (group, mut title_label, title_size) = prepare(&title_node, ctx)?;
    let p = node.padding;
    let half_padding = p / 2.0;
    let descriptions = node.description.clone().unwrap_or_default();

    // A description is joined with `<br/>` and measured below the title; an empty one draws
    // nothing and does not grow the node.
    let description = (!descriptions.is_empty()).then(|| {
        let mut description_node = node.clone();
        description_node.label = descriptions.join("\n");
        description_node.wrapping_width = None;
        description_node.min_width = None;
        description_node
    });
    let description_measure = match &description {
        Some(description_node) => {
            let (_, label, size) = prepare(description_node, ctx)?;
            Some((label, size))
        }
        None => None,
    };
    let (description_width, description_height) = description_measure
        .as_ref()
        .map(|(_, size)| (size.width, size.height))
        .unwrap_or((0.0, 0.0));
    let text_width = title_size.width.max(description_width);
    let text_height = if description_measure.is_some() {
        title_size.height + half_padding + 5.0 + description_height
    } else {
        title_size.height
    };
    let w = (text_width + p).max(node.width.unwrap_or(0.0));
    let h = (text_height + p).max(node.height.unwrap_or(0.0));

    let mut out = match description_measure {
        None => finish(node, group, title_label, box_points(w, h), Point::ZERO)?,
        Some((mut description_label, _)) => {
            let title_x = if description_width > title_size.width {
                0.0
            } else {
                (title_size.width - description_width) / 2.0
            };
            let description_x = if description_width < title_size.width {
                0.0
            } else {
                -(title_size.width - description_width) / 2.0
            };
            let description_y = title_size.height + half_padding + 5.0;
            title_label.transform = crate::scene::Transform::translate(title_x, 0.0);
            description_label.transform =
                crate::scene::Transform::translate(description_x, description_y);
            let mut label = crate::scene::Group::new().with_class("label");
            label.push(Item::Group(title_label));
            label.push(Item::Group(description_label));
            finish(
                node,
                group,
                label,
                box_points(w, h),
                Point::new(-text_width / 2.0, -text_height / 2.0 - half_padding + 3.0),
            )?
        }
    };

    let divider_y = -text_height / 2.0 + title_size.height;
    let mut divider = Path::new();
    divider
        .move_to(Point::new(-w / 2.0, divider_y))
        .line_to(Point::new(w / 2.0, divider_y));
    out.group.push(
        PathShape::new(divider, shape_style()).with_source(SourceRef::node(out.node.id.clone())),
    );
    Ok(out)
}
