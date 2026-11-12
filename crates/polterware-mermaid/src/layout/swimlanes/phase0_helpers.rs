//! Port of `phase0.helpers.ts`: graph normalization and the small graph queries the phases
//! share.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use super::helpers::{EdgeRef, Graph, NodeId, locale_compare, node_is_group};

/// Normalizes a graph view: drops edges with unknown endpoints and de-duplicates by
/// id + endpoints, and keys its nodes in `node_by_id` order.
pub fn normalize_graph(g: &Graph) -> Graph {
    let node_by_id = g.node_by_id.clone();
    let mut seen: HashSet<String> = HashSet::new();
    let mut edges: Vec<EdgeRef> = Vec::new();
    for edge in &g.edges {
        if !node_by_id.contains_key(&edge.src) || !node_by_id.contains_key(&edge.dst) {
            continue;
        }
        let key = format!("{}:{}->{}", edge.id, edge.src, edge.dst);
        if !seen.insert(key) {
            continue;
        }
        edges.push(edge.clone());
    }
    let nodes: Vec<NodeId> = g.nodes.clone();
    Graph {
        nodes,
        edges,
        layout: g.layout.clone(),
        node_by_id,
    }
}

pub fn incoming<'a>(g: &'a Graph, v: &str) -> Vec<&'a EdgeRef> {
    g.edges.iter().filter(|edge| edge.dst == v).collect()
}

pub fn outgoing<'a>(g: &'a Graph, v: &str) -> Vec<&'a EdgeRef> {
    g.edges.iter().filter(|edge| edge.src == v).collect()
}

pub fn build_successor_map(g: &Graph) -> HashMap<NodeId, Vec<NodeId>> {
    let mut successors: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for node in &g.nodes {
        successors.insert(node.clone(), Vec::new());
    }
    for edge in &g.edges {
        successors
            .entry(edge.src.clone())
            .or_default()
            .push(edge.dst.clone());
    }
    successors
}

pub fn build_sorted_successor_map(g: &Graph) -> HashMap<NodeId, Vec<NodeId>> {
    let mut successors = build_successor_map(g);
    for list in successors.values_mut() {
        list.sort_by(|a, b| locale_compare(a, b));
    }
    successors
}

pub fn build_in_degree_map(g: &Graph) -> HashMap<NodeId, usize> {
    let mut in_degree: HashMap<NodeId, usize> = HashMap::new();
    for node in &g.nodes {
        in_degree.insert(node.clone(), 0);
    }
    for edge in &g.edges {
        *in_degree.entry(edge.dst.clone()).or_insert(0) += 1;
    }
    in_degree
}

pub fn sorted_zero_in_degree_nodes(g: &Graph, in_degree: &HashMap<NodeId, usize>) -> Vec<NodeId> {
    let mut sorted: Vec<NodeId> = in_degree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| id.clone())
        .collect();
    sorted.sort_by(|a, b| locale_compare(a, b));

    // Nodes without any edge are unconstrained, so sorting them by id is arbitrary and
    // reorders a lane of unconnected nodes alphabetically (#8355). Their slots are refilled
    // in source order and every other node stays in place.
    let linked: HashSet<&str> = g
        .edges
        .iter()
        .flat_map(|edge| [edge.src.as_str(), edge.dst.as_str()])
        .collect();
    let is_unconnected =
        |id: &str| !linked.contains(id) && !g.node_by_id.get(id).is_some_and(node_is_group);
    let unconnected: Vec<NodeId> = g
        .nodes
        .iter()
        .filter(|id| in_degree.get(*id).copied().unwrap_or(0) == 0 && is_unconnected(id))
        .cloned()
        .collect();
    let mut next = 0;
    sorted
        .into_iter()
        .map(|id| {
            if is_unconnected(&id) {
                let replacement = unconnected[next].clone();
                next += 1;
                replacement
            } else {
                id
            }
        })
        .collect()
}

pub fn build_predecessor_successor_maps(
    g: &Graph,
    include_edge: impl Fn(&EdgeRef) -> bool,
) -> (HashMap<NodeId, Vec<NodeId>>, HashMap<NodeId, Vec<NodeId>>) {
    let mut preds: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    let mut succs: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for node in &g.nodes {
        preds.insert(node.clone(), Vec::new());
        succs.insert(node.clone(), Vec::new());
    }
    for edge in &g.edges {
        if !include_edge(edge) {
            continue;
        }
        succs
            .entry(edge.src.clone())
            .or_default()
            .push(edge.dst.clone());
        preds
            .entry(edge.dst.clone())
            .or_default()
            .push(edge.src.clone());
    }
    (preds, succs)
}

pub fn build_layers_from_ranks(
    g: &Graph,
    order: &[NodeId],
    rank_of: &HashMap<NodeId, i64>,
    skip_groups: bool,
) -> Vec<Vec<NodeId>> {
    let mut max_rank: i64 = 0;
    for node in &g.nodes {
        if skip_groups && g.node_by_id.get(node).is_some_and(node_is_group) {
            continue;
        }
        max_rank = max_rank.max(rank_of.get(node).copied().unwrap_or(0));
    }

    let mut layers: Vec<Vec<NodeId>> = vec![Vec::new(); max_rank.max(0) as usize + 1];
    for node in order {
        if skip_groups && g.node_by_id.get(node).is_some_and(node_is_group) {
            continue;
        }
        let rank = rank_of.get(node).copied().unwrap_or(0).max(0) as usize;
        layers[rank].push(node.clone());
    }
    layers
}

/// Detects acyclicity via DFS (white/gray/black sets).
pub fn is_acyclic(g: &Graph) -> bool {
    let mut color: HashMap<String, u8> = g.nodes.iter().map(|id| (id.clone(), 0)).collect();
    let adj = build_successor_map(g);

    fn dfs(u: &str, color: &mut HashMap<String, u8>, adj: &HashMap<NodeId, Vec<NodeId>>) -> bool {
        color.insert(u.to_string(), 1);
        if let Some(neighbors) = adj.get(u) {
            for v in neighbors {
                match color.get(v.as_str()).copied().unwrap_or(0) {
                    0 => {
                        if !dfs(v, color, adj) {
                            return false;
                        }
                    }
                    1 => return false,
                    _ => {}
                }
            }
        }
        color.insert(u.to_string(), 2);
        true
    }

    for node in &g.nodes {
        if color.get(node.as_str()).copied().unwrap_or(0) == 0 && !dfs(node, &mut color, &adj) {
            return false;
        }
    }
    true
}

/// Topological sort (Kahn). Returns `None` if cycles exist.
pub fn topo_sort_if_acyclic(g: &Graph) -> Option<Vec<NodeId>> {
    let mut in_degree = build_in_degree_map(g);
    let mut queue = sorted_zero_in_degree_nodes(g, &in_degree);
    let mut order: Vec<NodeId> = Vec::new();
    let adj = build_sorted_successor_map(g);

    while !queue.is_empty() {
        let u = queue.remove(0);
        order.push(u.clone());
        if let Some(neighbors) = adj.get(&u) {
            for v in neighbors {
                let degree = in_degree.entry(v.clone()).or_insert(0);
                *degree = degree.saturating_sub(1);
                if *degree == 0 {
                    // Insert keeping sorted order for determinism.
                    let mut i = 0;
                    while i < queue.len() && queue[i] < *v {
                        i += 1;
                    }
                    queue.insert(i, v.clone());
                }
            }
        }
    }

    (order.len() == g.nodes.len()).then_some(order)
}

/// An index map from node ids to their positions in a layer.
pub fn build_layer_index(layer: &[NodeId]) -> HashMap<NodeId, usize> {
    layer
        .iter()
        .enumerate()
        .map(|(index, id)| (id.clone(), index))
        .collect()
}

pub fn count_inversions(values: &mut [f64]) -> usize {
    fn count(values: &mut [f64], tmp: &mut [f64], left: usize, right: usize) -> usize {
        if right - left <= 1 {
            return 0;
        }
        let mid = (left + right) >> 1;
        let mut inversions = count(values, tmp, left, mid) + count(values, tmp, mid, right);
        let mut i = left;
        let mut j = mid;
        let mut k = left;
        while i < mid || j < right {
            if j >= right || (i < mid && values[i] <= values[j]) {
                tmp[k] = values[i];
                i += 1;
            } else {
                tmp[k] = values[j];
                j += 1;
                inversions += mid - i;
            }
            k += 1;
        }
        values[left..right].copy_from_slice(&tmp[left..right]);
        inversions
    }

    if values.is_empty() {
        return 0;
    }
    let mut tmp = vec![0.0; values.len()];
    count(values, &mut tmp, 0, values.len())
}

/// `Math.min` over an iterator; `f64::INFINITY` for an empty one, as upstream's `Math.min(...[])`.
pub fn min_of(values: impl Iterator<Item = f64>) -> f64 {
    let mut min = f64::INFINITY;
    for value in values {
        min = min.min(value);
    }
    min
}

/// A stable sort by rank then identifier, used where upstream calls `Array.prototype.sort`.
pub fn sort_by_rank_then_id(nodes: &mut [NodeId], rank_of: &HashMap<NodeId, i64>) {
    nodes.sort_by(|a, b| {
        let ra = rank_of.get(a).copied().unwrap_or(0);
        let rb = rank_of.get(b).copied().unwrap_or(0);
        if ra == rb {
            locale_compare(a, b)
        } else {
            ra.cmp(&rb)
        }
    });
}

/// `Number.isFinite` on an optional measure.
pub fn finite(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite())
}

/// Orders two identifiers by code unit, upstream's default `Array.prototype.sort` on strings.
pub fn code_unit_compare(a: &str, b: &str) -> Ordering {
    a.cmp(b)
}
