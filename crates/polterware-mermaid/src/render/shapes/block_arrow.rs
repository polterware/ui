//! Classic port of upstream `shapes/blockArrow.ts`.
//!
//! The full direction table is ported here. `Node` has no `directions` field yet (the block
//! diagram's AST does), so `draw` follows upstream's default of no directions and
//! [`draw_with_directions`] is the entry point for a caller that has them.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Group, Item, Path, PathShape, Point, Rect, Size, SourceRef, Transform};

const DIRECTION_ORDER: [&str; 4] = ["right", "left", "up", "down"];

fn expand_directions(directions: &[&str]) -> Vec<&'static str> {
    let mut unique: Vec<&'static str> = Vec::new();
    for direction in directions {
        let expanded: &[&'static str] = match *direction {
            "x" => &["right", "left"],
            "y" => &["up", "down"],
            "right" => &["right"],
            "left" => &["left"],
            "up" => &["up"],
            "down" => &["down"],
            _ => &[],
        };
        for name in expanded {
            if !unique.contains(name) {
                unique.push(name);
            }
        }
    }
    unique
}

fn direction_key(directions: &[&str]) -> String {
    let expanded = expand_directions(directions);
    let key = DIRECTION_ORDER
        .iter()
        .filter(|name| expanded.contains(name))
        .copied()
        .collect::<Vec<_>>()
        .join("|");
    if key.is_empty() {
        "point".to_string()
    } else {
        key
    }
}

/// Upstream `getArrowPoints`: the polygon points for a set of block-arrow directions.
pub fn get_arrow_points(
    directions: &[&str],
    bbox: Size,
    padding: f64,
    total_width: Option<f64>,
) -> Vec<Point> {
    let padding = padding / 2.0;
    let height = bbox.height + 4.0 * padding;
    let midpoint = height / 2.0;
    let width = total_width.unwrap_or(bbox.width + 2.0 * midpoint + 2.0 * padding);
    let p = |x: f64, y: f64| Point::new(x, y);
    match direction_key(directions).as_str() {
        "right|left|up|down" => vec![
            p(0.0, 0.0),
            p(midpoint, 0.0),
            p(width / 2.0, 2.0 * padding),
            p(width - midpoint, 0.0),
            p(width, 0.0),
            p(width, -height / 3.0),
            p(width + 2.0 * padding, -height / 2.0),
            p(width, (-2.0 * height) / 3.0),
            p(width, -height),
            p(width - midpoint, -height),
            p(width / 2.0, -height - 2.0 * padding),
            p(midpoint, -height),
            p(0.0, -height),
            p(0.0, (-2.0 * height) / 3.0),
            p(-2.0 * padding, -height / 2.0),
            p(0.0, -height / 3.0),
        ],
        "right|left|up" => vec![
            p(midpoint, 0.0),
            p(width - midpoint, 0.0),
            p(width, -height / 2.0),
            p(width - midpoint, -height),
            p(midpoint, -height),
            p(0.0, -height / 2.0),
        ],
        "right|left|down" => vec![
            p(0.0, 0.0),
            p(midpoint, -height),
            p(width - midpoint, -height),
            p(width, 0.0),
        ],
        "right|up|down" => vec![
            p(0.0, 0.0),
            p(width, -midpoint),
            p(width, -height + midpoint),
            p(0.0, -height),
        ],
        "left|up|down" => vec![
            p(width, 0.0),
            p(0.0, -midpoint),
            p(0.0, -height + midpoint),
            p(width, -height),
        ],
        "right|left" => vec![
            p(midpoint, 0.0),
            p(midpoint, -padding),
            p(width - midpoint, -padding),
            p(width - midpoint, 0.0),
            p(width, -height / 2.0),
            p(width - midpoint, -height),
            p(width - midpoint, -height + padding),
            p(midpoint, -height + padding),
            p(midpoint, -height),
            p(0.0, -height / 2.0),
        ],
        "up|down" => vec![
            p(width / 2.0, 0.0),
            p(0.0, -padding),
            p(midpoint, -padding),
            p(midpoint, -height + padding),
            p(0.0, -height + padding),
            p(width / 2.0, -height),
            p(width, -height + padding),
            p(width - midpoint, -height + padding),
            p(width - midpoint, -padding),
            p(width, -padding),
        ],
        "right|up" => vec![p(0.0, 0.0), p(width, -midpoint), p(0.0, -height)],
        "right|down" => vec![p(0.0, 0.0), p(width, 0.0), p(0.0, -height)],
        "left|up" => vec![p(width, 0.0), p(0.0, -midpoint), p(width, -height)],
        "left|down" => vec![p(width, 0.0), p(0.0, 0.0), p(width, -height)],
        "right" => vec![
            p(midpoint, -padding),
            p(midpoint, -padding),
            p(width - midpoint, -padding),
            p(width - midpoint, 0.0),
            p(width, -height / 2.0),
            p(width - midpoint, -height),
            p(width - midpoint, -height + padding),
            p(midpoint, -height + padding),
            p(midpoint, -height + padding),
        ],
        "left" => vec![
            p(midpoint, 0.0),
            p(midpoint, -padding),
            p(width - midpoint, -padding),
            p(width - midpoint, -height + padding),
            p(midpoint, -height + padding),
            p(midpoint, -height),
            p(0.0, -height / 2.0),
        ],
        "up" => vec![
            p(midpoint, -padding),
            p(midpoint, -height + padding),
            p(0.0, -height + padding),
            p(width / 2.0, -height),
            p(width, -height + padding),
            p(width - midpoint, -height + padding),
            p(width - midpoint, -padding),
        ],
        "down" => vec![
            p(width / 2.0, 0.0),
            p(0.0, -padding),
            p(midpoint, -padding),
            p(midpoint, -height + padding),
            p(width - midpoint, -height + padding),
            p(width - midpoint, -padding),
            p(width, -padding),
        ],
        _ => vec![p(0.0, 0.0)],
    }
}

fn finish(
    node: Node,
    group: Group,
    label: Group,
    text_size: Size,
    width: f64,
    height: f64,
    arrow_points: Vec<Point>,
) -> RenderedNode {
    let mut node = node;
    let points: Vec<Point> = arrow_points
        .iter()
        .map(|point| Point::new(point.x - width / 2.0, point.y + height / 2.0))
        .collect();
    let min_x = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let max_x = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let min_y = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let max_y = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    let bounds_width = max_x - min_x;
    let bounds_height = max_y - min_y;
    node.bounds = Rect::centered(Point::ZERO, Size::new(bounds_width, bounds_height));

    let mut group = group;
    let mut shape = Group::new().with_class("label-container");
    shape.push(
        PathShape::new(Path::polygon(points.iter().copied()), shape_style())
            .with_source(SourceRef::node(node.id.clone())),
    );
    group.push(shape);

    let mut label = label;
    label.transform = Transform::translate(-text_size.width / 2.0, -text_size.height / 2.0);
    group.push(Item::Group(label));

    node.intersect = IntersectKind::Polygon(arrow_points);
    RenderedNode { node, group }
}

/// Draws a block arrow with an explicit direction list (upstream's `blockNode.directions`).
pub fn draw_with_directions(
    node: &Node,
    ctx: &ShapeContext<'_>,
    directions: &[&str],
) -> Result<RenderedNode, ShapeError> {
    let node = node.clone();
    let (group, label, text_size) = prepare(&node, ctx)?;
    let node_padding = node.padding.max(0.0);
    let height = text_size.height + 2.0 * node_padding;
    let midpoint = height / 2.0;
    let natural_width = text_size.width + 2.0 * midpoint + node_padding;
    let width = natural_width;
    let points = get_arrow_points(directions, text_size, node_padding, Some(width));
    Ok(finish(node, group, label, text_size, width, height, points))
}

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    draw_with_directions(node, ctx, &[])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::render::intersect::Intersectable;
    use crate::scene::Item;
    use serde_json::Value;

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() <= 0.01, "{actual} != {expected}");
    }

    fn polygon_path(items: &[Item]) -> Option<&PathShape> {
        for item in items {
            if let Item::Group(group) = item
                && group.class.as_deref() == Some("label-container")
                && let Some(Item::Path(path)) = group.items.first()
            {
                return Some(path);
            }
        }
        None
    }

    #[test]
    fn matches_upstream_block_arrow_fixtures() {
        let fixture: Value =
            serde_json::from_str(include_str!("../testdata/shapes/block_arrow.json")).unwrap();
        let ctx = ShapeContext {
            measure: &FallbackMeasure,
            theme: &crate::theme::ThemeVariables::default(),
        };
        for case in fixture["cases"].as_array().unwrap() {
            let label = case["label"].as_str().unwrap();
            let direction_names: Vec<String> = case
                .get("directions")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|value| value.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let direction_refs: Vec<&str> = direction_names.iter().map(String::as_str).collect();
            let mut node = Node::new("n1", label, "block_arrow");
            node.padding = 15.0;
            node.look = "classic".into();
            let rendered = draw_with_directions(&node, &ctx, &direction_refs).unwrap();
            close(
                rendered.node.bounds.width,
                case["bbox"]["width"].as_f64().unwrap(),
            );
            close(
                rendered.node.bounds.height,
                case["bbox"]["height"].as_f64().unwrap(),
            );
            for (index, query) in [(0, Point::new(1000.0, 0.0)), (1, Point::new(0.0, -1000.0))] {
                let hit = rendered.node.intersect(query);
                close(hit.x, case["intersects"][index]["x"].as_f64().unwrap());
                close(hit.y, case["intersects"][index]["y"].as_f64().unwrap());
            }

            let path = polygon_path(&rendered.group.items).expect("polygon");
            let expected = case["elements"][0]["world_points"].as_array().unwrap();
            let points: Vec<Point> = path
                .path
                .commands
                .iter()
                .filter_map(|c| c.end_point())
                .collect();
            assert_eq!(points.len(), expected.len());
            for (point, expected) in points.iter().zip(expected) {
                close(point.x, expected[0].as_f64().unwrap());
                close(point.y, expected[1].as_f64().unwrap());
            }
        }
    }
}
