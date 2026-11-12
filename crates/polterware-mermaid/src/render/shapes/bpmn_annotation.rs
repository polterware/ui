//! Port of upstream `shapes/bpmnAnnotation.ts`: a 160-wide wrapped caption with a bracket down
//! one side, sized by its label plus 8px padding.
//!
//! Classic look only; upstream never calls rough.js here. Upstream reads
//! `node.metadata.attachFace`; the shared [`Node`] does not carry metadata, so [`draw`] uses the
//! default left face and [`draw_with_face`] carries the right-side variant. The shared
//! intersection model cannot express `faceProjectIntersect` with a fixed face; [`intersect`] is
//! the exact port, while the node carries a rectangle for the annotation box.

use super::bpmn_shape_core::{
    BpmnLabel, ShapeFace, bpmn_group, bpmn_label, classed_path, face_project_intersect_at,
    reserve_bounds,
};
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError};
use crate::scene::{Group, Item, Path, Point, SourceRef, Transform};

/// The annotation's `node.wrappingWidth`.
pub const WRAPPING_WIDTH: f64 = 160.0;
const PADDING: f64 = 8.0;

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    draw_with_face(node, ctx, ShapeFace::Left)
}

/// The full upstream handler, including the bracket side selected by `attachFace`.
pub fn draw_with_face(
    node: &Node,
    ctx: &ShapeContext<'_>,
    face: ShapeFace,
) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();

    let BpmnLabel {
        group: label,
        width: label_width,
        height: label_height,
    } = bpmn_label(&node, ctx, WRAPPING_WIDTH)?;

    let height = 28.0_f64.max(label_height + PADDING * 2.0);
    let width = label_width + PADDING * 2.0 + 6.0;

    let mut group = bpmn_group(&node, "bpmn-annotation");
    let mut body = Group::new().with_class("bpmn-annotation-body");

    let mut bracket = Path::new();
    if face == ShapeFace::Right {
        bracket
            .move_to(Point::new(width / 2.0 - 6.0, -height / 2.0))
            .line_to(Point::new(width / 2.0, -height / 2.0))
            .line_to(Point::new(width / 2.0, height / 2.0))
            .line_to(Point::new(width / 2.0 - 6.0, height / 2.0));
    } else {
        bracket
            .move_to(Point::new(-width / 2.0 + 6.0, -height / 2.0))
            .line_to(Point::new(-width / 2.0, -height / 2.0))
            .line_to(Point::new(-width / 2.0, height / 2.0))
            .line_to(Point::new(-width / 2.0 + 6.0, height / 2.0));
    }
    body.push(classed_path(
        "bpmn-annotation-bracket",
        bracket,
        SourceRef::node(node.id.clone()),
    ));
    group.push(body);

    let text_left = if face == ShapeFace::Right {
        -width / 2.0 + PADDING
    } else {
        -width / 2.0 + PADDING + 6.0
    };
    let mut label = label;
    label.transform = Transform::translate(text_left, -label_height / 2.0);
    group.push(Item::Group(label));

    group.push(reserve_bounds(&mut node, width, height));
    group.items.rotate_right(1);

    node.intersect = IntersectKind::FaceProject {
        width,
        height,
        face: Some(face),
    };
    Ok(RenderedNode { node, group })
}

/// The exact `faceProjectIntersect` attachment for an annotation of `width × height`.
pub fn intersect(face: ShapeFace, width: f64, height: f64, point: Point) -> Point {
    face_project_intersect_at(Point::ZERO, width, height, point, Some(face))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::shapes::bpmn_shape_core::fixture_support as fixture;
    use crate::theme::ThemeVariables;

    #[test]
    fn matches_upstream_annotation_geometry() {
        let fixture = fixture::load(include_str!("../testdata/shapes/bpmn-annotation.json"));
        let measure = fixture.measure();
        let theme = ThemeVariables::default();
        for case in &fixture.cases {
            let rendered = draw(
                &fixture.node(case),
                &ShapeContext {
                    measure: &measure,
                    theme: &theme,
                },
            )
            .expect("annotation renders");
            fixture::assert_node(case, &rendered);
            fixture::assert_label(case, &rendered.group);
            fixture::assert_elements(case, &rendered.group);
            fixture::assert_intersects(case, |point| {
                intersect(ShapeFace::Left, case.node.width, case.node.height, point)
            });
        }
    }

    #[test]
    fn the_right_face_mirrors_the_bracket_and_text() {
        let node = Node::new("n", "Go", "bpmn-annotation");
        let measure = fixture::FixedMeasure {
            char_width: 7.0,
            line_height: 16.0,
        };
        let theme = ThemeVariables::default();
        let rendered = draw_with_face(
            &node,
            &ShapeContext {
                measure: &measure,
                theme: &theme,
            },
            ShapeFace::Right,
        )
        .expect("annotation renders");
        let label = rendered
            .group
            .items
            .iter()
            .find_map(|item| match item {
                Item::Group(group) if group.class.as_deref() == Some("label") => Some(group),
                _ => None,
            })
            .expect("label");
        assert_eq!(label.transform.e, -18.0 + PADDING);
        assert_eq!(
            intersect(ShapeFace::Right, 36.0, 32.0, Point::new(1000.0, 0.0)).x,
            18.0
        );
    }
}
