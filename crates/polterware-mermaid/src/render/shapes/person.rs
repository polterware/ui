//! Classic port of upstream `shapes/person.ts`: a circular head above a rounded body, with
//! the edge outline built from the head's exposed arc and four body corner arcs.
use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, generate_circle_points, prepare,
    shape_style,
};
use crate::scene::{EllipseShape, Group, Item, Point, Rect, RectShape, Size, SourceRef, Transform};

/// `HEAD_SEGMENTS` in person.ts.
const HEAD_SEGMENTS: usize = 24;
/// Number of points per body corner arc.
const CORNER_SEGMENTS: usize = 12;

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (group, label, text_size) = prepare(&node, ctx)?;
    let padding = node.padding;
    let w = (text_size.width + padding * 2.0)
        .max(node.width.unwrap_or(0.0))
        .max(100.0);
    let head_radius = (w * 0.23).clamp(16.0, 56.0);
    let overlap = head_radius * 0.27;
    let body_height = (text_size.height + padding * 2.0).max(match node.height {
        Some(height) => height - (2.0 * head_radius - overlap),
        None => 0.0,
    });
    let body_radius = (w * 0.177).min(body_height * 0.45);
    let total_height = body_height + 2.0 * head_radius - overlap;
    let top = -total_height / 2.0;
    let body_top = top + 2.0 * head_radius - overlap;
    let bounds = Rect::centered(Point::ZERO, Size::new(w, total_height));
    node.bounds = bounds;

    let mut group = group;
    let mut shape = Group::new().with_class("basic label-container");
    // handDrawn: rough.js (later)
    shape.push(
        RectShape::new(
            Rect::new(-w / 2.0, body_top, w, body_height),
            body_radius,
            shape_style(),
        )
        .with_source(SourceRef::node(node.id.clone())),
    );
    shape.push(
        EllipseShape::new(
            Point::new(0.0, top + head_radius),
            head_radius,
            head_radius,
            shape_style(),
        )
        .with_source(SourceRef::node(node.id.clone())),
    );
    group.push(shape);

    let body_center_y = body_top + body_height / 2.0;
    let mut label = label;
    label.transform = Transform::translate(
        -text_size.width / 2.0,
        body_center_y - text_size.height / 2.0,
    );
    group.push(Item::Group(label));

    let head_center_y = top + head_radius;
    let phi_right_degrees = ((body_top - head_center_y) / head_radius)
        .min(1.0)
        .asin()
        .to_degrees();
    let mut outline = generate_circle_points(
        0.0,
        -head_center_y,
        head_radius,
        HEAD_SEGMENTS,
        180.0 + phi_right_degrees,
        -phi_right_degrees,
    );
    outline.extend(generate_circle_points(
        -(-w / 2.0 + body_radius),
        -(body_top + body_radius),
        body_radius,
        CORNER_SEGMENTS,
        90.0,
        0.0,
    ));
    outline.extend(generate_circle_points(
        -(-w / 2.0 + body_radius),
        -(total_height / 2.0 - body_radius),
        body_radius,
        CORNER_SEGMENTS,
        360.0,
        270.0,
    ));
    outline.extend(generate_circle_points(
        -(w / 2.0 - body_radius),
        -(total_height / 2.0 - body_radius),
        body_radius,
        CORNER_SEGMENTS,
        270.0,
        180.0,
    ));
    outline.extend(generate_circle_points(
        -(w / 2.0 - body_radius),
        -(body_top + body_radius),
        body_radius,
        CORNER_SEGMENTS,
        180.0,
        90.0,
    ));
    node.intersect = IntersectKind::Polygon(outline);

    Ok(RenderedNode { node, group })
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

    #[test]
    fn matches_upstream_person_fixtures() {
        let fixture: Value =
            serde_json::from_str(include_str!("../testdata/shapes/person.json")).unwrap();
        let ctx = ShapeContext {
            measure: &FallbackMeasure,
            theme: &crate::theme::ThemeVariables::default(),
        };
        for case in fixture["cases"].as_array().unwrap() {
            let label = case["label"].as_str().unwrap();
            let mut node = Node::new("n1", label, "person");
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
            let body_element = elements.iter().find(|e| e["tag"] == "rect").unwrap();
            let head_element = elements.iter().find(|e| e["tag"] == "circle").unwrap();
            let mut found_body = None;
            let mut found_head = None;
            for item in &rendered.group.items {
                if let Item::Group(group) = item {
                    for child in &group.items {
                        match child {
                            Item::Rect(rect) => found_body = Some(rect),
                            Item::Ellipse(ellipse) => found_head = Some(ellipse),
                            _ => {}
                        }
                    }
                }
            }
            let body = found_body.expect("body");
            let head = found_head.expect("head");
            close(body.rect.x, body_element["x"].as_f64().unwrap());
            close(body.rect.y, body_element["y"].as_f64().unwrap());
            close(body.rect.width, body_element["width"].as_f64().unwrap());
            close(body.rect.height, body_element["height"].as_f64().unwrap());
            close(body.rx, body_element["rx"].as_f64().unwrap());
            close(head.center.x, head_element["cx"].as_f64().unwrap());
            close(head.center.y, head_element["cy"].as_f64().unwrap());
            close(head.radius_x, head_element["r"].as_f64().unwrap());
        }
    }
}
