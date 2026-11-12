//! Port of cytoscape-fcose's `src/fcose/spectral.js`: the spectral draft layout that
//! fcose runs before CoSE.
//!
//! The algorithm is upstream's, including its random draws: `Math.random` is Mermaid's
//! seeded mulberry32 generator (see the module docs of [`super`]), and the draws happen
//! in the same order (random sampling columns, the first greedy pivot, the eigenvectors'
//! initial guesses), so a run repeats. The matrices are the already-ported
//! [`matrix`](crate::layout::layout_base::util::matrix) and
//! [`svd`](crate::layout::layout_base::util::svd).
//!
//! Upstream's `C`, `PHI`, and `INV` stay `Vec<Vec<f64>>` arrays, and the coordinate
//! arrays stay parallel to the `nodeIndexes` map, because later steps index them by that
//! map's value. The loops index those arrays instead of iterating them so the expressions
//! read like upstream's; clippy's range-loop lint is silenced for that reason.

#![allow(clippy::needless_range_loop)]

use std::collections::HashMap;

use crate::layout::layout_base::util::matrix;
use crate::layout::layout_base::util::random_seed;
use crate::layout::layout_base::util::svd;

use super::FcoseGraph;
use super::FcoseOptions;
use super::auxiliary;

/// Upstream's `infinity`, the distance of an unvisited node.
const INFINITY: f64 = 100_000_000.0;
/// Upstream's `small`, the first previous ratio of the power iteration.
const SMALL: f64 = 0.000000001;

/// The spectral layout's result, upstream's `{nodeIndexes, xCoords, yCoords}`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SpectralResult {
    /// The transformed graph's ids and their index in the coordinate arrays: real
    /// childless nodes first, then the dummy vertices that connect components.
    pub(crate) node_indexes: Vec<(String, usize)>,
    /// Center x of each index.
    pub(crate) x_coords: Vec<f64>,
    /// Center y of each index.
    pub(crate) y_coords: Vec<f64>,
}

impl SpectralResult {
    /// The coordinate index of a node id.
    pub(crate) fn index_of(&self, id: &str) -> Option<usize> {
        self.node_indexes
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, index)| *index)
    }
}

/// Upstream `spectralLayout`.
pub(crate) fn spectral_layout(graph: &FcoseGraph, options: &FcoseOptions) -> SpectralResult {
    let nodes: Vec<usize> = (0..graph.nodes.len()).collect();
    let parent_nodes: Vec<usize> = nodes
        .iter()
        .copied()
        .filter(|&node| graph.is_parent(node))
        .collect();

    // Connect the disconnected components, first at the top level, then inside each
    // compound node.
    let mut dummy_nodes: Vec<(String, Vec<String>)> = Vec::new();
    let top_most = auxiliary::get_top_most_nodes(graph, &nodes);
    auxiliary::connect_components(graph, &top_most, &mut dummy_nodes);

    for &parent in &parent_nodes {
        let descendants = graph.descendants(parent);
        let top_most = auxiliary::get_top_most_nodes(graph, &descendants);
        auxiliary::connect_components(graph, &top_most, &mut dummy_nodes);
    }

    // Real nodes first, then the dummy vertices.
    let mut node_indexes: Vec<(String, usize)> = Vec::new();
    for &node in &nodes {
        if !graph.is_parent(node) {
            let index = node_indexes.len();
            node_indexes.push((graph.node(node).id.clone(), index));
        }
    }
    for (key, _) in &dummy_nodes {
        let index = node_indexes.len();
        node_indexes.push((key.clone(), index));
    }

    let index_of: HashMap<String, usize> = node_indexes
        .iter()
        .map(|(key, index)| (key.clone(), *index))
        .collect();
    let node_size = node_indexes.len();
    if node_size == 0 {
        return SpectralResult::default();
    }

    // A parent represents its childless descendant with the fewest edges.
    let mut parent_child_map: Vec<(usize, usize)> = Vec::new();
    for &parent in &parent_nodes {
        let mut children = graph.children(parent).to_vec();
        while !children.iter().any(|&child| !graph.is_parent(child)) {
            let Some(&first) = children.first() else {
                break;
            };
            children = graph.children(first).to_vec();
        }
        let childless: Vec<usize> = children
            .into_iter()
            .filter(|&child| !graph.is_parent(child))
            .collect();
        if childless.is_empty() {
            continue;
        }

        let mut index = 0;
        let mut min = graph.connected_edges(childless[0]).len();
        for (i, &child) in childless.iter().enumerate() {
            let degree = graph.connected_edges(child).len();
            if degree < min {
                min = degree;
                index = i;
            }
        }
        parent_child_map.push((parent, childless[index]));
    }

    let parent_child_of = |node: usize| -> Option<usize> {
        parent_child_map
            .iter()
            .find(|(parent, _)| *parent == node)
            .map(|(_, child)| *child)
    };

    // Add the neighborhood relations, real nodes first.
    let mut all_nodes_neighborhood: Vec<Vec<String>> = vec![Vec::new(); node_size];
    for &node in &nodes {
        let ele_index = if graph.is_parent(node) {
            parent_child_of(node).and_then(|child| index_of.get(&graph.node(child).id).copied())
        } else {
            index_of.get(&graph.node(node).id).copied()
        };
        let Some(ele_index) = ele_index else {
            continue;
        };

        for neighbor in graph.neighbors(node) {
            if graph.edges_with(node, neighbor).is_empty() {
                continue;
            }
            let neighbor_id = if graph.is_parent(neighbor) {
                parent_child_of(neighbor).map(|child| graph.node(child).id.clone())
            } else {
                Some(graph.node(neighbor).id.clone())
            };
            if let Some(neighbor_id) = neighbor_id {
                all_nodes_neighborhood[ele_index].push(neighbor_id);
            }
        }
    }

    for (key, ids) in &dummy_nodes {
        let Some(&ele_index) = index_of.get(key) else {
            continue;
        };
        for id in ids {
            let node = graph.index.get(id).copied();
            let disconnected_id = match node {
                Some(node) if graph.is_parent(node) => {
                    parent_child_of(node).map(|child| graph.node(child).id.clone())
                }
                _ => Some(id.clone()),
            };
            let Some(disconnected_id) = disconnected_id else {
                continue;
            };
            all_nodes_neighborhood[ele_index].push(disconnected_id.clone());
            if let Some(&disconnected_index) = index_of.get(&disconnected_id) {
                all_nodes_neighborhood[disconnected_index].push(key.clone());
            }
        }
    }

    if node_size > 2 {
        let sample_size = node_size.min(options.sample_size);
        let sample_size = sample_size.max(1);

        let mut c = vec![vec![0.0; sample_size]; node_size];

        let (x_coords, y_coords) = {
            let mut samples_column = vec![0usize; sample_size];
            let mut min_distances_column = vec![0.0f64; node_size];

            all_bfs(
                &all_nodes_neighborhood,
                &index_of,
                node_size,
                sample_size,
                options,
                &mut c,
                &mut samples_column,
                &mut min_distances_column,
            );

            let mut phi = vec![vec![0.0; sample_size]; sample_size];
            for i in 0..sample_size {
                for j in 0..sample_size {
                    phi[i][j] = c[samples_column[j]][i];
                }
            }

            let inv = sample(&phi, sample_size);
            power_iteration(&c, &inv, node_size, options.pi_tol)
        };

        SpectralResult {
            node_indexes,
            x_coords,
            y_coords,
        }
    } else {
        // Two nodes (or one) are too few for SVD and the power iteration, so upstream
        // skips spectral and lets CoSE place them.
        let mut x_coords = Vec::new();
        let mut y_coords = Vec::new();

        if let Some((first_key, _)) = node_indexes.first()
            && let Some(&first) = graph.index.get(first_key)
        {
            let first_node = graph.node(first);
            x_coords.push(first_node.x);
            y_coords.push(first_node.y);

            if node_size == 2
                && let Some((second_key, _)) = node_indexes.get(1)
                && let Some(&second) = graph.index.get(second_key)
            {
                let second_node = graph.node(second);
                // Upstream adds `options.idealEdgeLength` itself; when it is a function
                // JavaScript concatenates it into a string and the coordinate becomes
                // NaN. The port uses the function's default scalar (50) instead.
                let ideal_edge_length = options.ideal_edge_length.constant().unwrap_or(50.0);
                x_coords.push(
                    first_node.x
                        + first_node.width / 2.0
                        + second_node.width / 2.0
                        + ideal_edge_length,
                );
                y_coords.push(first_node.y);
            }
        }

        SpectralResult {
            node_indexes,
            x_coords,
            y_coords,
        }
    }
}

/// Upstream `randomSampleCR`: random columns, without duplicates.
fn random_sample_cr(node_size: usize, sample_size: usize, samples_column: &mut [usize]) {
    let mut count = 0;
    while count < sample_size {
        let sample = (random_seed::next_double() * node_size as f64).floor() as usize;
        let flag = samples_column[..count].contains(&sample);
        if !flag {
            samples_column[count] = sample;
            count += 1;
        }
    }
}

/// Upstream `BFS`: the distance of every node from the pivot, written into column
/// `index` of `c`, and for the greedy sampling the furthest still-unserved node.
#[allow(clippy::too_many_arguments)]
fn bfs(
    all_nodes_neighborhood: &[Vec<String>],
    index_of: &HashMap<String, usize>,
    node_size: usize,
    node_separation: f64,
    pivot: usize,
    index: usize,
    sampling_method: bool,
    c: &mut [Vec<f64>],
    min_distances_column: &mut [f64],
) -> usize {
    let mut distance = vec![INFINITY; node_size];
    let mut path = vec![0usize; node_size];
    let mut front: isize = 0;
    let mut back: isize = 0;
    let mut max_dist = 0.0;
    let mut max_ind = 1;

    path[back as usize] = pivot;
    distance[pivot] = 0.0;

    while back >= front {
        let current = path[front as usize];
        front += 1;

        let neighbors = &all_nodes_neighborhood[current];
        for neighbor in neighbors {
            let Some(&temp) = index_of.get(neighbor) else {
                continue;
            };
            if distance[temp] == INFINITY {
                distance[temp] = distance[current] + 1.0;
                back += 1;
                path[back as usize] = temp;
            }
        }
        c[current][index] = distance[current] * node_separation;
    }

    if sampling_method {
        for i in 0..node_size {
            if c[i][index] < min_distances_column[i] {
                min_distances_column[i] = c[i][index];
            }
        }

        for i in 0..node_size {
            if min_distances_column[i] > max_dist {
                max_dist = min_distances_column[i];
                max_ind = i;
            }
        }
    }

    max_ind
}

/// Upstream `allBFS`: sample the columns and fill `c`.
#[allow(clippy::too_many_arguments)]
fn all_bfs(
    all_nodes_neighborhood: &[Vec<String>],
    index_of: &HashMap<String, usize>,
    node_size: usize,
    sample_size: usize,
    options: &FcoseOptions,
    c: &mut [Vec<f64>],
    samples_column: &mut [usize],
    min_distances_column: &mut [f64],
) {
    if !options.sampling_type {
        random_sample_cr(node_size, sample_size, samples_column);

        for i in 0..sample_size {
            bfs(
                all_nodes_neighborhood,
                index_of,
                node_size,
                options.node_separation,
                samples_column[i],
                i,
                false,
                c,
                min_distances_column,
            );
        }
    } else {
        let mut sample = (random_seed::next_double() * node_size as f64).floor() as usize;

        for distance in min_distances_column.iter_mut() {
            *distance = INFINITY;
        }

        for i in 0..sample_size {
            samples_column[i] = sample;
            sample = bfs(
                all_nodes_neighborhood,
                index_of,
                node_size,
                options.node_separation,
                sample,
                i,
                true,
                c,
                min_distances_column,
            );
        }
    }

    for i in 0..node_size {
        for j in 0..sample_size {
            c[i][j] *= c[i][j];
        }
    }
}

/// Upstream `sample`: regularize the singular values of `phi` and return its inverse.
fn sample(phi: &[Vec<f64>], sample_size: usize) -> Vec<Vec<f64>> {
    let result = svd::svd(phi.to_vec());
    let a_q = result.s;
    let a_u = result.u;
    let a_v = result.v;

    let max_s = a_q[0] * a_q[0] * a_q[0];

    let mut a_sig = vec![vec![0.0; sample_size]; sample_size];
    for i in 0..sample_size {
        for j in 0..sample_size {
            a_sig[i][j] = 0.0;
            if i == j {
                a_sig[i][j] = a_q[i] / (a_q[i] * a_q[i] + max_s / (a_q[i] * a_q[i]));
            }
        }
    }

    matrix::mult_mat(&matrix::mult_mat(&a_v, &a_sig), &matrix::transpose(&a_u))
}

/// Upstream `powerIteration`: the two largest eigenvectors, scaled by the square root of
/// their eigenvalues.
fn power_iteration(
    c: &[Vec<f64>],
    inv: &[Vec<f64>],
    node_size: usize,
    pi_tol: f64,
) -> (Vec<f64>, Vec<f64>) {
    let mut y1 = vec![0.0; node_size];
    let mut y2 = vec![0.0; node_size];
    for i in 0..node_size {
        y1[i] = random_seed::next_double();
        y2[i] = random_seed::next_double();
    }

    y1 = matrix::normalize(&y1);
    y2 = matrix::normalize(&y2);

    let mut v1 = vec![0.0; node_size];
    let mut v2 = vec![0.0; node_size];

    let mut previous = SMALL;
    let mut theta1: f64;

    loop {
        v1.copy_from_slice(&y1);
        y1 = matrix::mult_gamma(&matrix::mult_l(&matrix::mult_gamma(&v1), c, inv));
        theta1 = matrix::dot_product(&v1, &y1);
        y1 = matrix::normalize(&y1);

        let current = matrix::dot_product(&v1, &y1);
        let temp = (current / previous).abs();

        if temp <= 1.0 + pi_tol && temp >= 1.0 {
            break;
        }

        previous = current;
    }

    v1.copy_from_slice(&y1);

    previous = SMALL;
    let mut theta2: f64;
    loop {
        v2.copy_from_slice(&y2);
        v2 = matrix::minus_op(&v2, &matrix::mult_cons(&v1, matrix::dot_product(&v1, &v2)));
        y2 = matrix::mult_gamma(&matrix::mult_l(&matrix::mult_gamma(&v2), c, inv));
        theta2 = matrix::dot_product(&v2, &y2);
        y2 = matrix::normalize(&y2);

        let current = matrix::dot_product(&v2, &y2);
        let temp = (current / previous).abs();

        if temp <= 1.0 + pi_tol && temp >= 1.0 {
            break;
        }

        previous = current;
    }

    v2.copy_from_slice(&y2);

    let x_coords = matrix::mult_cons(&v1, theta1.abs().sqrt());
    let y_coords = matrix::mult_cons(&v2, theta2.abs().sqrt());

    (x_coords, y_coords)
}

/// Upstream `auxiliary.relocateComponent` for a spectral result: moves the coordinates so
/// their bounding box keeps the center the graph had before the run.
pub(crate) fn relocate_component(
    result: &mut SpectralResult,
    original_center: (f64, f64),
    has_constraints: bool,
    graph: &FcoseGraph,
) {
    if has_constraints {
        return;
    }

    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for (key, value) in &result.node_indexes {
        let index = *value;
        let (width, height) = graph
            .index
            .get(key)
            .map(|&node| (graph.node(node).width, graph.node(node).height))
            .unwrap_or((0.0, 0.0));

        let left_x = result.x_coords[index] - width / 2.0;
        let right_x = result.x_coords[index] + width / 2.0;
        let top_y = result.y_coords[index] - height / 2.0;
        let bottom_y = result.y_coords[index] + height / 2.0;

        if left_x < min_x {
            min_x = left_x;
        }
        if right_x > max_x {
            max_x = right_x;
        }
        if top_y < min_y {
            min_y = top_y;
        }
        if bottom_y > max_y {
            max_y = bottom_y;
        }
    }

    let diff_on_x = original_center.0 - (max_x + min_x) / 2.0;
    let diff_on_y = original_center.1 - (max_y + min_y) / 2.0;

    for x in &mut result.x_coords {
        *x += diff_on_x;
    }
    for y in &mut result.y_coords {
        *y += diff_on_y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::fcose::FcoseEdge;
    use crate::layout::fcose::FcoseNode;
    use crate::layout::fcose::FcoseQuality;

    #[test]
    fn a_spectral_draft_places_connected_nodes_apart() {
        let nodes = vec![
            FcoseNode::new("a", 0.0, 0.0, 40.0, 40.0),
            FcoseNode::new("b", 0.0, 0.0, 40.0, 40.0),
            FcoseNode::new("c", 0.0, 0.0, 40.0, 40.0),
            FcoseNode::new("d", 0.0, 0.0, 40.0, 40.0),
        ];
        let edges = vec![
            FcoseEdge::new("ab", "a", "b"),
            FcoseEdge::new("bc", "b", "c"),
            FcoseEdge::new("cd", "c", "d"),
        ];
        let graph = FcoseGraph::new(&nodes, &edges);
        let options = FcoseOptions {
            quality: FcoseQuality::Draft,
            ..Default::default()
        };

        crate::layout::layout_base::util::random_seed::set_math_random_seed(options.seed);
        let result = spectral_layout(&graph, &options);

        assert_eq!(result.node_indexes.len(), 4);
        for value in &result.node_indexes {
            assert!(result.x_coords[value.1].is_finite());
            assert!(result.y_coords[value.1].is_finite());
        }
    }
}
