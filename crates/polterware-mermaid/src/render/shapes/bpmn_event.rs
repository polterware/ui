//! Port of upstream `shapes/bpmnEvent.ts` (`renderBpmnEvent`), the shared event body used by
//! `bpmnStart`, `bpmnIntermediate`, `bpmnBoundary`, and `bpmnEnd`.
//!
//! The classic look is what upstream draws: concentric SVG circles with an inline ring weight,
//! an optional `bpmn:...` glyph, and the caption below the notation. Upstream's `BpmnNode.icon`
//! is not part of this crate's shared [`Node`], so [`draw`] renders the no-glyph case and
//! [`render_with_icon`] carries the full upstream logic for a caller that has the icon. The
//! shared intersection model cannot express `ringIntersect`; [`super::bpmn_shape_core::ring_intersect`]
//! is the exact port, while the node carries an 18-radius ellipse for the drawn ring.

use super::bpmn_shape_core::{
    BpmnLabel, EVENT_DIAMETER, ICON_SIZE, LABEL_GAP, append_glyph, bpmn_group, bpmn_label,
    classed_circle, event_rings, label_wrap_width, position_label_below, reserve_bounds,
    ring_style,
};
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError};
use crate::scene::{Group, Item, Point, SourceRef};

/// `renderBpmnEvent`: draw the rings and caption of one event position.
pub fn render(
    node: &Node,
    ctx: &ShapeContext<'_>,
    position: &str,
) -> Result<RenderedNode, ShapeError> {
    render_with_icon(node, ctx, position, None)
}

/// The full upstream handler, including the optional `bpmn:` glyph.
pub fn render_with_icon(
    node: &Node,
    ctx: &ShapeContext<'_>,
    position: &str,
    icon: Option<&str>,
) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let rings = event_rings(position).ok_or_else(|| ShapeError {
        message: format!("unknown BPMN event position: {position}"),
    })?;
    let radius = EVENT_DIAMETER / 2.0;

    let BpmnLabel {
        group: label,
        width: label_width,
        height: label_height,
    } = bpmn_label(&node, ctx, label_wrap_width(&node))?;

    let mut group = bpmn_group(&node, &format!("bpmn-event bpmn-event-{position}"));

    let has_label = label_height > 0.0 && !node.label.is_empty();
    let caption_band = if has_label {
        LABEL_GAP + label_height
    } else {
        0.0
    };
    let total_height = EVENT_DIAMETER + 2.0 * caption_band;
    let total_width = EVENT_DIAMETER.max(if has_label { label_width } else { 0.0 });

    group.push(reserve_bounds(&mut node, total_width, total_height));

    let mut body = Group::new().with_class("bpmn-event-body");
    for ring in 0..rings.rings {
        let class = if ring == 0 {
            "bpmn-event-ring"
        } else {
            "bpmn-event-ring bpmn-event-ring-inner"
        };
        body.push(classed_circle(
            class,
            Point::ZERO,
            radius - ring as f64 * 3.0,
            ring_style(rings.stroke_width),
            SourceRef::node(node.id.clone()),
        ));
    }
    append_glyph(&mut body, icon, ICON_SIZE);
    group.push(body);

    let mut label = label;
    if has_label {
        position_label_below(
            &mut label,
            label_width,
            label_height,
            radius + LABEL_GAP + label_height / 2.0,
        );
    }
    group.push(Item::Group(label));

    node.intersect = IntersectKind::Ring {
        diameter: EVENT_DIAMETER,
    };
    Ok(RenderedNode { node, group })
}

// Compatibility entry for the current registry; upstream `bpmnEvent.ts` is a helper, not a shape.
pub fn draw(node: &Node, _ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    Err(ShapeError {
        message: format!(
            "{} is a rendering helper, not a shape (rendering-elements/shapes/bpmnEvent.ts)",
            node.shape
        ),
    })
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::render::shapes::bpmn_shape_core::fixture_support as fixture;
    use crate::theme::ThemeVariables;

    /// Compares `render` with one generated upstream fixture.
    pub fn assert_fixture(fixture_text: &str, position: &str) {
        let fixture = fixture::load(fixture_text);
        let measure = fixture.measure();
        let theme = ThemeVariables::default();
        for case in &fixture.cases {
            let node = fixture.node(case);
            let rendered = render(
                &node,
                &ShapeContext {
                    measure: &measure,
                    theme: &theme,
                },
                position,
            )
            .expect("event renders");
            fixture::assert_node(case, &rendered);
            fixture::assert_label(case, &rendered.group);
            fixture::assert_elements(case, &rendered.group);
            fixture::assert_intersects(case, |point| {
                crate::render::shapes::bpmn_shape_core::ring_intersect(
                    Point::ZERO,
                    EVENT_DIAMETER,
                    point,
                )
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::shapes::bpmn_shape_core::fixture_support as fixture;
    use crate::theme::ThemeVariables;

    #[test]
    fn matches_upstream_event_geometry() {
        test_support::assert_fixture(
            include_str!("../testdata/shapes/bpmn-event.json"),
            "intermediate",
        );
    }

    #[test]
    fn glyphs_are_centred_on_the_event_body() {
        let node = Node::new("n", "Go", "bpmn-start");
        let measure = fixture::FixedMeasure {
            char_width: 7.0,
            line_height: 16.0,
        };
        let theme = ThemeVariables::default();
        let rendered = render_with_icon(
            &node,
            &ShapeContext {
                measure: &measure,
                theme: &theme,
            },
            "start",
            Some("bpmn:message"),
        )
        .expect("event renders");
        let glyph = rendered
            .group
            .items
            .iter()
            .find_map(|item| match item {
                Item::Group(group) if group.class.as_deref() == Some("bpmn-event-body") => {
                    group.items.iter().find_map(|item| match item {
                        Item::Group(group) if group.class.as_deref() == Some("bpmn-glyph") => {
                            Some(group)
                        }
                        _ => None,
                    })
                }
                _ => None,
            })
            .expect("glyph group");
        assert_eq!(glyph.transform.e, -ICON_SIZE / 2.0);
        assert_eq!(glyph.transform.f, -ICON_SIZE / 2.0);
        assert!((glyph.transform.a - ICON_SIZE / 24.0).abs() < 1e-9);
    }
}
