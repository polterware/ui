//! Classic port of upstream `shapes/classBox.ts`.
//!
//! `Node` carries no class members, methods, or annotations, so this ports the empty-box
//! branch of `textHelper`: an annotation row, a bold name label, and an empty members row.
//! The outer rectangle and both dividers keep upstream's coordinates, including the negative
//! group heights the empty annotation and members rows measure to.
use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, get_node_classes, shape_style,
};
use crate::measure::Font;
use crate::render::labels::{LabelOptions, create_label};
use crate::scene::{
    Group, Item, Path, PathShape, Point, Rect, RectShape, Size, SourceRef, Transform,
};

/// `config.class.padding`'s default (the schema default of 5 is intentionally not used).
const PADDING: f64 = 12.0;
/// `GAP = PADDING`.
const GAP: f64 = PADDING;

fn text_size(label: &Group) -> Size {
    match label.items.first() {
        Some(Item::Text(text)) => text.size,
        _ => Size::ZERO,
    }
}

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let mut group = Group::new()
        .with_class(get_node_classes(&node, None))
        .with_source(SourceRef::node(node.id.clone()));

    let font_size = ctx
        .theme
        .color("fontSize")
        .and_then(|s| s.trim_end_matches("px").parse::<f64>().ok())
        .or_else(|| ctx.theme.number("fontSize"))
        .unwrap_or(Font::default().size);
    let font_family = ctx
        .theme
        .color("fontFamily")
        .map(|s| s.to_string())
        .unwrap_or_else(|| Font::default().family);
    let is_monospace = font_family.to_ascii_lowercase().contains("mono");
    let font = Font {
        family: font_family,
        size: font_size,
        weight: if is_monospace {
            crate::scene::NORMAL_WEIGHT
        } else {
            crate::scene::BOLD_WEIGHT
        },
        italic: false,
    };

    let mut label = create_label(
        &LabelOptions {
            text: node.label.clone(),
            font,
            markdown: false,
            width: f64::INFINITY,
            centered: false,
            class: "label".into(),
            source: Some(SourceRef::node(node.id.clone())),
        },
        ctx.measure,
    )
    .map_err(|error| ShapeError {
        message: error.to_string(),
    })?;
    label.class = Some("label label-group text".into());
    let bbox = text_size(&label);

    let w = node.width.unwrap_or(0.0).max(bbox.width);
    let mut h = node.height.unwrap_or(0.0).max(bbox.height);
    // No members and no methods: `h += GAP`, and `renderExtraBox` is true by default.
    h += GAP;

    let y = -h / 2.0;
    let extra_height = PADDING * 2.0;
    let rect = Rect::centered(
        Point::ZERO,
        Size::new(w + 2.0 * PADDING, h + 2.0 * PADDING + extra_height),
    );
    node.bounds = rect;

    let mut shape = Group::new().with_class("basic label-container outer-path");
    // handDrawn: rough.js (later)
    shape.push(
        RectShape::new(rect, 0.0, shape_style()).with_source(SourceRef::node(node.id.clone())),
    );
    group.push(shape);

    // The empty annotation and members groups measure 0, less the PADDING/2 the empty-box
    // layout subtracts, so they contribute -PADDING/2 to the divider positions.
    let annotation_height = -PADDING / 2.0;
    let label_height = bbox.height - PADDING / 2.0;
    let members_height = -PADDING / 2.0;
    let first_line = annotation_height + label_height + y + PADDING;
    let second_line = annotation_height + label_height + members_height + y + GAP * 2.0 + PADDING;
    for line_y in [first_line, second_line] {
        let mut divider = Path::new();
        divider
            .move_to(Point::new(rect.x, line_y))
            .line_to(Point::new(rect.right(), line_y + 0.001));
        let mut divider_group = Group::new()
            .with_class("divider")
            .with_source(SourceRef::node(node.id.clone()));
        divider_group.push(
            PathShape::new(divider, shape_style()).with_source(SourceRef::node(node.id.clone())),
        );
        group.push(divider_group);
    }

    label.transform = Transform::translate(-bbox.width, y);
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

    fn collect_paths<'a>(items: &'a [Item], out: &mut Vec<&'a PathShape>) {
        for item in items {
            match item {
                Item::Path(path) => out.push(path),
                Item::Group(group) => collect_paths(&group.items, out),
                _ => {}
            }
        }
    }

    #[test]
    fn matches_upstream_class_box_fixtures() {
        let fixture: Value =
            serde_json::from_str(include_str!("../testdata/shapes/class_box.json")).unwrap();
        let ctx = ShapeContext {
            measure: &FallbackMeasure,
            theme: &crate::theme::ThemeVariables::default(),
        };
        for case in fixture["cases"].as_array().unwrap() {
            let label = case["label"].as_str().unwrap();
            let mut node = Node::new("n1", label, "classBox");
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

            let mut paths = Vec::new();
            collect_paths(&rendered.group.items, &mut paths);
            let dividers: Vec<_> = paths
                .into_iter()
                .filter(|path| path.path.commands.len() == 2)
                .collect();
            let expected: Vec<_> = case["elements"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|element| {
                    element["tag"] == "path"
                        && element["commands"]
                            .as_array()
                            .is_some_and(|commands| commands.len() == 4 && commands[1]["op"] == "C")
                })
                .collect();
            assert_eq!(dividers.len(), expected.len());
            for (divider, element) in dividers.iter().zip(expected) {
                let start = divider.path.commands[0].end_point().unwrap();
                let end = divider.path.commands[1].end_point().unwrap();
                close(start.x, element["commands"][0]["x"].as_f64().unwrap());
                close(start.y, element["commands"][0]["y"].as_f64().unwrap());
                close(end.x, element["commands"][1]["x"].as_f64().unwrap());
                close(end.y, element["commands"][1]["y"].as_f64().unwrap());
            }
        }
    }
}
