//! Classic port of upstream `shapes/collapsedGroup.ts` for a flowchart's default collapsed
//! subgraph (`@{ view: collapsed }`). `Node` carries no `metadata.containerType`, so the
//! agentflow `flow` variant (transparent fill, 0.75px stroke, radius 10) is not selected.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare};
use crate::scene::{
    Color, EllipseShape, Group, Item, Path, PathShape, PathStyle, Point, Rect, RectShape, Size,
    SourceRef, Transform,
};

/// Height reserved for the ellipsis indicator row.
const INDICATOR_ROW_HEIGHT: f64 = 20.0;
/// Vertical gap between the title and the indicator row.
const SEPARATOR_GAP: f64 = 8.0;
/// Minimum width for the collapsed group shape.
const MIN_WIDTH: f64 = 80.0;
/// Corner radius matching the expanded subgraph.
const RADIUS: f64 = 8.0;
/// Ellipsis dot radius and spacing.
const DOT_RADIUS: f64 = 2.5;
const DOT_SPACING: f64 = 10.0;

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, label, text_size) = prepare(&node, ctx)?;
    let fill = ctx
        .theme
        .color("clusterBkg")
        .unwrap_or("#ffffde")
        .to_string();
    let stroke = ctx
        .theme
        .color("clusterBorder")
        .unwrap_or("#aaaa33")
        .to_string();

    let padding = node.padding;
    let title_height = text_size.height;
    let total_width = (text_size.width + padding * 2.0)
        .max(MIN_WIDTH)
        .max(node.width.unwrap_or(0.0));
    let total_height = (title_height + SEPARATOR_GAP + INDICATOR_ROW_HEIGHT + padding * 2.0)
        .max(node.height.unwrap_or(0.0));
    let bounds = Rect::centered(Point::ZERO, Size::new(total_width, total_height));
    node.bounds = bounds;
    let x = -total_width / 2.0;
    let y = -total_height / 2.0;

    let rect_style = PathStyle {
        fill: Some(Color::new(fill)),
        stroke: Some(Color::new(stroke.clone())),
        ..PathStyle::default()
    };
    let mut shape = Group::new().with_class("basic label-container collapsed-group");
    // handDrawn: rough.js (later)
    shape.push(
        RectShape::new(bounds, RADIUS, rect_style).with_source(SourceRef::node(node.id.clone())),
    );
    group.push(shape);

    let label_shift_y = -(SEPARATOR_GAP + INDICATOR_ROW_HEIGHT) / 2.0;
    let mut label = label;
    label.transform = Transform::translate(
        -text_size.width / 2.0,
        -text_size.height / 2.0 + label_shift_y,
    );
    group.push(Item::Group(label));

    let separator_y = y + padding + title_height + SEPARATOR_GAP;
    let mut separator = Path::new();
    separator
        .move_to(Point::new(x + 8.0, separator_y))
        .line_to(Point::new(x + total_width - 8.0, separator_y));
    let separator_style = PathStyle::stroked(Color::new(stroke.clone()), 1.0).with_dash([3.0, 3.0]);
    let mut separator_group = Group::new()
        .with_class("collapsed-separator")
        .with_source(SourceRef::node(node.id.clone()));
    separator_group.push(
        PathShape::new(separator, separator_style).with_source(SourceRef::node(node.id.clone())),
    );
    group.push(separator_group);

    let dot_y = separator_y + INDICATOR_ROW_HEIGHT / 2.0;
    for index in -1..=1 {
        let mut dot_group = Group::new()
            .with_class("collapsed-indicator")
            .with_source(SourceRef::node(node.id.clone()));
        dot_group.push(
            EllipseShape::new(
                Point::new(index as f64 * DOT_SPACING, dot_y),
                DOT_RADIUS,
                DOT_RADIUS,
                PathStyle::filled(Color::new(stroke.clone())),
            )
            .with_source(SourceRef::node(node.id.clone())),
        );
        group.push(dot_group);
    }

    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::render::intersect::Intersectable;
    use crate::scene::{EllipseShape, Item, PathShape, RectShape};
    use serde_json::Value;

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() <= 0.01, "{actual} != {expected}");
    }

    fn collect<'a>(
        items: &'a [Item],
        rects: &mut Vec<&'a RectShape>,
        paths: &mut Vec<&'a PathShape>,
        dots: &mut Vec<&'a EllipseShape>,
    ) {
        for item in items {
            match item {
                Item::Rect(rect) => rects.push(rect),
                Item::Path(path) => paths.push(path),
                Item::Ellipse(dot) => dots.push(dot),
                Item::Group(group) => collect(&group.items, rects, paths, dots),
                _ => {}
            }
        }
    }

    #[test]
    fn matches_upstream_collapsed_group_fixtures() {
        let fixture: Value =
            serde_json::from_str(include_str!("../testdata/shapes/collapsed_group.json")).unwrap();
        let ctx = ShapeContext {
            measure: &FallbackMeasure,
            theme: &crate::theme::ThemeVariables::default(),
        };
        for case in fixture["cases"].as_array().unwrap() {
            let label = case["label"].as_str().unwrap();
            let mut node = Node::new("n1", label, "collapsedGroup");
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

            let elements = case["elements"].as_array().unwrap();
            let rect_expected = elements.iter().find(|e| e["tag"] == "rect").unwrap();
            let line_expected = elements.iter().find(|e| e["tag"] == "line").unwrap();
            let dots_expected: Vec<_> = elements.iter().filter(|e| e["tag"] == "circle").collect();

            let (mut rects, mut paths, mut dots) = (Vec::new(), Vec::new(), Vec::new());
            collect(&rendered.group.items, &mut rects, &mut paths, &mut dots);
            let rect = rects.first().expect("rect");
            close(rect.rect.x, rect_expected["x"].as_f64().unwrap());
            close(rect.rect.y, rect_expected["y"].as_f64().unwrap());
            close(rect.rect.width, rect_expected["width"].as_f64().unwrap());
            close(rect.rect.height, rect_expected["height"].as_f64().unwrap());
            close(rect.rx, rect_expected["rx"].as_f64().unwrap());
            assert_eq!(
                rect.style.fill.as_ref().map(|c| c.0.as_str()),
                rect_expected["fill"].as_str()
            );
            assert_eq!(
                rect.style.stroke.as_ref().map(|c| c.0.as_str()),
                rect_expected["stroke"].as_str()
            );

            let line = paths.first().expect("separator");
            let start = line.path.commands[0].end_point().unwrap();
            let end = line.path.commands[1].end_point().unwrap();
            close(start.x, line_expected["x1"].as_f64().unwrap());
            close(start.y, line_expected["y1"].as_f64().unwrap());
            close(end.x, line_expected["x2"].as_f64().unwrap());
            close(end.y, line_expected["y2"].as_f64().unwrap());
            assert_eq!(line.style.dash, vec![3.0, 3.0]);

            assert_eq!(dots.len(), dots_expected.len());
            for (dot, expected) in dots.iter().zip(dots_expected) {
                close(dot.center.x, expected["cx"].as_f64().unwrap());
                close(dot.center.y, expected["cy"].as_f64().unwrap());
                close(dot.radius_x, expected["r"].as_f64().unwrap());
            }
        }
    }
}
