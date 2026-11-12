//! Port of cytoscape-fcose's `src/fcose/auxiliary.js`: the collection helpers the
//! spectral layout uses to connect disconnected components and to bound a compound.
//!
//! Upstream works over cytoscape collections; here the collections are lists of node
//! indices in cytoscape's order (the order the input listed them, children before
//! siblings, as `descendants()` walks). The iteration orders of `merge`, `intersection`,
//! and `difference` are preserved, because the component walk picks the first element of
//! an intersection and the dummy node depends on it.

use super::FcoseGraph;

/// A connected component of the layout, upstream's `cmpt` collection.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Component {
    /// The component's nodes, in `merge` order.
    pub(crate) nodes: Vec<usize>,
    /// The component's edges, in `merge` order.
    pub(crate) edges: Vec<usize>,
}

/// Upstream `auxiliary.getTopMostNodes`: the nodes that have no ancestor in the list.
pub(crate) fn get_top_most_nodes(graph: &FcoseGraph, nodes: &[usize]) -> Vec<usize> {
    nodes
        .iter()
        .copied()
        .filter(|&node| {
            let mut parent = graph.node(node).parent;
            while let Some(ancestor) = parent {
                if nodes.contains(&ancestor) {
                    return false;
                }
                parent = graph.node(ancestor).parent;
            }
            true
        })
        .collect()
}

/// Upstream `auxiliary.connectComponents`: walks the top-most nodes in components and,
/// when they are disconnected, records the minimum-degree node of each component so the
/// spectral layout can add one dummy vertex that joins them.
///
/// The component variables (the visited set, the queue, and the remaining top-most
/// nodes) are exactly upstream's. The returned components are upstream's `components`,
/// which the spectral layout ignores.
pub(crate) fn connect_components(
    graph: &FcoseGraph,
    top_most_nodes: &[usize],
    dummy_nodes: &mut Vec<(String, Vec<String>)>,
) -> Vec<Component> {
    let mut queue: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    let mut visited: Vec<usize> = Vec::new();
    let mut visited_top_most_nodes: Vec<usize> = Vec::new();
    let mut nodes_connected_to_dummy: Vec<String> = Vec::new();
    let mut components: Vec<Component> = Vec::new();
    let mut top_most: Vec<usize> = top_most_nodes.to_vec();

    let mut is_connected = false;
    let mut count = 1;

    loop {
        let mut component = Component::default();
        components.push(component.clone());

        let current_node = top_most[0];
        let mut children_of_current_node = Vec::new();
        children_of_current_node.push(current_node);
        children_of_current_node.extend(graph.descendants(current_node));
        visited_top_most_nodes.push(current_node);

        for node in children_of_current_node {
            queue.push_back(node);
            if !visited.contains(&node) {
                visited.push(node);
            }
            if !component.nodes.contains(&node) {
                component.nodes.push(node);
            }
        }

        while let Some(current_node) = queue.pop_front() {
            let mut neighbor_nodes: Vec<usize> = Vec::new();
            for neighbor in graph.neighbors(current_node) {
                if !graph.edges_with(current_node, neighbor).is_empty()
                    && !neighbor_nodes.contains(&neighbor)
                {
                    neighbor_nodes.push(neighbor);
                }
            }

            for neighbor_node in neighbor_nodes {
                let mut neighbor_and_ancestors = vec![neighbor_node];
                neighbor_and_ancestors.extend(graph.ancestors(neighbor_node));
                let current_neighbor = intersect(&top_most, &neighbor_and_ancestors);

                if let Some(&current_neighbor) = current_neighbor.first()
                    && !visited.contains(&current_neighbor)
                {
                    let mut children_of_neighbor = vec![current_neighbor];
                    children_of_neighbor.extend(graph.descendants(current_neighbor));

                    for node in children_of_neighbor {
                        queue.push_back(node);
                        if !visited.contains(&node) {
                            visited.push(node);
                        }
                        if !component.nodes.contains(&node) {
                            component.nodes.push(node);
                        }
                        if top_most.contains(&node) && !visited_top_most_nodes.contains(&node) {
                            visited_top_most_nodes.push(node);
                        }
                    }
                }
            }
        }

        // Take the component's edges whose endpoints are both in it.
        let nodes_in_component = component.nodes.clone();
        for node in nodes_in_component {
            for edge in graph.connected_edges(node) {
                let edge_data = graph.edge(edge);
                if component.nodes.contains(&edge_data.source)
                    && component.nodes.contains(&edge_data.target)
                    && !component.edges.contains(&edge)
                {
                    component.edges.push(edge);
                }
            }
        }
        *components.last_mut().expect("component pushed") = component;

        if visited_top_most_nodes.len() == top_most.len() {
            is_connected = true;
        }

        if !is_connected || count > 1 {
            let mut min_degree_node = visited_top_most_nodes[0];
            let mut min_degree = graph.connected_edges(min_degree_node).len();
            for &node in &visited_top_most_nodes {
                let degree = graph.connected_edges(node).len();
                if degree < min_degree {
                    min_degree = degree;
                    min_degree_node = node;
                }
            }

            nodes_connected_to_dummy.push(graph.node(min_degree_node).id.clone());

            let mut temp: Vec<usize> = Vec::new();
            for &node in &visited_top_most_nodes {
                if !temp.contains(&node) {
                    temp.push(node);
                }
            }
            visited_top_most_nodes.clear();
            top_most.retain(|node| !temp.contains(node));
            count += 1;
        }

        if is_connected {
            break;
        }
    }

    if !nodes_connected_to_dummy.is_empty() {
        dummy_nodes.push((
            format!("dummy{}", dummy_nodes.len() + 1),
            nodes_connected_to_dummy,
        ));
    }

    components
}

/// Upstream `auxiliary.calcBoundingBox`: the bounding box of a compound's descendant
/// simple nodes at the given coordinates.
pub(crate) fn calc_bounding_box(
    graph: &FcoseGraph,
    parent_node: usize,
    x_coords: &[f64],
    y_coords: &[f64],
    node_indexes: &[(String, usize)],
) -> (f64, f64, f64, f64) {
    // JavaScript's `Number.MAX_SAFE_INTEGER` and `Number.MIN_SAFE_INTEGER`.
    let mut left = 9007199254740991.0;
    let mut right = -9007199254740991.0;
    let mut top = 9007199254740991.0;
    let mut bottom = -9007199254740991.0;

    for node in graph.descendants(parent_node) {
        if graph.is_parent(node) {
            continue;
        }

        let Some(index) = node_indexes
            .iter()
            .find(|(id, _)| id == &graph.node(node).id)
            .map(|(_, index)| *index)
        else {
            continue;
        };

        let node = graph.node(node);
        let node_left = x_coords[index] - node.width / 2.0;
        let node_right = x_coords[index] + node.width / 2.0;
        let node_top = y_coords[index] - node.height / 2.0;
        let node_bottom = y_coords[index] + node.height / 2.0;

        if left > node_left {
            left = node_left;
        }
        if right < node_right {
            right = node_right;
        }
        if top > node_top {
            top = node_top;
        }
        if bottom < node_bottom {
            bottom = node_bottom;
        }
    }

    (left, top, right - left, bottom - top)
}

/// Upstream's collection `intersection`, including its order rule: it walks the smaller
/// collection, and the second one when the sizes are equal.
fn intersect(a: &[usize], b: &[usize]) -> Vec<usize> {
    let (small, large) = if a.len() < b.len() { (a, b) } else { (b, a) };

    small
        .iter()
        .copied()
        .filter(|node| large.contains(node))
        .collect()
}
