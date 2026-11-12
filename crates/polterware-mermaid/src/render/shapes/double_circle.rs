//! Classic port of upstream `shapes/doubleCircle.ts`.
//!
//! The label is sized with `labelHelper` (wrapping off, minimum node width ignored); the
//! inner ring is that box's diagonal radius plus the label padding, and the outer ring adds
//! the look's gap. Both rings sit in the nested `basic label-container` group upstream
//! inserts, and intersections test the outer ring. Upstream's `outer-circle`/`inner-circle`
//! leaf classes have no scene equivalent (ellipses carry no class); the nesting keeps the
//! group role.
use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare_ignoring_min_width, shape_style,
};
use crate::scene::{EllipseShape, Item, Point, Rect, Size, SourceRef};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text_size) = prepare_ignoring_min_width(&node, ctx)?;

    // handDrawn: rough.js (later)
    let gap = if node.look == "neo" { 12.0 } else { 5.0 };
    let label_padding = if node.look == "neo" {
        16.0
    } else {
        node.padding
    };
    let label_radius = (text_size.width.powi(2) + text_size.height.powi(2)).sqrt() / 2.0;
    let inner_radius = node
        .width
        .filter(|width| *width != 0.0)
        .map(|width| width / 2.0)
        .unwrap_or(label_radius)
        + label_padding;
    let outer_radius = inner_radius + gap;

    node.bounds = Rect::centered(
        Point::ZERO,
        Size::new(outer_radius * 2.0, outer_radius * 2.0),
    );

    let mut circles = crate::scene::Group::new()
        .with_class("basic label-container")
        .with_source(SourceRef::node(node.id.clone()));
    circles.push(
        EllipseShape::new(Point::ZERO, outer_radius, outer_radius, shape_style())
            .with_source(SourceRef::node(node.id.clone())),
    );
    circles.push(
        EllipseShape::new(Point::ZERO, inner_radius, inner_radius, shape_style())
            .with_source(SourceRef::node(node.id.clone())),
    );
    group.push(circles);
    group.items.push(Item::Group(label));

    node.intersect = IntersectKind::Circle;
    Ok(RenderedNode { node, group })
}

// TODO(dedupe): shared differential-fixture support for the shape ports that need it. It
// lives here rather than in `shapes/mod.rs` because this run may not edit that file.
#[cfg(test)]
pub(crate) mod fixture {
    use serde::Deserialize;
    use serde_json::{Value, json};

    use crate::measure::{Font, TextMeasure};
    use crate::render::intersect::Intersectable;
    use crate::scene::{Group, Item, PathCommand, Point, Transform};
    use crate::theme::ThemeVariables;

    use super::{Node, RenderedNode, ShapeContext, ShapeError};

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        shape: String,
        padding: f64,
        label_size: LabelSize,
        cases: Vec<Case>,
    }

    #[derive(Deserialize)]
    struct LabelSize {
        width: f64,
        height: f64,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Case {
        label: String,
        primitives: Vec<Primitive>,
        label_transform: Option<[f64; 6]>,
        bounds: Bounds,
        intersects: Vec<IntersectCase>,
    }

    #[derive(Deserialize)]
    #[serde(tag = "kind", rename_all = "camelCase")]
    enum Primitive {
        Ellipse {
            cx: f64,
            cy: f64,
            rx: f64,
            ry: f64,
        },
        Rect {
            x: f64,
            y: f64,
            width: f64,
            height: f64,
            rx: f64,
            ry: f64,
        },
        Path {
            commands: Vec<Value>,
        },
    }

    #[derive(Deserialize)]
    struct Bounds {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    }

    #[derive(Deserialize)]
    struct IntersectCase {
        point: PointRecord,
        result: PointRecord,
    }

    #[derive(Deserialize)]
    struct PointRecord {
        x: f64,
        y: f64,
    }

    /// The fixture's fixed label measurement: any non-empty label is exactly this box.
    struct FixedMeasure {
        width: f64,
        height: f64,
    }

    impl TextMeasure for FixedMeasure {
        fn width(&self, _text: &str, _font: &Font) -> f64 {
            self.width
        }

        fn line_height(&self, _font: &Font) -> f64 {
            self.height
        }
    }

    #[derive(Debug)]
    enum ActualPrimitive {
        Ellipse {
            cx: f64,
            cy: f64,
            rx: f64,
            ry: f64,
        },
        Rect {
            x: f64,
            y: f64,
            width: f64,
            height: f64,
            rx: f64,
            ry: f64,
        },
        Path {
            commands: Vec<Value>,
        },
    }

    fn is_label(group: &Group) -> bool {
        group
            .class
            .as_deref()
            .is_some_and(|class| class.split_whitespace().any(|word| word == "label"))
    }

    fn collect(group: &Group, transform: Transform, out: &mut Vec<ActualPrimitive>) {
        let local = transform.then(group.transform);
        for item in &group.items {
            match item {
                Item::Group(group) => {
                    if !is_label(group) {
                        collect(group, local, out);
                    }
                }
                Item::Ellipse(ellipse) => {
                    let center = local.apply(ellipse.center);
                    out.push(ActualPrimitive::Ellipse {
                        cx: center.x,
                        cy: center.y,
                        rx: ellipse.radius_x,
                        ry: ellipse.radius_y,
                    });
                }
                Item::Rect(rect) => {
                    let top_left = local.apply(Point::new(rect.rect.x, rect.rect.y));
                    out.push(ActualPrimitive::Rect {
                        x: top_left.x,
                        y: top_left.y,
                        width: rect.rect.width,
                        height: rect.rect.height,
                        rx: rect.rx,
                        ry: rect.ry,
                    });
                }
                Item::Path(path) => {
                    let commands = path
                        .path
                        .commands
                        .iter()
                        .map(|command| command_value(*command, local))
                        .collect();
                    out.push(ActualPrimitive::Path { commands });
                }
                Item::Text(_) | Item::Image(_) => {}
            }
        }
    }

    fn command_value(command: PathCommand, transform: Transform) -> Value {
        let point = |x: f64, y: f64| {
            let p = transform.apply(Point::new(x, y));
            (p.x, p.y)
        };
        match command {
            PathCommand::MoveTo { x, y } => {
                let (x, y) = point(x, y);
                json!(["M", x, y])
            }
            PathCommand::LineTo { x, y } => {
                let (x, y) = point(x, y);
                json!(["L", x, y])
            }
            PathCommand::Close => json!(["Z"]),
            PathCommand::CubicTo { .. }
            | PathCommand::QuadTo { .. }
            | PathCommand::ArcTo { .. } => {
                panic!("the shape fixtures only carry move/line/close paths")
            }
        }
    }

    fn label_transform(group: &Group) -> Option<Transform> {
        if is_label(group) {
            return Some(group.transform);
        }
        group.items.iter().find_map(|item| match item {
            Item::Group(group) => label_transform(group),
            _ => None,
        })
    }

    fn assert_close(actual: f64, expected: f64, context: &str) {
        assert!(
            (actual - expected).abs() <= 0.01,
            "{context}: expected {expected}, got {actual}"
        );
    }

    fn assert_json_close(actual: &Value, expected: &Value, context: &str) {
        match (actual, expected) {
            (Value::Number(actual), Value::Number(expected)) => assert_close(
                actual.as_f64().expect("fixture number"),
                expected.as_f64().expect("fixture number"),
                context,
            ),
            (Value::String(actual), Value::String(expected)) => {
                assert_eq!(actual, expected, "{context}")
            }
            (Value::Array(actual), Value::Array(expected)) => {
                assert_eq!(actual.len(), expected.len(), "{context}");
                for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                    assert_json_close(actual, expected, &format!("{context}[{index}]"));
                }
            }
            _ => panic!("{context}: expected {expected:?}, got {actual:?}"),
        }
    }

    fn assert_primitive(actual: &ActualPrimitive, expected: &Primitive, case: &str) {
        match (actual, expected) {
            (
                ActualPrimitive::Ellipse { cx, cy, rx, ry },
                Primitive::Ellipse {
                    cx: ecx,
                    cy: ecy,
                    rx: erx,
                    ry: ery,
                },
            ) => {
                assert_close(*cx, *ecx, &format!("{case} ellipse cx"));
                assert_close(*cy, *ecy, &format!("{case} ellipse cy"));
                assert_close(*rx, *erx, &format!("{case} ellipse rx"));
                assert_close(*ry, *ery, &format!("{case} ellipse ry"));
            }
            (
                ActualPrimitive::Rect {
                    x,
                    y,
                    width,
                    height,
                    rx,
                    ry,
                },
                Primitive::Rect {
                    x: ex,
                    y: ey,
                    width: ew,
                    height: eh,
                    rx: erx,
                    ry: ery,
                },
            ) => {
                assert_close(*x, *ex, &format!("{case} rect x"));
                assert_close(*y, *ey, &format!("{case} rect y"));
                assert_close(*width, *ew, &format!("{case} rect width"));
                assert_close(*height, *eh, &format!("{case} rect height"));
                assert_close(*rx, *erx, &format!("{case} rect rx"));
                assert_close(*ry, *ery, &format!("{case} rect ry"));
            }
            (ActualPrimitive::Path { commands }, Primitive::Path { commands: expected }) => {
                assert_eq!(commands.len(), expected.len(), "{case} path length");
                for (index, (actual, expected)) in commands.iter().zip(expected).enumerate() {
                    assert_json_close(actual, expected, &format!("{case} path[{index}]"));
                }
            }
            _ => panic!("{case}: primitive kind mismatch"),
        }
    }

    /// Render every fixture case through `draw` and compare geometry, bounds, label placement
    /// and intersections with the captured upstream values.
    pub(crate) fn check(
        json: &str,
        draw: impl Fn(&Node, &ShapeContext<'_>) -> Result<RenderedNode, ShapeError>,
    ) {
        let fixture: Fixture = serde_json::from_str(json).expect("valid shape fixture");
        let measure = FixedMeasure {
            width: fixture.label_size.width,
            height: fixture.label_size.height,
        };
        let theme = ThemeVariables::default();
        let context = ShapeContext {
            measure: &measure,
            theme: &theme,
        };

        for case in &fixture.cases {
            let mut node = Node::new("n1", &case.label, &fixture.shape);
            node.padding = fixture.padding;
            node.look = "classic".to_string();

            let rendered = draw(&node, &context).expect("shape renders");

            assert_close(rendered.node.bounds.x, case.bounds.x, "bounds x");
            assert_close(rendered.node.bounds.y, case.bounds.y, "bounds y");
            assert_close(
                rendered.node.bounds.width,
                case.bounds.width,
                "bounds width",
            );
            assert_close(
                rendered.node.bounds.height,
                case.bounds.height,
                "bounds height",
            );

            let mut primitives = Vec::new();
            collect(&rendered.group, Transform::IDENTITY, &mut primitives);
            assert_eq!(
                primitives.len(),
                case.primitives.len(),
                "{} primitives",
                case.label
            );
            for (actual, expected) in primitives.iter().zip(&case.primitives) {
                assert_primitive(actual, expected, &case.label);
            }

            match (label_transform(&rendered.group), case.label_transform) {
                (Some(actual), Some(expected)) => {
                    let actual = [actual.a, actual.b, actual.c, actual.d, actual.e, actual.f];
                    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                        assert_close(*actual, expected, &format!("label transform[{index}]"));
                    }
                }
                (None, None) => {}
                _ => panic!("{}: label transform mismatch", case.label),
            }

            for intersect in &case.intersects {
                let point = Point::new(intersect.point.x, intersect.point.y);
                let actual = rendered.node.intersect(point);
                assert_close(actual.x, intersect.result.x, "intersect x");
                assert_close(actual.y, intersect.result.y, "intersect y");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_upstream_classic_fixture() {
        crate::render::shapes::double_circle::fixture::check(
            include_str!("../testdata/shapes/double_circle.json"),
            draw,
        );
    }
}
