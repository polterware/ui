//! Port of Mermaid 12.3.0's
//! `rendering-util/layout-algorithms/grid/labelGeometry.ts`.

use crate::layout::grid::types::{GridOrientation, Node, is_finite_positive_number};

pub const GRID_LABEL_CLEARANCE: f64 = 6.0;
pub const GRID_EDGE_END_MARKER_CLEARANCE: f64 = 12.0;
pub const GRID_LABEL_SPAN_MARGIN: f64 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridLabelSpanRequirement {
    pub along: f64,
    pub cross: f64,
    pub start_inset: f64,
    pub end_inset: f64,
    pub required_along_span: f64,
}

pub fn grid_label_span_requirement(
    label_node: &Node,
    orientation: GridOrientation,
) -> Option<GridLabelSpanRequirement> {
    if !is_finite_positive_number(label_node.width) || !is_finite_positive_number(label_node.height)
    {
        return None;
    }

    let width = label_node.width?;
    let height = label_node.height?;
    let along = if orientation == GridOrientation::H {
        width
    } else {
        height
    };
    let cross = if orientation == GridOrientation::H {
        height
    } else {
        width
    };
    let start_inset = GRID_LABEL_CLEARANCE.max(GRID_EDGE_END_MARKER_CLEARANCE);
    let end_inset = GRID_LABEL_CLEARANCE.max(GRID_EDGE_END_MARKER_CLEARANCE);

    Some(GridLabelSpanRequirement {
        along,
        cross,
        start_inset,
        end_inset,
        required_along_span: along + start_inset + end_inset + GRID_LABEL_SPAN_MARGIN,
    })
}

pub fn grid_label_required_segment_length(label_node: &Node, orientation: GridOrientation) -> f64 {
    let width = label_node.width.unwrap_or(f64::NAN);
    let height = label_node.height.unwrap_or(f64::NAN);
    let along = if orientation == GridOrientation::H {
        width
    } else {
        height
    };
    if is_finite_positive_number(Some(along)) {
        along + GRID_LABEL_CLEARANCE * 2.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(width: Option<f64>, height: Option<f64>) -> Node {
        Node {
            id: "label".to_string(),
            width,
            height,
            is_group: false,
            ..Node::default()
        }
    }

    #[test]
    fn maps_horizontal_and_vertical_label_dimensions_to_their_route_axes() {
        assert_eq!(
            grid_label_span_requirement(&label(Some(90.0), Some(20.0)), GridOrientation::H),
            Some(GridLabelSpanRequirement {
                along: 90.0,
                cross: 20.0,
                start_inset: GRID_EDGE_END_MARKER_CLEARANCE,
                end_inset: GRID_EDGE_END_MARKER_CLEARANCE,
                required_along_span: 90.0
                    + GRID_EDGE_END_MARKER_CLEARANCE * 2.0
                    + GRID_LABEL_SPAN_MARGIN,
            })
        );
        assert_eq!(
            grid_label_span_requirement(&label(Some(90.0), Some(20.0)), GridOrientation::V),
            Some(GridLabelSpanRequirement {
                along: 20.0,
                cross: 90.0,
                start_inset: GRID_EDGE_END_MARKER_CLEARANCE,
                end_inset: GRID_EDGE_END_MARKER_CLEARANCE,
                required_along_span: 20.0
                    + GRID_EDGE_END_MARKER_CLEARANCE * 2.0
                    + GRID_LABEL_SPAN_MARGIN,
            })
        );
    }

    #[test]
    fn retains_the_existing_nonterminal_segment_clearance_calculation() {
        assert_eq!(
            grid_label_required_segment_length(&label(Some(90.0), Some(20.0)), GridOrientation::H),
            90.0 + GRID_LABEL_CLEARANCE * 2.0
        );
        assert_eq!(
            grid_label_required_segment_length(&label(Some(90.0), Some(20.0)), GridOrientation::V),
            20.0 + GRID_LABEL_CLEARANCE * 2.0
        );
    }

    #[test]
    fn rejects_invalid_measurements() {
        for (width, height) in [
            (None, Some(20.0)),
            (Some(90.0), None),
            (Some(0.0), Some(20.0)),
            (Some(90.0), Some(f64::NAN)),
        ] {
            assert_eq!(
                grid_label_span_requirement(&label(width, height), GridOrientation::H),
                None
            );
        }
    }
}
