//! Port of upstream `shapes/bpmnDataStore.ts`: the 50 × 50 cylinder with three store rings and
//! the caption below.
//!
//! Classic look only; upstream never calls rough.js here. The shared intersection model cannot
//! express `faceProjectIntersect`; [`intersect`] is the exact port, while the node carries a
//! rectangle for the drawn store.

use super::bpmn_shape_core::{
    BpmnLabel, LABEL_GAP, bpmn_group, bpmn_label, classed_path, face_project_intersect,
    label_wrap_width, position_label_below, reserve_bounds,
};
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError};
use crate::scene::{Group, Item, Path, Point, SourceRef};

const STORE_SIZE: f64 = 50.0;

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();

    let BpmnLabel {
        group: label,
        width: label_width,
        height: label_height,
    } = bpmn_label(&node, ctx, label_wrap_width(&node))?;

    let mut group = bpmn_group(&node, "bpmn-data-store");
    let mut body = Group::new().with_class("bpmn-data-body");

    let half = STORE_SIZE / 2.0;
    let ry = 6.0;
    let mut store = Path::new();
    store
        .move_to(Point::new(-half, -half + ry))
        .arc_to(half, ry, 0.0, false, true, Point::new(half, -half + ry))
        .line_to(Point::new(half, half - ry))
        .arc_to(half, ry, 0.0, false, true, Point::new(-half, half - ry))
        .close();
    body.push(classed_path(
        "bpmn-store-body",
        store,
        SourceRef::node(node.id.clone()),
    ));

    for offset in [0.0, 5.0, 10.0] {
        let mut ring = Path::new();
        ring.move_to(Point::new(-half, -half + ry + offset)).arc_to(
            half,
            ry,
            0.0,
            false,
            false,
            Point::new(half, -half + ry + offset),
        );
        body.push(classed_path(
            "bpmn-store-rings",
            ring,
            SourceRef::node(node.id.clone()),
        ));
    }

    group.push(body);

    let has_label = label_height > 0.0 && !node.label.is_empty();
    let caption_band = if has_label {
        LABEL_GAP + label_height
    } else {
        0.0
    };
    let total_height = STORE_SIZE + 2.0 * caption_band;
    let total_width = STORE_SIZE.max(if has_label { label_width } else { 0.0 });

    let mut label = label;
    if has_label {
        position_label_below(
            &mut label,
            label_width,
            label_height,
            half + LABEL_GAP + label_height / 2.0,
        );
    }
    group.push(Item::Group(label));

    group.push(reserve_bounds(&mut node, total_width, total_height));
    group.items.rotate_right(1);

    node.intersect = IntersectKind::FaceProject {
        width: STORE_SIZE,
        height: STORE_SIZE,
        face: None,
    };
    Ok(RenderedNode { node, group })
}

/// The exact `faceProjectIntersect` attachment for a data store centred at zero.
pub fn intersect(point: Point) -> Point {
    face_project_intersect(Point::ZERO, STORE_SIZE, STORE_SIZE, point)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::shapes::bpmn_shape_core::fixture_support as fixture;
    use crate::theme::ThemeVariables;

    #[test]
    fn matches_upstream_data_store_geometry() {
        let fixture = fixture::load(include_str!("../testdata/shapes/bpmn-data-store.json"));
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
            .expect("data store renders");
            fixture::assert_node(case, &rendered);
            fixture::assert_label(case, &rendered.group);
            fixture::assert_elements(case, &rendered.group);
            fixture::assert_intersects(case, intersect);
        }
    }
}
