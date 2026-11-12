//! Port of upstream `shapes/bpmnShapeCore.ts`: the shared BPMN notation constants, label
//! placement, reserved bounds, face/dial intersection rules, and glyph placement.
//!
//! The upstream module is a helper, not a registered shape; its [`draw`] entry keeps the
//! registry's "not a shape" error. The classic look uses the paths the shapes draw themselves:
//! upstream never calls rough.js here, so there is no hand-drawn variant to defer.

use super::{
    Node, RenderedNode, ShapeContext, ShapeError, bpmn_icons, get_node_classes, shape_style,
};
use crate::measure::{Font, TextOptions, shape_text};
use crate::scene::{
    EllipseShape, Group, Path, PathShape, PathStyle, Point, Rect, RectShape, Size, SourceRef,
    TextRun, Transform,
};

pub const EVENT_DIAMETER: f64 = 36.0;
pub const GATEWAY_SIZE: f64 = 50.0;
pub const ACTIVITY_WIDTH: f64 = 100.0;
pub const ACTIVITY_HEIGHT: f64 = 80.0;
pub const LABEL_GAP: f64 = 6.0;
pub const ICON_SIZE: f64 = 16.0;
pub const BPMN_ICON_PREFIX: &str = "bpmn";

/// The face an edge docks on, upstream `ShapeFace`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeFace {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EventRings {
    pub rings: u8,
    pub stroke_width: f64,
}

/// `EVENT_RINGS`: the ring count and inline stroke weight of each event position.
pub fn event_rings(position: &str) -> Option<EventRings> {
    match position {
        "start" => Some(EventRings {
            rings: 1,
            stroke_width: 1.6,
        }),
        "intermediate" | "boundary" => Some(EventRings {
            rings: 2,
            stroke_width: 1.4,
        }),
        "end" => Some(EventRings {
            rings: 1,
            stroke_width: 3.4,
        }),
        _ => None,
    }
}

/// The face a point projects onto, upstream `faceProjectIntersect`'s selection.
pub fn face_for(width: f64, height: f64, point: Point) -> ShapeFace {
    let half_width = width / 2.0;
    let half_height = height / 2.0;
    if point.x.abs() * half_height > point.y.abs() * half_width {
        if point.x >= 0.0 {
            ShapeFace::Right
        } else {
            ShapeFace::Left
        }
    } else if point.y >= 0.0 {
        ShapeFace::Bottom
    } else {
        ShapeFace::Top
    }
}

fn corner_reach(half: f64) -> f64 {
    (half - 8.0_f64.min(half / 2.0)).max(0.0)
}

/// `faceProjectIntersect`: project onto a face, abreast of the query and kept clear of corners.
pub fn face_project_intersect_at(
    center: Point,
    width: f64,
    height: f64,
    point: Point,
    fixed_face: Option<ShapeFace>,
) -> Point {
    let half_width = width / 2.0;
    let half_height = height / 2.0;
    let face = fixed_face.unwrap_or_else(|| {
        face_for(
            width,
            height,
            Point::new(point.x - center.x, point.y - center.y),
        )
    });
    match face {
        ShapeFace::Left | ShapeFace::Right => Point::new(
            center.x
                + if face == ShapeFace::Right {
                    half_width
                } else {
                    -half_width
                },
            point.y.clamp(
                center.y - corner_reach(half_height),
                center.y + corner_reach(half_height),
            ),
        ),
        ShapeFace::Top | ShapeFace::Bottom => Point::new(
            point.x.clamp(
                center.x - corner_reach(half_width),
                center.x + corner_reach(half_width),
            ),
            center.y
                + if face == ShapeFace::Bottom {
                    half_height
                } else {
                    -half_height
                },
        ),
    }
}

/// The centred form kept for callers that only need the default face choice.
pub fn face_project_intersect(center: Point, width: f64, height: f64, point: Point) -> Point {
    face_project_intersect_at(center, width, height, point, None)
}

/// `ringIntersect`: dock on the notation circle, sliding only to 85% of its radius.
pub fn ring_intersect(center: Point, diameter: f64, point: Point) -> Point {
    let radius = diameter / 2.0;
    let dx = point.x - center.x;
    let dy = point.y - center.y;
    let reach = radius * 0.85;
    if dy.abs() >= dx.abs() {
        let along = dx.clamp(-reach, reach);
        Point::new(
            center.x + along,
            center.y
                + if dy == 0.0 { 1.0 } else { dy.signum() }
                    * (radius * radius - along * along).sqrt(),
        )
    } else {
        let along = dy.clamp(-reach, reach);
        Point::new(
            center.x
                + if dx == 0.0 { 1.0 } else { dx.signum() }
                    * (radius * radius - along * along).sqrt(),
            center.y + along,
        )
    }
}

/// `faceCentreIntersect`: dock on the middle of the nearest face, as a gateway expects.
pub fn face_centre_intersect(center: Point, width: f64, height: f64, point: Point) -> Point {
    let half_width = width / 2.0;
    let half_height = height / 2.0;
    let dx = point.x - center.x;
    let dy = point.y - center.y;
    if dx.abs() * half_height > dy.abs() * half_width {
        Point::new(center.x + dx.signum() * half_width, center.y)
    } else {
        Point::new(
            center.x,
            center.y + if dy == 0.0 { 1.0 } else { dy.signum() } * half_height,
        )
    }
}

/// `positionLabelBelow`: centre the caption under the notation and offset it by `center_y`.
pub fn position_label_below(label: &mut Group, width: f64, height: f64, center_y: f64) {
    label.transform = Transform::translate(-width / 2.0, center_y - height / 2.0);
}

/// A measured caption, the `bbox` and `label` `labelHelper` returns.
pub struct BpmnLabel {
    pub group: Group,
    pub width: f64,
    pub height: f64,
}

/// `labelHelper` for the SVG (non-HTML) label path: measure at `wrap_width`, keep the label
/// group's `translate(0, -height / 2)` until the shape positions it.
pub fn bpmn_label(
    node: &Node,
    ctx: &ShapeContext<'_>,
    wrap_width: f64,
) -> Result<BpmnLabel, ShapeError> {
    let font_size = ctx
        .theme
        .color("fontSize")
        .and_then(|s| s.trim_end_matches("px").parse::<f64>().ok())
        .or_else(|| ctx.theme.number("fontSize"))
        .unwrap_or(12.0);
    let font_family = ctx.theme.color("fontFamily").unwrap_or("Arial").to_string();
    let font = Font {
        family: font_family,
        size: font_size,
        weight: crate::scene::NORMAL_WEIGHT,
        italic: false,
    };
    let has_explicit_width = node.width.is_some_and(|width| width != 0.0);
    let min_width = if !node.label.is_empty() && !has_explicit_width {
        node.min_width.unwrap_or(0.0)
    } else {
        0.0
    };
    let block = shape_text(
        &node.label,
        &TextOptions {
            width: wrap_width,
            markdown: false,
            min_width,
        },
        &font,
        ctx.measure,
    )
    .map_err(|error| ShapeError {
        message: error.to_string(),
    })?;
    let mut group = Group::translated(0.0, -block.size.height / 2.0).with_class("label");
    group.source = Some(SourceRef::node(node.id.clone()));
    group.push(TextRun::new(Point::ZERO, font, block.clone()));
    Ok(BpmnLabel {
        group,
        width: block.size.width,
        height: block.size.height,
    })
}

/// The label wrapping width `labelHelper` reads: an explicit width, else the flowchart default.
pub fn label_wrap_width(node: &Node) -> f64 {
    node.width.filter(|width| *width != 0.0).unwrap_or(120.0)
}

/// `reserveBounds`: an invisible rect that fixes the node's box without painting.
pub fn reserve_bounds(node: &mut Node, width: f64, height: f64) -> Group {
    node.bounds = Rect::centered(Point::ZERO, Size::new(width, height));
    let mut style = shape_style();
    style.opacity = 0.0;
    let mut group = Group::new().with_class("bpmn-bounds");
    group.push(
        RectShape::new(
            Rect::new(-width / 2.0, -height / 2.0, width, height),
            0.0,
            style,
        )
        .with_source(SourceRef::node(node.id.clone())),
    );
    group
}

/// `updateNodeBounds(node, body)` with the body's precomputed box.
pub fn update_bounds(node: &mut Node, width: f64, height: f64) {
    node.bounds = Rect::centered(Point::ZERO, Size::new(width, height));
}

/// A stroked ring with the inline weight `append('circle').style('stroke-width', ...)` sets.
pub fn ring_style(stroke_width: f64) -> PathStyle {
    let mut style = shape_style();
    style.stroke_width = stroke_width;
    style
}

/// `appendGlyph`: scale the 24-unit glyph to `size` and centre it on the origin.
pub fn append_glyph(parent: &mut Group, icon: Option<&str>, size: f64) {
    if let Some(name) = icon
        && let Some(glyph) = bpmn_icons::size_glyph(name, size)
    {
        parent.push(glyph);
    }
}

/// A glyph placed at `origin` (the uncentred form an activity corner and marker cell use).
pub fn append_glyph_at(parent: &mut Group, icon: Option<&str>, size: f64, origin: Point) {
    if let Some(name) = icon
        && let Some(mut glyph) = bpmn_icons::glyph_group(name)
    {
        glyph.transform = Transform::scale(
            size / bpmn_icons::GLYPH_VIEWBOX,
            size / bpmn_icons::GLYPH_VIEWBOX,
        )
        .then(Transform::translate(origin.x, origin.y));
        parent.push(glyph);
    }
}

/// The outer `<g class="node ...">` a BPMN shape handler inserts.
pub fn bpmn_group(node: &Node, extra: &str) -> Group {
    Group::new()
        .with_class(get_node_classes(node, Some(extra)))
        .with_source(SourceRef::node(node.id.clone()))
}

/// A circle item wrapped in the upstream element class, so the class survives in the scene.
pub fn classed_circle(
    class: &str,
    center: Point,
    radius: f64,
    style: PathStyle,
    source: SourceRef,
) -> Group {
    let mut group = Group::new().with_class(class);
    group.push(EllipseShape::new(center, radius, radius, style).with_source(source));
    group
}

/// A path item wrapped in the upstream element class, painted with the node's face style.
pub fn classed_path(class: &str, path: Path, source: SourceRef) -> Group {
    let mut group = Group::new().with_class(class);
    group.push(PathShape::new(path, shape_style()).with_source(source));
    group
}

// Compatibility entry for the current registry; this upstream module is a helper, not a shape.
pub fn draw(node: &Node, _ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    Err(ShapeError {
        message: format!("{} is a rendering helper, not a shape", node.shape),
    })
}

#[cfg(test)]
pub(crate) mod fixture_support {
    use super::*;
    use crate::measure::TextMeasure;
    use crate::scene::{Item, Path, PathCommand};
    use serde::Deserialize;

    pub const TOLERANCE: f64 = 0.01;

    #[derive(Debug, Deserialize)]
    pub struct Fixture {
        pub shape: String,
        pub measure: MeasureSpec,
        pub cases: Vec<Case>,
    }

    #[derive(Debug, Deserialize)]
    pub struct MeasureSpec {
        pub char_width: f64,
        pub line_height: f64,
    }

    #[derive(Debug, Deserialize)]
    pub struct Case {
        pub label: String,
        #[allow(dead_code)]
        pub wrapping_width: f64,
        pub node: NodeBox,
        pub label_transform: Vector,
        pub label_size: SizeBox,
        pub elements: Vec<Element>,
        pub intersects: Vec<Intersection>,
    }

    #[derive(Debug, Deserialize)]
    pub struct NodeBox {
        pub width: f64,
        pub height: f64,
    }

    #[derive(Debug, Deserialize)]
    pub struct Vector {
        pub x: f64,
        pub y: f64,
    }

    #[derive(Debug, Deserialize)]
    pub struct SizeBox {
        pub width: f64,
        pub height: f64,
    }

    #[derive(Debug, Deserialize)]
    pub struct Intersection {
        pub point: Vector,
        pub result: Vector,
    }

    #[derive(Debug, Deserialize)]
    #[serde(tag = "tag")]
    pub enum Element {
        #[serde(rename = "rect")]
        Rect {
            #[serde(rename = "class")]
            class: String,
            x: f64,
            y: f64,
            width: f64,
            height: f64,
            rx: f64,
            opacity: f64,
        },
        #[serde(rename = "circle")]
        Circle {
            #[serde(rename = "class")]
            class: String,
            cx: f64,
            cy: f64,
            r: f64,
            stroke_width: f64,
        },
        #[serde(rename = "path")]
        Path {
            #[serde(rename = "class")]
            class: String,
            d: String,
        },
        #[serde(rename = "polygon")]
        Polygon {
            #[serde(rename = "class")]
            class: String,
            points: String,
        },
    }

    pub struct FixedMeasure {
        pub char_width: f64,
        pub line_height: f64,
    }

    impl TextMeasure for FixedMeasure {
        fn width(&self, text: &str, _font: &Font) -> f64 {
            text.chars().count() as f64 * self.char_width
        }

        fn line_height(&self, _font: &Font) -> f64 {
            self.line_height
        }
    }

    pub fn load(data: &str) -> Fixture {
        serde_json::from_str(data).expect("valid generated BPMN fixture")
    }

    impl Fixture {
        pub fn measure(&self) -> FixedMeasure {
            FixedMeasure {
                char_width: self.measure.char_width,
                line_height: self.measure.line_height,
            }
        }

        pub fn node(&self, case: &Case) -> Node {
            let mut node = Node::new("bpmn-node", case.label.clone(), self.shape.clone());
            node.css_classes = format!("bpmn-{}", self.shape);
            node.look = "classic".into();
            node
        }
    }

    fn close(actual: f64, expected: f64, what: &str) {
        assert!(
            (actual - expected).abs() <= TOLERANCE,
            "{what}: expected {expected}, got {actual}"
        );
    }

    pub fn assert_node(case: &Case, rendered: &RenderedNode) {
        close(rendered.node.bounds.width, case.node.width, "node width");
        close(rendered.node.bounds.height, case.node.height, "node height");
    }

    fn find_group<'a>(group: &'a Group, class: &str) -> Option<&'a Group> {
        for item in &group.items {
            if let Item::Group(child) = item {
                if child.class.as_deref() == Some(class) {
                    return Some(child);
                }
                if let Some(found) = find_group(child, class) {
                    return Some(found);
                }
            }
        }
        None
    }

    pub fn assert_label(case: &Case, group: &Group) {
        let label = find_group(group, "label").expect("label group");
        close(label.transform.e, case.label_transform.x, "label x");
        close(label.transform.f, case.label_transform.y, "label y");
        let text = label
            .items
            .iter()
            .find_map(|item| match item {
                Item::Text(text) => Some(text),
                _ => None,
            })
            .expect("label text");
        close(text.size.width, case.label_size.width, "label width");
        close(text.size.height, case.label_size.height, "label height");
    }

    enum Actual {
        Rect {
            class: String,
            x: f64,
            y: f64,
            width: f64,
            height: f64,
            rx: f64,
            opacity: f64,
        },
        Circle {
            class: String,
            cx: f64,
            cy: f64,
            r: f64,
            stroke_width: f64,
        },
        Path {
            class: String,
            path: Path,
        },
    }

    fn collect(group: &Group, class: Option<&str>, out: &mut Vec<Actual>) {
        let class = group.class.as_deref().or(class);
        for item in &group.items {
            match item {
                Item::Group(child) => {
                    if child.class.as_deref() == Some("label") {
                        continue;
                    }
                    collect(child, child.class.as_deref().or(class), out);
                }
                Item::Rect(rect) => out.push(Actual::Rect {
                    class: class.unwrap_or("").to_string(),
                    x: rect.rect.x,
                    y: rect.rect.y,
                    width: rect.rect.width,
                    height: rect.rect.height,
                    rx: rect.rx,
                    opacity: rect.style.opacity,
                }),
                Item::Ellipse(ellipse) => out.push(Actual::Circle {
                    class: class.unwrap_or("").to_string(),
                    cx: ellipse.center.x,
                    cy: ellipse.center.y,
                    r: ellipse.radius_x,
                    stroke_width: ellipse.style.stroke_width,
                }),
                Item::Path(path) => out.push(Actual::Path {
                    class: class.unwrap_or("").to_string(),
                    path: path.path.clone(),
                }),
                Item::Text(_) | Item::Image(_) => {}
            }
        }
    }

    fn expected_path(element: &Element) -> Option<Path> {
        match element {
            Element::Path { d, .. } => Some(bpmn_icons::parse_path_data(d)),
            Element::Polygon { points, .. } => {
                let numbers: Vec<f64> = points
                    .split(|c: char| c.is_whitespace() || c == ',')
                    .filter(|part| !part.is_empty())
                    .filter_map(|part| part.parse().ok())
                    .collect();
                let points: Vec<Point> = numbers
                    .chunks_exact(2)
                    .map(|pair| Point::new(pair[0], pair[1]))
                    .collect();
                Some(Path::polygon(points))
            }
            _ => None,
        }
    }

    fn assert_path(actual: &Path, expected: &Path, class: &str) {
        assert_eq!(
            actual.commands.len(),
            expected.commands.len(),
            "{class} path command count"
        );
        for (actual, expected) in actual.commands.iter().zip(&expected.commands) {
            match (actual, expected) {
                (PathCommand::Close, PathCommand::Close) => {}
                (PathCommand::MoveTo { x, y }, PathCommand::MoveTo { x: ex, y: ey })
                | (PathCommand::LineTo { x, y }, PathCommand::LineTo { x: ex, y: ey }) => {
                    close(*x, *ex, &format!("{class} x"));
                    close(*y, *ey, &format!("{class} y"));
                }
                (
                    PathCommand::CubicTo {
                        c1x,
                        c1y,
                        c2x,
                        c2y,
                        x,
                        y,
                    },
                    PathCommand::CubicTo {
                        c1x: e1x,
                        c1y: e1y,
                        c2x: e2x,
                        c2y: e2y,
                        x: ex,
                        y: ey,
                    },
                ) => {
                    close(*c1x, *e1x, &format!("{class} c1x"));
                    close(*c1y, *e1y, &format!("{class} c1y"));
                    close(*c2x, *e2x, &format!("{class} c2x"));
                    close(*c2y, *e2y, &format!("{class} c2y"));
                    close(*x, *ex, &format!("{class} x"));
                    close(*y, *ey, &format!("{class} y"));
                }
                (
                    PathCommand::QuadTo { cx, cy, x, y },
                    PathCommand::QuadTo {
                        cx: ecx,
                        cy: ecy,
                        x: ex,
                        y: ey,
                    },
                ) => {
                    close(*cx, *ecx, &format!("{class} cx"));
                    close(*cy, *ecy, &format!("{class} cy"));
                    close(*x, *ex, &format!("{class} x"));
                    close(*y, *ey, &format!("{class} y"));
                }
                (
                    PathCommand::ArcTo {
                        rx,
                        ry,
                        rotation,
                        large_arc,
                        sweep,
                        x,
                        y,
                    },
                    PathCommand::ArcTo {
                        rx: erx,
                        ry: ery,
                        rotation: erotation,
                        large_arc: elarge_arc,
                        sweep: esweep,
                        x: ex,
                        y: ey,
                    },
                ) => {
                    close(*rx, *erx, &format!("{class} rx"));
                    close(*ry, *ery, &format!("{class} ry"));
                    close(*rotation, *erotation, &format!("{class} rotation"));
                    assert_eq!(large_arc, elarge_arc, "{class} large-arc flag");
                    assert_eq!(sweep, esweep, "{class} sweep flag");
                    close(*x, *ex, &format!("{class} x"));
                    close(*y, *ey, &format!("{class} y"));
                }
                _ => panic!("{class} path command kind differs"),
            }
        }
    }

    pub fn assert_elements(case: &Case, group: &Group) {
        let mut actual = Vec::new();
        collect(group, None, &mut actual);
        assert_eq!(
            actual.len(),
            case.elements.len(),
            "drawn element count; expected {:?}",
            case.elements
        );
        for (actual, expected) in actual.iter().zip(&case.elements) {
            match (actual, expected) {
                (
                    Actual::Rect {
                        class,
                        x,
                        y,
                        width,
                        height,
                        rx,
                        opacity,
                    },
                    Element::Rect {
                        class: expected_class,
                        x: ex,
                        y: ey,
                        width: ewidth,
                        height: eheight,
                        rx: erx,
                        opacity: eopacity,
                    },
                ) => {
                    assert_eq!(class, expected_class, "rect class");
                    close(*x, *ex, "rect x");
                    close(*y, *ey, "rect y");
                    close(*width, *ewidth, "rect width");
                    close(*height, *eheight, "rect height");
                    close(*rx, *erx, "rect rx");
                    close(*opacity, *eopacity, "rect opacity");
                }
                (
                    Actual::Circle {
                        class,
                        cx,
                        cy,
                        r,
                        stroke_width,
                    },
                    Element::Circle {
                        class: expected_class,
                        cx: ecx,
                        cy: ecy,
                        r: er,
                        stroke_width: estroke_width,
                    },
                ) => {
                    assert_eq!(class, expected_class, "circle class");
                    close(*cx, *ecx, "circle cx");
                    close(*cy, *ecy, "circle cy");
                    close(*r, *er, "circle r");
                    close(*stroke_width, *estroke_width, "circle stroke width");
                }
                (
                    Actual::Path { class, path },
                    element @ (Element::Path { .. } | Element::Polygon { .. }),
                ) => {
                    let expected_class = match element {
                        Element::Path { class, .. } | Element::Polygon { class, .. } => class,
                        _ => unreachable!(),
                    };
                    assert_eq!(class, expected_class, "path class");
                    let expected = expected_path(element).expect("path data");
                    assert_path(path, &expected, class);
                }
                _ => panic!("element kind differs: expected {expected:?}"),
            }
        }
    }

    pub fn assert_intersects(case: &Case, intersect: impl Fn(Point) -> Point) {
        for expected in &case.intersects {
            let result = intersect(Point::new(expected.point.x, expected.point.y));
            close(result.x, expected.result.x, "intersect x");
            close(result.y, expected.result.y, "intersect y");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: Point, expected: Point) {
        assert!((actual.x - expected.x).abs() < 1e-9, "{actual:?}");
        assert!((actual.y - expected.y).abs() < 1e-9, "{actual:?}");
    }

    #[test]
    fn ring_intersection_lands_on_the_circle_and_stops_at_85_percent() {
        let center = Point::new(10.0, 20.0);
        close(
            ring_intersect(center, 36.0, Point::new(10.0, 200.0)),
            Point::new(10.0, 38.0),
        );
        close(
            ring_intersect(center, 36.0, Point::new(300.0, 20.0)),
            Point::new(28.0, 20.0),
        );
        let clamped = ring_intersect(center, 36.0, Point::new(510.0, 620.0));
        assert!((clamped.x - 25.3).abs() < 1e-9);
        assert!(clamped.y > 20.0);
        close(
            ring_intersect(Point::ZERO, 36.0, Point::new(0.0, -100.0)),
            Point::new(0.0, -18.0),
        );
    }

    #[test]
    fn face_centre_intersection_middles_the_nearest_face() {
        let center = Point::new(10.0, 20.0);
        close(
            face_centre_intersect(center, 50.0, 50.0, Point::new(200.0, 30.0)),
            Point::new(35.0, 20.0),
        );
        close(
            face_centre_intersect(center, 50.0, 50.0, Point::new(12.0, 200.0)),
            Point::new(10.0, 45.0),
        );
        close(
            face_centre_intersect(center, 50.0, 50.0, Point::new(110.0, 120.0)),
            Point::new(10.0, 45.0),
        );
    }

    #[test]
    fn face_projection_keeps_clear_of_the_corner_and_honours_a_fixed_face() {
        let center = Point::new(100.0, 100.0);
        close(
            face_project_intersect(center, 100.0, 60.0, Point::new(160.0, 400.0)),
            Point::new(142.0, 130.0),
        );
        close(
            face_project_intersect_at(
                center,
                100.0,
                60.0,
                Point::new(400.0, 100.0),
                Some(ShapeFace::Left),
            ),
            Point::new(50.0, 100.0),
        );
    }
}
