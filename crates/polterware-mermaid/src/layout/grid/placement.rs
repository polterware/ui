//! Port of Mermaid 12.3.0's
//! `rendering-util/layout-algorithms/grid/placement.ts`.
//!
//! Normalizes placement input and deterministically assigns each direct child of a container to a
//! grid cell. Placement metadata on a node takes precedence over the container-level placement
//! map.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use crate::layout::grid::types::{
    GRID_DEFAULTS, GridCellStack, GridError, GridErrorCode, GridHorizontalAlign,
    GridLayoutConfigNormalized, GridPlacement, GridResolvedPlacement, GridVerticalAlign,
    LayoutData, Node, compare_code_units, grid_error, resolve_edge_corner_radius,
};

const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

fn is_object_record(value: Option<&Value>) -> Option<&Map<String, Value>> {
    value.and_then(Value::as_object)
}

fn is_grid_horizontal_align(value: Option<&Value>) -> Option<GridHorizontalAlign> {
    match value.and_then(Value::as_str) {
        Some("left") => Some(GridHorizontalAlign::Left),
        Some("center") => Some(GridHorizontalAlign::Center),
        Some("right") => Some(GridHorizontalAlign::Right),
        _ => None,
    }
}

fn is_grid_vertical_align(value: Option<&Value>) -> Option<GridVerticalAlign> {
    match value.and_then(Value::as_str) {
        Some("top") => Some(GridVerticalAlign::Top),
        Some("center") => Some(GridVerticalAlign::Center),
        Some("bottom") => Some(GridVerticalAlign::Bottom),
        _ => None,
    }
}

fn is_valid_grid_coordinate(value: f64) -> bool {
    value.is_finite()
        && value.fract() == 0.0
        && value.abs() <= MAX_SAFE_INTEGER as f64
        && value > 0.0
}

fn placement_id(node: &Node) -> &str {
    node.placement_id.as_deref().unwrap_or(&node.id)
}

fn own_placement_from(value: Option<&Value>) -> GridPlacement {
    let Some(record) = is_object_record(value) else {
        return GridPlacement::default();
    };
    let mut placement = GridPlacement::default();
    if let Some(row) = record.get("row") {
        placement.has_row = true;
        placement.row = row.as_f64();
    }
    if let Some(column) = record.get("column") {
        placement.has_column = true;
        placement.column = column.as_f64();
    }
    placement.horizontal_align = is_grid_horizontal_align(record.get("horizontalAlign"));
    placement.vertical_align = is_grid_vertical_align(record.get("verticalAlign"));
    placement
}

fn validate_coordinate(
    node_id: &str,
    authored_placement_id: &str,
    field: &'static str,
    value: Option<f64>,
    present: bool,
) -> Result<(), GridError> {
    if !present {
        return Ok(());
    }
    if matches!(value, Some(value) if is_valid_grid_coordinate(value)) {
        return Ok(());
    }
    let generated_node_context = if node_id == authored_placement_id {
        String::new()
    } else {
        format!(" (node \"{node_id}\")")
    };
    let invalid_value = match value {
        Some(value) => Value::from(value),
        None => Value::Null,
    };
    Err(grid_error(
        GridErrorCode::InvalidCoordinate,
        format!(
            "Invalid {field} for placement \"{authored_placement_id}\"{generated_node_context}"
        ),
        Some(serde_json::json!({
            "nodeId": node_id,
            "placementId": authored_placement_id,
            "field": field,
            "value": invalid_value,
        })),
    ))
}

pub fn build_grid_source_order(nodes: &[Node]) -> HashMap<String, i64> {
    let indices: Vec<usize> = (0..nodes.len()).collect();
    build_grid_source_order_indices(nodes, &indices)
}

pub fn build_grid_source_order_indices(nodes: &[Node], indices: &[usize]) -> HashMap<String, i64> {
    let mut order: HashMap<String, i64> = HashMap::new();
    let mut next = 0i64;
    for &index in indices {
        let node = &nodes[index];
        if !order.contains_key(&node.id) {
            order.insert(node.id.clone(), next);
            next += 1;
        }
    }
    order
}

pub fn read_grid_config(data: &LayoutData) -> GridLayoutConfigNormalized {
    let raw = is_object_record(data.config.grid.as_ref());
    let placements_raw = raw.and_then(|raw| is_object_record(raw.get("placements")));

    let mut placements: HashMap<String, GridPlacement> = HashMap::new();
    if let Some(placements_raw) = placements_raw {
        for (key, value) in placements_raw {
            placements.insert(key.clone(), own_placement_from(Some(value)));
        }
    }

    let number_at = |key: &str| raw.and_then(|raw| raw.get(key)).and_then(Value::as_f64);
    let non_negative_at =
        |key: &str| number_at(key).filter(|value| value.is_finite() && *value >= 0.0);

    let columns =
        non_negative_at("columns").map_or(GRID_DEFAULTS.columns, |value| value.trunc() as usize);

    GridLayoutConfigNormalized {
        placements,
        columns,
        row_gap: non_negative_at("rowGap").unwrap_or(GRID_DEFAULTS.row_gap),
        column_gap: non_negative_at("columnGap").unwrap_or(GRID_DEFAULTS.column_gap),
        cell_gap: non_negative_at("cellGap").unwrap_or(GRID_DEFAULTS.cell_gap),
        container_padding: non_negative_at("containerPadding")
            .unwrap_or(GRID_DEFAULTS.container_padding),
        title_gap: non_negative_at("titleGap").unwrap_or(GRID_DEFAULTS.title_gap),
        horizontal_align: raw
            .and_then(|raw| is_grid_horizontal_align(raw.get("horizontalAlign")))
            .unwrap_or(GRID_DEFAULTS.horizontal_align),
        vertical_align: raw
            .and_then(|raw| is_grid_vertical_align(raw.get("verticalAlign")))
            .unwrap_or(GRID_DEFAULTS.vertical_align),
        // Normalize rendering options with placement so every routed edge receives one stable
        // style.
        curve: raw
            .and_then(|raw| raw.get("curve"))
            .and_then(Value::as_str)
            .and_then(crate::layout::grid::types::GridCurve::parse)
            .unwrap_or(GRID_DEFAULTS.curve),
        edge_corner_radius: resolve_edge_corner_radius(number_at("edgeCornerRadius")),
    }
}

/// Returns the upstream `log.warn` lines instead of writing to a logger. Callers are free to
/// ignore them, matching the layout core, which discards the result.
pub fn validate_grid_placement_map(
    items: &[usize],
    nodes: &[Node],
    config: &GridLayoutConfigNormalized,
) -> Vec<String> {
    let mut known_ids: HashSet<&str> = HashSet::new();
    for &item in items {
        known_ids.insert(placement_id(&nodes[item]));
    }
    let mut keys: Vec<&String> = config.placements.keys().collect();
    keys.sort_by(|a, b| compare_code_units(a, b));
    let mut warnings = Vec::new();
    for key in keys {
        if !known_ids.contains(key.as_str()) {
            warnings.push(format!(
                "Ignoring grid placement for unknown target \"{key}\""
            ));
        }
    }
    warnings
}

fn item_comparator<'a>(
    source_order: &'a HashMap<String, i64>,
) -> impl Fn(&Node, &Node) -> std::cmp::Ordering + 'a {
    move |a, b| {
        let a_order = source_order.get(&a.id).copied().unwrap_or(MAX_SAFE_INTEGER);
        let b_order = source_order.get(&b.id).copied().unwrap_or(MAX_SAFE_INTEGER);
        a_order
            .cmp(&b_order)
            .then_with(|| compare_code_units(&a.id, &b.id))
    }
}

fn resolve_item_placement(
    node_index: usize,
    node: &Node,
    source_order: &HashMap<String, i64>,
    config: &GridLayoutConfigNormalized,
) -> Result<GridResolvedPlacement, GridError> {
    let authored_placement_id = placement_id(node);
    let config_placement = config
        .placements
        .get(authored_placement_id)
        .cloned()
        .unwrap_or_default();
    let metadata_placement = own_placement_from(node.metadata.as_ref());

    // Node metadata is closest to the authored node, so it deliberately wins over the shared map.
    let row = if metadata_placement.has_row {
        metadata_placement.row
    } else {
        config_placement.row
    };
    let column = if metadata_placement.has_column {
        metadata_placement.column
    } else {
        config_placement.column
    };
    let row_present = metadata_placement.has_row || config_placement.has_row;
    let column_present = metadata_placement.has_column || config_placement.has_column;
    validate_coordinate(&node.id, authored_placement_id, "row", row, row_present)?;
    validate_coordinate(
        &node.id,
        authored_placement_id,
        "column",
        column,
        column_present,
    )?;

    Ok(GridResolvedPlacement {
        item: node_index,
        item_id: node.id.clone(),
        row: row.map_or(0, |value| value as i64),
        column: column.map_or(0, |value| value as i64),
        horizontal_align: metadata_placement
            .horizontal_align
            .or(config_placement.horizontal_align)
            .unwrap_or(config.horizontal_align),
        vertical_align: metadata_placement
            .vertical_align
            .or(config_placement.vertical_align)
            .unwrap_or(config.vertical_align),
        source_order: source_order
            .get(&node.id)
            .copied()
            .unwrap_or(MAX_SAFE_INTEGER),
        explicit_row: row.is_some(),
        explicit_column: column.is_some(),
        explicit_cell: row.is_some() && column.is_some(),
    })
}

fn next_free_column(row: i64, occupied: &HashSet<String>, start_column: i64) -> i64 {
    let mut column = start_column;
    while occupied.contains(&format!("{row}:{column}")) {
        column += 1;
    }
    column
}

fn next_free_row(column: i64, occupied: &HashSet<String>, start_row: i64) -> i64 {
    let mut row = start_row;
    while occupied.contains(&format!("{row}:{column}")) {
        row += 1;
    }
    row
}

struct AutoCell {
    row: i64,
    column: i64,
    next_index: i64,
}

fn next_auto_cell(
    occupied: &HashSet<String>,
    candidate_columns: i64,
    start_index: i64,
) -> AutoCell {
    let mut index = start_index;
    // The occupied set is finite while the row-major coordinate space is unbounded, so a free cell
    // must eventually be found.
    loop {
        let row = index / candidate_columns + 1;
        let column = index % candidate_columns + 1;
        index += 1;
        if !occupied.contains(&format!("{row}:{column}")) {
            return AutoCell {
                row,
                column,
                next_index: index,
            };
        }
    }
}

#[derive(Debug)]
pub struct GridPlacementResolution {
    pub placements: Vec<GridResolvedPlacement>,
    pub cells: HashMap<String, GridCellStack>,
}

pub fn resolve_grid_placements(
    nodes: &[Node],
    items: &[usize],
    source_order: &HashMap<String, i64>,
    config: &GridLayoutConfigNormalized,
) -> Result<GridPlacementResolution, GridError> {
    let mut sorted_items: Vec<usize> = items.to_vec();
    sorted_items.sort_by(|&a, &b| item_comparator(source_order)(&nodes[a], &nodes[b]));

    let mut resolved: Vec<GridResolvedPlacement> = Vec::with_capacity(sorted_items.len());
    for item in sorted_items {
        resolved.push(resolve_item_placement(
            item,
            &nodes[item],
            source_order,
            config,
        )?);
    }

    let mut cells: HashMap<String, GridCellStack> = HashMap::new();
    let mut occupied: HashSet<String> = HashSet::new();

    // Reserve fully specified cells first. Auto-placement must route around authored coordinates
    // regardless of source order, while multiple explicit occupants intentionally form a stack.
    for placement in &resolved {
        if !placement.explicit_cell {
            continue;
        }
        let key = format!("{}:{}", placement.row, placement.column);
        occupied.insert(key.clone());
        if let Some(existing) = cells.get_mut(&key) {
            if existing.vertical_align != placement.vertical_align {
                let mut item_ids: Vec<String> = existing
                    .items
                    .iter()
                    .map(|item| item.item_id.clone())
                    .collect();
                item_ids.push(placement.item_id.clone());
                let mut values: Vec<&str> = existing
                    .items
                    .iter()
                    .map(|item| item.vertical_align.as_str())
                    .collect();
                values.push(placement.vertical_align.as_str());
                return Err(grid_error(
                    GridErrorCode::CellAlignmentConflict,
                    format!(
                        "Conflicting vertical alignment in cell ({}, {})",
                        placement.row, placement.column
                    ),
                    Some(serde_json::json!({
                        "containerId": nodes[placement.item]
                            .parent_id
                            .clone()
                            .unwrap_or_else(|| "root".to_string()),
                        "row": placement.row,
                        "column": placement.column,
                        "itemIds": item_ids,
                        "values": values,
                    })),
                ));
            }
            existing.items.push(placement.clone());
        } else {
            cells.insert(
                key,
                GridCellStack {
                    row: placement.row,
                    column: placement.column,
                    items: vec![placement.clone()],
                    width: 0.0,
                    height: 0.0,
                    vertical_align: placement.vertical_align,
                },
            );
        }
    }

    let candidate_columns = if config.columns > 0 {
        config.columns as i64
    } else {
        (resolved.len() as f64).sqrt().ceil().max(1.0) as i64
    };
    let mut next_auto_index = 0i64;
    let mut next_column_by_row: HashMap<i64, i64> = HashMap::new();
    let mut next_row_by_column: HashMap<i64, i64> = HashMap::new();

    // Partially specified placements scan only their missing axis. Fully automatic placements use a
    // compact row-major grid whose default width keeps roughly square diagrams. Each cursor only
    // moves forward because occupied cells are never released.
    for placement in &mut resolved {
        if placement.explicit_cell {
            continue;
        }

        if placement.explicit_row {
            let column = next_free_column(
                placement.row,
                &occupied,
                next_column_by_row.get(&placement.row).copied().unwrap_or(1),
            );
            placement.column = column;
            next_column_by_row.insert(placement.row, column + 1);
        } else if placement.explicit_column {
            let row = next_free_row(
                placement.column,
                &occupied,
                next_row_by_column
                    .get(&placement.column)
                    .copied()
                    .unwrap_or(1),
            );
            placement.row = row;
            next_row_by_column.insert(placement.column, row + 1);
        } else {
            let next = next_auto_cell(&occupied, candidate_columns, next_auto_index);
            placement.row = next.row;
            placement.column = next.column;
            next_auto_index = next.next_index;
        }

        let key = format!("{}:{}", placement.row, placement.column);
        occupied.insert(key.clone());
        cells.insert(
            key,
            GridCellStack {
                row: placement.row,
                column: placement.column,
                items: vec![placement.clone()],
                width: 0.0,
                height: 0.0,
                vertical_align: placement.vertical_align,
            },
        );
    }

    Ok(GridPlacementResolution {
        placements: resolved,
        cells,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(id: &str, metadata: Option<Value>, placement_id: Option<&str>) -> Node {
        Node {
            id: id.to_string(),
            placement_id: placement_id.map(str::to_string),
            is_group: false,
            width: Some(100.0),
            height: Some(40.0),
            metadata,
            ..Node::default()
        }
    }

    fn config(grid: Value) -> LayoutData {
        LayoutData {
            config: crate::layout::grid::types::GridConfig { grid: Some(grid) },
            ..LayoutData::default()
        }
    }

    fn resolve(items: &[Node], data: &LayoutData) -> GridPlacementResolution {
        let source_order = build_grid_source_order(items);
        let indices: Vec<usize> = (0..items.len()).collect();
        resolve_grid_placements(items, &indices, &source_order, &read_grid_config(data)).unwrap()
    }

    #[test]
    fn ignores_arrays_where_grid_configuration_expects_keyed_records() {
        let data = config(json!({ "placements": [{ "row": 1, "column": 1 }] }));
        assert_eq!(read_grid_config(&data).placements.len(), 0);
    }

    #[test]
    fn defaults_the_grid_curve_and_corner_radius_and_replaces_invalid_values() {
        let defaults = read_grid_config(&config(json!({})));
        assert_eq!(
            defaults.curve,
            crate::layout::grid::types::GridCurve::Rounded
        );
        assert_eq!(defaults.edge_corner_radius, 5.0);

        let rounded = read_grid_config(&config(
            json!({ "curve": "rounded", "edgeCornerRadius": 12 }),
        ));
        assert_eq!(
            rounded.curve,
            crate::layout::grid::types::GridCurve::Rounded
        );
        assert_eq!(rounded.edge_corner_radius, 12.0);

        let invalid = read_grid_config(&config(
            json!({ "curve": "unknown", "edgeCornerRadius": -1 }),
        ));
        assert_eq!(
            invalid.curve,
            crate::layout::grid::types::GridCurve::Rounded
        );
        assert_eq!(invalid.edge_corner_radius, 5.0);
    }

    #[test]
    fn resolves_full_partial_automatic_sparse_and_precedence_cases_deterministically() {
        let items = vec![
            node("explicit-a", Some(json!({ "row": 1, "column": 1 })), None),
            node("explicit-b", Some(json!({ "row": 100, "column": 3 })), None),
            node(
                "stack-top",
                Some(
                    json!({ "row": 2, "column": 2, "horizontalAlign": "left", "verticalAlign": "bottom" }),
                ),
                None,
            ),
            node(
                "stack-bottom",
                Some(
                    json!({ "row": 2, "column": 2, "horizontalAlign": "right", "verticalAlign": "bottom" }),
                ),
                None,
            ),
            node("row-only", Some(json!({ "row": 1 })), None),
            node("column-only", Some(json!({ "column": 1 })), None),
            node("auto-a", None, None),
            node("auto-b", None, None),
            node("map-only", None, None),
            node(
                "metadata-wins",
                Some(json!({ "column": 4, "horizontalAlign": "left" })),
                None,
            ),
        ];
        let data = config(json!({
            "columns": 3,
            "placements": {
                "map-only": { "row": 3, "column": 3 },
                "metadata-wins": { "row": 4, "column": 2, "horizontalAlign": "right" },
            },
        }));

        let resolution = resolve(&items, &data);
        let by_id: HashMap<&str, &GridResolvedPlacement> = resolution
            .placements
            .iter()
            .map(|placement| (placement.item_id.as_str(), placement))
            .collect();

        assert_eq!(
            (by_id["explicit-a"].row, by_id["explicit-a"].column),
            (1, 1)
        );
        assert_eq!(
            (by_id["explicit-b"].row, by_id["explicit-b"].column),
            (100, 3)
        );
        assert_eq!(
            resolution.cells["2:2"]
                .items
                .iter()
                .map(|item| item.item_id.as_str())
                .collect::<Vec<_>>(),
            vec!["stack-top", "stack-bottom"]
        );
        assert_eq!((by_id["row-only"].row, by_id["row-only"].column), (1, 2));
        assert_eq!(
            (by_id["column-only"].row, by_id["column-only"].column),
            (2, 1)
        );
        assert_eq!((by_id["auto-a"].row, by_id["auto-a"].column), (1, 3));
        assert_eq!((by_id["auto-b"].row, by_id["auto-b"].column), (2, 3));
        assert_eq!((by_id["map-only"].row, by_id["map-only"].column), (3, 3));
        assert_eq!(
            (
                by_id["metadata-wins"].row,
                by_id["metadata-wins"].column,
                by_id["metadata-wins"].horizontal_align
            ),
            (4, 4, GridHorizontalAlign::Left)
        );
    }

    #[test]
    fn advances_partial_placement_cursors_past_cells_occupied_by_other_modes() {
        let items = vec![
            node("explicit", Some(json!({ "row": 1, "column": 1 })), None),
            node("row-a", Some(json!({ "row": 1 })), None),
            node("column-a", Some(json!({ "column": 2 })), None),
            node("auto-a", None, None),
            node("row-b", Some(json!({ "row": 1 })), None),
            node("column-b", Some(json!({ "column": 2 })), None),
            node("auto-b", None, None),
        ];
        let data = config(json!({ "columns": 3 }));

        let resolution = resolve(&items, &data);
        let by_id: HashMap<&str, &GridResolvedPlacement> = resolution
            .placements
            .iter()
            .map(|placement| (placement.item_id.as_str(), placement))
            .collect();

        assert_eq!((by_id["row-a"].row, by_id["row-a"].column), (1, 2));
        assert_eq!((by_id["column-a"].row, by_id["column-a"].column), (2, 2));
        assert_eq!((by_id["auto-a"].row, by_id["auto-a"].column), (1, 3));
        assert_eq!((by_id["row-b"].row, by_id["row-b"].column), (1, 4));
        assert_eq!((by_id["column-b"].row, by_id["column-b"].column), (3, 2));
        assert_eq!((by_id["auto-b"].row, by_id["auto-b"].column), (2, 1));
    }

    #[test]
    fn throws_for_invalid_row_and_column_coordinates() {
        for (field, value) in [
            ("row", json!(0)),
            ("row", json!(-1)),
            ("row", json!(1.5)),
            ("row", json!("2")),
            ("column", json!(0)),
            ("column", json!(-1)),
            ("column", json!(1.5)),
            ("column", json!("2")),
        ] {
            let items = vec![node("bad", Some(json!({ field: value })), None)];
            let source_order = build_grid_source_order(&items);
            let error = resolve_grid_placements(
                &items,
                &[0],
                &source_order,
                &read_grid_config(&config(json!({}))),
            )
            .unwrap_err();
            assert_eq!(error.code, GridErrorCode::InvalidCoordinate);
        }
    }

    #[test]
    fn throws_for_conflicting_stack_alignment() {
        let items = vec![
            node(
                "one",
                Some(json!({ "row": 1, "column": 1, "verticalAlign": "top" })),
                None,
            ),
            node(
                "two",
                Some(json!({ "row": 1, "column": 1, "verticalAlign": "bottom" })),
                None,
            ),
        ];
        let source_order = build_grid_source_order(&items);
        let error = resolve_grid_placements(
            &items,
            &[0, 1],
            &source_order,
            &read_grid_config(&config(json!({}))),
        )
        .unwrap_err();
        assert_eq!(error.code, GridErrorCode::CellAlignmentConflict);
    }

    #[test]
    fn ignores_and_warns_about_unknown_placement_ids() {
        let data = config(json!({ "placements": { "unknown": { "row": 1, "column": 1 } } }));
        let config = read_grid_config(&data);
        let known = [node("known", None, None)];
        let warnings = validate_grid_placement_map(&[0], &known, &config);
        assert_eq!(
            warnings,
            vec!["Ignoring grid placement for unknown target \"unknown\"".to_string()]
        );
    }

    #[test]
    fn resolves_generated_nodes_exclusively_through_their_authored_placement_ids() {
        let items = vec![
            node("entity-CUSTOMER-0", None, Some("CUSTOMER")),
            node("entity-ORDER-1", None, Some("ORDER")),
        ];
        let data = config(json!({
            "placements": {
                "CUSTOMER": { "row": 1, "column": 1 },
                "ORDER": { "row": 1, "column": 2 },
                "entity-ORDER-1": { "row": 2, "column": 2 },
            },
        }));
        let grid_config = read_grid_config(&data);
        let warnings = validate_grid_placement_map(&[0, 1], &items, &grid_config);
        assert_eq!(
            warnings,
            vec!["Ignoring grid placement for unknown target \"entity-ORDER-1\"".to_string()]
        );

        let resolution = resolve(&items, &data);
        let by_id: HashMap<&str, &GridResolvedPlacement> = resolution
            .placements
            .iter()
            .map(|placement| (placement.item_id.as_str(), placement))
            .collect();
        assert_eq!(
            (
                by_id["entity-CUSTOMER-0"].row,
                by_id["entity-CUSTOMER-0"].column
            ),
            (1, 1)
        );
        assert_eq!(
            (by_id["entity-ORDER-1"].row, by_id["entity-ORDER-1"].column),
            (1, 2)
        );
    }

    #[test]
    fn reports_authored_placement_ids_for_invalid_generated_node_coordinates() {
        let items = vec![node("entity-CUSTOMER-0", None, Some("CUSTOMER"))];
        let data = config(json!({ "placements": { "CUSTOMER": { "row": 0, "column": 1 } } }));
        let source_order = build_grid_source_order(&items);
        let error = resolve_grid_placements(&items, &[0], &source_order, &read_grid_config(&data))
            .unwrap_err();

        assert_eq!(error.code, GridErrorCode::InvalidCoordinate);
        assert_eq!(
            error.to_string(),
            "GRID_INVALID_COORDINATE: Invalid row for placement \"CUSTOMER\" (node \"entity-CUSTOMER-0\")"
        );
        let details = error.details.expect("details");
        assert_eq!(details["nodeId"], json!("entity-CUSTOMER-0"));
        assert_eq!(details["placementId"], json!("CUSTOMER"));
        assert_eq!(details["field"], json!("row"));
        assert_eq!(details["value"].as_f64(), Some(0.0));
    }

    #[test]
    fn rejects_explicit_null_coordinates_instead_of_treating_them_as_omitted() {
        let items = vec![node("bad", Some(json!({ "row": null, "column": 1 })), None)];
        let data = config(json!({ "placements": { "bad": { "row": 2, "column": 2 } } }));
        let source_order = build_grid_source_order(&items);
        let error = resolve_grid_placements(&items, &[0], &source_order, &read_grid_config(&data))
            .unwrap_err();
        assert_eq!(error.code, GridErrorCode::InvalidCoordinate);
    }
}
