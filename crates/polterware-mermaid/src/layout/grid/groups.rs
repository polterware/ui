//! Port of Mermaid 12.3.0's
//! `rendering-util/layout-algorithms/grid/groups.ts`.

use std::collections::{HashMap, HashSet};

use serde_json::json;

use crate::layout::grid::types::{
    GridContainerId, GridError, GridErrorCode, GridForest, Node, ROOT_CONTAINER_ID, grid_error,
    is_edge_label_node,
};

fn as_group(nodes: &[Node], index: usize) -> Option<usize> {
    nodes[index].is_group.then_some(index)
}

/// Builds the containment model used by every later grid-layout phase.
///
/// The returned forest excludes synthetic edge-label nodes from the geometry hierarchy, validates
/// every parent reference, and orders groups children-first so nested containers can be measured
/// before their parents.
pub fn build_grid_forest(nodes: &[Node]) -> Result<GridForest, GridError> {
    let mut node_by_id: HashMap<String, usize> = HashMap::new();
    let mut group_by_id: HashMap<String, usize> = HashMap::new();
    let mut helper_node_ids: HashSet<String> = HashSet::new();

    for (index, node) in nodes.iter().enumerate() {
        if node.id == ROOT_CONTAINER_ID {
            return Err(grid_error(
                GridErrorCode::InvalidContainment,
                format!("Node id \"{ROOT_CONTAINER_ID}\" is reserved for the grid layout root"),
                Some(json!({ "traversedIds": [node.id] })),
            ));
        }
        if node_by_id.contains_key(&node.id) {
            return Err(grid_error(
                GridErrorCode::InvalidContainment,
                format!("Duplicate node id \"{}\"", node.id),
                Some(json!({ "traversedIds": [node.id] })),
            ));
        }
        node_by_id.insert(node.id.clone(), index);
        if is_edge_label_node(node) {
            helper_node_ids.insert(node.id.clone());
            continue;
        }
        if let Some(group) = as_group(nodes, index) {
            group_by_id.insert(node.id.clone(), group);
        }
    }

    let mut children_by_parent: HashMap<GridContainerId, Vec<usize>> = HashMap::new();
    children_by_parent.insert(ROOT_CONTAINER_ID.to_string(), Vec::new());

    for (index, node) in nodes.iter().enumerate() {
        if helper_node_ids.contains(&node.id) {
            continue;
        }
        let parent_id = node
            .parent_id
            .clone()
            .unwrap_or_else(|| ROOT_CONTAINER_ID.to_string());
        if parent_id != ROOT_CONTAINER_ID && !group_by_id.contains_key(&parent_id) {
            return Err(grid_error(
                GridErrorCode::InvalidContainment,
                format!("Missing parent \"{parent_id}\" for \"{}\"", node.id),
                Some(json!({ "traversedIds": [node.id, parent_id] })),
            ));
        }
        children_by_parent.entry(parent_id).or_default().push(index);
    }

    // 0 = unvisited, 1 = on the active path, 2 = finished.
    let mut state: HashMap<String, u8> = HashMap::new();
    let mut post_order_groups: Vec<usize> = Vec::new();

    // An explicit stack rather than recursion keeps deeply nested diagrams off the call stack.
    let direct_child_groups = |group_id: &str| -> Vec<usize> {
        children_by_parent
            .get(group_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .iter()
            .filter_map(|&child| as_group(nodes, child))
            .collect()
    };

    for root_child in children_by_parent
        .get(ROOT_CONTAINER_ID)
        .cloned()
        .unwrap_or_default()
    {
        if as_group(nodes, root_child).is_some() {
            visit_group(
                nodes,
                root_child,
                &group_by_id,
                &direct_child_groups,
                &mut state,
                &mut post_order_groups,
            )?;
        }
    }

    // Normally every group is reached from a root child. This second pass also validates malformed
    // disconnected components and produces a useful cycle error instead of silently dropping them.
    let mut remaining_groups: Vec<usize> = group_by_id.values().copied().collect();
    remaining_groups.sort_unstable();
    for group in remaining_groups {
        if state.get(&nodes[group].id).copied().unwrap_or(0) == 0 {
            visit_group(
                nodes,
                group,
                &group_by_id,
                &direct_child_groups,
                &mut state,
                &mut post_order_groups,
            )?;
        }
    }

    let root_children = children_by_parent
        .get(ROOT_CONTAINER_ID)
        .cloned()
        .unwrap_or_default();

    Ok(GridForest {
        node_by_id,
        group_by_id,
        children_by_parent,
        root_children,
        post_order_groups,
        helper_node_ids,
    })
}

#[allow(clippy::too_many_arguments)]
fn visit_group(
    nodes: &[Node],
    start_group: usize,
    group_by_id: &HashMap<String, usize>,
    direct_child_groups: &impl Fn(&str) -> Vec<usize>,
    state: &mut HashMap<String, u8>,
    post_order_groups: &mut Vec<usize>,
) -> Result<(), GridError> {
    struct VisitFrame {
        group_id: String,
        child_groups: Vec<usize>,
        next_child_index: usize,
    }

    let start_group_id = nodes[start_group].id.clone();
    let current_state = state.get(&start_group_id).copied().unwrap_or(0);
    if current_state == 1 {
        return Err(grid_error(
            GridErrorCode::InvalidContainment,
            format!("Containment cycle at \"{start_group_id}\""),
            Some(json!({ "traversedIds": [start_group_id] })),
        ));
    }
    if current_state == 2 {
        return Ok(());
    }

    state.insert(start_group_id.clone(), 1);
    let mut stack: Vec<VisitFrame> = vec![VisitFrame {
        child_groups: direct_child_groups(&start_group_id),
        group_id: start_group_id,
        next_child_index: 0,
    }];

    while !stack.is_empty() {
        let frame_index = stack.len() - 1;
        let child = stack[frame_index]
            .child_groups
            .get(stack[frame_index].next_child_index)
            .copied();

        if let Some(child) = child {
            stack[frame_index].next_child_index += 1;
            let child_id = nodes[child].id.clone();
            let child_state = state.get(&child_id).copied().unwrap_or(0);
            if child_state == 1 {
                let mut traversed: Vec<String> =
                    stack.iter().map(|item| item.group_id.clone()).collect();
                traversed.push(child_id.clone());
                return Err(grid_error(
                    GridErrorCode::InvalidContainment,
                    format!("Containment cycle at \"{child_id}\""),
                    Some(json!({ "traversedIds": traversed })),
                ));
            }
            if child_state == 2 {
                continue;
            }

            state.insert(child_id.clone(), 1);
            stack.push(VisitFrame {
                child_groups: direct_child_groups(&child_id),
                group_id: child_id,
                next_child_index: 0,
            });
            continue;
        }

        let frame = stack.pop().expect("stack is not empty");
        state.insert(frame.group_id.clone(), 2);
        if let Some(&group) = group_by_id.get(&frame.group_id) {
            post_order_groups.push(group);
        }
    }

    Ok(())
}

pub fn is_ancestor_group(
    ancestor_id: &str,
    node: Option<usize>,
    nodes: &[Node],
    node_by_id: &HashMap<String, usize>,
) -> bool {
    let mut current = node;
    let mut seen: HashSet<String> = HashSet::new();
    // The forest builder already rejects cycles. The guard keeps this predicate safe when callers
    // provide a partially constructed or independently sourced node map.
    while let Some(index) = current {
        let Some(parent_id) = nodes[index].parent_id.clone() else {
            break;
        };
        if parent_id.is_empty() {
            break;
        }
        if parent_id == ancestor_id {
            return true;
        }
        if !seen.insert(parent_id.clone()) {
            return false;
        }
        current = node_by_id.get(&parent_id).copied();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::grid::types::Node;

    fn group(id: &str, parent_id: Option<&str>) -> Node {
        Node {
            id: id.to_string(),
            parent_id: parent_id.map(str::to_string),
            is_group: true,
            width: Some(100.0),
            height: Some(80.0),
            ..Node::default()
        }
    }

    fn leaf(id: &str, parent_id: Option<&str>) -> Node {
        Node {
            id: id.to_string(),
            parent_id: parent_id.map(str::to_string),
            is_group: false,
            width: Some(60.0),
            height: Some(40.0),
            ..Node::default()
        }
    }

    #[test]
    fn builds_a_stable_post_order_forest_and_ancestor_checks() {
        let nodes = vec![
            group("outer", None),
            leaf("a", Some("outer")),
            group("inner", Some("outer")),
            leaf("b", Some("inner")),
            leaf("root", None),
        ];

        let forest = build_grid_forest(&nodes).unwrap();
        let ids = |indices: &[usize]| nodes_ids(&nodes, indices);
        assert_eq!(ids(&forest.root_children), vec!["outer", "root"]);
        assert_eq!(ids(&forest.post_order_groups), vec!["inner", "outer"]);
        assert!(is_ancestor_group(
            "outer",
            forest.node_by_id.get("b").copied(),
            &nodes,
            &forest.node_by_id
        ));
        assert!(!is_ancestor_group(
            "inner",
            forest.node_by_id.get("a").copied(),
            &nodes,
            &forest.node_by_id
        ));
    }

    fn nodes_ids<'a>(nodes: &'a [Node], indices: &[usize]) -> Vec<&'a str> {
        indices
            .iter()
            .map(|&index| nodes[index].id.as_str())
            .collect()
    }

    #[test]
    fn rejects_missing_parents_and_cycles() {
        let missing = build_grid_forest(&[leaf("orphan", Some("missing"))]).unwrap_err();
        assert_eq!(missing.code, GridErrorCode::InvalidContainment);

        let cycle = build_grid_forest(&[group("a", Some("b")), group("b", Some("a"))]).unwrap_err();
        assert_eq!(cycle.code, GridErrorCode::InvalidContainment);
    }

    #[test]
    fn rejects_the_reserved_synthetic_root_id() {
        let error = build_grid_forest(&[group("__grid_root__", None)]).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Node id \"__grid_root__\" is reserved")
        );
    }

    #[test]
    fn rejects_duplicate_ids_for_regular_nodes_and_label_helpers() {
        let duplicate = vec![leaf("duplicate", None), leaf("duplicate", None)];
        assert_eq!(
            build_grid_forest(&duplicate).unwrap_err().to_string(),
            "GRID_INVALID_CONTAINMENT: Duplicate node id \"duplicate\""
        );

        let helper = Node {
            is_edge_label: true,
            ..leaf("duplicate", None)
        };
        let mixed = vec![helper, leaf("duplicate", None)];
        assert_eq!(
            build_grid_forest(&mixed).unwrap_err().to_string(),
            "GRID_INVALID_CONTAINMENT: Duplicate node id \"duplicate\""
        );
    }

    #[test]
    fn supports_fifteen_thousand_levels_of_valid_nesting() {
        let depth = 15_000usize;
        let mut nodes = vec![group("g0", None)];
        for index in 1..depth {
            nodes.push(group(
                &format!("g{index}"),
                Some(&format!("g{}", index - 1)),
            ));
        }
        nodes.push(leaf("terminal", Some(&format!("g{}", depth - 1))));

        let forest = build_grid_forest(&nodes).unwrap();

        assert_eq!(nodes_ids(&nodes, &forest.root_children), vec!["g0"]);
        assert_eq!(forest.post_order_groups.len(), depth);
        assert_eq!(
            nodes[forest.post_order_groups[0]].id,
            format!("g{}", depth - 1)
        );
        assert_eq!(nodes[*forest.post_order_groups.last().unwrap()].id, "g0");
        assert!(is_ancestor_group(
            "g0",
            forest.node_by_id.get("terminal").copied(),
            &nodes,
            &forest.node_by_id
        ));
    }
}
