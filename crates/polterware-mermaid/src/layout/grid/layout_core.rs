//! Port of Mermaid 12.3.0's
//! `rendering-util/layout-algorithms/grid/layoutCore.ts`.
//!
//! Computes grid geometry in two coordinate systems:
//!
//! 1. Each container lays out its direct children in local coordinates, children-first.
//! 2. A top-down pass translates those local results into absolute diagram coordinates.
//!
//! The public entry point runs both phases on a clone and commits only after the complete geometry
//! succeeds, so invalid input cannot leave the caller's graph partially positioned.

use std::collections::{HashMap, HashSet};

use serde_json::json;

use crate::layout::grid::edge_labels::{GridEdgeLabelOptions, position_grid_edge_labels};
use crate::layout::grid::groups::build_grid_forest;
use crate::layout::grid::label_spacing::{
    GridContainerLabelGaps, GridLabelRequirements, build_grid_label_requirements,
    derive_grid_container_label_gaps,
};
use crate::layout::grid::placement::{
    build_grid_source_order_indices, read_grid_config, resolve_grid_placements,
    validate_grid_placement_map,
};
use crate::layout::grid::router::{GridRoutingTestOptions, route_grid_edges};
use crate::layout::grid::router_instrumentation::GridRoutingInstrumentation;
use crate::layout::grid::types::{
    GRID_DEFAULTS, GridCellStack, GridContainerLayoutMeta, GridError, GridErrorCode, GridForest,
    GridHorizontalAlign, GridItemLayoutMeta, GridLayoutConfigNormalized, GridLayoutResult,
    GridVerticalAlign, GroupTitleRect, LayoutData, Node, ROOT_CONTAINER_ID, grid_error,
    is_edge_label_node, is_finite_positive_number, js_optional_number,
};

const GROUP_ROUTING_GUTTER: f64 = 24.0;
const GROUP_ROUTING_CLEARANCE: f64 = 20.0;

fn title_band_for(node: &Node, title_gap: f64) -> f64 {
    let Some(label) = node.label.as_ref() else {
        return 0.0;
    };
    if label.is_empty() {
        return 0.0;
    }
    let Some(label_bbox) = node.label_bbox else {
        return 0.0;
    };
    if label_bbox.height.is_nan() || label_bbox.height <= 0.0 {
        return 0.0;
    }
    label_bbox.height + title_gap
}

fn ensure_measured_node(node: &Node) -> Result<(), GridError> {
    if !is_finite_positive_number(node.width) || !is_finite_positive_number(node.height) {
        return Err(grid_error(
            GridErrorCode::MissingMeasurement,
            format!(
                "Node \"{}\" must have width and height greater than 0 (width: {}, height: {})",
                node.id,
                js_optional_number(node.width),
                js_optional_number(node.height)
            ),
            Some(json!({
                "nodeId": node.id,
                "width": node.width,
                "height": node.height,
            })),
        ));
    }
    Ok(())
}

fn measure_child(node: &Node) -> Result<(f64, f64), GridError> {
    ensure_measured_node(node)?;
    Ok((
        node.width.expect("measured width"),
        node.height.expect("measured height"),
    ))
}

fn insert_unique(values: &mut Vec<f64>, value: f64) {
    if !values.contains(&value) {
        values.push(value);
    }
}

#[allow(clippy::too_many_arguments)]
fn layout_container(
    container_id: &str,
    nodes: &mut [Node],
    forest: &GridForest,
    config: &GridLayoutConfigNormalized,
    source_order: &HashMap<String, i64>,
    label_requirements: &GridLabelRequirements,
    containers: &mut HashMap<String, GridContainerLayoutMeta>,
    item_meta: &mut HashMap<String, GridItemLayoutMeta>,
    mut metrics: Option<&mut GridRoutingInstrumentation>,
) -> Result<(), GridError> {
    let group_index = if container_id == ROOT_CONTAINER_ID {
        None
    } else {
        forest.group_by_id.get(container_id).copied()
    };
    let (padding, title_band, routing_gutter, group_title_width) = match group_index {
        Some(index) => {
            let group = &nodes[index];
            (
                group.padding.unwrap_or(0.0).max(config.container_padding),
                title_band_for(group, config.title_gap),
                GROUP_ROUTING_GUTTER,
                group.label_bbox.map_or(0.0, |label_bbox| label_bbox.width),
            )
        }
        None => (0.0, 0.0, 0.0, 0.0),
    };
    let content_top = if group_index.is_some() {
        padding + title_band + routing_gutter
    } else {
        0.0
    };

    let direct_items: Vec<usize> = forest
        .children_by_parent
        .get(container_id)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|&index| !is_edge_label_node(&nodes[index]))
        .collect();

    let resolution = resolve_grid_placements(nodes, &direct_items, source_order, config)?;
    let mut cell_entries: Vec<GridCellStack> = resolution.cells.into_values().collect();
    cell_entries.sort_by(|a, b| a.row.cmp(&b.row).then_with(|| a.column.cmp(&b.column)));

    // A cell may contain an authored vertical stack. Columns and rows are sized from the largest
    // complete stack in that track, which keeps neighboring cells aligned.
    let mut column_widths: HashMap<i64, f64> = HashMap::new();
    let mut row_heights: HashMap<i64, f64> = HashMap::new();
    let mut rows: HashSet<i64> = HashSet::new();
    let mut columns: HashSet<i64> = HashSet::new();

    for cell in &mut cell_entries {
        let mut stack_width = 0.0f64;
        let mut stack_height = 0.0f64;
        for placement in &cell.items {
            let (width, height) = measure_child(&nodes[placement.item])?;
            stack_width = stack_width.max(width);
            stack_height += height;
        }
        if cell.items.len() > 1 {
            stack_height += config.cell_gap * (cell.items.len() - 1) as f64;
        }
        cell.width = stack_width;
        cell.height = stack_height;
        rows.insert(cell.row);
        columns.insert(cell.column);
        let column_width = column_widths.entry(cell.column).or_insert(0.0);
        *column_width = (*column_width).max(stack_width);
        let row_height = row_heights.entry(cell.row).or_insert(0.0);
        *row_height = (*row_height).max(stack_height);
    }

    let mut sorted_rows: Vec<i64> = rows.into_iter().collect();
    sorted_rows.sort_unstable();
    let mut sorted_columns: Vec<i64> = columns.into_iter().collect();
    sorted_columns.sort_unstable();
    let cells: HashMap<String, GridCellStack> = cell_entries
        .iter()
        .map(|cell| (format!("{}:{}", cell.row, cell.column), cell.clone()))
        .collect();
    let label_gaps: GridContainerLabelGaps = derive_grid_container_label_gaps(
        label_requirements
            .by_container
            .get(container_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        &cells,
        &row_heights,
        &column_widths,
        &sorted_rows,
        &sorted_columns,
        config.row_gap,
        config.column_gap,
        metrics.as_deref_mut(),
    );
    let row_gap_after = |row: i64| {
        config
            .row_gap
            .max(label_gaps.row_gap_after.get(&row).copied().unwrap_or(0.0))
    };
    let column_gap_after = |column: i64| {
        config.column_gap.max(
            label_gaps
                .column_gap_after
                .get(&column)
                .copied()
                .unwrap_or(0.0),
        )
    };
    if let Some(metrics) = metrics {
        for gap in label_gaps.row_gap_after.values() {
            if *gap > config.row_gap {
                metrics.label_spacing_boundaries_expanded += 1;
                metrics.label_spacing_pixels_added += gap - config.row_gap;
            }
        }
        for gap in label_gaps.column_gap_after.values() {
            if *gap > config.column_gap {
                metrics.label_spacing_boundaries_expanded += 1;
                metrics.label_spacing_pixels_added += gap - config.column_gap;
            }
        }
    }

    let mut content_width = 0.0f64;
    for (index, column) in sorted_columns.iter().enumerate() {
        content_width += column_widths.get(column).copied().unwrap_or(0.0);
        if index < sorted_columns.len() - 1 {
            content_width += column_gap_after(*column);
        }
    }
    // A title sets the minimum group width. If it is wider than the content and its insets,
    // center the occupied tracks so the extra horizontal space is split evenly on both sides.
    let width = if group_index.is_some() {
        (content_width + (padding + routing_gutter) * 2.0).max(group_title_width + padding * 2.0)
    } else {
        content_width
    };
    let content_left = if group_index.is_some() {
        (width - content_width) / 2.0
    } else {
        0.0
    };

    let mut column_origins: HashMap<i64, f64> = HashMap::new();
    let mut x_cursor = content_left;
    for (index, column) in sorted_columns.iter().enumerate() {
        column_origins.insert(*column, x_cursor);
        x_cursor += column_widths.get(column).copied().unwrap_or(0.0);
        if index < sorted_columns.len() - 1 {
            x_cursor += column_gap_after(*column);
        }
    }

    let mut row_origins: HashMap<i64, f64> = HashMap::new();
    let mut y_cursor = content_top;
    for (index, row) in sorted_rows.iter().enumerate() {
        row_origins.insert(*row, y_cursor);
        y_cursor += row_heights.get(row).copied().unwrap_or(0.0);
        if index < sorted_rows.len() - 1 {
            y_cursor += row_gap_after(*row);
        }
    }
    let grid_height = if sorted_rows.is_empty() {
        0.0
    } else {
        y_cursor - content_top
    };

    let outer_left_corridor = if group_index.is_some() {
        content_left - GROUP_ROUTING_CLEARANCE
    } else {
        -(config.column_gap / 2.0).max(GRID_DEFAULTS.container_padding)
    };
    let outer_right_corridor = if group_index.is_some() {
        content_left + content_width + GROUP_ROUTING_CLEARANCE
    } else {
        content_left
            + content_width
            + (config.column_gap / 2.0).max(GRID_DEFAULTS.container_padding)
    };
    let outer_top_corridor = if group_index.is_some() {
        content_top - GROUP_ROUTING_CLEARANCE
    } else {
        -(config.row_gap / 2.0).max(GRID_DEFAULTS.container_padding)
    };
    let outer_bottom_corridor = if group_index.is_some() {
        content_top + grid_height + GROUP_ROUTING_CLEARANCE
    } else {
        content_top + grid_height + (config.row_gap / 2.0).max(GRID_DEFAULTS.container_padding)
    };

    // Record the empty bands between tracks while geometry is available. The routing layer
    // consumes these coordinates later without needing to reconstruct placement decisions.
    let mut vertical_corridors: Vec<f64> = vec![outer_left_corridor, outer_right_corridor];
    let mut horizontal_corridors: Vec<f64> = vec![outer_top_corridor, outer_bottom_corridor];
    let mut left_corridor_by_column: HashMap<i64, f64> = HashMap::new();
    let mut right_corridor_by_column: HashMap<i64, f64> = HashMap::new();
    let mut top_corridor_by_row: HashMap<i64, f64> = HashMap::new();
    let mut bottom_corridor_by_row: HashMap<i64, f64> = HashMap::new();

    if let (Some(first), Some(last)) = (sorted_columns.first(), sorted_columns.last()) {
        left_corridor_by_column.insert(*first, outer_left_corridor);
        right_corridor_by_column.insert(*last, outer_right_corridor);
    }
    for index in 0..sorted_columns.len().saturating_sub(1) {
        let current = sorted_columns[index];
        let next = sorted_columns[index + 1];
        let corridor = (column_origins[&current]
            + column_widths.get(&current).copied().unwrap_or(0.0)
            + column_origins[&next])
            / 2.0;
        insert_unique(&mut vertical_corridors, corridor);
        right_corridor_by_column.insert(current, corridor);
        left_corridor_by_column.insert(next, corridor);
    }
    if let (Some(first), Some(last)) = (sorted_rows.first(), sorted_rows.last()) {
        top_corridor_by_row.insert(*first, outer_top_corridor);
        bottom_corridor_by_row.insert(*last, outer_bottom_corridor);
    }
    for index in 0..sorted_rows.len().saturating_sub(1) {
        let current = sorted_rows[index];
        let next = sorted_rows[index + 1];
        let corridor = (row_origins[&current]
            + row_heights.get(&current).copied().unwrap_or(0.0)
            + row_origins[&next])
            / 2.0;
        insert_unique(&mut horizontal_corridors, corridor);
        bottom_corridor_by_row.insert(current, corridor);
        top_corridor_by_row.insert(next, corridor);
    }

    for cell in &cell_entries {
        let cell_left = column_origins
            .get(&cell.column)
            .copied()
            .unwrap_or(content_left);
        let cell_top = row_origins.get(&cell.row).copied().unwrap_or(content_top);
        let cell_width = column_widths
            .get(&cell.column)
            .copied()
            .unwrap_or(cell.width);
        let cell_height = row_heights.get(&cell.row).copied().unwrap_or(cell.height);
        let vertical_factor = match cell.vertical_align {
            GridVerticalAlign::Top => 0.0,
            GridVerticalAlign::Bottom => 1.0,
            GridVerticalAlign::Center => 0.5,
        };
        let mut item_top = cell_top + (cell_height - cell.height) * vertical_factor;

        for (item_index, placement) in cell.items.iter().enumerate() {
            let (width, height) = measure_child(&nodes[placement.item])?;
            let horizontal_factor = match placement.horizontal_align {
                GridHorizontalAlign::Left => 0.0,
                GridHorizontalAlign::Right => 1.0,
                GridHorizontalAlign::Center => 0.5,
            };
            let item_left = cell_left + (cell_width - width) * horizontal_factor;
            nodes[placement.item].x = Some(item_left + width / 2.0);
            nodes[placement.item].y = Some(item_top + height / 2.0);
            let top_corridor_y = if item_index > 0 {
                item_top - config.cell_gap / 2.0
            } else {
                top_corridor_by_row
                    .get(&cell.row)
                    .copied()
                    .unwrap_or(outer_top_corridor)
            };
            let bottom_corridor_y = if item_index < cell.items.len() - 1 {
                item_top + height + config.cell_gap / 2.0
            } else {
                bottom_corridor_by_row
                    .get(&cell.row)
                    .copied()
                    .unwrap_or(outer_bottom_corridor)
            };
            if item_index > 0 {
                insert_unique(&mut horizontal_corridors, top_corridor_y);
            }
            if item_index < cell.items.len() - 1 {
                insert_unique(&mut horizontal_corridors, bottom_corridor_y);
            }

            item_meta.insert(
                nodes[placement.item].id.clone(),
                GridItemLayoutMeta {
                    container_id: container_id.to_string(),
                    row: cell.row,
                    column: cell.column,
                    cell_left,
                    cell_top,
                    cell_width,
                    cell_height,
                    left_corridor_x: left_corridor_by_column
                        .get(&cell.column)
                        .copied()
                        .unwrap_or(outer_left_corridor),
                    right_corridor_x: right_corridor_by_column
                        .get(&cell.column)
                        .copied()
                        .unwrap_or(outer_right_corridor),
                    top_corridor_y,
                    bottom_corridor_y,
                },
            );

            item_top += height + config.cell_gap;
        }
    }

    let height = if group_index.is_some() {
        grid_height + title_band + (padding + routing_gutter) * 2.0
    } else {
        grid_height
    };

    vertical_corridors.sort_by(f64::total_cmp);
    horizontal_corridors.sort_by(f64::total_cmp);
    containers.insert(
        container_id.to_string(),
        GridContainerLayoutMeta {
            id: container_id.to_string(),
            left: 0.0,
            top: 0.0,
            width,
            height,
            content_left,
            content_top,
            content_right: if group_index.is_some() {
                content_left + content_width
            } else {
                content_width
            },
            content_bottom: if group_index.is_some() {
                content_top + grid_height
            } else {
                grid_height
            },
            vertical_corridors,
            horizontal_corridors,
        },
    );

    if let Some(index) = group_index {
        nodes[index].width = Some(width);
        nodes[index].height = Some(height);
        nodes[index].x = Some(width / 2.0);
        nodes[index].y = Some(height / 2.0);
        nodes[index].group_title_rect = if title_band > 0.0 {
            Some(GroupTitleRect {
                left: 0.0,
                right: width,
                top: 0.0,
                bottom: padding + title_band,
            })
        } else {
            None
        };
    }

    Ok(())
}

fn translate_rect(rect: GroupTitleRect, dx: f64, dy: f64) -> GroupTitleRect {
    GroupTitleRect {
        left: rect.left + dx,
        right: rect.right + dx,
        top: rect.top + dy,
        bottom: rect.bottom + dy,
    }
}

fn materialize_absolute_geometry(result: &mut GridLayoutResult, nodes: &mut [Node]) {
    let GridLayoutResult {
        forest,
        containers,
        item_meta,
        ..
    } = result;

    // Container layout is local to its parent. Walk top-down with accumulated offsets so nodes,
    // title rectangles, cell metadata, and routing corridors all move as one coordinate system.
    struct ShiftFrame {
        container_id: String,
        offset_x: f64,
        offset_y: f64,
        children: Vec<usize>,
        next_child_index: usize,
        entered: bool,
    }

    let mut stack: Vec<ShiftFrame> = vec![ShiftFrame {
        container_id: ROOT_CONTAINER_ID.to_string(),
        offset_x: 0.0,
        offset_y: 0.0,
        children: forest
            .children_by_parent
            .get(ROOT_CONTAINER_ID)
            .cloned()
            .unwrap_or_default(),
        next_child_index: 0,
        entered: false,
    }];

    while !stack.is_empty() {
        let frame_index = stack.len() - 1;

        if !stack[frame_index].entered {
            stack[frame_index].entered = true;
            let container_id = stack[frame_index].container_id.clone();
            let offset_x = stack[frame_index].offset_x;
            let offset_y = stack[frame_index].offset_y;
            if let Some(container) = containers.get_mut(&container_id) {
                container.left += offset_x;
                container.top += offset_y;
                container.content_left += offset_x;
                container.content_right += offset_x;
                container.content_top += offset_y;
                container.content_bottom += offset_y;
                for value in &mut container.vertical_corridors {
                    *value += offset_x;
                }
                for value in &mut container.horizontal_corridors {
                    *value += offset_y;
                }
            }
        }

        let child_index = if stack[frame_index].next_child_index < stack[frame_index].children.len()
        {
            let child = stack[frame_index].children[stack[frame_index].next_child_index];
            stack[frame_index].next_child_index += 1;
            Some(child)
        } else {
            None
        };
        let Some(child_index) = child_index else {
            stack.pop();
            continue;
        };

        if is_edge_label_node(&nodes[child_index]) {
            continue;
        }

        let offset_x = stack[frame_index].offset_x;
        let offset_y = stack[frame_index].offset_y;
        let child_id = nodes[child_index].id.clone();
        if let Some(meta) = item_meta.get_mut(&child_id) {
            meta.cell_left += offset_x;
            meta.cell_top += offset_y;
            meta.left_corridor_x += offset_x;
            meta.right_corridor_x += offset_x;
            meta.top_corridor_y += offset_y;
            meta.bottom_corridor_y += offset_y;
        }

        let child_x = nodes[child_index].x.unwrap_or(0.0) + offset_x;
        let child_y = nodes[child_index].y.unwrap_or(0.0) + offset_y;
        nodes[child_index].x = Some(child_x);
        nodes[child_index].y = Some(child_y);

        if !nodes[child_index].is_group {
            continue;
        }

        let width = nodes[child_index].width.unwrap_or(0.0);
        let height = nodes[child_index].height.unwrap_or(0.0);
        if let Some(title_rect) = nodes[child_index].group_title_rect {
            nodes[child_index].group_title_rect = Some(translate_rect(
                title_rect,
                child_x - width / 2.0,
                child_y - height / 2.0,
            ));
        }

        stack.push(ShiftFrame {
            container_id: nodes[child_index].id.clone(),
            offset_x: child_x - width / 2.0,
            offset_y: child_y - height / 2.0,
            children: forest
                .children_by_parent
                .get(&nodes[child_index].id)
                .cloned()
                .unwrap_or_default(),
            next_child_index: 0,
            entered: false,
        });
    }
}

fn commit_grid_geometry(source: &LayoutData, target: &mut LayoutData) {
    // Preserve caller-owned node and edge identities. Downstream renderers may already hold
    // references to these objects, so only calculated geometry is copied from the working graph.
    let source_node_by_id: HashMap<&str, &Node> = source
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    for target_node in &mut target.nodes {
        let Some(source_node) = source_node_by_id.get(target_node.id.as_str()) else {
            continue;
        };
        target_node.x = source_node.x;
        target_node.y = source_node.y;
        target_node.width = source_node.width;
        target_node.height = source_node.height;
        target_node.group_title_rect = source_node.group_title_rect;
    }

    let source_edge_by_id: HashMap<&str, &crate::layout::grid::types::Edge> = source
        .edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect();
    for target_edge in &mut target.edges {
        let Some(source_edge) = source_edge_by_id.get(target_edge.id.as_str()) else {
            continue;
        };
        target_edge.points = source_edge.points.clone();
        target_edge.curve = source_edge.curve;
        target_edge.corner_radius = source_edge.corner_radius;
        target_edge.port_clipping =
            Some(crate::layout::grid::types::PortClipping::OutlineOrthogonal);
        target_edge.skip_corner_fix =
            source_edge.curve == Some(crate::layout::grid::types::GridCurve::Linear);
    }
}

fn clone_grid_layout_data(data: &LayoutData) -> LayoutData {
    data.clone()
}

#[derive(Default)]
pub struct GridLayoutCoreInternalOptions<'a> {
    pub metrics: Option<&'a mut GridRoutingInstrumentation>,
    pub routing: GridRoutingTestOptions,
}

fn run_grid_layout_core_in_place(
    data: &mut LayoutData,
    mut options: GridLayoutCoreInternalOptions<'_>,
) -> Result<GridLayoutResult, GridError> {
    let forest = build_grid_forest(&data.nodes)?;
    let config = read_grid_config(data);
    let layout_items: Vec<usize> = data
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| !is_edge_label_node(node))
        .map(|(index, _)| index)
        .collect();
    let source_order = build_grid_source_order_indices(&data.nodes, &layout_items);
    let _ = validate_grid_placement_map(&layout_items, &data.nodes, &config);

    let label_requirements =
        build_grid_label_requirements(data, &forest, options.metrics.as_deref_mut());

    let mut result = GridLayoutResult {
        forest,
        config,
        containers: HashMap::new(),
        item_meta: HashMap::new(),
        source_order,
        label_requirements,
    };

    // Post-order is the key sizing invariant: a group becomes a measured child only after all of
    // its descendants have established the group's final width and height.
    let post_order_groups = result.forest.post_order_groups.clone();
    for group_index in post_order_groups {
        let container_id = data.nodes[group_index].id.clone();
        layout_container(
            &container_id,
            &mut data.nodes,
            &result.forest,
            &result.config,
            &result.source_order,
            &result.label_requirements,
            &mut result.containers,
            &mut result.item_meta,
            options.metrics.as_deref_mut(),
        )?;
    }
    layout_container(
        ROOT_CONTAINER_ID,
        &mut data.nodes,
        &result.forest,
        &result.config,
        &result.source_order,
        &result.label_requirements,
        &mut result.containers,
        &mut result.item_meta,
        options.metrics.as_deref_mut(),
    )?;
    materialize_absolute_geometry(&mut result, &mut data.nodes);
    // Routing consumes absolute node bounds and corridor coordinates, so it must run after the
    // children-first sizing and top-down translation phases have both completed.
    let _ = route_grid_edges(
        data,
        &result,
        options.metrics.as_deref_mut(),
        &options.routing,
    );
    // Labels consume final routes and may transactionally reroute them, so they are the last
    // layout phase before the working copy is committed to the render model.
    let _ = position_grid_edge_labels(
        data,
        None,
        options.metrics.as_deref_mut(),
        &GridEdgeLabelOptions::default(),
    );
    Ok(result)
}

pub fn run_grid_layout_core(
    data: &mut LayoutData,
    options: GridLayoutCoreInternalOptions<'_>,
) -> Result<GridLayoutResult, GridError> {
    // Routing and label placement are transactional: a failed recovery must not leave partial
    // coordinates on the shared render model. Commit only the geometry from a complete run.
    let mut working = clone_grid_layout_data(data);
    let result = run_grid_layout_core_in_place(&mut working, options)?;
    commit_grid_geometry(&working, data);
    // The forest holds node indices, so the committed order matches the working clone and no
    // rebinding pass is needed.
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::grid::types::{Edge, GridConfig, PortClipping};

    fn leaf(
        id: &str,
        width: f64,
        height: f64,
        metadata: Option<serde_json::Value>,
        parent_id: Option<&str>,
    ) -> Node {
        Node {
            id: id.to_string(),
            parent_id: parent_id.map(str::to_string),
            is_group: false,
            width: Some(width),
            height: Some(height),
            metadata,
            ..Node::default()
        }
    }

    fn group(
        id: &str,
        label: &str,
        parent_id: Option<&str>,
        metadata: Option<serde_json::Value>,
    ) -> Node {
        Node {
            id: id.to_string(),
            parent_id: parent_id.map(str::to_string),
            label: Some(label.to_string()),
            label_bbox: Some(crate::layout::grid::types::LabelSize {
                width: 60.0,
                height: 20.0,
            }),
            is_group: true,
            metadata,
            ..Node::default()
        }
    }

    fn layout(nodes: Vec<Node>, grid: serde_json::Value) -> LayoutData {
        LayoutData {
            nodes,
            edges: Vec::new(),
            config: GridConfig { grid: Some(grid) },
        }
    }

    type GeometrySnapshot = Vec<(
        String,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<GroupTitleRect>,
    )>;

    fn geometry_snapshot(nodes: &[Node]) -> GeometrySnapshot {
        nodes
            .iter()
            .map(|node| {
                (
                    node.id.clone(),
                    node.x,
                    node.y,
                    node.width,
                    node.height,
                    node.group_title_rect,
                )
            })
            .collect()
    }

    #[test]
    fn handles_an_empty_graph() {
        let mut data = layout(Vec::new(), json!({}));

        let result =
            run_grid_layout_core(&mut data, GridLayoutCoreInternalOptions::default()).unwrap();

        assert!(data.nodes.is_empty());
        assert!(data.edges.is_empty());
        assert!(result.forest.root_children.is_empty());
        assert_eq!(result.item_meta.len(), 0);
    }

    #[test]
    fn assigns_finite_geometry_to_a_single_automatically_placed_node() {
        let mut data = layout(vec![leaf("only", 40.0, 20.0, None, None)], json!({}));

        let result =
            run_grid_layout_core(&mut data, GridLayoutCoreInternalOptions::default()).unwrap();

        assert_eq!(
            (
                data.nodes[0].x,
                data.nodes[0].y,
                data.nodes[0].width,
                data.nodes[0].height
            ),
            (Some(20.0), Some(10.0), Some(40.0), Some(20.0))
        );
        assert_eq!(
            (
                result.item_meta["only"].row,
                result.item_meta["only"].column
            ),
            (1, 1)
        );
    }

    #[test]
    fn marks_routed_edges_with_their_rendering_capabilities() {
        let mut data = layout(
            vec![
                leaf(
                    "source",
                    40.0,
                    20.0,
                    Some(json!({ "row": 1, "column": 1 })),
                    None,
                ),
                leaf(
                    "target",
                    40.0,
                    20.0,
                    Some(json!({ "row": 1, "column": 2 })),
                    None,
                ),
            ],
            json!({ "curve": "linear" }),
        );
        data.edges.push(Edge {
            id: "source-target".to_string(),
            start: Some("source".to_string()),
            end: Some("target".to_string()),
            ..Edge::default()
        });

        run_grid_layout_core(&mut data, GridLayoutCoreInternalOptions::default()).unwrap();

        assert_eq!(
            data.edges[0].port_clipping,
            Some(PortClipping::OutlineOrthogonal)
        );
        assert!(data.edges[0].skip_corner_fix);
    }

    #[test]
    fn expands_only_the_labelled_vertical_boundary() {
        let mut data = layout(
            vec![
                leaf(
                    "source",
                    80.0,
                    40.0,
                    Some(json!({ "row": 1, "column": 1 })),
                    None,
                ),
                leaf(
                    "target",
                    80.0,
                    40.0,
                    Some(json!({ "row": 2, "column": 1 })),
                    None,
                ),
                leaf(
                    "peer",
                    80.0,
                    40.0,
                    Some(json!({ "row": 3, "column": 1 })),
                    None,
                ),
                Node {
                    id: "edge-label-source-target".to_string(),
                    is_edge_label: true,
                    width: Some(20.0),
                    height: Some(60.0),
                    ..Node::default()
                },
            ],
            json!({ "rowGap": 10 }),
        );
        data.edges.push(Edge {
            id: "source-target".to_string(),
            start: Some("source".to_string()),
            end: Some("target".to_string()),
            label: Some("tall".to_string()),
            label_node_id: Some("edge-label-source-target".to_string()),
            ..Edge::default()
        });

        run_grid_layout_core(&mut data, GridLayoutCoreInternalOptions::default()).unwrap();

        let by_id: HashMap<&str, &Node> = data
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        let source = by_id["source"];
        let target = by_id["target"];
        let peer = by_id["peer"];
        assert_eq!(target.y.unwrap() - 20.0 - (source.y.unwrap() + 20.0), 86.0);
        assert_eq!(peer.y.unwrap() - 20.0 - (target.y.unwrap() + 20.0), 10.0);
    }

    #[test]
    fn sizes_stacked_cells_and_applies_horizontal_and_vertical_alignment() {
        let mut data = layout(
            vec![
                leaf(
                    "stack-left",
                    40.0,
                    20.0,
                    Some(
                        json!({ "row": 1, "column": 1, "horizontalAlign": "left", "verticalAlign": "bottom" }),
                    ),
                    None,
                ),
                leaf(
                    "stack-right",
                    20.0,
                    20.0,
                    Some(
                        json!({ "row": 1, "column": 1, "horizontalAlign": "right", "verticalAlign": "bottom" }),
                    ),
                    None,
                ),
                leaf(
                    "peer",
                    100.0,
                    100.0,
                    Some(json!({ "row": 1, "column": 2 })),
                    None,
                ),
            ],
            json!({ "cellGap": 10 }),
        );

        run_grid_layout_core(&mut data, GridLayoutCoreInternalOptions::default()).unwrap();
        let by_id: HashMap<&str, &Node> = data
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();

        assert_eq!(
            (by_id["stack-left"].x, by_id["stack-left"].y),
            (Some(20.0), Some(60.0))
        );
        assert_eq!(
            (by_id["stack-right"].x, by_id["stack-right"].y),
            (Some(30.0), Some(90.0))
        );
        assert_eq!(
            (by_id["peer"].x, by_id["peer"].y),
            (Some(140.0), Some(50.0))
        );
    }

    #[test]
    fn returns_a_forest_backed_by_the_caller_owned_nodes() {
        let owner = group("owner", "Owner", None, None);
        let child = leaf(
            "child",
            40.0,
            20.0,
            Some(json!({ "row": 1, "column": 1 })),
            Some("owner"),
        );
        let peer = leaf(
            "peer",
            40.0,
            20.0,
            Some(json!({ "row": 1, "column": 2 })),
            None,
        );
        let mut data = layout(vec![owner, child, peer], json!({}));

        let result =
            run_grid_layout_core(&mut data, GridLayoutCoreInternalOptions::default()).unwrap();

        assert_eq!(result.forest.node_by_id["child"], 1);
        assert_eq!(result.forest.group_by_id["owner"], 0);
        assert_eq!(
            result.forest.children_by_parent["owner"]
                .iter()
                .map(|&index| data.nodes[index].id.as_str())
                .collect::<Vec<_>>(),
            vec!["child"]
        );
        assert_eq!(
            result
                .forest
                .root_children
                .iter()
                .map(|&index| data.nodes[index].id.as_str())
                .collect::<Vec<_>>(),
            vec!["owner", "peer"]
        );
        assert_eq!(
            result
                .forest
                .post_order_groups
                .iter()
                .map(|&index| data.nodes[index].id.as_str())
                .collect::<Vec<_>>(),
            vec!["owner"]
        );
    }

    #[test]
    fn sizes_nested_groups_bottom_up_and_keeps_coordinates_direction_independent() {
        let build = || {
            layout(
                vec![
                    group("outer", "Outer", None, None),
                    group(
                        "middle",
                        "Middle",
                        Some("outer"),
                        Some(json!({ "row": 1, "column": 1 })),
                    ),
                    leaf(
                        "inner",
                        60.0,
                        30.0,
                        Some(json!({ "row": 1, "column": 1 })),
                        Some("middle"),
                    ),
                    leaf(
                        "sibling",
                        80.0,
                        40.0,
                        Some(json!({ "row": 1, "column": 2 })),
                        Some("outer"),
                    ),
                    leaf(
                        "root-leaf",
                        50.0,
                        20.0,
                        Some(json!({ "row": 1, "column": 2 })),
                        None,
                    ),
                ],
                json!({}),
            )
        };

        let mut tb = build();
        run_grid_layout_core(&mut tb, GridLayoutCoreInternalOptions::default()).unwrap();

        let mut lr = build();
        run_grid_layout_core(&mut lr, GridLayoutCoreInternalOptions::default()).unwrap();

        let by_id: HashMap<&str, &Node> = tb
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        let outer = by_id["outer"];
        let middle = by_id["middle"];
        let inner = by_id["inner"];

        assert!(outer.group_title_rect.is_some());
        assert!(middle.group_title_rect.is_some());

        let outer_left = outer.x.unwrap() - outer.width.unwrap() / 2.0;
        let outer_right = outer.x.unwrap() + outer.width.unwrap() / 2.0;
        let outer_top = outer.y.unwrap() - outer.height.unwrap() / 2.0;
        let outer_bottom = outer.y.unwrap() + outer.height.unwrap() / 2.0;
        let middle_left = middle.x.unwrap() - middle.width.unwrap() / 2.0;
        let middle_right = middle.x.unwrap() + middle.width.unwrap() / 2.0;
        let middle_top = middle.y.unwrap() - middle.height.unwrap() / 2.0;
        let middle_bottom = middle.y.unwrap() + middle.height.unwrap() / 2.0;

        let outer_title = outer.group_title_rect.unwrap();
        assert_eq!((outer_title.left, outer_title.top), (outer_left, outer_top));
        let middle_title = middle.group_title_rect.unwrap();
        assert_eq!(
            (middle_title.left, middle_title.top),
            (middle_left, middle_top)
        );
        assert!(middle_left >= outer_left);
        assert!(middle_right <= outer_right);
        assert!(middle_top >= outer_top);
        assert!(middle_bottom <= outer_bottom);
        assert!(inner.x.unwrap() > middle_left);
        assert!(inner.x.unwrap() < middle_right);
        assert!(inner.y.unwrap() > middle_top);
        assert!(inner.y.unwrap() < middle_bottom);

        let tb_coords = geometry_snapshot(&tb.nodes);
        let lr_coords = geometry_snapshot(&lr.nodes);
        assert_eq!(lr_coords, tb_coords);
    }

    #[test]
    fn centers_content_and_routing_corridors_when_the_title_determines_the_group_width() {
        let mut container = group("wide-title", "Wide title", None, None);
        container.label_bbox = Some(crate::layout::grid::types::LabelSize {
            width: 200.0,
            height: 20.0,
        });
        let child = leaf(
            "child",
            40.0,
            20.0,
            Some(json!({ "row": 1, "column": 1 })),
            Some("wide-title"),
        );
        let mut data = layout(vec![container, child], json!({}));

        let result =
            run_grid_layout_core(&mut data, GridLayoutCoreInternalOptions::default()).unwrap();
        let container_meta = &result.containers["wide-title"];

        assert_eq!(
            (data.nodes[0].x, data.nodes[0].width),
            (Some(120.0), Some(240.0))
        );
        assert_eq!(data.nodes[1].x, Some(120.0));
        assert_eq!(container_meta.content_left, 100.0);
        assert_eq!(container_meta.content_right, 140.0);
        assert_eq!(container_meta.vertical_corridors, vec![80.0, 160.0]);
    }

    #[test]
    fn precomputes_boundary_and_midpoint_corridors_for_sparse_rows_and_columns() {
        let mut data = layout(
            vec![
                leaf(
                    "top-left",
                    40.0,
                    20.0,
                    Some(json!({ "row": 1, "column": 1 })),
                    None,
                ),
                leaf(
                    "bottom-right",
                    60.0,
                    30.0,
                    Some(json!({ "row": 100, "column": 50 })),
                    None,
                ),
            ],
            json!({}),
        );

        let result =
            run_grid_layout_core(&mut data, GridLayoutCoreInternalOptions::default()).unwrap();
        let top_left = &result.item_meta["top-left"];
        let bottom_right = &result.item_meta["bottom-right"];

        assert_eq!(
            (
                top_left.left_corridor_x,
                top_left.right_corridor_x,
                top_left.top_corridor_y,
                top_left.bottom_corridor_y
            ),
            (-25.0, 65.0, -25.0, 45.0)
        );
        assert_eq!(
            (
                bottom_right.left_corridor_x,
                bottom_right.right_corridor_x,
                bottom_right.top_corridor_y,
                bottom_right.bottom_corridor_y
            ),
            (65.0, 175.0, 45.0, 125.0)
        );
    }

    #[test]
    fn reports_invalid_input_before_mutating_any_geometry() {
        let cases: Vec<(GridErrorCode, LayoutData)> = vec![
            (
                GridErrorCode::InvalidCoordinate,
                layout(
                    vec![
                        group("g", "Group", None, None),
                        leaf(
                            "member",
                            60.0,
                            30.0,
                            Some(json!({ "row": 1, "column": 1 })),
                            Some("g"),
                        ),
                        leaf(
                            "bad",
                            40.0,
                            20.0,
                            Some(json!({ "row": 0, "column": 1 })),
                            None,
                        ),
                    ],
                    json!({}),
                ),
            ),
            (
                GridErrorCode::CellAlignmentConflict,
                layout(
                    vec![
                        leaf(
                            "stack-top",
                            40.0,
                            20.0,
                            Some(json!({ "row": 1, "column": 1, "verticalAlign": "top" })),
                            None,
                        ),
                        leaf(
                            "stack-bottom",
                            40.0,
                            20.0,
                            Some(json!({ "row": 1, "column": 1, "verticalAlign": "bottom" })),
                            None,
                        ),
                        leaf(
                            "peer",
                            80.0,
                            80.0,
                            Some(json!({ "row": 1, "column": 2 })),
                            None,
                        ),
                    ],
                    json!({}),
                ),
            ),
        ];

        for (code, mut data) in cases {
            let before = geometry_snapshot(&data.nodes);
            let error = run_grid_layout_core(&mut data, GridLayoutCoreInternalOptions::default())
                .unwrap_err();
            assert_eq!(error.code, code);
            assert_eq!(geometry_snapshot(&data.nodes), before);
        }
    }

    #[test]
    fn reports_the_positive_size_requirement() {
        for (id, width, height) in [("zero-width", 0.0, 20.0), ("zero-height", 40.0, 0.0)] {
            let mut data = layout(vec![leaf(id, width, height, None, None)], json!({}));
            let error = run_grid_layout_core(&mut data, GridLayoutCoreInternalOptions::default())
                .unwrap_err();
            assert_eq!(
                error.to_string(),
                format!(
                    "GRID_MISSING_MEASUREMENT: Node \"{id}\" must have width and height greater than 0 (width: {}, height: {})",
                    crate::layout::grid::types::js_number(width),
                    crate::layout::grid::types::js_number(height)
                )
            );
        }
    }

    #[test]
    fn lays_out_fifteen_thousand_nested_groups_without_recursion_overflow() {
        let depth = 15_000usize;
        let mut nodes = vec![group("g0", "Group 0", None, None)];
        for index in 1..depth {
            nodes.push(group(
                &format!("g{index}"),
                &format!("Group {index}"),
                Some(&format!("g{}", index - 1)),
                Some(json!({ "row": 1, "column": 1 })),
            ));
        }
        nodes.push(leaf(
            "terminal",
            60.0,
            30.0,
            Some(json!({ "row": 1, "column": 1 })),
            Some(&format!("g{}", depth - 1)),
        ));

        let mut data = layout(nodes, json!({}));
        let result =
            run_grid_layout_core(&mut data, GridLayoutCoreInternalOptions::default()).unwrap();
        let terminal = data
            .nodes
            .iter()
            .find(|node| node.id == "terminal")
            .unwrap();
        let root_group = data.nodes.iter().find(|node| node.id == "g0").unwrap();
        let deepest_group = data
            .nodes
            .iter()
            .find(|node| node.id == format!("g{}", depth - 1))
            .unwrap();

        assert_eq!(result.forest.post_order_groups.len(), depth);
        for node in [root_group, deepest_group, terminal] {
            assert!(node.x.is_some_and(f64::is_finite));
            assert!(node.y.is_some_and(f64::is_finite));
        }
        assert!(root_group.width.is_some_and(f64::is_finite));
        assert!(root_group.height.is_some_and(f64::is_finite));
    }
}
