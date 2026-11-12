//! Port of upstream `shapes/bpmnActivity.ts`: a rounded activity box whose height grows with the
//! caption, with an optional corner glyph and a bottom marker row.
//!
//! Classic look only; upstream never calls rough.js for an activity. Upstream reads
//! `BpmnNode.icon` and `.markers`, which the shared [`Node`] does not carry yet: [`draw`]
//! renders the plain case, and [`draw_with_extras`] carries the full upstream geometry for a
//! caller that has them. The shared intersection model cannot express `faceProjectIntersect`;
//! [`intersect`] is the exact port, while the node carries a rectangle so an edge still docks
//! on the painted box.

use super::bpmn_shape_core::{
    ACTIVITY_HEIGHT, ACTIVITY_WIDTH, BpmnLabel, ICON_SIZE, append_glyph_at, bpmn_group, bpmn_label,
    face_project_intersect, label_wrap_width, update_bounds,
};
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError};
use crate::scene::{Group, Item, Point, Rect, RectShape, SourceRef, Transform};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    draw_with_extras(node, ctx, None, &[])
}

/// The full upstream handler, including the corner glyph and the bottom marker row.
pub fn draw_with_extras(
    node: &Node,
    ctx: &ShapeContext<'_>,
    icon: Option<&str>,
    markers: &[&str],
) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();

    let BpmnLabel {
        group: label,
        width: label_width,
        height: label_height,
    } = bpmn_label(&node, ctx, label_wrap_width(&node))?;

    let top_inset = if icon.is_some() {
        ICON_SIZE + 6.0
    } else {
        10.0
    };
    let bottom_inset = if markers.is_empty() {
        10.0
    } else {
        ICON_SIZE + 4.0
    };
    let width = ACTIVITY_WIDTH.max(label_width + 16.0);
    let height = ACTIVITY_HEIGHT.max(label_height + top_inset + bottom_inset);

    let mut group = bpmn_group(&node, "bpmn-activity");
    let mut body = Group::new().with_class("bpmn-activity-body");
    let mut rect = Group::new().with_class("bpmn-activity-rect");
    rect.push(
        RectShape::new(
            Rect::new(-width / 2.0, -height / 2.0, width, height),
            10.0,
            super::shape_style(),
        )
        .with_source(SourceRef::node(node.id.clone())),
    );
    body.push(rect);

    if icon.is_some() {
        let mut corner = Group::new().with_class("bpmn-activity-icon");
        append_glyph_at(
            &mut corner,
            icon,
            ICON_SIZE,
            Point::new(-width / 2.0 + 6.0, -height / 2.0 + 6.0),
        );
        body.push(corner);
    }

    if !markers.is_empty() {
        let step = ICON_SIZE - 2.0;
        let row_width = markers.len() as f64 * step;
        let mut row = Group::new().with_class("bpmn-activity-markers");
        for (index, marker) in markers.iter().enumerate() {
            let mut cell = Group::new();
            append_glyph_at(
                &mut cell,
                Some(marker),
                step,
                Point::new(-row_width / 2.0 + index as f64 * step, 0.0),
            );
            row.push(cell);
        }
        row.transform = Transform::translate(0.0, height / 2.0 - step - 3.0);
        body.push(row);
    }

    group.push(body);

    let mut label = label;
    if label_height > 0.0 {
        let band_top = -height / 2.0 + top_inset;
        let band_bottom = height / 2.0 - bottom_inset;
        super::bpmn_shape_core::position_label_below(
            &mut label,
            label_width,
            label_height,
            (band_top + band_bottom) / 2.0,
        );
    }
    group.push(Item::Group(label));

    update_bounds(&mut node, width, height);
    node.intersect = IntersectKind::Rect;
    Ok(RenderedNode { node, group })
}

/// The exact `faceProjectIntersect` attachment for an activity box centred at zero.
pub fn intersect(width: f64, height: f64, point: Point) -> Point {
    face_project_intersect(Point::ZERO, width, height, point)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::shapes::bpmn_shape_core::fixture_support as fixture;
    use crate::render::shapes::bpmn_shape_core::{ACTIVITY_HEIGHT, ACTIVITY_WIDTH};
    use crate::theme::ThemeVariables;

    #[test]
    fn matches_upstream_activity_geometry() {
        let fixture = fixture::load(include_str!("../testdata/shapes/bpmn-activity.json"));
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
            .expect("activity renders");
            fixture::assert_node(case, &rendered);
            fixture::assert_label(case, &rendered.group);
            fixture::assert_elements(case, &rendered.group);
            fixture::assert_intersects(case, |point| {
                intersect(
                    case.node.width.max(ACTIVITY_WIDTH),
                    case.node.height.max(ACTIVITY_HEIGHT),
                    point,
                )
            });
        }
    }

    #[test]
    fn extras_grow_the_box_and_add_glyphs() {
        let node = Node::new("n", "Work", "bpmn-activity");
        let measure = fixture::FixedMeasure {
            char_width: 7.0,
            line_height: 16.0,
        };
        let theme = ThemeVariables::default();
        let rendered = draw_with_extras(
            &node,
            &ShapeContext {
                measure: &measure,
                theme: &theme,
            },
            Some("bpmn:service"),
            &["bpmn:loop", "bpmn:parallel"],
        )
        .expect("activity renders");
        assert_eq!(rendered.node.bounds.height, ACTIVITY_HEIGHT);
        let body = rendered
            .group
            .items
            .iter()
            .find_map(|item| match item {
                Item::Group(group) if group.class.as_deref() == Some("bpmn-activity-body") => {
                    Some(group)
                }
                _ => None,
            })
            .expect("body group");
        assert!(body.items.iter().any(|item| {
            matches!(item, Item::Group(group) if group.class.as_deref() == Some("bpmn-activity-icon"))
        }));
        assert!(body.items.iter().any(|item| {
            matches!(item, Item::Group(group) if group.class.as_deref() == Some("bpmn-activity-markers"))
        }));
    }
}
