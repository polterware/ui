//! Port of Mermaid 12.3.0's
//! `rendering-util/layout-algorithms/grid/labelSpacing.ts`.

use std::collections::HashMap;

use crate::layout::grid::label_geometry::{GridLabelSpanRequirement, grid_label_span_requirement};
use crate::layout::grid::router_instrumentation::GridRoutingInstrumentation;
use crate::layout::grid::types::{
    Edge, GridCellStack, GridForest, GridOrientation, LayoutData, Node, ROOT_CONTAINER_ID,
};

#[derive(Clone, Debug, PartialEq)]
pub struct GridLabelledEdge {
    pub edge: Edge,
    pub label_node: Node,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridLabelRequirements {
    pub by_container: HashMap<String, Vec<GridLabelledEdge>>,
    pub self_loops: HashMap<String, GridLabelSpanRequirement>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridContainerLabelGaps {
    pub row_gap_after: HashMap<i64, f64>,
    pub column_gap_after: HashMap<i64, f64>,
}

pub fn build_grid_label_requirements(
    data: &LayoutData,
    forest: &GridForest,
    mut metrics: Option<&mut GridRoutingInstrumentation>,
) -> GridLabelRequirements {
    let mut by_container: HashMap<String, Vec<GridLabelledEdge>> = HashMap::new();
    let mut self_loops: HashMap<String, GridLabelSpanRequirement> = HashMap::new();

    for edge in &data.edges {
        let (Some(start), Some(end), Some(label_node_id)) = (
            edge.start.as_ref(),
            edge.end.as_ref(),
            edge.label_node_id.as_ref(),
        ) else {
            continue;
        };
        if let Some(metrics) = metrics.as_deref_mut() {
            metrics.label_spacing_edges_examined += 1;
        }
        let Some(&label_index) = forest.node_by_id.get(label_node_id) else {
            continue;
        };
        let Some(&source_index) = forest.node_by_id.get(start) else {
            continue;
        };
        let Some(&target_index) = forest.node_by_id.get(end) else {
            continue;
        };
        let label_node = &data.nodes[label_index];
        if start == end {
            if let Some(horizontal) = grid_label_span_requirement(label_node, GridOrientation::H) {
                self_loops.insert(edge.id.clone(), horizontal);
            }
            continue;
        }
        let source_container = data.nodes[source_index]
            .parent_id
            .clone()
            .unwrap_or_else(|| ROOT_CONTAINER_ID.to_string());
        let target_container = data.nodes[target_index]
            .parent_id
            .clone()
            .unwrap_or_else(|| ROOT_CONTAINER_ID.to_string());
        if source_container != target_container {
            continue;
        }
        by_container
            .entry(source_container)
            .or_default()
            .push(GridLabelledEdge {
                edge: edge.clone(),
                label_node: label_node.clone(),
            });
    }

    GridLabelRequirements {
        by_container,
        self_loops,
    }
}

fn track_origins(tracks: &[i64], sizes: &HashMap<i64, f64>, gap: f64) -> HashMap<i64, f64> {
    let mut origins: HashMap<i64, f64> = HashMap::new();
    let mut cursor = 0.0;
    for &track in tracks {
        origins.insert(track, cursor);
        cursor += sizes.get(&track).copied().unwrap_or(0.0) + gap;
    }
    origins
}

fn cross_axis_fits(
    track: i64,
    cross_size: f64,
    tracks: &[i64],
    sizes: &HashMap<i64, f64>,
    gap: f64,
) -> bool {
    let origins = track_origins(tracks, sizes, gap);
    let Some(&origin) = origins.get(&track) else {
        return false;
    };
    let Some(&size) = sizes.get(&track) else {
        return false;
    };
    let center = origin + size / 2.0;
    let low = center - cross_size / 2.0;
    let high = center + cross_size / 2.0;
    tracks.iter().all(|&other| {
        if other == track {
            return true;
        }
        let other_origin = origins[&other];
        let other_end = other_origin + sizes.get(&other).copied().unwrap_or(0.0);
        high <= other_origin || low >= other_end
    })
}

fn placement_by_node_id(cells: &HashMap<String, GridCellStack>) -> HashMap<String, (i64, i64)> {
    let mut placements: HashMap<String, (i64, i64)> = HashMap::new();
    for cell in cells.values() {
        for placement in &cell.items {
            placements.insert(placement.item_id.clone(), (placement.row, placement.column));
        }
    }
    placements
}

fn are_consecutive(first: i64, second: i64, tracks: &[i64]) -> bool {
    let low = first.min(second);
    let high = first.max(second);
    let Some(index) = tracks.iter().position(|&track| track == low) else {
        return false;
    };
    tracks.get(index + 1) == Some(&high)
}

fn add_gap_requirement(gaps: &mut HashMap<i64, f64>, track: i64, required: f64) {
    let current = gaps.get(&track).copied().unwrap_or(0.0);
    gaps.insert(track, current.max(required));
}

#[allow(clippy::too_many_arguments)]
pub fn derive_grid_container_label_gaps(
    entries: &[GridLabelledEdge],
    cells: &HashMap<String, GridCellStack>,
    row_heights: &HashMap<i64, f64>,
    column_widths: &HashMap<i64, f64>,
    sorted_rows: &[i64],
    sorted_columns: &[i64],
    configured_row_gap: f64,
    configured_column_gap: f64,
    mut metrics: Option<&mut GridRoutingInstrumentation>,
) -> GridContainerLabelGaps {
    let mut row_gap_after: HashMap<i64, f64> = HashMap::new();
    let mut column_gap_after: HashMap<i64, f64> = HashMap::new();
    let placements = placement_by_node_id(cells);

    for entry in entries {
        let source = entry
            .edge
            .start
            .as_ref()
            .and_then(|start| placements.get(start).copied());
        let target = entry
            .edge
            .end
            .as_ref()
            .and_then(|end| placements.get(end).copied());
        let (Some((source_row, source_column)), Some((target_row, target_column))) =
            (source, target)
        else {
            continue;
        };
        if source_row == target_row && are_consecutive(source_column, target_column, sorted_columns)
        {
            let requirement = grid_label_span_requirement(&entry.label_node, GridOrientation::H);
            if let Some(requirement) = requirement
                && cross_axis_fits(
                    source_row,
                    requirement.cross,
                    sorted_rows,
                    row_heights,
                    configured_row_gap,
                )
            {
                if let Some(metrics) = metrics.as_deref_mut() {
                    metrics.label_spacing_eligible_edges += 1;
                }
                add_gap_requirement(
                    &mut column_gap_after,
                    source_column.min(target_column),
                    requirement.required_along_span,
                );
            }
            continue;
        }
        if source_column == target_column && are_consecutive(source_row, target_row, sorted_rows) {
            let requirement = grid_label_span_requirement(&entry.label_node, GridOrientation::V);
            if let Some(requirement) = requirement
                && cross_axis_fits(
                    source_column,
                    requirement.cross,
                    sorted_columns,
                    column_widths,
                    configured_column_gap,
                )
            {
                if let Some(metrics) = metrics.as_deref_mut() {
                    metrics.label_spacing_eligible_edges += 1;
                }
                add_gap_requirement(
                    &mut row_gap_after,
                    source_row.min(target_row),
                    requirement.required_along_span,
                );
            }
        }
    }

    GridContainerLabelGaps {
        row_gap_after,
        column_gap_after,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::grid::groups::build_grid_forest;
    use crate::layout::grid::types::{
        GridHorizontalAlign, GridResolvedPlacement, GridVerticalAlign,
    };

    fn node(id: &str, width: f64, height: f64, parent_id: Option<&str>) -> Node {
        Node {
            id: id.to_string(),
            width: Some(width),
            height: Some(height),
            parent_id: parent_id.map(str::to_string),
            is_group: false,
            ..Node::default()
        }
    }

    fn label(id: &str, width: f64, height: f64) -> Node {
        Node {
            id: id.to_string(),
            width: Some(width),
            height: Some(height),
            is_group: false,
            is_edge_label: true,
            ..Node::default()
        }
    }

    fn placement(item: Node, row: i64, column: i64) -> GridResolvedPlacement {
        GridResolvedPlacement {
            item: 0,
            item_id: item.id,
            row,
            column,
            horizontal_align: GridHorizontalAlign::Center,
            vertical_align: GridVerticalAlign::Center,
            source_order: 0,
            explicit_row: true,
            explicit_column: true,
            explicit_cell: true,
        }
    }

    fn cells(values: &[GridResolvedPlacement]) -> HashMap<String, GridCellStack> {
        values
            .iter()
            .map(|value| {
                (
                    format!("{}:{}", value.row, value.column),
                    GridCellStack {
                        row: value.row,
                        column: value.column,
                        items: vec![value.clone()],
                        width: 0.0,
                        height: 0.0,
                        vertical_align: GridVerticalAlign::Center,
                    },
                )
            })
            .collect()
    }

    fn edge(id: &str, start: &str, end: &str, label_node_id: Option<&str>) -> Edge {
        Edge {
            id: id.to_string(),
            start: Some(start.to_string()),
            end: Some(end.to_string()),
            label_node_id: label_node_id.map(str::to_string),
            ..Edge::default()
        }
    }

    #[test]
    fn indexes_only_measured_labels_whose_endpoints_share_a_direct_container() {
        let a = node("a", 80.0, 40.0, None);
        let b = node("b", 80.0, 40.0, None);
        let helper = label("edge-label-ab", 90.0, 20.0);
        let data = LayoutData {
            nodes: vec![a, b, helper],
            edges: vec![edge("ab", "a", "b", Some("edge-label-ab"))],
            ..LayoutData::default()
        };

        let requirements =
            build_grid_label_requirements(&data, &build_grid_forest(&data.nodes).unwrap(), None);

        assert_eq!(requirements.by_container[ROOT_CONTAINER_ID].len(), 1);
        assert_eq!(requirements.self_loops.len(), 0);
    }

    #[test]
    fn uses_the_maximum_label_span_for_a_canonical_adjacent_boundary() {
        let a = node("a", 80.0, 40.0, None);
        let b = node("b", 80.0, 40.0, None);
        let first = label("first", 60.0, 20.0);
        let second = label("second", 90.0, 20.0);
        let entries = vec![
            GridLabelledEdge {
                edge: edge("ab", "a", "b", None),
                label_node: first,
            },
            GridLabelledEdge {
                edge: edge("ba", "b", "a", None),
                label_node: second,
            },
        ];

        let gaps = derive_grid_container_label_gaps(
            &entries,
            &cells(&[placement(a, 1, 1), placement(b, 1, 2)]),
            &HashMap::from([(1, 40.0)]),
            &HashMap::from([(1, 80.0), (2, 80.0)]),
            &[1],
            &[1, 2],
            50.0,
            50.0,
            None,
        );

        assert_eq!(gaps.column_gap_after, HashMap::from([(1, 116.0)]));
        assert_eq!(gaps.row_gap_after.len(), 0);
    }

    #[test]
    fn skips_non_adjacent_endpoints_and_cross_axis_track_collisions() {
        let a = node("a", 80.0, 40.0, None);
        let b = node("b", 80.0, 40.0, None);
        let helper = label("label", 90.0, 120.0);
        let entries = vec![GridLabelledEdge {
            edge: edge("ab", "a", "b", None),
            label_node: helper,
        }];

        let gaps = derive_grid_container_label_gaps(
            &entries,
            &cells(&[placement(a.clone(), 1, 1), placement(b.clone(), 1, 3)]),
            &HashMap::from([(1, 40.0), (2, 40.0)]),
            &HashMap::from([(1, 80.0), (2, 80.0), (3, 80.0)]),
            &[1, 2],
            &[1, 2, 3],
            10.0,
            50.0,
            None,
        );
        assert_eq!(gaps.column_gap_after.len(), 0);

        let adjacent = derive_grid_container_label_gaps(
            &entries,
            &cells(&[placement(a, 1, 1), placement(b, 1, 2)]),
            &HashMap::from([(1, 40.0), (2, 40.0)]),
            &HashMap::from([(1, 80.0), (2, 80.0)]),
            &[1, 2],
            &[1, 2],
            10.0,
            50.0,
            None,
        );
        assert_eq!(adjacent.column_gap_after.len(), 0);
    }

    #[test]
    fn records_a_measured_self_loop_label() {
        let a = node("a", 80.0, 40.0, None);
        let helper = label("edge-label-aa", 90.0, 20.0);
        let data = LayoutData {
            nodes: vec![a, helper],
            edges: vec![edge("aa", "a", "a", Some("edge-label-aa"))],
            ..LayoutData::default()
        };
        let requirements =
            build_grid_label_requirements(&data, &build_grid_forest(&data.nodes).unwrap(), None);
        assert_eq!(
            requirements.self_loops["aa"],
            grid_label_span_requirement(&data.nodes[1], GridOrientation::H).unwrap()
        );
    }
}
