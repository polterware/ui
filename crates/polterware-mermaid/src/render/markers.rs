//! Marker definitions captured from upstream `rendering-util/rendering-elements/markers.js`.
use std::collections::BTreeMap;

use serde::Deserialize;

use crate::scene::{
    Color, FillRule, LineCap, LineJoin, MarkerDef, MarkerOrient, MarkerRef, MarkerUnits, Path,
    PathCommand, PathShape, PathStyle, Point, Rect,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerKind {
    Cross,
    Point,
    Barb,
    BarbNeo,
    Circle,
    OpenArrow,
    HollowCircle,
    Hierarchy,
    Aggregation,
    Extension,
    Composition,
    Dependency,
    Lollipop,
    OnlyOne,
    ZeroOrOne,
    OneOrMore,
    ZeroOrMore,
    RequirementArrow,
    RequirementContains,
}

impl MarkerKind {
    pub fn from_arrow_type(value: &str) -> Option<Self> {
        Some(match value {
            "arrow_cross" => Self::Cross,
            "arrow_point" => Self::Point,
            "arrow_barb" => Self::Barb,
            "arrow_barb_neo" => Self::BarbNeo,
            "arrow_circle" => Self::Circle,
            "arrow_open_head" => Self::OpenArrow,
            "arrow_hollow_circle" => Self::HollowCircle,
            "arrow_hierarchy" => Self::Hierarchy,
            "aggregation" => Self::Aggregation,
            "extension" => Self::Extension,
            "composition" => Self::Composition,
            "dependency" => Self::Dependency,
            "lollipop" => Self::Lollipop,
            "only_one" => Self::OnlyOne,
            "zero_or_one" => Self::ZeroOrOne,
            "one_or_more" => Self::OneOrMore,
            "zero_or_more" => Self::ZeroOrMore,
            "requirement_arrow" => Self::RequirementArrow,
            "requirement_contains" => Self::RequirementContains,
            _ => return None,
        })
    }

    fn upstream_name(self) -> &'static str {
        match self {
            Self::Cross => "cross",
            Self::Point => "point",
            Self::Barb => "barb",
            Self::BarbNeo => "barbNeo",
            Self::Circle => "circle",
            Self::OpenArrow => "openArrow",
            Self::HollowCircle => "hollowCircle",
            Self::Hierarchy => "hierarchy",
            Self::Aggregation => "aggregation",
            Self::Extension => "extension",
            Self::Composition => "composition",
            Self::Dependency => "dependency",
            Self::Lollipop => "lollipop",
            Self::OnlyOne => "only_one",
            Self::ZeroOrOne => "zero_or_one",
            Self::OneOrMore => "one_or_more",
            Self::ZeroOrMore => "zero_or_more",
            Self::RequirementArrow => "requirement_arrow",
            Self::RequirementContains => "requirement_contains",
        }
    }

    fn id_stem(self) -> &'static str {
        match self {
            Self::Cross => "cross",
            Self::Point => "point",
            Self::Barb | Self::BarbNeo => "barb",
            Self::Circle => "circle",
            Self::OpenArrow => "openArrow",
            Self::HollowCircle => "hollowCircle",
            Self::Hierarchy => "hierarchy",
            Self::Aggregation => "aggregation",
            Self::Extension => "extension",
            Self::Composition => "composition",
            Self::Dependency => "dependency",
            Self::Lollipop => "lollipop",
            Self::OnlyOne => "onlyOne",
            Self::ZeroOrOne => "zeroOrOne",
            Self::OneOrMore => "oneOrMore",
            Self::ZeroOrMore => "zeroOrMore",
            Self::RequirementArrow => "requirement_arrow",
            Self::RequirementContains => "requirement_contains",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MarkerPlacement {
    pub start: Option<MarkerRef>,
    pub end: Option<MarkerRef>,
}

/// Return every marker definition emitted by upstream for these registered marker names.
pub fn marker_definitions(
    diagram_id: &str,
    diagram_type: &str,
    registered: &[&str],
) -> Vec<MarkerDef> {
    registered
        .iter()
        .flat_map(|name| {
            let source = fixture(*name);
            source
                .definitions
                .into_iter()
                .map(|definition| to_scene(definition, diagram_id, diagram_type))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Create one scene marker using the matching upstream start/end and margin definition.
pub fn marker_definition(
    diagram_id: &str,
    diagram_type: &str,
    kind: MarkerKind,
    start: bool,
    margin: bool,
) -> MarkerDef {
    let source = fixture(kind.upstream_name());
    let suffix = format!(
        "-{}{suffix}",
        kind.id_stem(),
        suffix = if start { "Start" } else { "End" }
    );
    let suffix = if margin {
        format!("{suffix}-margin")
    } else {
        suffix
    };
    let base_suffix = format!("-{}{}", kind.id_stem(), if start { "Start" } else { "End" });
    let chosen = source
        .definitions
        .iter()
        .find(|definition| {
            definition
                .attrs
                .get("id")
                .is_some_and(|id| id.ends_with(&suffix))
        })
        .or_else(|| {
            source.definitions.iter().find(|definition| {
                definition
                    .attrs
                    .get("id")
                    .is_some_and(|id| id.ends_with(&base_suffix))
            })
        })
        .or_else(|| source.definitions.first())
        .expect("upstream marker fixture has definitions")
        .clone();
    to_scene(chosen, diagram_id, diagram_type)
}

/// Map arrow names to marker references; empty/none are no-ops and unknown names are errors.
pub fn marker_placement(
    start: Option<&str>,
    end: Option<&str>,
    diagram_id: &str,
    diagram_type: &str,
    margin: bool,
) -> Result<MarkerPlacement, String> {
    fn one(
        value: Option<&str>,
        at_start: bool,
        diagram_id: &str,
        diagram_type: &str,
        margin: bool,
    ) -> Result<Option<MarkerRef>, String> {
        match value {
            None | Some("") | Some("none") => Ok(None),
            Some(value) => {
                let kind = MarkerKind::from_arrow_type(value)
                    .ok_or_else(|| format!("Unknown arrow type: {value}"))?;
                let definition =
                    marker_definition(diagram_id, diagram_type, kind, at_start, margin);
                Ok(Some(MarkerRef::new(definition.id)))
            }
        }
    }
    Ok(MarkerPlacement {
        start: one(start, true, diagram_id, diagram_type, margin)?,
        end: one(end, false, diagram_id, diagram_type, margin)?,
    })
}

#[derive(Clone, Debug, Deserialize)]
struct Fixture {
    name: String,
    definitions: Vec<RawNode>,
}

#[derive(Clone, Debug, Deserialize)]
struct RawNode {
    tag: String,
    #[serde(default)]
    attrs: BTreeMap<String, String>,
    #[serde(default)]
    styles: BTreeMap<String, String>,
    #[serde(default)]
    children: Vec<RawNode>,
}

fn fixture(name: &str) -> Fixture {
    let data = match name {
        "extension" => include_str!("testdata/markers/extension.json"),
        "composition" => include_str!("testdata/markers/composition.json"),
        "aggregation" => include_str!("testdata/markers/aggregation.json"),
        "dependency" => include_str!("testdata/markers/dependency.json"),
        "lollipop" => include_str!("testdata/markers/lollipop.json"),
        "point" => include_str!("testdata/markers/point.json"),
        "circle" => include_str!("testdata/markers/circle.json"),
        "cross" => include_str!("testdata/markers/cross.json"),
        "barb" => include_str!("testdata/markers/barb.json"),
        "barbNeo" => include_str!("testdata/markers/barbNeo.json"),
        "hierarchy" => include_str!("testdata/markers/hierarchy.json"),
        "only_one" => include_str!("testdata/markers/only_one.json"),
        "zero_or_one" => include_str!("testdata/markers/zero_or_one.json"),
        "one_or_more" => include_str!("testdata/markers/one_or_more.json"),
        "zero_or_more" => include_str!("testdata/markers/zero_or_more.json"),
        "only_one_neo" => include_str!("testdata/markers/only_one_neo.json"),
        "zero_or_one_neo" => include_str!("testdata/markers/zero_or_one_neo.json"),
        "one_or_more_neo" => include_str!("testdata/markers/one_or_more_neo.json"),
        "zero_or_more_neo" => include_str!("testdata/markers/zero_or_more_neo.json"),
        "requirement_arrow" => include_str!("testdata/markers/requirement_arrow.json"),
        "requirement_contains" => include_str!("testdata/markers/requirement_contains.json"),
        "requirement_arrow_neo" => include_str!("testdata/markers/requirement_arrow_neo.json"),
        "requirement_contains_neo" => {
            include_str!("testdata/markers/requirement_contains_neo.json")
        }
        "openArrow" => include_str!("testdata/markers/openArrow.json"),
        "hollowCircle" => include_str!("testdata/markers/hollowCircle.json"),
        _ => panic!("unknown upstream marker name: {name}"),
    };
    let parsed: Fixture = serde_json::from_str(data).expect("valid generated marker fixture");
    debug_assert_eq!(parsed.name, name);
    parsed
}

fn to_scene(mut raw: RawNode, diagram_id: &str, diagram_type: &str) -> MarkerDef {
    let id = raw.attrs.get("id").cloned().unwrap_or_default();
    let prefix = "diagram-id_diagram-type";
    let id = id.replacen(prefix, &format!("{diagram_id}_{diagram_type}"), 1);
    let width = number(&raw.attrs, "markerWidth").unwrap_or(3.0);
    let height = number(&raw.attrs, "markerHeight").unwrap_or(3.0);
    let view_box = raw
        .attrs
        .get("viewBox")
        .and_then(|value| parse_view_box(value))
        .unwrap_or(Rect::new(0.0, 0.0, width, height));
    let reference = Point::new(
        number(&raw.attrs, "refX").unwrap_or(0.0),
        number(&raw.attrs, "refY").unwrap_or(0.0),
    );
    let mut definition = MarkerDef::new(id, view_box)
        .with_reference(reference)
        .with_size(width, height)
        .with_orient(MarkerOrient::Auto);
    if let Some(class) = raw.attrs.get("class") {
        definition.class = Some(class.clone());
    }
    definition.units = match raw.attrs.get("markerUnits").map(String::as_str) {
        Some("userSpaceOnUse") => MarkerUnits::UserSpaceOnUse,
        _ => MarkerUnits::StrokeWidth,
    };
    for child in raw.children.drain(..) {
        append_shapes(&mut definition, &child, &raw.styles);
    }
    definition
}

fn append_shapes(definition: &mut MarkerDef, node: &RawNode, inherited: &BTreeMap<String, String>) {
    if node.tag == "g" {
        for child in &node.children {
            append_shapes(definition, child, inherited);
        }
        return;
    }
    let mut path = Path::new();
    match node.tag.as_str() {
        "path" => {
            if let Some(data) = node.attrs.get("d") {
                path = parse_path(data);
            }
        }
        "polygon" => {
            let points = node
                .attrs
                .get("points")
                .map(|s| numbers(s))
                .unwrap_or_default();
            for (index, pair) in points.chunks_exact(2).enumerate() {
                if index == 0 {
                    path.move_to(Point::new(pair[0], pair[1]));
                } else {
                    path.line_to(Point::new(pair[0], pair[1]));
                }
            }
            path.close();
        }
        "circle" => {
            let cx = number(&node.attrs, "cx").unwrap_or(0.0);
            let cy = number(&node.attrs, "cy").unwrap_or(0.0);
            let radius = number(&node.attrs, "r").unwrap_or(0.0);
            path.move_to(Point::new(cx + radius, cy))
                .arc_to(
                    radius,
                    radius,
                    0.0,
                    false,
                    true,
                    Point::new(cx - radius, cy),
                )
                .arc_to(
                    radius,
                    radius,
                    0.0,
                    false,
                    true,
                    Point::new(cx + radius, cy),
                )
                .close();
        }
        "line" => {
            path.move_to(Point::new(
                number(&node.attrs, "x1").unwrap_or(0.0),
                number(&node.attrs, "y1").unwrap_or(0.0),
            ))
            .line_to(Point::new(
                number(&node.attrs, "x2").unwrap_or(0.0),
                number(&node.attrs, "y2").unwrap_or(0.0),
            ));
        }
        _ => return,
    }
    let mut properties = inherited.clone();
    properties.extend(node.styles.clone());
    for (key, value) in &node.attrs {
        if matches!(
            key.as_str(),
            "fill"
                | "stroke"
                | "stroke-width"
                | "stroke-dasharray"
                | "stroke-linecap"
                | "stroke-linejoin"
        ) {
            properties.insert(key.clone(), value.clone());
        }
    }
    let fill = properties.get("fill").map(String::as_str);
    let stroke = properties.get("stroke").map(String::as_str);
    let mut style = if fill == Some("none") {
        PathStyle {
            fill: Some(Color::none()),
            stroke: Some(Color::current()),
            stroke_width: properties
                .get("stroke-width")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1.0),
            ..PathStyle::default()
        }
    } else if stroke.is_some() || properties.contains_key("stroke-width") {
        PathStyle {
            fill: fill.map(Color::new),
            stroke: Some(Color::current()),
            stroke_width: properties
                .get("stroke-width")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1.0),
            ..PathStyle::default()
        }
    } else if fill.is_some() || matches!(node.tag.as_str(), "polygon" | "circle") {
        PathStyle::filled(fill.unwrap_or("currentColor"))
    } else {
        PathStyle {
            fill: Some(Color::none()),
            stroke: Some(Color::current()),
            stroke_width: 1.0,
            ..PathStyle::default()
        }
    };
    style.dash = properties
        .get("stroke-dasharray")
        .filter(|value| value.as_str() != "none")
        .map(|value| numbers(value))
        .unwrap_or_default();
    style.line_cap = match properties.get("stroke-linecap").map(String::as_str) {
        Some("round") => LineCap::Round,
        Some("square") => LineCap::Square,
        _ => LineCap::Butt,
    };
    style.line_join = match properties.get("stroke-linejoin").map(String::as_str) {
        Some("round") => LineJoin::Round,
        Some("bevel") => LineJoin::Bevel,
        _ => LineJoin::Miter,
    };
    definition.push(PathShape {
        path,
        style,
        fill_rule: FillRule::default(),
        ..PathShape::default()
    });
}

fn number(attrs: &BTreeMap<String, String>, name: &str) -> Option<f64> {
    attrs.get(name)?.parse().ok()
}

fn parse_view_box(value: &str) -> Option<Rect> {
    let values = numbers(value);
    (values.len() == 4).then(|| Rect::new(values[0], values[1], values[2], values[3]))
}

fn numbers(value: &str) -> Vec<f64> {
    let mut output = Vec::new();
    let mut start = None;
    for (index, character) in value.char_indices() {
        if character.is_ascii_digit() || matches!(character, '.' | '-' | '+') {
            start.get_or_insert(index);
        } else if let Some(begin) = start.take() {
            if let Ok(number) = value[begin..index].parse() {
                output.push(number);
            }
        }
    }
    if let Some(begin) = start {
        if let Ok(number) = value[begin..].parse() {
            output.push(number);
        }
    }
    output
}

fn parse_path(data: &str) -> Path {
    let mut tokens = Vec::new();
    let mut number_start = None;
    for (index, character) in data.char_indices() {
        if character.is_ascii_alphabetic() {
            if let Some(begin) = number_start.take() {
                tokens.push(PathToken::Number(
                    data[begin..index]
                        .parse::<f64>()
                        .expect("valid SVG path number"),
                ));
            }
            tokens.push(PathToken::Command(character));
        } else if character.is_ascii_digit() || matches!(character, '.' | '-' | '+') {
            if number_start.is_none() {
                number_start = Some(index);
            }
        } else if let Some(begin) = number_start.take() {
            tokens.push(PathToken::Number(
                data[begin..index]
                    .parse::<f64>()
                    .expect("valid SVG path number"),
            ));
        }
    }
    if let Some(begin) = number_start {
        tokens.push(PathToken::Number(
            data[begin..].parse::<f64>().expect("valid SVG path number"),
        ));
    }
    let mut path = Path::new();
    let mut index = 0;
    let mut command = 'M';
    let mut current = Point::ZERO;
    let mut subpath = Point::ZERO;
    while index < tokens.len() {
        if let PathToken::Command(next) = tokens[index] {
            command = next;
            index += 1;
        }
        let relative = command.is_ascii_lowercase();
        let op = command.to_ascii_uppercase();
        let point = |x: f64, y: f64, current: Point| {
            if relative {
                Point::new(current.x + x, current.y + y)
            } else {
                Point::new(x, y)
            }
        };
        match op {
            'Z' => {
                path.close();
                current = subpath;
                command = 'M';
            }
            'M' | 'L' => {
                let p = point(
                    token_num(&tokens, &mut index),
                    token_num(&tokens, &mut index),
                    current,
                );
                if op == 'M' {
                    path.move_to(p);
                    subpath = p;
                    command = if relative { 'l' } else { 'L' };
                } else {
                    path.line_to(p);
                }
                current = p;
            }
            'H' => {
                let x = token_num(&tokens, &mut index);
                current = Point::new(if relative { current.x + x } else { x }, current.y);
                path.line_to(current);
            }
            'V' => {
                let y = token_num(&tokens, &mut index);
                current = Point::new(current.x, if relative { current.y + y } else { y });
                path.line_to(current);
            }
            'Q' => {
                let control = point(
                    token_num(&tokens, &mut index),
                    token_num(&tokens, &mut index),
                    current,
                );
                let to = point(
                    token_num(&tokens, &mut index),
                    token_num(&tokens, &mut index),
                    current,
                );
                path.quad_to(control, to);
                current = to;
            }
            'C' => {
                let c1 = point(
                    token_num(&tokens, &mut index),
                    token_num(&tokens, &mut index),
                    current,
                );
                let c2 = point(
                    token_num(&tokens, &mut index),
                    token_num(&tokens, &mut index),
                    current,
                );
                let to = point(
                    token_num(&tokens, &mut index),
                    token_num(&tokens, &mut index),
                    current,
                );
                path.cubic_to(c1, c2, to);
                current = to;
            }
            'A' => {
                let rx = token_num(&tokens, &mut index);
                let ry = token_num(&tokens, &mut index);
                let rotation = token_num(&tokens, &mut index);
                let large = token_num(&tokens, &mut index) != 0.0;
                let sweep = token_num(&tokens, &mut index) != 0.0;
                let to = point(
                    token_num(&tokens, &mut index),
                    token_num(&tokens, &mut index),
                    current,
                );
                path.commands
                    .push(PathCommand::arc_to(rx, ry, rotation, large, sweep, to));
                current = to;
            }
            _ => panic!("unsupported SVG marker path command {command}"),
        }
    }
    path
}

#[derive(Clone, Copy, Debug)]
enum PathToken {
    Command(char),
    Number(f64),
}

fn token_num(tokens: &[PathToken], index: &mut usize) -> f64 {
    let PathToken::Number(value) = tokens[*index] else {
        panic!("missing SVG path number")
    };
    *index += 1;
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches_node_fixture(name: &str) {
        let generated = fixture(name);
        assert_eq!(generated.name, name);
        let definitions = marker_definitions("diagram-id", "diagram-type", &[name]);
        assert_eq!(definitions.len(), generated.definitions.len(), "{name}");
        for (actual, raw) in definitions.iter().zip(generated.definitions) {
            let expected = to_scene(raw, "diagram-id", "diagram-type");
            assert_eq!(actual, &expected, "{name} {}", actual.id);
            assert!(!actual.shapes.is_empty(), "{name} scene geometry");
        }
    }

    macro_rules! marker_fixture_test {
        ($test_name:ident, $marker:literal) => {
            #[test]
            fn $test_name() {
                matches_node_fixture($marker);
            }
        };
    }

    marker_fixture_test!(fixture_extension_matches_node, "extension");
    marker_fixture_test!(fixture_composition_matches_node, "composition");
    marker_fixture_test!(fixture_aggregation_matches_node, "aggregation");
    marker_fixture_test!(fixture_dependency_matches_node, "dependency");
    marker_fixture_test!(fixture_lollipop_matches_node, "lollipop");
    marker_fixture_test!(fixture_point_matches_node, "point");
    marker_fixture_test!(fixture_circle_matches_node, "circle");
    marker_fixture_test!(fixture_cross_matches_node, "cross");
    marker_fixture_test!(fixture_barb_matches_node, "barb");
    marker_fixture_test!(fixture_barb_neo_matches_node, "barbNeo");
    marker_fixture_test!(fixture_hierarchy_matches_node, "hierarchy");
    marker_fixture_test!(fixture_only_one_matches_node, "only_one");
    marker_fixture_test!(fixture_zero_or_one_matches_node, "zero_or_one");
    marker_fixture_test!(fixture_one_or_more_matches_node, "one_or_more");
    marker_fixture_test!(fixture_zero_or_more_matches_node, "zero_or_more");
    marker_fixture_test!(fixture_only_one_neo_matches_node, "only_one_neo");
    marker_fixture_test!(fixture_zero_or_one_neo_matches_node, "zero_or_one_neo");
    marker_fixture_test!(fixture_one_or_more_neo_matches_node, "one_or_more_neo");
    marker_fixture_test!(fixture_zero_or_more_neo_matches_node, "zero_or_more_neo");
    marker_fixture_test!(fixture_requirement_arrow_matches_node, "requirement_arrow");
    marker_fixture_test!(
        fixture_requirement_contains_matches_node,
        "requirement_contains"
    );
    marker_fixture_test!(
        fixture_requirement_arrow_neo_matches_node,
        "requirement_arrow_neo"
    );
    marker_fixture_test!(
        fixture_requirement_contains_neo_matches_node,
        "requirement_contains_neo"
    );
    marker_fixture_test!(fixture_open_arrow_matches_node, "openArrow");
    marker_fixture_test!(fixture_hollow_circle_matches_node, "hollowCircle");

    #[test]
    fn maps_arrow_aliases_and_preserves_unknown_error() {
        for alias in [
            "arrow_cross",
            "arrow_point",
            "arrow_barb",
            "arrow_barb_neo",
            "arrow_circle",
            "arrow_open_head",
            "arrow_hollow_circle",
            "arrow_hierarchy",
            "aggregation",
            "extension",
            "composition",
            "dependency",
            "lollipop",
            "only_one",
            "zero_or_one",
            "one_or_more",
            "zero_or_more",
            "requirement_arrow",
            "requirement_contains",
        ] {
            assert!(MarkerKind::from_arrow_type(alias).is_some(), "{alias}");
        }
        assert_eq!(
            marker_placement(Some("none"), Some("not-real"), "d", "flowchart", false).unwrap_err(),
            "Unknown arrow type: not-real"
        );
    }

    #[test]
    fn marker_placement_keeps_upstream_ids_for_margin_variants() {
        let placement = marker_placement(
            Some("arrow_point"),
            Some("arrow_open_head"),
            "diagram",
            "flowchart-v2",
            true,
        )
        .unwrap();
        assert_eq!(
            placement.start.unwrap().def,
            "diagram_flowchart-v2-pointStart-margin"
        );
        assert_eq!(
            placement.end.unwrap().def,
            "diagram_flowchart-v2-openArrowEnd"
        );
    }
}
