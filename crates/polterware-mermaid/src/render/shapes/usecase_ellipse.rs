//! Classic port of upstream `shapes/usecaseEllipse.ts`.
//!
//! Look: the use case oval of the web design system's use case diagram, an `<ellipse
//! class="basic label-container">` sized from the label with the use case's padding.
//! Behavior: upstream `usecaseEllipse.ts`; this crate's `Node` carries the label size and
//! padding the upstream use case DB supplies (`padding: 20` for oval use cases).
//!
//! The per-item colour slot (`stampColorSlot`), the ARIA role and the leaf's own
//! `basic label-container` class belong to the DOM: the scene keeps the shape class on the
//! group and paints the oval with the theme's node style. Upstream also installs a
//! `calcIntersect` for layout-time bounds, which the shared `Node` does not model.
//! `handDrawn` is not rendered: rough.js jitter stays a renderer TODO.

use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, get_node_classes, prepare,
    shape_style,
};
use crate::scene::{EllipseShape, Item, Point, Rect, Size, SourceRef};

// handDrawn: rough.js (later)

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, bbox) = prepare(&node, ctx)?;
    group.class = Some(get_node_classes(&node, None));
    // `labelHelper`'s `halfPadding = (node.padding ?? 0) / 2`; the `?? 10` fallback can only
    // fire for a missing node, which cannot happen here.
    let padding = node.padding / 2.0;
    // Upstream: the same air above, below and beside the label.
    let radius_x = bbox.width / 2.0 + padding * 2.0;
    let radius_y = bbox.height / 2.0 + padding * 2.0;

    node.bounds = Rect::centered(Point::ZERO, Size::new(radius_x * 2.0, radius_y * 2.0));
    let mut ellipse = EllipseShape::new(Point::ZERO, radius_x, radius_y, shape_style());
    ellipse.source = Some(SourceRef::node(node.id.clone()));
    // `insert('ellipse', ':first-child')` paints the oval under the label.
    group.items.insert(0, Item::Ellipse(ellipse));
    group.items.push(Item::Group(label));

    // `labelHelper`'s SVG label sits at `translate(0, -bbox.height / 2)`, which is exactly the
    // label group `prepare` builds, so the label keeps its own transform.
    node.intersect = IntersectKind::Ellipse {
        rx: radius_x,
        ry: radius_y,
    };
    Ok(RenderedNode { node, group })
}

/// Shared differential-fixture harness for the use case shapes: every sibling shape's test
/// reads `testdata/shapes/<shape>.json` and checks its scene against the upstream run.
///
/// The fixtures were produced outside the repository by running the upstream 12.3.0 shape
/// modules under jsdom (only `createText`, the diagram config and the icon registry are
/// stubbed), with a fixed label size per case, and recording the path data, the node box,
/// the label transforms and two intersection points. The tests compare within
/// [`TOLERANCE`], 0.01 user units.
#[cfg(test)]
pub(crate) mod difftest {
    use crate::measure::{Font, TextMeasure};
    use crate::render::intersect::Intersectable;
    use crate::render::shapes::{Item, Node, RenderedNode};
    use crate::scene::{Color, Group, Point};
    use serde_json::{Value, json};

    pub(crate) const TOLERANCE: f64 = 0.01;

    /// Upstream's label measurement, fixed per fixture case so the comparison is exact.
    pub(crate) struct FixedMeasure {
        width: f64,
        height: f64,
    }

    impl FixedMeasure {
        pub(crate) fn from_case(case: &Value) -> Self {
            let size = case["label_size"].as_array().expect("label_size");
            Self {
                width: size[0].as_f64().expect("label width"),
                height: size[1].as_f64().expect("label height"),
            }
        }
    }

    impl TextMeasure for FixedMeasure {
        fn width(&self, text: &str, _font: &Font) -> f64 {
            if text.is_empty() { 0.0 } else { self.width }
        }

        fn line_height(&self, _font: &Font) -> f64 {
            self.height
        }
    }

    /// Builds the node a fixture case describes.
    pub(crate) fn node_for(case: &Value, shape: &str) -> Node {
        let mut node = Node::new(
            "n",
            case["label"].as_str().expect("label"),
            shape.to_string(),
        );
        node.padding = case["padding"].as_f64().expect("padding");
        node
    }

    fn normalize(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn fill_value(fill: &Option<Color>) -> Value {
        match fill {
            None => json!("none"),
            Some(color) if color.0 == "none" => json!("none"),
            Some(_) => Value::Null,
        }
    }

    fn is_label(group: &Group) -> bool {
        group
            .class
            .as_deref()
            .is_some_and(|class| class.split_whitespace().any(|word| word == "label"))
    }

    fn label_text(group: &Group) -> String {
        let mut parts = Vec::new();
        for item in &group.items {
            if let Item::Text(text) = item {
                for line in &text.lines {
                    parts.push(line.text());
                }
            }
        }
        parts.join("\n")
    }

    fn walk(items: &[Item]) -> Vec<Value> {
        let mut out = Vec::new();
        for item in items {
            match item {
                Item::Path(path) => out.push(json!({
                    "tag": "path",
                    // The scene keeps classes on groups only; leaf paint roles come from
                    // the style, and the fixture's leaf class is documentation.
                    "class": Value::Null,
                    "fill": fill_value(&path.style.fill),
                    "commands": serde_json::to_value(&path.path).expect("path serializes"),
                })),
                Item::Ellipse(ellipse) => out.push(json!({
                    "tag": "ellipse",
                    "class": Value::Null,
                    "fill": fill_value(&ellipse.style.fill),
                    "cx": ellipse.center.x,
                    "cy": ellipse.center.y,
                    "rx": ellipse.radius_x,
                    "ry": ellipse.radius_y,
                })),
                Item::Rect(rect) => out.push(json!({
                    "tag": "rect",
                    "class": Value::Null,
                    "x": rect.rect.x,
                    "y": rect.rect.y,
                    "width": rect.rect.width,
                    "height": rect.rect.height,
                    "rx": if rect.rx == 0.0 { Value::Null } else { json!(rect.rx) },
                    "opacity": if (rect.style.opacity - 1.0).abs() < f64::EPSILON {
                        Value::Null
                    } else {
                        json!(rect.style.opacity)
                    },
                })),
                Item::Group(group) if is_label(group) => out.push(json!({
                    "tag": "label",
                    "class": group.class.clone().unwrap_or_default(),
                    "x": group.transform.e,
                    "y": group.transform.f,
                    "text": label_text(group),
                })),
                Item::Group(group) => out.push(json!({
                    "tag": "g",
                    "class": group.class.clone().unwrap_or_default(),
                    "x": group.transform.e,
                    "y": group.transform.f,
                    "children": Value::Array(walk(&group.items)),
                })),
                // Icons are asset references the painter resolves, not geometry.
                Item::Image(_) | Item::Text(_) => {}
            }
        }
        out
    }

    /// The scene as the fixture speaks about it.
    pub(crate) fn elements(rendered: &RenderedNode) -> Value {
        Value::Array(walk(&rendered.group.items))
    }

    fn compare(actual: &Value, expected: &Value, path: &str) {
        match (actual, expected) {
            (Value::Number(actual), Value::Number(expected)) => {
                let actual = actual.as_f64().expect("number");
                let expected = expected.as_f64().expect("number");
                assert!(
                    (actual - expected).abs() <= TOLERANCE,
                    "{path}: {actual} != {expected}"
                );
            }
            (Value::String(actual), Value::String(expected)) => {
                assert_eq!(
                    normalize(actual),
                    normalize(expected),
                    "{path}: class or text differs"
                );
            }
            (Value::Null, Value::Null) => {}
            (Value::Array(actual), Value::Array(expected)) => {
                assert_eq!(actual.len(), expected.len(), "{path}: element count");
                for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                    compare(actual, expected, &format!("{path}[{index}]"));
                }
            }
            (Value::Object(actual), Value::Object(expected)) => {
                for (key, expected) in expected {
                    let actual = actual
                        .get(key)
                        .unwrap_or_else(|| panic!("{path}: missing {key}"));
                    if key == "class" && actual.is_null() {
                        continue;
                    }
                    compare(actual, expected, &format!("{path}.{key}"));
                }
                for key in actual.keys() {
                    assert!(expected.contains_key(key), "{path}: unexpected {key}");
                }
            }
            _ => panic!("{path}: {actual:?} != {expected:?}"),
        }
    }

    /// An SVG `<circle>` is the same scene ellipse with two equal radii.
    fn normalize_expected(value: &Value) -> Value {
        match value {
            Value::Array(items) => Value::Array(items.iter().map(normalize_expected).collect()),
            Value::Object(map) => {
                if map.get("tag").and_then(Value::as_str) == Some("circle") {
                    let mut ellipse = map.clone();
                    ellipse.insert("tag".into(), json!("ellipse"));
                    let radius = map.get("r").cloned().unwrap_or(json!(0.0));
                    ellipse.insert("rx".into(), radius.clone());
                    ellipse.insert("ry".into(), radius);
                    ellipse.remove("r");
                    return Value::Object(ellipse);
                }
                let mut out = serde_json::Map::new();
                for (key, item) in map {
                    out.insert(key.clone(), normalize_expected(item));
                }
                Value::Object(out)
            }
            other => other.clone(),
        }
    }

    /// Compares one rendered shape with its recorded upstream case.
    pub(crate) fn assert_case(rendered: &RenderedNode, case: &Value) {
        let name = case["name"].as_str().unwrap_or("case");
        let expected_elements = normalize_expected(&case["elements"]);
        compare(
            &elements(rendered),
            &expected_elements,
            &format!("{name}: elements"),
        );

        let width = case["width"].as_f64().expect("width");
        let height = case["height"].as_f64().expect("height");
        assert!(
            (rendered.node.bounds.width - width).abs() <= TOLERANCE,
            "{name}: node width {} != {width}",
            rendered.node.bounds.width
        );
        assert!(
            (rendered.node.bounds.height - height).abs() <= TOLERANCE,
            "{name}: node height {} != {height}",
            rendered.node.bounds.height
        );
        compare(
            &json!(rendered.group.class.clone().unwrap_or_default()),
            &case["class"],
            &format!("{name}: shape class"),
        );

        for entry in case["intersect"].as_array().expect("intersect") {
            let query = entry["query"].as_array().expect("query");
            let point = rendered.node.intersect(Point::new(
                query[0].as_f64().expect("x"),
                query[1].as_f64().expect("y"),
            ));
            let expected = entry["point"].as_array().expect("point");
            assert!(
                (point.x - expected[0].as_f64().expect("x")).abs() <= TOLERANCE
                    && (point.y - expected[1].as_f64().expect("y")).abs() <= TOLERANCE,
                "{name}: intersect at {query:?} was {point:?}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::difftest;
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::theme::ThemeVariables;

    #[test]
    fn oval_is_the_label_plus_two_padding_units() {
        let mut node = Node::new("u", "A", "usecaseEllipse");
        node.padding = 20.0;
        let rendered = draw(
            &node,
            &ShapeContext {
                measure: &FallbackMeasure,
                theme: &ThemeVariables::default(),
            },
        )
        .unwrap();
        assert!(rendered.node.bounds.width > 0.0);
        assert_eq!(
            rendered
                .group
                .items
                .iter()
                .filter(|item| matches!(item, Item::Ellipse(_)))
                .count(),
            1
        );
    }

    #[test]
    fn matches_upstream_fixtures() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../testdata/shapes/usecaseEllipse.json"))
                .expect("fixture parses");
        let theme = ThemeVariables::default();
        for case in fixture["cases"].as_array().expect("cases") {
            let measure = difftest::FixedMeasure::from_case(case);
            let node = difftest::node_for(case, "usecaseEllipse");
            let rendered = draw(
                &node,
                &ShapeContext {
                    measure: &measure,
                    theme: &theme,
                },
            )
            .expect("shape renders");
            difftest::assert_case(&rendered, case);
        }
    }
}
