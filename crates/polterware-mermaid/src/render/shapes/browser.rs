//! Classic port of upstream `shapes/browser.ts`: a rounded window with a chrome bar of three
//! dots, a separator line, and an address-bar outline, with the label in the body.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{
    Color, EllipseShape, Group, Item, Path, PathShape, PathStyle, Point, Rect, RectShape, Size,
    SourceRef, Transform,
};

/// Height of the browser chrome bar.
const BAR_HEIGHT: f64 = 18.0;
/// Window corner radius.
const RADIUS: f64 = 12.0;
/// Minimum browser width.
const MIN_WIDTH: f64 = 90.0;
/// Chrome dot radius.
const DOT_RADIUS: f64 = 2.5;

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
    let h = (text_size.height + padding * 2.0 + BAR_HEIGHT).max(node.height.unwrap_or(0.0));
    let bounds = Rect::centered(Point::ZERO, Size::new(w, h));
    node.bounds = bounds;
    let top = -h / 2.0;

    let mut group = group;
    let mut shape = Group::new().with_class("basic label-container");
    // handDrawn: rough.js (later)
    shape.push(
        RectShape::new(bounds, RADIUS, shape_style()).with_source(SourceRef::node(node.id.clone())),
    );

    let stroke = accent.clone().unwrap_or_else(|| "currentColor".to_string());
    let line_style = PathStyle::stroked(Color::new(stroke.clone()), 1.0);
    let mut line = Path::new();
    line.move_to(Point::new(-w / 2.0, top + BAR_HEIGHT))
        .line_to(Point::new(w / 2.0, top + BAR_HEIGHT));
    shape.push(PathShape::new(line, line_style).with_source(SourceRef::node(node.id.clone())));

    for index in 0..3 {
        shape.push(
            EllipseShape::new(
                Point::new(-w / 2.0 + 12.0 + index as f64 * 9.0, top + BAR_HEIGHT / 2.0),
                DOT_RADIUS,
                DOT_RADIUS,
                PathStyle::filled(Color::new(stroke.clone())),
            )
            .with_source(SourceRef::node(node.id.clone())),
        );
    }

    let mut address_style = PathStyle::stroked(Color::new(stroke), 1.0);
    address_style.opacity = 0.6;
    let address = Rect::new(
        -w / 2.0 + 44.0,
        top + 4.0,
        (w - 56.0).max(10.0),
        BAR_HEIGHT - 8.0,
    );
    let mut address_shape =
        RectShape::new(address, 3.0, address_style).with_source(SourceRef::node(node.id.clone()));
    address_shape.style.fill = Some(Color::none());
    shape.push(address_shape);
    group.push(shape);

    let body_center_y = top + BAR_HEIGHT + (h - BAR_HEIGHT) / 2.0;
    let mut label = label;
    label.transform = Transform::translate(
        -text_size.width / 2.0,
        body_center_y - text_size.height / 2.0,
    );
    group.push(Item::Group(label));

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
    fn matches_upstream_browser_fixtures() {
        let fixture: Value =
            serde_json::from_str(include_str!("../testdata/shapes/browser.json")).unwrap();
        let ctx = ShapeContext {
            measure: &FallbackMeasure,
            theme: &crate::theme::ThemeVariables::default(),
        };
        for case in fixture["cases"].as_array().unwrap() {
            let label = case["label"].as_str().unwrap();
            let mut node = Node::new("n1", label, "browser");
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
            let rects_expected: Vec<_> = elements.iter().filter(|e| e["tag"] == "rect").collect();
            let line_expected = elements.iter().find(|e| e["tag"] == "line").unwrap();
            let dots_expected: Vec<_> = elements.iter().filter(|e| e["tag"] == "circle").collect();

            let (mut rects, mut paths, mut dots) = (Vec::new(), Vec::new(), Vec::new());
            collect(&rendered.group.items, &mut rects, &mut paths, &mut dots);
            assert_eq!(rects.len(), rects_expected.len());
            for (rect, expected) in rects.iter().zip(&rects_expected) {
                close(rect.rect.x, expected["x"].as_f64().unwrap());
                close(rect.rect.y, expected["y"].as_f64().unwrap());
                close(rect.rect.width, expected["width"].as_f64().unwrap());
                close(rect.rect.height, expected["height"].as_f64().unwrap());
                close(rect.rx, expected["rx"].as_f64().unwrap());
            }

            let line = paths.first().expect("chrome line");
            let start = line.path.commands[0].end_point().unwrap();
            let end = line.path.commands[1].end_point().unwrap();
            close(start.x, line_expected["x1"].as_f64().unwrap());
            close(start.y, line_expected["y1"].as_f64().unwrap());
            close(end.x, line_expected["x2"].as_f64().unwrap());
            close(end.y, line_expected["y2"].as_f64().unwrap());

            assert_eq!(dots.len(), dots_expected.len());
            for (dot, expected) in dots.iter().zip(dots_expected) {
                close(dot.center.x, expected["cx"].as_f64().unwrap());
                close(dot.center.y, expected["cy"].as_f64().unwrap());
                close(dot.radius_x, expected["r"].as_f64().unwrap());
            }
        }
    }
}
