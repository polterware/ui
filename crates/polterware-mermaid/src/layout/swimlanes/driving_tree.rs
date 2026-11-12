//! Port of `driving-tree.ts`: the spanning forest and traversal metadata tree ordering uses.

use std::collections::{HashMap, HashSet};

use super::helpers::{Graph, NodeId, locale_compare};
use super::phase0_helpers::{
    build_predecessor_successor_maps, normalize_graph, topo_sort_if_acyclic,
};
use super::phase2_options::TopLaneResolver;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DrivingTreeBlock {
    pub id: usize,
    pub nodes: Vec<NodeId>,
    pub edges: Vec<(NodeId, NodeId)>,
}

#[derive(Clone, Debug, Default)]
pub struct DrivingTree {
    pub parent: HashMap<NodeId, Option<NodeId>>,
    pub children: HashMap<NodeId, Vec<NodeId>>,
    pub roots: Vec<NodeId>,
    pub component_of: HashMap<NodeId, usize>,
    pub blocks: Vec<DrivingTreeBlock>,
    pub node_blocks: HashMap<NodeId, Vec<usize>>,
    pub adjacency: HashMap<NodeId, Vec<NodeId>>,
    pub preorder: Vec<NodeId>,
    pub postorder: Vec<NodeId>,
    pub topological_order: Vec<NodeId>,
}

#[derive(Clone, Debug, Default)]
pub struct DrivingTreeBuildOptions {
    pub rank_hint: Option<HashMap<NodeId, i64>>,
    pub lane_resolver: Option<TopLaneResolver>,
}

struct ParentSelectionContext<'a> {
    lane_of: &'a TopLaneResolver,
    rank_hint: Option<&'a HashMap<NodeId, i64>>,
    topo_index: &'a HashMap<NodeId, usize>,
}

fn choose_parent(node: &str, candidates: &[NodeId], ctx: ParentSelectionContext<'_>) -> NodeId {
    let lane_node = ctx.lane_of.lane_of(node);
    let mut sorted = candidates.to_vec();
    sorted.sort_by(|a, b| {
        let lane_a = ctx.lane_of.lane_of(a);
        let lane_b = ctx.lane_of.lane_of(b);
        let same_lane_a = lane_a.is_some() && lane_a == lane_node;
        let same_lane_b = lane_b.is_some() && lane_b == lane_node;
        if same_lane_a != same_lane_b {
            return if same_lane_a {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }

        if let Some(rank_hint) = ctx.rank_hint
            && let (Some(rank_a), Some(rank_b)) = (rank_hint.get(a), rank_hint.get(b))
            && rank_a != rank_b
        {
            return rank_b.cmp(rank_a);
        }

        let index_a = ctx.topo_index.get(a).copied().unwrap_or(0);
        let index_b = ctx.topo_index.get(b).copied().unwrap_or(0);
        if index_a != index_b {
            return index_a.cmp(&index_b);
        }

        locale_compare(a, b)
    });
    sorted[0].clone()
}

fn build_adjacency(g: &Graph) -> HashMap<NodeId, HashSet<NodeId>> {
    let mut adjacency: HashMap<NodeId, HashSet<NodeId>> = HashMap::new();
    for node in &g.nodes {
        adjacency.insert(node.clone(), HashSet::new());
    }
    for edge in &g.edges {
        adjacency
            .entry(edge.src.clone())
            .or_default()
            .insert(edge.dst.clone());
        adjacency
            .entry(edge.dst.clone())
            .or_default()
            .insert(edge.src.clone());
    }
    adjacency
}

fn assign_components(
    adjacency: &HashMap<NodeId, Vec<NodeId>>,
    node_order: &[NodeId],
) -> HashMap<NodeId, usize> {
    let mut component_of: HashMap<NodeId, usize> = HashMap::new();
    let mut component_id = 0;
    for node in node_order {
        if component_of.contains_key(node) {
            continue;
        }
        let mut stack = vec![node.clone()];
        while let Some(current) = stack.pop() {
            if component_of.contains_key(&current) {
                continue;
            }
            component_of.insert(current.clone(), component_id);
            if let Some(neighbors) = adjacency.get(&current) {
                for next in neighbors {
                    if !component_of.contains_key(next) {
                        stack.push(next.clone());
                    }
                }
            }
        }
        component_id += 1;
    }
    component_of
}

fn pop_block(u: &str, v: &str, stack: &mut Vec<(NodeId, NodeId)>, id: usize) -> DrivingTreeBlock {
    let mut edges = Vec::new();
    let mut nodes = Vec::new();
    let mut seen = HashSet::new();
    while let Some(edge) = stack.pop() {
        if seen.insert(edge.0.clone()) {
            nodes.push(edge.0.clone());
        }
        if seen.insert(edge.1.clone()) {
            nodes.push(edge.1.clone());
        }
        let closes = (edge.0 == u && edge.1 == v) || (edge.0 == v && edge.1 == u);
        edges.push(edge);
        if closes {
            break;
        }
    }
    DrivingTreeBlock { id, nodes, edges }
}

fn compute_blocks(
    adjacency: &HashMap<NodeId, Vec<NodeId>>,
    node_order: &[NodeId],
) -> Vec<DrivingTreeBlock> {
    let mut discovery: HashMap<NodeId, usize> = HashMap::new();
    let mut low: HashMap<NodeId, usize> = HashMap::new();
    let mut edge_stack: Vec<(NodeId, NodeId)> = Vec::new();
    let mut blocks: Vec<DrivingTreeBlock> = Vec::new();
    let mut time = 0;

    #[allow(clippy::too_many_arguments)]
    fn visit(
        node: &str,
        parent: Option<&str>,
        adjacency: &HashMap<NodeId, Vec<NodeId>>,
        discovery: &mut HashMap<NodeId, usize>,
        low: &mut HashMap<NodeId, usize>,
        edge_stack: &mut Vec<(NodeId, NodeId)>,
        blocks: &mut Vec<DrivingTreeBlock>,
        time: &mut usize,
    ) {
        *time += 1;
        discovery.insert(node.to_string(), *time);
        low.insert(node.to_string(), *time);

        if let Some(neighbors) = adjacency.get(node) {
            for next in neighbors {
                if Some(next.as_str()) == parent {
                    continue;
                }
                if !discovery.contains_key(next) {
                    edge_stack.push((node.to_string(), next.clone()));
                    visit(
                        next,
                        Some(node),
                        adjacency,
                        discovery,
                        low,
                        edge_stack,
                        blocks,
                        time,
                    );
                    let low_node = low.get(node).copied().unwrap_or(*time);
                    let low_next = low.get(next).copied().unwrap_or(*time);
                    low.insert(node.to_string(), low_node.min(low_next));
                    let discovered_node = discovery.get(node).copied().unwrap_or(0);
                    if low.get(next).copied().unwrap_or(0) >= discovered_node {
                        let id = blocks.len();
                        blocks.push(pop_block(node, next, edge_stack, id));
                    }
                } else if discovery.get(next).copied().unwrap_or(0)
                    < discovery.get(node).copied().unwrap_or(0)
                {
                    edge_stack.push((node.to_string(), next.clone()));
                    let low_node = low.get(node).copied().unwrap_or(*time);
                    let discovered_next = discovery.get(next).copied().unwrap_or(*time);
                    low.insert(node.to_string(), low_node.min(discovered_next));
                }
            }
        }
    }

    for node in node_order {
        if !discovery.contains_key(node) {
            visit(
                node,
                None,
                adjacency,
                &mut discovery,
                &mut low,
                &mut edge_stack,
                &mut blocks,
                &mut time,
            );
        }
    }

    blocks
}

/// Builds the spanning forest and traversal metadata used by tree ordering.
pub fn build_driving_tree(graph: &Graph, opts: DrivingTreeBuildOptions) -> DrivingTree {
    let g = normalize_graph(graph);
    let lane_of = opts.lane_resolver.unwrap_or_default();
    let rank_hint = opts.rank_hint;

    let (mut preds, _) = build_predecessor_successor_maps(&g, |_| true);
    for list in preds.values_mut() {
        list.sort_by(|a, b| locale_compare(a, b));
    }

    let mut topo_order = topo_sort_if_acyclic(&g).unwrap_or_else(|| {
        let mut nodes = g.nodes.clone();
        nodes.sort_by(|a, b| locale_compare(a, b));
        nodes
    });
    let mut topo_index: HashMap<NodeId, usize> = HashMap::new();
    for (index, id) in topo_order.iter().enumerate() {
        topo_index.insert(id.clone(), index);
    }

    let mut parent: HashMap<NodeId, Option<NodeId>> = HashMap::new();
    let mut children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for node in &g.nodes {
        children.insert(node.clone(), Vec::new());
    }

    for node in &topo_order {
        let candidates: Vec<NodeId> = preds
            .get(node)
            .map(|list| {
                list.iter()
                    .filter(|candidate| parent.contains_key(*candidate))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if !candidates.is_empty() {
            let chosen = choose_parent(
                node,
                &candidates,
                ParentSelectionContext {
                    lane_of: &lane_of,
                    rank_hint: rank_hint.as_ref(),
                    topo_index: &topo_index,
                },
            );
            parent.insert(node.clone(), Some(chosen.clone()));
            children.entry(chosen).or_default().push(node.clone());
        } else if !parent.contains_key(node) {
            parent.insert(node.clone(), None);
        }
    }

    for node in &g.nodes {
        parent.entry(node.clone()).or_insert(None);
    }

    let mut roots: Vec<NodeId> = g
        .nodes
        .iter()
        .filter(|node| parent.get(*node).cloned().flatten().is_none())
        .cloned()
        .collect();
    roots.sort_by(|a, b| {
        let ta = topo_index.get(a).copied().unwrap_or(0);
        let tb = topo_index.get(b).copied().unwrap_or(0);
        if ta == tb {
            locale_compare(a, b)
        } else {
            ta.cmp(&tb)
        }
    });

    let adjacency_sets = build_adjacency(&g);
    let mut adjacency_list: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for (node, set) in &adjacency_sets {
        let mut neighbors: Vec<NodeId> = set.iter().cloned().collect();
        neighbors.sort_by(|a, b| locale_compare(a, b));
        adjacency_list.insert(node.clone(), neighbors);
    }

    let component_of = assign_components(&adjacency_list, &g.nodes);
    let blocks = compute_blocks(&adjacency_list, &g.nodes);
    let mut node_blocks: HashMap<NodeId, Vec<usize>> = HashMap::new();
    for node in &g.nodes {
        node_blocks.insert(node.clone(), Vec::new());
    }
    for block in &blocks {
        for node in &block.nodes {
            node_blocks.entry(node.clone()).or_default().push(block.id);
        }
    }

    let mut preorder: Vec<NodeId> = Vec::new();
    let mut postorder: Vec<NodeId> = Vec::new();
    let mut seen: HashSet<NodeId> = HashSet::new();

    fn walk(
        node: &str,
        children: &HashMap<NodeId, Vec<NodeId>>,
        seen: &mut HashSet<NodeId>,
        preorder: &mut Vec<NodeId>,
        postorder: &mut Vec<NodeId>,
    ) {
        if !seen.insert(node.to_string()) {
            return;
        }
        preorder.push(node.to_string());
        if let Some(child_list) = children.get(node) {
            for child in child_list {
                walk(child, children, seen, preorder, postorder);
            }
        }
        postorder.push(node.to_string());
    }

    for root in &roots {
        walk(root, &children, &mut seen, &mut preorder, &mut postorder);
    }
    for node in &topo_order {
        walk(node, &children, &mut seen, &mut preorder, &mut postorder);
    }

    DrivingTree {
        parent,
        children,
        roots,
        component_of,
        blocks,
        node_blocks,
        adjacency: adjacency_list,
        preorder,
        postorder,
        topological_order: std::mem::take(&mut topo_order),
    }
}
