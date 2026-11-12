//! Port of `phase1.cycles.ts`: deterministic DFS cycle removal, like Graphviz dot's back-edge
//! marking.

use std::collections::{HashMap, HashSet};

use super::helpers::{Edge, EdgeRef, Graph, NodeId, locale_compare};
use super::phase0_helpers::{build_in_degree_map, normalize_graph};

#[derive(Clone, Debug, Default)]
pub struct CycleRemovalResult {
    pub acyclic: Graph,
    /// The edges that were reversed, in their original orientation.
    pub reversed: Vec<Edge>,
}

pub fn remove_cycles_dfs(g: &Graph) -> CycleRemovalResult {
    let gn = normalize_graph(g);

    let mut adj: HashMap<NodeId, Vec<EdgeRef>> = HashMap::new();
    for node in &gn.nodes {
        adj.insert(node.clone(), Vec::new());
    }
    for edge in &gn.edges {
        adj.entry(edge.src.clone()).or_default().push(edge.clone());
    }
    for edges in adj.values_mut() {
        edges.sort_by(|a, b| {
            if a.dst == b.dst {
                locale_compare(&a.id, &b.id)
            } else {
                locale_compare(&a.dst, &b.dst)
            }
        });
    }

    let mut color: HashMap<NodeId, u8> = gn.nodes.iter().map(|id| (id.clone(), 0)).collect();
    let mut reversed: Vec<EdgeRef> = Vec::new();

    fn dfs(
        u: &str,
        color: &mut HashMap<NodeId, u8>,
        adj: &HashMap<NodeId, Vec<EdgeRef>>,
        reversed: &mut Vec<EdgeRef>,
    ) {
        color.insert(u.to_string(), 1);
        if let Some(edges) = adj.get(u) {
            for edge in edges {
                match color.get(edge.dst.as_str()).copied().unwrap_or(0) {
                    0 => dfs(&edge.dst, color, adj, reversed),
                    1 => reversed.push(edge.clone()),
                    _ => {}
                }
            }
        }
        color.insert(u.to_string(), 2);
    }

    // Seed the DFS from true sources (in-degree 0) first, in declaration order, then sweep
    // any remaining unvisited nodes in declaration order. This follows dot (Gansner et al.
    // 1993, section 2.1).
    let in_degree = build_in_degree_map(&gn);
    let mut seeds: Vec<NodeId> = gn
        .nodes
        .iter()
        .filter(|node| in_degree.get(*node).copied().unwrap_or(0) == 0)
        .cloned()
        .collect();
    seeds.extend(
        gn.nodes
            .iter()
            .filter(|node| in_degree.get(*node).copied().unwrap_or(0) != 0)
            .cloned(),
    );
    for node in seeds {
        if color.get(&node).copied().unwrap_or(0) == 0 {
            dfs(&node, &mut color, &adj, &mut reversed);
        }
    }

    let to_reverse: HashSet<String> = reversed
        .iter()
        .map(|edge| format!("{}:{}->{}", edge.id, edge.src, edge.dst))
        .collect();
    let acyclic_edges: Vec<EdgeRef> = gn
        .edges
        .iter()
        .map(|edge| {
            if to_reverse.contains(&format!("{}:{}->{}", edge.id, edge.src, edge.dst)) {
                EdgeRef {
                    id: edge.id.clone(),
                    src: edge.dst.clone(),
                    dst: edge.src.clone(),
                    weight: edge.weight,
                    reference: edge.reference.clone(),
                }
            } else {
                edge.clone()
            }
        })
        .collect();

    CycleRemovalResult {
        acyclic: Graph {
            nodes: gn.nodes.clone(),
            edges: acyclic_edges,
            layout: gn.layout.clone(),
            node_by_id: gn.node_by_id.clone(),
        },
        reversed,
    }
}
