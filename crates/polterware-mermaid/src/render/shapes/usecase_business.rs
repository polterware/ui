//! Classic port of upstream `shapes/usecaseBusiness.ts`.
//!
//! Look: the business use case oval of the web design system: an ellipse with a diagonal
//! marker across its top-right edge, the same silhouette as `usecaseEllipse`.
//! Behavior: upstream `usecaseBusiness.ts`; [`draw_with`] takes the stereotype the upstream
//! node carries, because this crate's shared `Node` has no stereotype field yet.
//!
//! The per-item colour slot, the ARIA role and the leaf classes (`basic label-container
//! usecase-business-ellipse`, `usecase-business-marker`) belong to the DOM: the scene keeps
//! the shape class on the group and paints with the theme's node style. `handDrawn` is not
//! rendered: rough.js jitter stays a renderer TODO.

use super::{
    IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, get_node_classes, prepare,
    shape_style,
};
use crate::measure::Font;
use crate::render::labels::{LabelOptions, create_label};
use crate::scene::{
    EllipseShape, Group, Item, Path, PathShape, PathStyle, Point, Rect, Size, SourceRef, Transform,
};

// handDrawn: rough.js (later)

/// Upstream's `«stereotype»` label text.
fn stereotype_text(stereotype: &str) -> String {
    format!("«{stereotype}»")
}

// TODO(dedupe): sibling usecase shapes measure extra labels the same way; this mirrors
// `insertLabel` for a centered SVG label.
fn measure_label(
    text: &str,
    class: &str,
    node_id: &str,
    font: Font,
    ctx: &ShapeContext<'_>,
) -> Result<(Group, Size), ShapeError> {
    let label = create_label(
        &LabelOptions {
            text: text.to_string(),
            font,
            markdown: false,
            width: f64::INFINITY,
            centered: true,
            class: class.to_string(),
            source: Some(SourceRef::node(node_id)),
        },
        ctx.measure,
    )
    .map_err(|error| ShapeError {
        message: error.to_string(),
    })?;
    let size = match label.items.first() {
        Some(Item::Text(text)) => text.size,
        _ => Size::ZERO,
    };
    Ok((label, size))
}

/// `positionLabel` for an SVG label: the bbox origin is the text origin, so it centres the
/// measured box on `(0, center_y)`.
fn position_label(label: &mut Group, size: Size, center_y: f64) {
    label.transform = Transform::translate(-size.width / 2.0, center_y - size.height / 2.0);
}

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    draw_with(node, ctx, None)
}

/// Draws the business oval, stacking an optional stereotype above the label as upstream
/// does when the node carries one.
pub fn draw_with(
    node: &Node,
    ctx: &ShapeContext<'_>,
    stereotype: Option<&str>,
) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();
    let (mut group, mut label, label_box) = prepare(&node, ctx)?;
    group.class = Some(get_node_classes(&node, Some("usecase-business-shape")));
    label.class = Some("label usecase-label".to_string());

    let mut stereotype_label = None;
    if let Some(stereotype) = stereotype {
        let (label_group, label_size) = measure_label(
            &stereotype_text(stereotype),
            "label usecase-stereotype",
            &node.id,
            super::base_label_font(ctx),
            ctx,
        )?;
        stereotype_label = Some((label_group, label_size));
    }

    let label_gap = if stereotype_label.is_some() { 2.0 } else { 0.0 };
    let label_height = label_box.height
        + stereotype_label
            .as_ref()
            .map(|(_, size)| size.height)
            .unwrap_or(0.0)
        + label_gap;
    let label_width = label_box.width.max(
        stereotype_label
            .as_ref()
            .map(|(_, size)| size.width)
            .unwrap_or(0.0),
    );
    // `halfPadding ?? 10`; a `Node` always carries a padding, so it is `padding / 2`.
    let padding = node.padding / 2.0;
    let radius_x = label_width / 2.0 + padding * 2.0;
    let radius_y = label_height / 2.0 + padding * 2.0;
    let marker_inset = (padding / 5.0).max(1.0).min(padding / 2.0);
    let marker_start_x = label_width / 2.0 + marker_inset;
    let marker_end_x = radius_x - marker_inset;
    let normalized_start_x = marker_start_x / radius_x;
    let normalized_end_x = marker_end_x / radius_x;
    let marker_start_y = radius_y
        * (1.0 - normalized_start_x * normalized_start_x)
            .max(0.0)
            .sqrt();
    let marker_end_y = -radius_y * (1.0 - normalized_end_x * normalized_end_x).max(0.0).sqrt();

    if let Some((label_group, size)) = stereotype_label.as_mut() {
        position_label(label_group, *size, -label_height / 2.0 + size.height / 2.0);
    }
    position_label(
        &mut label,
        label_box,
        label_height / 2.0 - label_box.height / 2.0,
    );

    // `insert('ellipse', ':first-child')`: the oval paints under both labels.
    let mut ellipse = EllipseShape::new(Point::ZERO, radius_x, radius_y, shape_style());
    ellipse.source = Some(SourceRef::node(node.id.clone()));
    group.items.insert(0, Item::Ellipse(ellipse));
    group.items.push(Item::Group(label));
    if let Some((label_group, _)) = stereotype_label {
        group.items.push(Item::Group(label_group));
    }

    // The diagonal marker of a business use case, filled by inheritance and painted last.
    let mut marker = Path::new();
    marker
        .move_to(Point::new(marker_start_x, marker_start_y))
        .line_to(Point::new(marker_end_x, marker_end_y));
    let marker_style = PathStyle {
        fill: None,
        ..shape_style()
    };
    group.push(PathShape::new(marker, marker_style).with_source(SourceRef::node(node.id.clone())));

    node.bounds = Rect::centered(Point::ZERO, Size::new(radius_x * 2.0, radius_y * 2.0));
    // Upstream also installs `calcIntersect` for layout-time bounds; the scene only carries
    // the node's own intersection.
    node.intersect = IntersectKind::Ellipse {
        rx: radius_x,
        ry: radius_y,
    };
    Ok(RenderedNode { node, group })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::theme::ThemeVariables;

    #[test]
    fn oval_adds_two_padding_units_on_each_axis() {
        let mut node = Node::new("u", "Pay", "usecaseBusiness");
        node.padding = 20.0;
        let rendered = draw(
            &node,
            &ShapeContext {
                measure: &FallbackMeasure,
                theme: &ThemeVariables::default(),
            },
        )
        .unwrap();
        assert_eq!(
            rendered.group.class.as_deref(),
            Some("node  usecase-business-shape")
        );
        let ellipse = rendered
            .group
            .items
            .iter()
            .find_map(|item| match item {
                Item::Ellipse(ellipse) => Some(ellipse),
                _ => None,
            })
            .expect("the oval is drawn");
        assert!(ellipse.radius_x > 10.0 && ellipse.radius_y > 10.0);
        // The marker is the last item and is unfilled.
        assert!(matches!(rendered.group.items.last(), Some(Item::Path(_))));
    }

    #[test]
    fn stereotype_stacks_above_the_label() {
        let mut node = Node::new("u", "Pay", "usecaseBusiness");
        node.padding = 20.0;
        let plain = draw(
            &node,
            &ShapeContext {
                measure: &FallbackMeasure,
                theme: &ThemeVariables::default(),
            },
        )
        .unwrap();
        let stacked = draw_with(
            &node,
            &ShapeContext {
                measure: &FallbackMeasure,
                theme: &ThemeVariables::default(),
            },
            Some("business"),
        )
        .unwrap();
        assert!(stacked.node.bounds.height > plain.node.bounds.height);
    }

    #[test]
    fn matches_upstream_fixtures() {
        use crate::render::shapes::usecase_ellipse::difftest;
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../testdata/shapes/usecase_business.json"))
                .expect("fixture parses");
        let theme = ThemeVariables::default();
        for case in fixture["cases"].as_array().expect("cases") {
            let measure = difftest::FixedMeasure::from_case(case);
            let node = difftest::node_for(case, "usecaseBusiness");
            let rendered = draw_with(
                &node,
                &ShapeContext {
                    measure: &measure,
                    theme: &theme,
                },
                case["stereotype"].as_str(),
            )
            .expect("shape renders");
            difftest::assert_case(&rendered, case);
        }
    }
}
