//! Port of upstream `shapes/bpmnGateway.ts`: the fixed 50 × 50 diamond, optional glyph, and
//! caption below.
//!
//! Classic look only; upstream never calls rough.js for a gateway. Upstream's `BpmnNode.icon`
//! is not part of the shared [`Node`], so [`draw`] renders the no-glyph case and
//! [`draw_with_icon`] carries the full upstream logic. The shared intersection model cannot
//! express `faceCentreIntersect`; [`super::bpmn_shape_core::face_centre_intersect`] is the exact
//! port, and the node carries the diamond polygon so an edge still docks on the drawn outline.

use super::bpmn_shape_core::{
    BpmnLabel, GATEWAY_SIZE, ICON_SIZE, LABEL_GAP, append_glyph, bpmn_group, bpmn_label,
    classed_path, face_centre_intersect, label_wrap_width, position_label_below, reserve_bounds,
};
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError};
use crate::scene::{Group, Item, Path, Point, SourceRef};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    draw_with_icon(node, ctx, None)
}

/// The full upstream handler, including the optional `bpmn:` glyph at `ICON_SIZE + 2`.
pub fn draw_with_icon(
    node: &Node,
    ctx: &ShapeContext<'_>,
    icon: Option<&str>,
) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let half = GATEWAY_SIZE / 2.0;

    let BpmnLabel {
        group: label,
        width: label_width,
        height: label_height,
    } = bpmn_label(&node, ctx, label_wrap_width(&node))?;

    let mut group = bpmn_group(&node, "bpmn-gateway");

    let has_label = label_height > 0.0 && !node.label.is_empty();
    let caption_band = if has_label {
        LABEL_GAP + label_height
    } else {
        0.0
    };
    let total_height = GATEWAY_SIZE + 2.0 * caption_band;
    let total_width = GATEWAY_SIZE.max(if has_label { label_width } else { 0.0 });

    group.push(reserve_bounds(&mut node, total_width, total_height));

    let mut body = Group::new().with_class("bpmn-gateway-body");
    let diamond = [
        Point::new(0.0, -half),
        Point::new(half, 0.0),
        Point::new(0.0, half),
        Point::new(-half, 0.0),
    ];
    body.push(classed_path(
        "bpmn-gateway-diamond",
        Path::polygon(diamond.iter().copied()),
        SourceRef::node(node.id.clone()),
    ));
    append_glyph(&mut body, icon, ICON_SIZE + 2.0);
    group.push(body);

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

    node.intersect = IntersectKind::FaceCentre {
        width: GATEWAY_SIZE,
        height: GATEWAY_SIZE,
    };
    Ok(RenderedNode { node, group })
}

/// The exact `faceCentreIntersect` attachment for a gateway of `GATEWAY_SIZE` centred at zero.
pub fn intersect(point: Point) -> Point {
    face_centre_intersect(Point::ZERO, GATEWAY_SIZE, GATEWAY_SIZE, point)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::shapes::bpmn_shape_core::fixture_support as fixture;
    use crate::theme::ThemeVariables;

    #[test]
    fn matches_upstream_gateway_geometry() {
        let fixture = fixture::load(include_str!("../testdata/shapes/bpmn-gateway.json"));
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
            .expect("gateway renders");
            fixture::assert_node(case, &rendered);
            fixture::assert_label(case, &rendered.group);
            fixture::assert_elements(case, &rendered.group);
            fixture::assert_intersects(case, intersect);
        }
    }
}
