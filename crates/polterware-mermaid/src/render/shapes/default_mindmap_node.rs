//! Classic port of upstream `shapes/defaultMindmapNode.ts`.
//!
//! Mindmap nodes carry a `type` used for the `node-<type>` class; `Node` has no such field,
//! so the class falls back to `node-default`. The classic path is the four-corner rounded
//! rectangle (the `neo` look flattens the bottom edge), with the same bottom rule and the
//! rectangular intersection.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Group, Item, Path, PathShape, Point, Rect, Size, SourceRef, Transform};

/// `const rd = 5` in defaultMindmapNode.
const RADIUS: f64 = 5.0;

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text_size) = prepare(&node, ctx)?;
    let half_padding = node.padding / 2.0;
    let w = text_size.width + 8.0 * half_padding;
    let h = text_size.height + 2.0 * half_padding;
    let bounds = Rect::centered(Point::ZERO, Size::new(w, h));
    node.bounds = bounds;

    let x0 = -w / 2.0;
    let mut path = Path::new();
    path.move_to(Point::new(x0, h / 2.0 - RADIUS))
        .line_to(Point::new(x0, -h / 2.0 + RADIUS))
        .quad_to(Point::new(x0, -h / 2.0), Point::new(x0 + RADIUS, -h / 2.0))
        .line_to(Point::new(w / 2.0 - RADIUS, -h / 2.0))
        .quad_to(
            Point::new(w / 2.0, -h / 2.0),
            Point::new(w / 2.0, -h / 2.0 + RADIUS),
        );
    if node.look == "neo" {
        path.line_to(Point::new(w / 2.0, h / 2.0))
            .line_to(Point::new(x0, h / 2.0));
    } else {
        path.line_to(Point::new(w / 2.0, h / 2.0 - RADIUS))
            .quad_to(
                Point::new(w / 2.0, h / 2.0),
                Point::new(w / 2.0 - RADIUS, h / 2.0),
            )
            .line_to(Point::new(x0 + RADIUS, h / 2.0))
            .quad_to(Point::new(x0, h / 2.0), Point::new(x0, h / 2.0 - RADIUS));
    }
    path.close();

    let mut shape = Group::new().with_class("node-bkg node-default");
    shape.push(PathShape::new(path, shape_style()).with_source(SourceRef::node(node.id.clone())));
    group.push(shape);

    let mut rule = Path::new();
    rule.move_to(Point::new(x0, h / 2.0))
        .line_to(Point::new(w / 2.0, h / 2.0));
    let mut rule_group = Group::new()
        .with_class("node-line-")
        .with_source(SourceRef::node(node.id.clone()));
    rule_group
        .push(PathShape::new(rule, shape_style()).with_source(SourceRef::node(node.id.clone())));
    group.push(rule_group);

    let mut label = label;
    label.transform = Transform::translate(-text_size.width / 2.0, -text_size.height / 2.0);
    group.push(Item::Group(label));

    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::render::intersect::Intersectable;
    use crate::scene::{Item, PathShape};
    use serde_json::Value;

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() <= 0.01, "{actual} != {expected}");
    }

    fn first_path(items: &[Item]) -> Option<&PathShape> {
        for item in items {
            match item {
                Item::Path(path) => return Some(path),
                Item::Group(group) => {
                    if let Some(path) = first_path(&group.items) {
                        return Some(path);
                    }
                }
                _ => {}
            }
        }
        None
    }

    #[test]
    fn matches_upstream_default_mindmap_node_fixtures() {
        let fixture: Value =
            serde_json::from_str(include_str!("../testdata/shapes/default_mindmap_node.json"))
                .unwrap();
        let ctx = ShapeContext {
            measure: &FallbackMeasure,
            theme: &crate::theme::ThemeVariables::default(),
        };
        for case in fixture["cases"].as_array().unwrap() {
            let label = case["label"].as_str().unwrap();
            let mut node = Node::new("n1", label, "defaultMindmapNode");
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

            let path = first_path(&rendered.group.items).expect("path");
            let commands = case["elements"][0]["commands"].as_array().unwrap();
            assert_eq!(path.path.commands.len(), commands.len());
            for (command, expected) in path.path.commands.iter().zip(commands) {
                match command {
                    crate::scene::PathCommand::MoveTo { x, y }
                    | crate::scene::PathCommand::LineTo { x, y } => {
                        close(*x, expected["x"].as_f64().unwrap());
                        close(*y, expected["y"].as_f64().unwrap());
                    }
                    crate::scene::PathCommand::QuadTo { x, y, .. } => {
                        close(*x, expected["x"].as_f64().unwrap());
                        close(*y, expected["y"].as_f64().unwrap());
                    }
                    crate::scene::PathCommand::Close => {}
                    other => panic!("unexpected command {other:?}"),
                }
            }
        }
    }
}
