//! Port of `dagre/lib/layout.ts` (MIT, Copyright (c) 2012-2014 Chris Pettitt), the
//! fork Mermaid uses: the full pipeline, cluster layouts with their own `rankdir`,
//! self edges, and the optional memory of a previous layout.
//!
//! [`build_layout_graph`] copies the whitelisted attributes that can influence
//! layout, [`run_layout`] runs the pipeline in upstream's order, and
//! [`recursive_cluster_layout`] runs it for the whole graph and for every cluster
//! that carries its own `rankdir`. The attributes read and written are the graph's
//! `rankdir`, `align`, `rankalign`, `nodesep`, `edgesep`, `ranksep`, `marginx`,
//! `marginy`, `acyclicer`, `ranker`, `width`, `height`; the nodes' `width`,
//! `height`, `rank`, `x`, `y`, and `order`; and the edges' `minlen`, `weight`,
//! `width`, `height`, `labelpos`, `labeloffset`, `labelRank`, `points`, `x`, and
//! `y`.
//!
//! The previous-layout cache upstream keeps in a `WeakMap` keyed by the input graph
//! cannot exist for a graph owned by Rust, so [`layout_with_history`] hands the new
//! [`PreviousLayout`] back to the caller instead. Mermaid lays out a fresh graph on
//! every render, so [`layout`] never sees a previous layout, as upstream does not.

use std::collections::{HashMap, HashSet, VecDeque};

use super::super::graphlib::{Edge, GraphOptions};
use super::{
    EdgeLabel, GraphLabel, LayeredGraph, LayoutError, NodeLabel, Point, SelfEdge, acyclic,
    add_border_segments, coordinate_system, nesting_graph, normalize, order, parent_dummy_chains,
    position, rank, util,
};

/// Options for [`layout`], upstream's `LayoutOptions` minus the JavaScript
/// callback and graph-label fields (those live on the graph label).
#[derive(Clone, Debug, Default)]
pub struct LayoutOptions {
    /// Whether to use the previous layout when one is supplied. Defaults to `true`.
    pub use_dynamic: Option<bool>,
    /// Ordering options: the optimal-order switch and constraints.
    pub order: order::OrderOptions,
    /// Node ids whose edges should be aligned, upstream's `corePath`.
    pub core_path: Option<Vec<String>>,
}

/// What a layout remembers for the next one, upstream's `PreviousLayout`.
#[derive(Clone, Debug, Default)]
pub struct PreviousLayout {
    /// The layout graph's node labels after positioning, before border removal.
    pub raw_nodes: Vec<(String, NodeLabel)>,
}

impl PreviousLayout {
    fn ranks(&self) -> HashMap<String, Option<f64>> {
        self.raw_nodes
            .iter()
            .map(|(v, node)| (v.clone(), node.rank))
            .collect()
    }
}

/// Lays out `graph` in place.
pub fn layout(graph: &mut LayeredGraph) -> Result<(), LayoutError> {
    layout_with_options(graph, &LayoutOptions::default())
}

/// Lays out `graph` in place with `opts`.
pub fn layout_with_options(
    graph: &mut LayeredGraph,
    opts: &LayoutOptions,
) -> Result<(), LayoutError> {
    recursive_cluster_layout(graph, opts, None).map(|_| ())
}

/// Lays out `graph` in place and returns what the next layout of the same graph may
/// use, replacing upstream's per-graph `WeakMap` entry.
pub fn layout_with_history(
    graph: &mut LayeredGraph,
    opts: &LayoutOptions,
    previous: Option<&PreviousLayout>,
) -> Result<PreviousLayout, LayoutError> {
    recursive_cluster_layout(graph, opts, previous)
}

/// Lays out every cluster that has a `rankdir` of its own, then lays out the graph.
fn recursive_cluster_layout(
    g: &mut LayeredGraph,
    opts: &LayoutOptions,
    previous: Option<&PreviousLayout>,
) -> Result<PreviousLayout, LayoutError> {
    let cluster_nodes: Vec<String> = g
        .nodes()
        .into_iter()
        .filter(|v| !g.children(v).is_empty())
        .collect();
    let mut cluster_bounds: HashMap<String, ClusterBounds> = HashMap::new();
    let mut cluster_subgraphs: HashMap<String, LayeredGraph> = HashMap::new();

    // First, recursively layout all clusters with their own rankdir.
    for v in &cluster_nodes {
        let Some(node) = g.node(v).cloned() else {
            continue;
        };
        let Some(rankdir) = node.rankdir.clone() else {
            continue;
        };
        let mut subgraph: LayeredGraph = LayeredGraph::with_options(GraphOptions {
            multigraph: true,
            compound: true,
            ..GraphOptions::default()
        });
        subgraph.set_graph(GraphLabel {
            rankdir: Some(rankdir),
            ..GraphLabel::default()
        });
        let children = g.children(v);
        for child_id in &children {
            let child_node = g.node(child_id).cloned().unwrap_or_default();
            subgraph.set_node_with(child_id.clone(), child_node);
            if let Some(parent) = g.parent(child_id)
                && parent != v
                && children.iter().any(|child| child == parent)
            {
                subgraph
                    .set_parent(child_id.as_str(), Some(parent))
                    .expect("the cluster subgraph is compound");
            }
        }
        // Copy edges between the cluster's direct children; an edge that starts or
        // ends inside a nested cluster becomes a proxy edge from that cluster node.
        let mut proxied_edges: Vec<(String, String)> = Vec::new();
        for edge in g.edges() {
            let source_child = top_level_child_of(g, &edge.v, v);
            let target_child = top_level_child_of(g, &edge.w, v);
            match (source_child, target_child) {
                (Some(source), Some(target)) if source == edge.v && target == edge.w => {
                    subgraph
                        .set_edge_object_label(&edge, g.edge_object(&edge).cloned())
                        .expect("the subgraph is a multigraph");
                }
                (Some(source), Some(target)) if source != target => {
                    let key = (source.clone(), target.clone());
                    if !proxied_edges.contains(&key) {
                        proxied_edges.push(key);
                        subgraph
                            .set_edge_object_label(
                                &Edge::new(source.as_str(), target.as_str()),
                                g.edge_object(&edge).cloned(),
                            )
                            .expect("the subgraph is a multigraph");
                    }
                }
                _ => {}
            }
        }

        recursive_cluster_layout(&mut subgraph, opts, None)?;

        let mut sub_layout_graph = build_layout_graph(&subgraph);
        run_layout(&mut sub_layout_graph, opts, None)?;
        update_input_graph(&mut subgraph, &sub_layout_graph);

        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for u in subgraph.nodes() {
            if u == *v {
                continue;
            }
            if let Some(node) = subgraph.node(&u)
                && let (Some(x), Some(y)) = (node.x, node.y)
            {
                min_x = min_x.min(x - node.width / 2.0);
                max_x = max_x.max(x + node.width / 2.0);
                min_y = min_y.min(y - node.height / 2.0);
                max_y = max_y.max(y + node.height / 2.0);
            }
        }
        if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
            min_x = 0.0;
            min_y = 0.0;
            max_x = 0.0;
            max_y = 0.0;
        }
        cluster_bounds.insert(
            v.clone(),
            ClusterBounds {
                min_x,
                min_y,
                max_x,
                max_y,
                width: max_x - min_x,
                height: max_y - min_y,
            },
        );
        cluster_subgraphs.insert(v.clone(), subgraph);
    }

    // Strict isolation: lay out each top-level cluster's children in a scratch graph,
    // then place them as a group in the parent layout.
    let isolated_clusters: Vec<IsolatedCluster>;

    // Step 1: collect top-level clusters with all their descendants.
    let all_descendants = |node_id: &str| -> Vec<String> {
        let mut result = Vec::new();
        let mut queue: VecDeque<String> = g
            .children(node_id)
            .into_iter()
            .filter(|child| child != node_id)
            .collect();
        while let Some(current) = queue.pop_front() {
            result.push(current.clone());
            for child in g.children(&current) {
                if child != current {
                    queue.push_back(child);
                }
            }
        }
        result
    };
    let mut raw_cluster_map: Vec<(String, Vec<String>)> = Vec::new();
    for v in &cluster_nodes {
        let cluster = g.node(v).cloned().unwrap_or_default();
        if cluster.rankdir.is_some() && cluster_bounds.contains_key(v) {
            let children = g
                .children(v)
                .into_iter()
                .filter(|child| child != v)
                .collect();
            raw_cluster_map.push((v.clone(), children));
        }
    }
    let all_direct_children: HashSet<String> = raw_cluster_map
        .iter()
        .flat_map(|(_, children)| children.iter().cloned())
        .collect();
    let mut cluster_children_map: Vec<(String, Vec<String>)> = Vec::new();
    for (v, _) in &raw_cluster_map {
        if !all_direct_children.contains(v) {
            cluster_children_map.push((v.clone(), all_descendants(v)));
        }
    }
    let all_isolated_children: HashSet<String> = cluster_children_map
        .iter()
        .flat_map(|(_, children)| children.iter().cloned())
        .collect();
    let top_level_node = |id: &str| -> String {
        for (cluster_id, children) in &cluster_children_map {
            if children.iter().any(|child| child == id) {
                return cluster_id.clone();
            }
        }
        id.to_string()
    };

    // Step 2: save edges incident to any isolated child before removing nodes.
    let mut saved_edges: Vec<(Edge, EdgeLabel)> = Vec::new();
    for edge in g.edges() {
        if all_isolated_children.contains(&edge.v) || all_isolated_children.contains(&edge.w) {
            saved_edges.push((
                edge.clone(),
                g.edge_object(&edge).cloned().unwrap_or_default(),
            ));
        }
    }

    // Step 3: remove cluster children and record per-cluster metadata.
    let mut saved_parents: HashMap<String, Option<String>> = HashMap::new();
    for id in &all_isolated_children {
        saved_parents.insert(id.clone(), g.parent(id).map(str::to_string));
    }
    let mut isolated: Vec<IsolatedCluster> = Vec::new();
    for (v, children) in &cluster_children_map {
        let node = g.node(v).cloned();
        let mut removed_nodes: Vec<(String, NodeLabel, Option<String>)> = Vec::new();
        for child_id in children {
            let Some(child_node) = g.node(child_id).cloned() else {
                continue;
            };
            removed_nodes.push((
                child_id.clone(),
                child_node,
                saved_parents.get(child_id).cloned().flatten(),
            ));
            g.remove_node(child_id);
        }
        let removed_edges: Vec<(Edge, EdgeLabel)> = saved_edges
            .iter()
            .filter(|(edge, _)| {
                children.iter().any(|child| child == &edge.v)
                    || children.iter().any(|child| child == &edge.w)
            })
            .cloned()
            .collect();
        let Some(bounds) = cluster_bounds.get(v).copied() else {
            continue;
        };
        if node.is_none() {
            continue;
        }
        let Some(subgraph) = cluster_subgraphs.remove(v) else {
            continue;
        };
        if let Some(label) = g.node_mut(v) {
            label.width = bounds.width;
            label.height = bounds.height;
        }
        isolated.push(IsolatedCluster {
            cluster_id: v.clone(),
            subgraph,
            bounds,
            removed_nodes,
            removed_edges,
        });
    }
    isolated_clusters = isolated;

    // Step 4: add proxy edges between cluster nodes so the parent layout respects
    // their order.
    let mut proxy_edges: Vec<(String, String)> = Vec::new();
    for (edge, label) in &saved_edges {
        let effective_v = top_level_node(&edge.v);
        let effective_w = top_level_node(&edge.w);
        if effective_v != effective_w && g.has_node(&effective_v) && g.has_node(&effective_w) {
            let key = (effective_v.clone(), effective_w.clone());
            if !proxy_edges.contains(&key) {
                proxy_edges.push(key);
                g.set_edge_with(
                    &effective_v,
                    &effective_w,
                    EdgeLabel {
                        width: Some(0.0),
                        height: Some(0.0),
                        ..label.clone()
                    },
                );
            }
        }
    }

    // Step 5: run the main layout.
    let mut layout_graph = build_layout_graph(g);
    let result = run_layout(&mut layout_graph, opts, previous)?;
    update_input_graph(g, &layout_graph);

    // Step 6: remove proxy edges, restore cluster internals, position children.
    for (effective_v, effective_w) in proxy_edges {
        if g.has_edge(&effective_v, &effective_w) {
            g.remove_edge(&effective_v, &effective_w);
        }
    }

    for cluster in &isolated_clusters {
        let cluster_label = g.node(&cluster.cluster_id).cloned().unwrap_or_default();
        let cluster_x = cluster_label.x.unwrap_or(0.0);
        let cluster_y = cluster_label.y.unwrap_or(0.0);
        let subgraph_center_x = (cluster.bounds.min_x + cluster.bounds.max_x) / 2.0;
        let subgraph_center_y = (cluster.bounds.min_y + cluster.bounds.max_y) / 2.0;

        for (id, node, parent) in &cluster.removed_nodes {
            g.set_node_with(id.clone(), node.clone());
            if let Some(parent) = parent {
                g.set_parent(id.as_str(), Some(parent.as_str()))
                    .expect("the layout graph is compound");
            }
        }
        for (edge, label) in &cluster.removed_edges {
            g.set_edge_object_label(edge, Some(label.clone()))
                .expect("restored edges stay inside the multigraph");
        }

        for u in cluster.subgraph.nodes() {
            if u == cluster.cluster_id {
                continue;
            }
            let Some(sub_node) = cluster.subgraph.node(&u) else {
                continue;
            };
            let (Some(sub_x), Some(sub_y)) = (sub_node.x, sub_node.y) else {
                continue;
            };
            if let Some(main_node) = g.node_mut(&u) {
                main_node.x = Some(cluster_x + (sub_x - subgraph_center_x));
                main_node.y = Some(cluster_y + (sub_y - subgraph_center_y));
            }
        }
        update_cluster_edges(
            g,
            &cluster.subgraph,
            cluster_x - subgraph_center_x,
            cluster_y - subgraph_center_y,
        );
    }
    // After the parent layout, place the internals of nested clusters that were
    // restored with their parent.
    for v in &cluster_nodes {
        let Some(node) = g.node(v).cloned() else {
            continue;
        };
        let Some(bounds) = cluster_bounds.get(v).copied() else {
            continue;
        };
        if node.rankdir.is_none() {
            continue;
        }
        let Some(subgraph) = cluster_subgraphs.get(v) else {
            continue;
        };
        let parent_x = node.x.unwrap_or(0.0);
        let parent_y = node.y.unwrap_or(0.0);
        let subgraph_center_x = (bounds.min_x + bounds.max_x) / 2.0;
        let subgraph_center_y = (bounds.min_y + bounds.max_y) / 2.0;

        for u in subgraph.nodes() {
            if u == *v {
                continue;
            }
            let Some(sub_node) = subgraph.node(&u) else {
                continue;
            };
            let (Some(sub_x), Some(sub_y)) = (sub_node.x, sub_node.y) else {
                continue;
            };
            if let Some(main_node) = g.node_mut(&u) {
                main_node.x = Some(parent_x + (sub_x - subgraph_center_x));
                main_node.y = Some(parent_y + (sub_y - subgraph_center_y));
            }
        }
        update_cluster_edges(
            g,
            subgraph,
            parent_x - subgraph_center_x,
            parent_y - subgraph_center_y,
        );
    }

    Ok(result)
}

#[derive(Clone, Copy, Debug)]
struct ClusterBounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    width: f64,
    height: f64,
}

struct IsolatedCluster {
    cluster_id: String,
    subgraph: LayeredGraph,
    bounds: ClusterBounds,
    removed_nodes: Vec<(String, NodeLabel, Option<String>)>,
    removed_edges: Vec<(Edge, EdgeLabel)>,
}

fn top_level_child_of(graph: &LayeredGraph, node: &str, cluster: &str) -> Option<String> {
    let mut current = Some(node.to_string());
    while let Some(value) = current.clone() {
        let parent = graph.parent(&value).map(str::to_string);
        if parent.as_deref() == Some(cluster) {
            return Some(value);
        }
        current = parent;
    }
    None
}

fn update_cluster_edges(
    g: &mut LayeredGraph,
    subgraph: &LayeredGraph,
    offset_x: f64,
    offset_y: f64,
) {
    for edge in subgraph.edges() {
        if !g.has_edge_object(&edge) {
            continue;
        }
        let Some(source) = subgraph.edge_object(&edge).cloned() else {
            continue;
        };
        let Some(target) = g.edge_object_mut(&edge) else {
            continue;
        };
        if let Some(points) = &source.points {
            target.points = Some(
                points
                    .iter()
                    .map(|point| Point::new(point.x + offset_x, point.y + offset_y))
                    .collect(),
            );
        }
        if source.x.is_some() {
            target.x = Some(source.x.unwrap_or(f64::NAN) + offset_x);
            target.y = Some(source.y.unwrap_or(f64::NAN) + offset_y);
        }
    }
}

/// Runs the layout pipeline on `g` and returns what a later layout may reuse.
pub fn run_layout(
    g: &mut LayeredGraph,
    opts: &LayoutOptions,
    previous: Option<&PreviousLayout>,
) -> Result<PreviousLayout, LayoutError> {
    let dynamic = opts.use_dynamic.unwrap_or(true);
    let old_ranks = if dynamic {
        previous.map(PreviousLayout::ranks)
    } else {
        None
    };
    let old_nodes = if dynamic {
        previous.map(|previous| previous.raw_nodes.as_slice())
    } else {
        None
    };

    make_space_for_edge_labels(g);
    remove_self_edges(g);
    acyclic::run(g, old_ranks.as_ref());
    nesting_graph::run(g);
    let mut non_compound = util::as_non_compound_graph(g);
    rank::rank(&mut non_compound)?;
    rank::util::copy_ranks(&non_compound, g);
    inject_edge_label_proxies(g);
    util::remove_empty_ranks(g);
    nesting_graph::cleanup(g);
    util::normalize_ranks(g);
    assign_rank_min_max(g);
    remove_edge_label_proxies(g);
    normalize::run(g);
    parent_dummy_chains::run(g);
    add_border_segments::run(g);
    order::order(g, &opts.order, old_nodes);
    insert_self_edges(g);
    coordinate_system::adjust(g);
    position::position(g, opts.core_path.as_deref());
    position_self_edges(g);

    let raw_nodes: Vec<(String, NodeLabel)> = g
        .nodes()
        .into_iter()
        .map(|v| (v.clone(), g.node(&v).cloned().unwrap_or_default()))
        .collect();

    remove_border_nodes(g);
    normalize::undo(g);
    fixup_edge_label_coords(g);
    coordinate_system::undo(g);
    translate_graph(g);
    assign_node_intersects(g)?;
    reverse_points_for_reversed_edges(g);
    acyclic::undo(g);

    Ok(PreviousLayout { raw_nodes })
}

/// Copies final layout information back to the input graph, only the attributes
/// upstream copies.
pub fn update_input_graph(input_graph: &mut LayeredGraph, layout_graph: &LayeredGraph) {
    for v in input_graph.nodes() {
        let layout_label = layout_graph.node(&v).cloned();
        if let Some(layout_label) = layout_label
            && let Some(input_label) = input_graph.node_mut(&v)
        {
            input_label.x = layout_label.x;
            input_label.y = layout_label.y;
            input_label.order = layout_label.order;
            input_label.rank = layout_label.rank;
            if !layout_graph.children(&v).is_empty() {
                input_label.width = layout_label.width;
                input_label.height = layout_label.height;
            }
        }
    }

    for edge in input_graph.edges() {
        let layout_label = layout_graph.edge_object(&edge).cloned();
        if let Some(layout_label) = layout_label
            && let Some(input_label) = input_graph.edge_object_mut(&edge)
        {
            input_label.points = layout_label.points;
            if layout_label.x.is_some() {
                input_label.x = layout_label.x;
                input_label.y = layout_label.y;
            }
        }
    }

    let (width, height) = layout_graph
        .graph()
        .map(|label| (label.width, label.height))
        .unwrap_or((None, None));
    if let Some(input_label) = input_graph.graph_mut() {
        input_label.width = width;
        input_label.height = height;
    }
}

/// Builds the layout graph from the whitelisted attributes of `input_graph`.
pub fn build_layout_graph(input_graph: &LayeredGraph) -> LayeredGraph {
    let input_label = input_graph.graph().cloned().unwrap_or_default();
    let label = util::graph_label_with_defaults(Some(&GraphLabel {
        rankdir: input_label.rankdir,
        align: input_label.align,
        rankalign: input_label.rankalign,
        acyclicer: input_label.acyclicer,
        ranker: input_label.ranker,
        nodesep: input_label.nodesep,
        edgesep: input_label.edgesep,
        ranksep: input_label.ranksep,
        marginx: input_label.marginx,
        marginy: input_label.marginy,
        ..GraphLabel::default()
    }));

    let mut g: LayeredGraph = LayeredGraph::with_options(GraphOptions {
        multigraph: true,
        compound: true,
        ..GraphOptions::default()
    });
    g.set_graph(label);

    for v in input_graph.nodes() {
        let node = input_graph.node(&v).cloned().unwrap_or_default();
        g.set_node_with(
            v.clone(),
            NodeLabel {
                width: node.width,
                height: node.height,
                rank: node.rank,
                ..NodeLabel::default()
            },
        );
        if let Some(parent) = input_graph.parent(&v) {
            g.set_parent(&v, Some(parent))
                .expect("the layout graph is compound");
        }
    }

    for edge in input_graph.edges() {
        let edge_label = input_graph.edge_object(&edge).cloned().unwrap_or_default();
        g.set_edge_object_label(
            &edge,
            Some(EdgeLabel {
                minlen: Some(edge_label.minlen.unwrap_or(1.0)),
                weight: Some(edge_label.weight.unwrap_or(1.0)),
                width: Some(edge_label.width.unwrap_or(0.0)),
                height: Some(edge_label.height.unwrap_or(0.0)),
                labeloffset: Some(edge_label.labeloffset.unwrap_or(10.0)),
                labelpos: Some(edge_label.labelpos.unwrap_or_else(|| "r".to_string())),
                ..EdgeLabel::default()
            }),
        )
        .expect("the layout graph is a multigraph");
    }

    g
}

fn make_space_for_edge_labels(g: &mut LayeredGraph) {
    if let Some(label) = g.graph_mut() {
        label.ranksep = Some(label.ranksep.unwrap_or(50.0) / 2.0);
    }
    let rankdir = g
        .graph()
        .and_then(|label| label.rankdir.clone())
        .unwrap_or_default();
    for edge in g.edges() {
        if let Some(label) = g.edge_object_mut(&edge) {
            label.minlen = Some(label.minlen() * 2.0);
            let labelpos = label
                .labelpos
                .clone()
                .unwrap_or_else(|| "r".to_string())
                .to_lowercase();
            if labelpos != "c" {
                if rankdir == "TB" || rankdir == "BT" {
                    label.width = Some(label.width() + label.labeloffset());
                } else {
                    label.height = Some(label.height() + label.labeloffset());
                }
            }
        }
    }
}

fn inject_edge_label_proxies(g: &mut LayeredGraph) {
    for edge in g.edges() {
        let Some(label) = g.edge_object(&edge).cloned() else {
            continue;
        };
        if label.width() == 0.0 || label.height() == 0.0 {
            continue;
        }
        let v_rank = g.node(&edge.v).and_then(|node| node.rank).unwrap_or(0.0);
        let w_rank = g.node(&edge.w).and_then(|node| node.rank).unwrap_or(0.0);
        let rank = (w_rank - v_rank) / 2.0 + v_rank;
        util::add_dummy_node(
            g,
            "edge-proxy",
            NodeLabel {
                rank: Some(rank),
                e: Some(edge.clone()),
                ..NodeLabel::default()
            },
            "_ep",
        );
    }
}

fn assign_rank_min_max(g: &mut LayeredGraph) {
    let mut max_rank = 0.0f64;
    for v in g.nodes() {
        let node = g.node(&v).cloned().unwrap_or_default();
        if let Some(border_top) = &node.border_top {
            let min_rank = g.node(border_top).and_then(|node| node.rank);
            let max = node
                .border_bottom
                .as_ref()
                .and_then(|border| g.node(border).and_then(|node| node.rank));
            if let Some(node) = g.node_mut(&v) {
                node.min_rank = min_rank;
                node.max_rank = max;
            }
            max_rank = max_rank.max(max.unwrap_or(0.0));
        }
    }
    if let Some(label) = g.graph_mut() {
        label.max_rank = Some(max_rank);
    }
}

fn remove_edge_label_proxies(g: &mut LayeredGraph) {
    for v in g.nodes() {
        let node = g.node(&v).cloned().unwrap_or_default();
        if node.dummy.as_deref() == Some("edge-proxy") {
            if let Some(edge) = node.e
                && let Some(label) = g.edge_object_mut(&edge)
            {
                label.label_rank = node.rank;
            }
            g.remove_node(&v);
        }
    }
}

fn translate_graph(g: &mut LayeredGraph) {
    let mut min_x = f64::INFINITY;
    let mut max_x = 0.0f64;
    let mut min_y = f64::INFINITY;
    let mut max_y = 0.0f64;

    let mut get_extremes = |x: f64, y: f64, width: f64, height: f64| {
        min_x = min_x.min(x - width / 2.0);
        max_x = max_x.max(x + width / 2.0);
        min_y = min_y.min(y - height / 2.0);
        max_y = max_y.max(y + height / 2.0);
    };

    for v in g.nodes() {
        if let Some(node) = g.node(&v) {
            get_extremes(
                node.x.unwrap_or(f64::NAN),
                node.y.unwrap_or(f64::NAN),
                node.width,
                node.height,
            );
        }
    }
    for edge in g.edges() {
        if let Some(label) = g.edge_object(&edge)
            && let Some(x) = label.x
        {
            get_extremes(
                x,
                label.y.unwrap_or(f64::NAN),
                label.width(),
                label.height(),
            );
        }
    }

    let (margin_x, margin_y) = g
        .graph()
        .map(|label| (label.marginx.unwrap_or(0.0), label.marginy.unwrap_or(0.0)))
        .unwrap_or((0.0, 0.0));
    min_x -= margin_x;
    min_y -= margin_y;

    for v in g.nodes() {
        if let Some(node) = g.node_mut(&v) {
            if let Some(x) = node.x {
                node.x = Some(x - min_x);
            }
            if let Some(y) = node.y {
                node.y = Some(y - min_y);
            }
        }
    }
    for edge in g.edges() {
        if let Some(label) = g.edge_object_mut(&edge) {
            if let Some(points) = &mut label.points {
                for point in points {
                    point.x -= min_x;
                    point.y -= min_y;
                }
            }
            if label.x.is_some() {
                label.x = label.x.map(|x| x - min_x);
            }
            if label.y.is_some() {
                label.y = label.y.map(|y| y - min_y);
            }
        }
    }

    if let Some(label) = g.graph_mut() {
        label.width = Some(max_x - min_x + margin_x);
        label.height = Some(max_y - min_y + margin_y);
    }
}

fn assign_node_intersects(g: &mut LayeredGraph) -> Result<(), LayoutError> {
    for edge in g.edges() {
        if edge.v == edge.w {
            continue;
        }
        let node_v = g.node(&edge.v).cloned().unwrap_or_default();
        let node_w = g.node(&edge.w).cloned().unwrap_or_default();
        let Some(label) = g.edge_object_mut(&edge) else {
            continue;
        };
        let (p1, p2) = match &label.points {
            Some(points) if !points.is_empty() => (
                *points.first().unwrap_or(&Point::default()),
                *points.last().unwrap_or(&Point::default()),
            ),
            _ => {
                label.points = Some(Vec::new());
                (
                    Point::new(node_w.x.unwrap_or(0.0), node_w.y.unwrap_or(0.0)),
                    Point::new(node_v.x.unwrap_or(0.0), node_v.y.unwrap_or(0.0)),
                )
            }
        };
        let start = util::intersect_rect(&node_v, p1)?;
        let end = util::intersect_rect(&node_w, p2)?;
        if let Some(points) = &mut label.points {
            points.insert(0, start);
            points.push(end);
        }
    }
    Ok(())
}

fn fixup_edge_label_coords(g: &mut LayeredGraph) {
    for edge in g.edges() {
        if let Some(label) = g.edge_object_mut(&edge)
            && label.x.is_some()
        {
            if label.labelpos.as_deref() == Some("l") || label.labelpos.as_deref() == Some("r") {
                label.width = Some(label.width() - label.labeloffset());
            }
            match label.labelpos.as_deref() {
                Some("l") => {
                    label.x =
                        Some(label.x.unwrap_or(0.0) - label.width() / 2.0 - label.labeloffset());
                }
                Some("r") => {
                    label.x =
                        Some(label.x.unwrap_or(0.0) + label.width() / 2.0 + label.labeloffset());
                }
                _ => {}
            }
        }
    }
}

fn reverse_points_for_reversed_edges(g: &mut LayeredGraph) {
    for edge in g.edges() {
        if let Some(label) = g.edge_object_mut(&edge)
            && label.reversed
            && let Some(points) = &mut label.points
        {
            points.reverse();
        }
    }
}

fn remove_border_nodes(g: &mut LayeredGraph) {
    for v in g.nodes() {
        if g.children(&v).is_empty() {
            continue;
        }
        let node = g.node(&v).cloned().unwrap_or_default();
        let top = node
            .border_top
            .as_ref()
            .and_then(|border| g.node(border))
            .cloned()
            .unwrap_or_default();
        let bottom = node
            .border_bottom
            .as_ref()
            .and_then(|border| g.node(border))
            .cloned()
            .unwrap_or_default();
        let left = node
            .border_left
            .last()
            .cloned()
            .flatten()
            .and_then(|border| g.node(&border))
            .cloned()
            .unwrap_or_default();
        let right = node
            .border_right
            .last()
            .cloned()
            .flatten()
            .and_then(|border| g.node(&border))
            .cloned()
            .unwrap_or_default();

        if let Some(label) = g.node_mut(&v) {
            label.width = (right.x.unwrap_or(0.0) - left.x.unwrap_or(0.0)).abs();
            label.height = (bottom.y.unwrap_or(0.0) - top.y.unwrap_or(0.0)).abs();
            label.x = Some(left.x.unwrap_or(0.0) + label.width / 2.0);
            label.y = Some(top.y.unwrap_or(0.0) + label.height / 2.0);
        }
    }

    for v in g.nodes() {
        if g.node(&v)
            .is_some_and(|node| node.dummy.as_deref() == Some("border"))
        {
            g.remove_node(&v);
        }
    }
}

fn remove_self_edges(g: &mut LayeredGraph) {
    for edge in g.edges() {
        if edge.v != edge.w {
            continue;
        }
        let Some(label) = g.edge_object(&edge).cloned() else {
            continue;
        };
        if let Some(node) = g.node_mut(&edge.v) {
            node.self_edges.get_or_insert_with(Vec::new).push(SelfEdge {
                e: edge.clone(),
                label,
            });
        }
        g.remove_edge_object(&edge);
    }
}

fn insert_self_edges(g: &mut LayeredGraph) {
    for layer in util::build_layer_matrix(g) {
        let mut order_shift = 0usize;
        for (i, v) in layer.iter().enumerate() {
            let Some((rank, self_edges)) = g.node_mut(v).map(|node| {
                if node.rank.is_none() {
                    node.rank = Some(0.0);
                }
                node.order = Some(i + order_shift);
                (node.rank, node.self_edges.take().unwrap_or_default())
            }) else {
                continue;
            };
            for self_edge in self_edges {
                order_shift += 1;
                let mut label = self_edge.label.clone();
                let valid_spline = label
                    .points
                    .as_ref()
                    .is_some_and(|points| points.len() == 7);
                if !valid_spline {
                    label.points = Some(default_self_edge_points());
                }
                util::add_dummy_node(
                    g,
                    "selfedge",
                    NodeLabel {
                        width: label.width(),
                        height: label.height(),
                        rank,
                        order: Some(i + order_shift),
                        e: Some(self_edge.e),
                        edge_label: Some(label),
                        ..NodeLabel::default()
                    },
                    "_se",
                );
            }
        }
    }
}

fn default_self_edge_points() -> Vec<Point> {
    vec![
        Point::new(0.0, -10.0),
        Point::new(0.0, -10.0),
        Point::new(0.0, 0.0),
        Point::new(0.0, 10.0),
        Point::new(0.0, 10.0),
        Point::new(0.0, 0.0),
        Point::new(0.0, 0.0),
    ]
}

fn position_self_edges(g: &mut LayeredGraph) {
    for v in g.nodes() {
        let node = g.node(&v).cloned().unwrap_or_default();
        if node.dummy.as_deref() == Some("selfedge") {
            let Some(edge) = node.e.clone() else {
                continue;
            };
            let self_node = g.node(&edge.v).cloned().unwrap_or_default();
            let x_value = finite(self_node.x);
            let y_value = finite(self_node.y);
            let width = self_node.width;
            let height = self_node.height;
            let node_x = node.x.filter(|value| value.is_finite()).unwrap_or(x_value);
            let node_y = node.y.filter(|value| value.is_finite()).unwrap_or(y_value);
            let dx = width / 2.0;
            let dy = height / 2.0;
            let mut label = node.edge_label.clone().unwrap_or_default();
            label.points = Some(vec![
                Point::new(node_x + dx, node_y - dy),
                Point::new(node_x + dx, node_y - dy),
                Point::new(node_x, node_y),
                Point::new(node_x - dx, node_y + dy),
                Point::new(node_x - dx, node_y + dy),
                Point::new(node_x, node_y),
                Point::new(node_x, node_y),
            ]);
            label.x = Some(node_x);
            label.y = Some(node_y);
            g.set_edge_object_label(&edge, Some(label))
                .expect("a self edge stays inside the multigraph");
            g.remove_node(&v);
        }
    }
}

fn finite(value: Option<f64>) -> f64 {
    value.filter(|value| value.is_finite()).unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{close, close_option, coordinates, layering, sized};
    use super::super::{GraphLabel, LayeredGraph, NodeLabel, layout};
    use super::{LayoutOptions, layout_with_options};

    fn graph() -> LayeredGraph {
        layering()
    }

    #[test]
    fn can_layout_a_single_node() {
        let mut graph = graph();
        graph.set_node_with("a", sized(50.0, 100.0));
        layout(&mut graph).unwrap();
        assert_eq!(coordinates(&graph), vec![("a".to_string(), 25.0, 50.0)]);
    }

    #[test]
    fn can_layout_two_nodes_on_the_same_rank() {
        let mut graph = graph();
        graph.graph_mut().unwrap().nodesep = Some(200.0);
        graph.set_node_with("a", sized(50.0, 100.0));
        graph.set_node_with("b", sized(75.0, 200.0));
        layout(&mut graph).unwrap();
        assert_eq!(
            coordinates(&graph),
            vec![
                ("a".to_string(), 25.0, 100.0),
                ("b".to_string(), 287.5, 100.0),
            ]
        );
    }

    #[test]
    fn can_layout_two_nodes_connected_by_an_edge() {
        let mut graph = graph();
        graph.graph_mut().unwrap().ranksep = Some(300.0);
        graph.set_node_with("a", sized(50.0, 100.0));
        graph.set_node_with("b", sized(75.0, 200.0));
        graph.set_edge("a", "b");
        layout(&mut graph).unwrap();
        assert_eq!(
            coordinates(&graph),
            vec![
                ("a".to_string(), 37.5, 50.0),
                ("b".to_string(), 37.5, 500.0),
            ]
        );
        let edge = graph.edge("a", "b").unwrap();
        assert_eq!(edge.x, None);
        assert_eq!(edge.y, None);
    }

    #[test]
    fn can_layout_an_edge_with_a_label() {
        let mut graph = graph();
        graph.graph_mut().unwrap().ranksep = Some(300.0);
        graph.set_node_with("a", sized(50.0, 100.0));
        graph.set_node_with("b", sized(75.0, 200.0));
        graph.set_edge_with(
            "a",
            "b",
            super::super::EdgeLabel {
                width: Some(60.0),
                height: Some(70.0),
                labelpos: Some("c".to_string()),
                ..Default::default()
            },
        );
        layout(&mut graph).unwrap();
        assert_eq!(
            coordinates(&graph),
            vec![
                ("a".to_string(), 37.5, 50.0),
                ("b".to_string(), 37.5, 570.0),
            ]
        );
        let edge = graph.edge("a", "b").unwrap();
        close_option(edge.x, 37.5);
        close_option(edge.y, 285.0);
    }

    #[test]
    fn lays_out_a_long_label_in_every_rankdir() {
        for rankdir in ["TB", "BT", "LR", "RL"] {
            let mut graph = graph();
            graph.set_graph(GraphLabel {
                rankdir: Some(rankdir.to_string()),
                ..GraphLabel::default()
            });
            {
                let label = graph.graph_mut().unwrap();
                label.nodesep = Some(10.0);
                label.edgesep = Some(10.0);
            }
            for v in ["a", "b", "c", "d"] {
                graph.set_node_with(v, sized(10.0, 10.0));
            }
            graph.set_edge_with(
                "a",
                "c",
                super::super::EdgeLabel {
                    width: Some(2000.0),
                    height: Some(10.0),
                    labelpos: Some("c".to_string()),
                    ..Default::default()
                },
            );
            graph.set_edge_with(
                "b",
                "d",
                super::super::EdgeLabel {
                    width: Some(1.0),
                    height: Some(1.0),
                    ..Default::default()
                },
            );
            layout(&mut graph).unwrap();
            let (p1_x, p2_x) = if rankdir == "TB" || rankdir == "BT" {
                (
                    graph.edge("a", "c").unwrap().x.unwrap_or(0.0),
                    graph.edge("b", "d").unwrap().x.unwrap_or(0.0),
                )
            } else {
                (
                    graph.node("a").unwrap().x.unwrap_or(0.0),
                    graph.node("c").unwrap().x.unwrap_or(0.0),
                )
            };
            assert!((p1_x - p2_x).abs() > 1000.0, "{rankdir}");
        }
    }

    #[test]
    fn applies_label_offsets_in_every_rankdir() {
        for rankdir in ["TB", "BT", "LR", "RL"] {
            let mut graph = graph();
            graph.set_graph(GraphLabel {
                rankdir: Some(rankdir.to_string()),
                ..GraphLabel::default()
            });
            {
                let label = graph.graph_mut().unwrap();
                label.nodesep = Some(10.0);
                label.edgesep = Some(10.0);
            }
            for v in ["a", "b", "c", "d"] {
                graph.set_node_with(v, sized(10.0, 10.0));
            }
            graph.set_edge_with(
                "a",
                "b",
                super::super::EdgeLabel {
                    width: Some(10.0),
                    height: Some(10.0),
                    labelpos: Some("l".to_string()),
                    labeloffset: Some(1000.0),
                    ..Default::default()
                },
            );
            graph.set_edge_with(
                "c",
                "d",
                super::super::EdgeLabel {
                    width: Some(10.0),
                    height: Some(10.0),
                    labelpos: Some("r".to_string()),
                    labeloffset: Some(1000.0),
                    ..Default::default()
                },
            );
            layout(&mut graph).unwrap();
            if rankdir == "TB" || rankdir == "BT" {
                let first = graph.edge("a", "b").unwrap();
                close(
                    first.x.unwrap() - first.points.as_ref().unwrap()[0].x,
                    -1005.0,
                );
                let second = graph.edge("c", "d").unwrap();
                close(
                    second.x.unwrap() - second.points.as_ref().unwrap()[0].x,
                    1005.0,
                );
            } else {
                let first = graph.edge("a", "b").unwrap();
                close(
                    first.y.unwrap() - first.points.as_ref().unwrap()[0].y,
                    -1005.0,
                );
                let second = graph.edge("c", "d").unwrap();
                close(
                    second.y.unwrap() - second.points.as_ref().unwrap()[0].y,
                    1005.0,
                );
            }
        }
    }

    #[test]
    fn can_layout_a_long_edge_with_a_label() {
        let mut graph = graph();
        graph.graph_mut().unwrap().ranksep = Some(300.0);
        graph.set_node_with("a", sized(50.0, 100.0));
        graph.set_node_with("b", sized(75.0, 200.0));
        graph.set_edge_with(
            "a",
            "b",
            super::super::EdgeLabel {
                width: Some(60.0),
                height: Some(70.0),
                minlen: Some(2.0),
                labelpos: Some("c".to_string()),
                ..Default::default()
            },
        );
        layout(&mut graph).unwrap();
        let edge = graph.edge("a", "b").unwrap();
        close_option(edge.x, 37.5);
        assert!(edge.y.unwrap() > graph.node("a").unwrap().y.unwrap());
        assert!(edge.y.unwrap() < graph.node("b").unwrap().y.unwrap());
    }

    #[test]
    fn can_layout_a_short_cycle() {
        let mut graph = graph();
        graph.graph_mut().unwrap().ranksep = Some(200.0);
        graph.set_node_with("a", sized(100.0, 100.0));
        graph.set_node_with("b", sized(100.0, 100.0));
        graph.set_edge_with(
            "a",
            "b",
            super::super::EdgeLabel {
                weight: Some(2.0),
                ..Default::default()
            },
        );
        graph.set_edge("b", "a");
        layout(&mut graph).unwrap();
        assert_eq!(
            coordinates(&graph),
            vec![
                ("a".to_string(), 50.0, 50.0),
                ("b".to_string(), 50.0, 350.0),
            ]
        );
        let forward = graph.edge("a", "b").unwrap().points.clone().unwrap();
        assert!(forward[1].y > forward[0].y);
        let backward = graph.edge("b", "a").unwrap().points.clone().unwrap();
        assert!(backward[0].y > backward[1].y);
    }

    #[test]
    fn adds_rectangle_intersects_for_edges() {
        let mut graph = graph();
        graph.graph_mut().unwrap().ranksep = Some(200.0);
        graph.set_node_with("a", sized(100.0, 100.0));
        graph.set_node_with("b", sized(100.0, 100.0));
        graph.set_edge("a", "b");
        layout(&mut graph).unwrap();
        let points = graph.edge("a", "b").unwrap().points.clone().unwrap();
        assert_eq!(points.len(), 3);
        close(points[0].x, 50.0);
        close(points[0].y, 100.0);
        close(points[1].x, 50.0);
        close(points[1].y, 200.0);
        close(points[2].x, 50.0);
        close(points[2].y, 300.0);
    }

    #[test]
    fn adds_rectangle_intersects_for_multi_rank_edges() {
        let mut graph = graph();
        graph.graph_mut().unwrap().ranksep = Some(200.0);
        graph.set_node_with("a", sized(100.0, 100.0));
        graph.set_node_with("b", sized(100.0, 100.0));
        graph.set_edge_with(
            "a",
            "b",
            super::super::EdgeLabel {
                minlen: Some(2.0),
                ..Default::default()
            },
        );
        layout(&mut graph).unwrap();
        let points = graph.edge("a", "b").unwrap().points.clone().unwrap();
        assert_eq!(points.len(), 5);
        for (point, expected) in points.iter().zip([100.0, 200.0, 300.0, 400.0, 500.0]) {
            close(point.x, 50.0);
            close(point.y, expected);
        }
    }

    #[test]
    fn adds_rectangle_intersects_for_zero_sized_nodes() {
        for rankdir in ["TB", "BT", "LR", "RL"] {
            for (width, height) in [(0.0, 40.0), (100.0, 0.0), (0.0, 0.0)] {
                let mut graph = graph();
                graph.set_graph(GraphLabel {
                    rankdir: Some(rankdir.to_string()),
                    ..GraphLabel::default()
                });
                graph.set_node_with("a", sized(width, height));
                graph.set_node_with("b", sized(width, height));
                graph.set_edge("a", "b");
                layout(&mut graph).unwrap();
                let a = graph.node("a").unwrap().clone();
                let b = graph.node("b").unwrap().clone();
                let points = graph.edge("a", "b").unwrap().points.clone().unwrap();
                let first = points[0];
                let last = points[points.len() - 1];
                if rankdir == "TB" || rankdir == "BT" {
                    let sign = if rankdir == "TB" { 1.0 } else { -1.0 };
                    close(first.x, a.x.unwrap());
                    close(first.y, a.y.unwrap() + sign * height / 2.0);
                    close(last.x, b.x.unwrap());
                    close(last.y, b.y.unwrap() - sign * height / 2.0);
                } else {
                    let sign = if rankdir == "LR" { 1.0 } else { -1.0 };
                    close(first.x, a.x.unwrap() + sign * width / 2.0);
                    close(first.y, a.y.unwrap());
                    close(last.x, b.x.unwrap() - sign * width / 2.0);
                    close(last.y, b.y.unwrap());
                }
            }
        }
    }

    #[test]
    fn can_layout_three_parallel_edges() {
        for rankdir in ["TB", "LR"] {
            let mut graph = graph();
            graph.set_graph(GraphLabel {
                rankdir: Some(rankdir.to_string()),
                ..GraphLabel::default()
            });
            for v in ["a", "b", "c", "d"] {
                graph.set_node_with(v, sized(100.0, 40.0));
            }
            graph
                .set_edge_full(
                    "a",
                    "b",
                    Some(super::super::EdgeLabel::default()),
                    Some("ab0"),
                )
                .unwrap();
            graph
                .set_edge_full(
                    "a",
                    "b",
                    Some(super::super::EdgeLabel::default()),
                    Some("ab1"),
                )
                .unwrap();
            graph
                .set_edge_full(
                    "a",
                    "b",
                    Some(super::super::EdgeLabel::default()),
                    Some("ab2"),
                )
                .unwrap();
            graph.set_edge("a", "c");
            graph.set_edge("b", "d");
            graph.set_edge("c", "d");
            layout(&mut graph).unwrap();
            for v in graph.nodes() {
                let node = graph.node(&v).unwrap();
                assert!(node.x.unwrap().is_finite(), "{rankdir} {v}");
                assert!(node.y.unwrap().is_finite(), "{rankdir} {v}");
            }
            let missing: Vec<_> = graph
                .edges()
                .into_iter()
                .filter(|edge| graph.edge_object(edge).unwrap().points.is_none())
                .collect();
            assert!(missing.is_empty(), "{rankdir} missing points {missing:?}");
            for edge in graph.edges() {
                for point in graph.edge_object(&edge).unwrap().points.clone().unwrap() {
                    assert!(point.x.is_finite());
                    assert!(point.y.is_finite());
                }
            }
        }
    }

    #[test]
    fn can_layout_a_self_loop() {
        for rankdir in ["TB", "BT", "LR", "RL"] {
            let mut graph = graph();
            graph.set_graph(GraphLabel {
                rankdir: Some(rankdir.to_string()),
                ..GraphLabel::default()
            });
            graph.graph_mut().unwrap().edgesep = Some(75.0);
            graph.set_node_with("a", sized(100.0, 100.0));
            graph.set_edge_with(
                "a",
                "a",
                super::super::EdgeLabel {
                    width: Some(50.0),
                    height: Some(50.0),
                    ..Default::default()
                },
            );
            layout(&mut graph).unwrap();
            let node = graph.node("a").unwrap().clone();
            let points = graph.edge("a", "a").unwrap().points.clone().unwrap();
            assert_eq!(points.len(), 7, "{rankdir}");
            for point in points {
                if rankdir != "LR" && rankdir != "RL" {
                    assert!(point.x > node.x.unwrap());
                    assert!((point.y - node.y.unwrap()).abs() <= node.height / 2.0);
                } else {
                    assert!(point.y > node.y.unwrap());
                    assert!((point.x - node.x.unwrap()).abs() <= node.width / 2.0);
                }
            }
        }
    }

    #[test]
    fn can_layout_a_graph_with_subgraphs() {
        let mut graph = graph();
        graph.set_node_with("a", sized(50.0, 50.0));
        graph.set_parent("a", Some("sg1")).unwrap();
        layout(&mut graph).unwrap();
    }

    #[test]
    fn minimizes_the_height_of_subgraphs() {
        let mut graph = graph();
        for v in ["a", "b", "c", "d", "x", "y"] {
            graph.set_node_with(v, sized(50.0, 50.0));
        }
        graph.set_path(&["a", "b", "c", "d"]);
        graph.set_edge_with(
            "a",
            "x",
            super::super::EdgeLabel {
                weight: Some(100.0),
                ..Default::default()
            },
        );
        graph.set_edge_with(
            "y",
            "d",
            super::super::EdgeLabel {
                weight: Some(100.0),
                ..Default::default()
            },
        );
        graph.set_parent("x", Some("sg")).unwrap();
        graph.set_parent("y", Some("sg")).unwrap();
        layout(&mut graph).unwrap();
        close(
            graph.node("x").unwrap().y.unwrap(),
            graph.node("y").unwrap().y.unwrap(),
        );
    }

    #[test]
    fn minimizes_separation_between_nodes_not_adjacent_to_subgraphs() {
        let mut graph = graph();
        for v in ["a", "b", "c"] {
            graph.set_node_with(v, sized(50.0, 50.0));
        }
        graph.set_path(&["a", "b", "c"]);
        graph.set_node_with("sg", NodeLabel::default());
        graph.set_parent("c", Some("sg")).unwrap();
        layout(&mut graph).unwrap();
        close(
            graph.node("b").unwrap().y.unwrap() - graph.node("a").unwrap().y.unwrap(),
            100.0,
        );
    }

    #[test]
    fn can_layout_subgraphs_with_different_rankdirs() {
        for rankdir in ["tb", "bt", "lr", "rl"] {
            let mut graph = graph();
            graph.set_graph(GraphLabel {
                rankdir: Some(rankdir.to_string()),
                ..GraphLabel::default()
            });
            graph.set_node_with("a", sized(50.0, 50.0));
            graph.set_node_with("sg", NodeLabel::default());
            graph.set_parent("a", Some("sg")).unwrap();
            layout(&mut graph).unwrap();
            let sg = graph.node("sg").unwrap();
            assert!(sg.width > 50.0, "{rankdir}");
            assert!(sg.height > 50.0, "{rankdir}");
            assert!(sg.x.unwrap() > 25.0, "{rankdir}");
            assert!(sg.y.unwrap() > 25.0, "{rankdir}");
        }
    }

    #[test]
    fn routes_edges_inside_a_cluster_with_its_own_direction() {
        for rankdir in ["TB", "BT", "LR", "RL"] {
            for name in [None, Some("internal")] {
                let mut graph = graph();
                graph.set_node_with(
                    "sg",
                    NodeLabel {
                        rankdir: Some(rankdir.to_string()),
                        ..NodeLabel::default()
                    },
                );
                graph.set_node_with("a", sized(100.0, 40.0));
                graph.set_node_with("b", sized(100.0, 40.0));
                graph.set_parent("a", Some("sg")).unwrap();
                graph.set_parent("b", Some("sg")).unwrap();
                graph
                    .set_edge_full(
                        "a",
                        "b",
                        Some(super::super::EdgeLabel {
                            width: Some(30.0),
                            height: Some(10.0),
                            labelpos: Some("c".to_string()),
                            ..Default::default()
                        }),
                        name,
                    )
                    .unwrap();
                graph.set_node_with("outside", sized(200.0, 80.0));
                layout(&mut graph).unwrap();
                let edge = graph.edge_label("a", "b", name).unwrap();
                let points = edge.points.clone().unwrap();
                assert_eq!(points.len(), 3);
                for point in &points {
                    assert!(point.x.is_finite());
                    assert!(point.y.is_finite());
                }
                let first = points[0];
                let last = points[points.len() - 1];
                let a = graph.node("a").unwrap().clone();
                let b = graph.node("b").unwrap().clone();
                if rankdir == "LR" || rankdir == "RL" {
                    let sign = if rankdir == "LR" { 1.0 } else { -1.0 };
                    close(first.x, a.x.unwrap() + sign * a.width / 2.0);
                    close(first.y, a.y.unwrap());
                    close(last.x, b.x.unwrap() - sign * b.width / 2.0);
                    close(last.y, b.y.unwrap());
                } else {
                    let sign = if rankdir == "TB" { 1.0 } else { -1.0 };
                    close(first.x, a.x.unwrap());
                    close(first.y, a.y.unwrap() + sign * a.height / 2.0);
                    close(last.x, b.x.unwrap());
                    close(last.y, b.y.unwrap() - sign * b.height / 2.0);
                }
                close(edge.x.unwrap(), (a.x.unwrap() + b.x.unwrap()) / 2.0);
                close(edge.y.unwrap(), (a.y.unwrap() + b.y.unwrap()) / 2.0);
            }
        }
    }

    #[test]
    fn preserves_parallel_and_self_loop_routes_in_a_directed_cluster() {
        let mut graph = graph();
        graph.set_node_with(
            "sg",
            NodeLabel {
                rankdir: Some("LR".to_string()),
                ..NodeLabel::default()
            },
        );
        for v in ["a", "b"] {
            graph.set_node_with(v, sized(100.0, 40.0));
            graph.set_parent(v, Some("sg")).unwrap();
        }
        graph
            .set_edge_full(
                "a",
                "b",
                Some(super::super::EdgeLabel::default()),
                Some("first"),
            )
            .unwrap();
        graph
            .set_edge_full(
                "a",
                "b",
                Some(super::super::EdgeLabel::default()),
                Some("second"),
            )
            .unwrap();
        graph
            .set_edge_full(
                "a",
                "a",
                Some(super::super::EdgeLabel::default()),
                Some("loop"),
            )
            .unwrap();
        layout(&mut graph).unwrap();
        assert_eq!(graph.edge_count(), 3);
        let missing: Vec<_> = graph
            .edges()
            .into_iter()
            .filter(|edge| graph.edge_object(edge).unwrap().points.is_none())
            .collect();
        assert!(missing.is_empty(), "missing points {missing:?}");
        for edge in graph.edges() {
            let points = graph.edge_object(&edge).unwrap().points.clone().unwrap();
            assert!(points.len() >= 3);
            for point in points {
                assert!(point.x.is_finite());
                assert!(point.y.is_finite());
            }
        }
        assert_ne!(
            graph.edge_label("a", "b", Some("first")).unwrap().points,
            graph.edge_label("a", "b", Some("second")).unwrap().points
        );
    }

    #[test]
    fn adds_dimensions_to_the_graph() {
        let mut graph = graph();
        graph.set_node_with("a", sized(100.0, 50.0));
        layout(&mut graph).unwrap();
        let label = graph.graph().unwrap();
        close(label.width.unwrap(), 100.0);
        close(label.height.unwrap(), 50.0);
    }

    #[test]
    fn keeps_node_coordinates_in_the_bounding_box() {
        for rankdir in ["TB", "BT", "LR", "RL"] {
            let mut graph = graph();
            graph.set_graph(GraphLabel {
                rankdir: Some(rankdir.to_string()),
                ..GraphLabel::default()
            });
            graph.set_node_with("a", sized(100.0, 200.0));
            layout(&mut graph).unwrap();
            close(graph.node("a").unwrap().x.unwrap(), 50.0);
            close(graph.node("a").unwrap().y.unwrap(), 100.0);
        }
    }

    #[test]
    fn keeps_edge_label_coordinates_in_the_bounding_box() {
        for rankdir in ["TB", "BT", "LR", "RL"] {
            let mut graph = graph();
            graph.set_graph(GraphLabel {
                rankdir: Some(rankdir.to_string()),
                ..GraphLabel::default()
            });
            graph.set_node_with("a", sized(100.0, 100.0));
            graph.set_node_with("b", sized(100.0, 100.0));
            graph.set_edge_with(
                "a",
                "b",
                super::super::EdgeLabel {
                    width: Some(1000.0),
                    height: Some(2000.0),
                    labelpos: Some("l".to_string()),
                    labeloffset: Some(0.0),
                    ..Default::default()
                },
            );
            layout(&mut graph).unwrap();
            let edge = graph.edge("a", "b").unwrap();
            if rankdir == "TB" || rankdir == "BT" {
                close_option(edge.x, 500.0);
            } else {
                close_option(edge.y, 1000.0);
            }
        }
    }

    #[test]
    fn produces_stable_positions_when_laying_out_repeatedly() {
        let mut graph = graph();
        for (v, w) in [("a", "c"), ("a", "b"), ("c", "b")] {
            graph.set_node_with(v, sized(100.0, 100.0));
            graph.set_node_with(w, sized(100.0, 100.0));
            graph.set_edge(v, w);
        }
        layout(&mut graph).unwrap();
        let first = coordinates(&graph);
        layout(&mut graph).unwrap();
        assert_eq!(coordinates(&graph), first);
        layout(&mut graph).unwrap();
        assert_eq!(coordinates(&graph), first);
    }

    #[test]
    fn can_disable_dynamic_layout() {
        let mut graph = graph();
        for (v, w) in [("a", "c"), ("a", "b"), ("c", "b")] {
            graph.set_node_with(v, sized(100.0, 100.0));
            graph.set_node_with(w, sized(100.0, 100.0));
            graph.set_edge(v, w);
        }
        let opts = LayoutOptions {
            use_dynamic: Some(false),
            ..LayoutOptions::default()
        };
        layout_with_options(&mut graph, &opts).unwrap();
    }

    #[test]
    fn keeps_unrelated_graphs_independent() {
        let mut first = graph();
        first.set_node_with("b", sized(100.0, 100.0));
        first.set_node_with("a", sized(100.0, 100.0));
        first.set_edge("b", "a");
        let mut second = graph();
        for (v, w) in [("a", "c"), ("a", "b"), ("c", "b")] {
            second.set_node_with(v, sized(100.0, 100.0));
            second.set_node_with(w, sized(100.0, 100.0));
            second.set_edge(v, w);
        }
        layout(&mut first).unwrap();
        layout(&mut second).unwrap();
        assert_eq!(first.node_count(), 2);
        assert_eq!(second.node_count(), 3);
    }

    #[test]
    fn build_layout_graph_copies_only_the_whitelisted_attributes() {
        let mut input = graph();
        input.set_graph(GraphLabel {
            rankdir: Some("LR".to_string()),
            align: Some("UL".to_string()),
            rankalign: Some("top".to_string()),
            nodesep: Some(11.0),
            edgesep: Some(12.0),
            ranksep: Some(13.0),
            marginx: Some(14.0),
            marginy: Some(15.0),
            acyclicer: Some("greedy".to_string()),
            ranker: Some("tight-tree".to_string()),
            width: Some(999.0),
            height: Some(999.0),
            ..GraphLabel::default()
        });
        input.set_node_with(
            "a",
            NodeLabel {
                width: 20.0,
                height: 21.0,
                rank: Some(3.0),
                rankdir: Some("BT".to_string()),
                x: Some(99.0),
                ..NodeLabel::default()
            },
        );
        input.set_edge_with(
            "a",
            "b",
            super::super::EdgeLabel {
                minlen: Some(2.0),
                weight: Some(3.0),
                width: Some(4.0),
                height: Some(5.0),
                labeloffset: Some(6.0),
                labelpos: Some("l".to_string()),
                x: Some(99.0),
                ..Default::default()
            },
        );
        let layout_graph = super::build_layout_graph(&input);
        let label = layout_graph.graph().unwrap();
        assert_eq!(label.rankdir.as_deref(), Some("LR"));
        assert_eq!(label.align.as_deref(), Some("UL"));
        assert_eq!(label.rankalign.as_deref(), Some("top"));
        assert_eq!(label.nodesep, Some(11.0));
        assert_eq!(label.edgesep, Some(12.0));
        assert_eq!(label.ranksep, Some(13.0));
        assert_eq!(label.marginx, Some(14.0));
        assert_eq!(label.marginy, Some(15.0));
        assert_eq!(label.acyclicer.as_deref(), Some("greedy"));
        assert_eq!(label.ranker.as_deref(), Some("tight-tree"));
        assert_eq!(label.width, None);
        assert_eq!(label.height, None);

        let node = layout_graph.node("a").unwrap();
        assert_eq!(node.width, 20.0);
        assert_eq!(node.height, 21.0);
        assert_eq!(node.rank, Some(3.0));
        assert_eq!(node.rankdir, None);
        assert_eq!(node.x, None);

        let edge = layout_graph.edge("a", "b").unwrap();
        assert_eq!(edge.minlen, Some(2.0));
        assert_eq!(edge.weight, Some(3.0));
        assert_eq!(edge.width, Some(4.0));
        assert_eq!(edge.height, Some(5.0));
        assert_eq!(edge.labeloffset, Some(6.0));
        assert_eq!(edge.labelpos.as_deref(), Some("l"));
        assert_eq!(edge.x, None);
    }

    #[test]
    fn build_layout_graph_fills_the_defaults() {
        let mut input = graph();
        input.set_node_with("a", NodeLabel::default());
        input.set_edge_with("a", "b", super::super::EdgeLabel::default());
        let layout_graph = super::build_layout_graph(&input);
        let label = layout_graph.graph().unwrap();
        assert_eq!(label.rankdir.as_deref(), Some("TB"));
        assert_eq!(label.rankalign.as_deref(), Some("center"));
        assert_eq!(label.nodesep, Some(50.0));
        assert_eq!(label.edgesep, Some(20.0));
        assert_eq!(label.ranksep, Some(50.0));
        let edge = layout_graph.edge("a", "b").unwrap();
        assert_eq!(edge.minlen, Some(1.0));
        assert_eq!(edge.weight, Some(1.0));
        assert_eq!(edge.width, Some(0.0));
        assert_eq!(edge.height, Some(0.0));
        assert_eq!(edge.labeloffset, Some(10.0));
        assert_eq!(edge.labelpos.as_deref(), Some("r"));
    }

    #[test]
    fn returns_a_previous_layout_for_history_aware_callers() {
        let mut graph = graph();
        graph.set_node_with("a", sized(50.0, 50.0));
        graph.set_node_with("b", sized(50.0, 50.0));
        graph.set_edge("a", "b");
        let previous =
            super::layout_with_history(&mut graph, &LayoutOptions::default(), None).unwrap();
        assert!(!previous.raw_nodes.is_empty());
        super::layout_with_history(&mut graph, &LayoutOptions::default(), Some(&previous)).unwrap();
    }
}

#[cfg(test)]
mod cluster_direction_tests {
    use super::super::test_support::{layering, sized};
    use super::super::{GraphLabel, LayeredGraph, NodeLabel, layout};

    fn approx(actual: Option<f64>, expected: f64) {
        assert!(
            (actual.unwrap_or(f64::NAN) - expected).abs() < 0.05,
            "expected {expected}, got {actual:?}"
        );
    }

    fn cluster(rankdir: &str) -> NodeLabel {
        NodeLabel {
            rankdir: Some(rankdir.to_string()),
            ..NodeLabel::default()
        }
    }

    fn graph(rankdir: &str) -> LayeredGraph {
        let mut graph = layering();
        graph.set_graph(GraphLabel {
            rankdir: Some(rankdir.to_string()),
            ..GraphLabel::default()
        });
        graph
    }

    #[test]
    fn lays_out_nested_clusters_with_different_rankdirs() {
        let mut graph = graph("TB");
        graph.set_node_with("outer", cluster("TB"));
        graph.set_node_with("inner", cluster("LR"));
        for v in ["x1", "x2", "x3"] {
            graph.set_node_with(v, sized(40.0, 40.0));
        }
        graph.set_parent("x1", Some("inner")).unwrap();
        graph.set_parent("x2", Some("inner")).unwrap();
        graph.set_parent("inner", Some("outer")).unwrap();
        graph.set_parent("x3", Some("outer")).unwrap();
        graph.set_edge("x1", "x2");
        graph.set_edge("x2", "x3");
        layout(&mut graph).unwrap();
        approx(
            graph.node("x1").unwrap().y,
            graph.node("x2").unwrap().y.unwrap(),
        );
        assert!(graph.node("x1").unwrap().x < graph.node("x2").unwrap().x);
        assert!(graph.node("inner").unwrap().y < graph.node("x3").unwrap().y);
    }

    #[test]
    fn routes_cross_cluster_edges_correctly() {
        let mut graph = graph("TB");
        graph.set_node_with("src", cluster("LR"));
        graph.set_node_with("tgt", cluster("LR"));
        for v in ["s1", "s2", "t1", "t2"] {
            graph.set_node_with(v, sized(40.0, 40.0));
        }
        graph.set_parent("s1", Some("src")).unwrap();
        graph.set_parent("s2", Some("src")).unwrap();
        graph.set_parent("t1", Some("tgt")).unwrap();
        graph.set_parent("t2", Some("tgt")).unwrap();
        graph.set_edge("s1", "s2");
        graph.set_edge("t1", "t2");
        graph.set_edge("s2", "t1");
        layout(&mut graph).unwrap();
        for v in ["src", "tgt"] {
            assert!(graph.node(v).unwrap().x.is_some());
            assert!(graph.node(v).unwrap().y.is_some());
        }
        assert!(graph.node("src").unwrap().y < graph.node("tgt").unwrap().y);
        assert!(graph.node("s1").unwrap().x < graph.node("s2").unwrap().x);
        assert!(graph.node("t1").unwrap().x < graph.node("t2").unwrap().x);
    }

    #[test]
    fn supports_all_rankdir_combinations() {
        let mut graph = graph("TB");
        for (cluster_name, rankdir) in [("ctb", "TB"), ("cbt", "BT"), ("clr", "LR"), ("crl", "RL")]
        {
            graph.set_node_with(cluster_name, cluster(rankdir));
        }
        for v in ["tb1", "tb2"] {
            graph.set_node_with(v, sized(40.0, 40.0));
            graph.set_parent(v, Some("ctb")).unwrap();
        }
        for v in ["bt1", "bt2"] {
            graph.set_node_with(v, sized(40.0, 40.0));
            graph.set_parent(v, Some("cbt")).unwrap();
        }
        for v in ["lr1", "lr2"] {
            graph.set_node_with(v, sized(40.0, 40.0));
            graph.set_parent(v, Some("clr")).unwrap();
        }
        for v in ["rl1", "rl2"] {
            graph.set_node_with(v, sized(40.0, 40.0));
            graph.set_parent(v, Some("crl")).unwrap();
        }
        graph.set_edge("tb1", "tb2");
        graph.set_edge("bt1", "bt2");
        graph.set_edge("lr1", "lr2");
        graph.set_edge("rl1", "rl2");
        graph.set_edge("ctb", "cbt");
        graph.set_edge("cbt", "clr");
        graph.set_edge("clr", "crl");
        layout(&mut graph).unwrap();

        approx(
            graph.node("tb1").unwrap().x,
            graph.node("tb2").unwrap().x.unwrap(),
        );
        assert!(graph.node("tb1").unwrap().y < graph.node("tb2").unwrap().y);
        approx(
            graph.node("bt1").unwrap().x,
            graph.node("bt2").unwrap().x.unwrap(),
        );
        assert!(graph.node("bt2").unwrap().y < graph.node("bt1").unwrap().y);
        approx(
            graph.node("lr1").unwrap().y,
            graph.node("lr2").unwrap().y.unwrap(),
        );
        assert!(graph.node("lr1").unwrap().x < graph.node("lr2").unwrap().x);
        approx(
            graph.node("rl1").unwrap().y,
            graph.node("rl2").unwrap().y.unwrap(),
        );
        assert!(graph.node("rl1").unwrap().x > graph.node("rl2").unwrap().x);
    }

    #[test]
    fn lays_out_two_clusters_with_different_directions() {
        let mut graph = graph("TB");
        graph.set_node_with("c1", cluster("LR"));
        graph.set_node_with("c2", cluster("TB"));
        for v in ["a1", "a2"] {
            graph.set_node_with(v, sized(40.0, 40.0));
            graph.set_parent(v, Some("c1")).unwrap();
        }
        for v in ["b1", "b2"] {
            graph.set_node_with(v, sized(40.0, 40.0));
            graph.set_parent(v, Some("c2")).unwrap();
        }
        graph.set_edge("a1", "a2");
        graph.set_edge("b1", "b2");
        graph.set_edge("a2", "b1");
        layout(&mut graph).unwrap();
        approx(
            graph.node("a1").unwrap().y,
            graph.node("a2").unwrap().y.unwrap(),
        );
        assert!(graph.node("a1").unwrap().x < graph.node("a2").unwrap().x);
        approx(
            graph.node("b1").unwrap().x,
            graph.node("b2").unwrap().x.unwrap(),
        );
        assert!(graph.node("b1").unwrap().y < graph.node("b2").unwrap().y);
    }

    #[test]
    fn lays_out_nested_clusters_with_outer_lr_and_inner_tb() {
        let mut graph = graph("LR");
        graph.set_node_with("outer", cluster("LR"));
        graph.set_node_with("inner", cluster("TB"));
        for v in ["n1", "n2"] {
            graph.set_node_with(v, sized(40.0, 40.0));
            graph.set_parent(v, Some("inner")).unwrap();
        }
        graph.set_parent("inner", Some("outer")).unwrap();
        graph.set_node_with("n3", sized(40.0, 40.0));
        graph.set_parent("n3", Some("outer")).unwrap();
        graph.set_edge("n1", "n2");
        graph.set_edge("n2", "n3");
        layout(&mut graph).unwrap();
        approx(
            graph.node("n1").unwrap().x,
            graph.node("n2").unwrap().x.unwrap(),
        );
        assert!(graph.node("n1").unwrap().y < graph.node("n2").unwrap().y);
        approx(
            graph.node("inner").unwrap().y,
            graph.node("n3").unwrap().y.unwrap(),
        );
        assert!(graph.node("inner").unwrap().x < graph.node("n3").unwrap().x);
    }

    #[test]
    fn inherits_the_direction_for_clusters_without_rankdir() {
        let mut graph = graph("RL");
        graph.set_node_with("c1", NodeLabel::default());
        for v in ["a1", "a2"] {
            graph.set_node_with(v, sized(40.0, 40.0));
            graph.set_parent(v, Some("c1")).unwrap();
        }
        graph.set_edge("a1", "a2");
        layout(&mut graph).unwrap();
        approx(
            graph.node("a1").unwrap().y,
            graph.node("a2").unwrap().y.unwrap(),
        );
        assert!(graph.node("a1").unwrap().x > graph.node("a2").unwrap().x);
    }

    #[test]
    fn lays_out_a_bt_cluster_inside_a_tb_graph() {
        let mut graph = graph("TB");
        graph.set_node_with("start", sized(40.0, 40.0));
        graph.set_node_with("c1", cluster("BT"));
        for v in ["n1", "n2", "n3"] {
            graph.set_node_with(v, sized(40.0, 40.0));
            graph.set_parent(v, Some("c1")).unwrap();
        }
        graph.set_node_with("end", sized(40.0, 40.0));
        graph.set_edge("start", "n1");
        graph.set_edge("n1", "n2");
        graph.set_edge("n2", "n3");
        graph.set_edge("n3", "end");
        layout(&mut graph).unwrap();
        assert!(graph.node("n3").unwrap().y < graph.node("n1").unwrap().y);
        approx(
            graph.node("n1").unwrap().x,
            graph.node("n2").unwrap().x.unwrap(),
        );
        approx(
            graph.node("n2").unwrap().x,
            graph.node("n3").unwrap().x.unwrap(),
        );
        assert!(graph.node("start").unwrap().y < graph.node("c1").unwrap().y);
        assert!(graph.node("c1").unwrap().y < graph.node("end").unwrap().y);
    }
}
