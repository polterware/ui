//! Classic port of upstream `shapes/bucket.ts`: an open-top container narrowing to a curved
//! base, with an elliptical rim and the label centred in the body.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{
    Color, EllipseShape, Group, Item, Path, PathShape, PathStyle, Point, Rect, Size, SourceRef,
    Transform,
};

/// Minimum bucket width.
const MIN_WIDTH: f64 = 80.0;
/// Base width as a fraction of the rim width.
const BASE_WIDTH_RATIO: f64 = 0.72;
/// Number of segments in each half of the intersection outline.
const ARC_SEGMENTS: usize = 12;

fn accent(ctx: &ShapeContext<'_>) -> Option<String> {
    ctx.theme
        .color("nodeBorder")
        .or_else(|| ctx.theme.color("lineColor"))
        .map(str::to_string)
}

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (group, label, text_size) = prepare(&node, ctx)?;
    let accent = accent(ctx);
    let padding = node.padding;
    let w = (text_size.width + padding * 2.0)
        .max(node.width.unwrap_or(0.0))
        .max(MIN_WIDTH);
    let rim_ry = (w * 0.08).clamp(5.0, 12.0);
    let total_height = (text_size.height + padding * 2.0 + rim_ry).max(node.height.unwrap_or(0.0));
    let top_y = -total_height / 2.0 + rim_ry;
    let bottom_y = total_height / 2.0;
    let bottom_w = w * BASE_WIDTH_RATIO;

    let bounds = Rect::centered(Point::ZERO, Size::new(w, total_height + rim_ry));
    node.bounds = bounds;

    let mut body = Path::new();
    body.move_to(Point::new(-w / 2.0, top_y))
        .line_to(Point::new(-bottom_w / 2.0, bottom_y))
        .arc_to(
            bottom_w / 2.0,
            rim_ry,
            0.0,
            false,
            false,
            Point::new(bottom_w / 2.0, bottom_y),
        )
        .line_to(Point::new(w / 2.0, top_y))
        .arc_to(
            w / 2.0,
            rim_ry,
            0.0,
            false,
            false,
            Point::new(-w / 2.0, top_y),
        )
        .close();

    let mut group = group;
    let mut shape = Group::new().with_class("basic label-container");
    // handDrawn: rough.js (later)
    shape.push(PathShape::new(body, shape_style()).with_source(SourceRef::node(node.id.clone())));

    let rim_style = PathStyle {
        fill: Some(Color::none()),
        stroke: Some(Color::new(
            accent.clone().unwrap_or_else(|| "currentColor".to_string()),
        )),
        ..PathStyle::default()
    };
    shape.push(
        EllipseShape::new(Point::new(0.0, top_y), w / 2.0, rim_ry, rim_style)
            .with_source(SourceRef::node(node.id.clone())),
    );
    group.push(shape);

    let body_center_y = top_y + (bottom_y - top_y) / 2.0;
    let mut label = label;
    label.transform = Transform::translate(
        -text_size.width / 2.0,
        body_center_y - text_size.height / 2.0,
    );
    group.push(Item::Group(label));

    let arc = |rx: f64, cy: f64, y_sign: f64| -> Vec<Point> {
        (0..=ARC_SEGMENTS)
            .map(|index| {
                let theta = std::f64::consts::PI
                    - index as f64 * std::f64::consts::PI / ARC_SEGMENTS as f64;
                Point::new(rx * theta.cos(), cy + y_sign * rim_ry * theta.sin())
            })
            .collect()
    };
    let mut outline = arc(w / 2.0, top_y, -1.0);
    let mut bottom_arc = arc(bottom_w / 2.0, bottom_y, 1.0);
    bottom_arc.reverse();
    outline.extend(bottom_arc);
    node.intersect = IntersectKind::Polygon(outline);

    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::render::intersect::Intersectable;
    use crate::scene::{EllipseShape, Item, PathCommand, PathShape};
    use serde_json::Value;

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() <= 0.01, "{actual} != {expected}");
    }

    fn collect<'a>(
        items: &'a [Item],
        paths: &mut Vec<&'a PathShape>,
        ellipses: &mut Vec<&'a EllipseShape>,
    ) {
        for item in items {
            match item {
                Item::Path(path) => paths.push(path),
                Item::Ellipse(ellipse) => ellipses.push(ellipse),
                Item::Group(group) => collect(&group.items, paths, ellipses),
                _ => {}
            }
        }
    }

    #[test]
    fn matches_upstream_bucket_fixtures() {
        let fixture: Value =
            serde_json::from_str(include_str!("../testdata/shapes/bucket.json")).unwrap();
        let ctx = ShapeContext {
            measure: &FallbackMeasure,
            theme: &crate::theme::ThemeVariables::default(),
        };
        for case in fixture["cases"].as_array().unwrap() {
            let label = case["label"].as_str().unwrap();
            let mut node = Node::new("n1", label, "bucket");
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
            for (index, query) in [(0, Point::new(1000.0, 0.0)), (1, Point::new(0.0, -1000.0))] {
                let hit = rendered.node.intersect(query);
                close(hit.x, case["intersects"][index]["x"].as_f64().unwrap());
                close(hit.y, case["intersects"][index]["y"].as_f64().unwrap());
            }

            let (mut paths, mut ellipses) = (Vec::new(), Vec::new());
            collect(&rendered.group.items, &mut paths, &mut ellipses);
            let path = paths.first().expect("body");
            let commands = case["elements"][0]["commands"].as_array().unwrap();
            assert_eq!(path.path.commands.len(), commands.len());
            for (command, expected) in path.path.commands.iter().zip(commands) {
                match command {
                    PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => {
                        close(*x, expected["x"].as_f64().unwrap());
                        close(*y, expected["y"].as_f64().unwrap());
                    }
                    PathCommand::ArcTo {
                        rx,
                        ry,
                        large_arc,
                        sweep,
                        x,
                        y,
                        ..
                    } => {
                        close(*rx, expected["rx"].as_f64().unwrap());
                        close(*ry, expected["ry"].as_f64().unwrap());
                        assert_eq!(*large_arc, expected["large"].as_f64().unwrap() == 1.0);
                        assert_eq!(*sweep, expected["sweep"].as_f64().unwrap() == 1.0);
                        close(*x, expected["x"].as_f64().unwrap());
                        close(*y, expected["y"].as_f64().unwrap());
                    }
                    PathCommand::Close => {}
                    other => panic!("unexpected command {other:?}"),
                }
            }

            let ellipse = ellipses.first().expect("rim");
            let rim = case["elements"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["tag"] == "ellipse")
                .unwrap();
            close(ellipse.center.x, rim["cx"].as_f64().unwrap());
            close(ellipse.center.y, rim["cy"].as_f64().unwrap());
            close(ellipse.radius_x, rim["rx"].as_f64().unwrap());
            close(ellipse.radius_y, rim["ry"].as_f64().unwrap());
        }
    }
}
