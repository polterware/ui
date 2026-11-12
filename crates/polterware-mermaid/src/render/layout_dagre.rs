//! Mermaid's `layout-algorithms/dagre`: compound cluster extraction, recursive layout,
//! and SVG coordinate adjustments over the ported `crate::layout::dagre` and graphlib.
//!
//! This module leaves shape measurement to its caller: supply each node's `(width, height)`.
//! Renderer-specific metadata is returned alongside graphlib because the shared Dagre labels
//! intentionally contain only layout attributes.

use std::collections::HashMap;

use crate::layout::dagre::{self, EdgeLabel, GraphLabel, LayeredGraph, NodeLabel, Point};
use crate::layout::graphlib::{Edge, GraphOptions};

/// Node dimensions supplied by the renderer's measurement pass.
pub type NodeSizes = HashMap<String, (f64, f64)>;

/// Per-cluster details used when external edges need a leaf endpoint.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClusterInfo {
    pub anchor: Option<String>,
    pub external_connections: bool,
    pub descendants: Vec<String>,
    pub data: NodeLabel,
}

#[derive(Default)]
pub struct ClusterIndex {
    pub clusters: HashMap<String, ClusterInfo>,
    pub extracted: HashMap<String, LayeredGraph>,
    pub extracted_parent: HashMap<String, Option<String>>,
    pub edge_cluster_origins: HashMap<Edge, (Option<String>, Option<String>)>,
    /// The painted shape of each cluster id, used when a cluster's rendered box feeds its
    /// size back into the layout.
    pub shapes: HashMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DagreConfig {
    pub node_spacing: Option<f64>,
    pub rank_spacing: Option<f64>,
    pub configured_node_spacing: Option<f64>,
    pub configured_rank_spacing: Option<f64>,
    pub flowchart_node_spacing: Option<f64>,
    pub flowchart_rank_spacing: Option<f64>,
    pub edge_spacing: Option<f64>,
    pub margin_x: f64,
    pub margin_y: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderEdge {
    pub edge: Edge,
    pub label: EdgeLabel,
    pub from_cluster: Option<String>,
    pub to_cluster: Option<String>,
}

/// Collapse the three layout edges of a self-loop into Mermaid's compact SVG route.
pub fn get_edges_to_render(
    g: &LayeredGraph,
    loops: &[SelfLoopSegment],
    y_offset: f64,
    merge_self_loops: bool,
) -> Vec<RenderEdge> {
    let mut result = Vec::new();
    let mut groups: HashMap<&str, Vec<&SelfLoopSegment>> = HashMap::new();
    for segment in loops {
        groups.entry(&segment.logical_id).or_default().push(segment);
    }
    for edge in g.edges() {
        let Some(label) = g.edge_object(&edge).cloned() else {
            continue;
        };
        if let Some(s) = loops.iter().find(|s| s.edge.edge == edge) {
            if merge_self_loops {
                continue;
            } else {
                let mut segment = s.edge.clone();
                offset_edge_points(&mut segment.label, y_offset);
                result.push(segment);
                continue;
            }
        }
        let mut label = label;
        offset_edge_points(&mut label, y_offset);
        result.push(RenderEdge {
            edge,
            label,
            from_cluster: None,
            to_cluster: None,
        });
    }
    for (id, segments) in groups {
        if !merge_self_loops || segments.len() != 3 {
            for s in segments {
                result.push(s.edge.clone())
            }
            continue;
        }
        let mut ordered = segments;
        ordered.sort_by_key(|s| s.order);
        let node_id = ordered[0].edge.edge.v.clone();
        let Some(node) = g.node(&node_id) else {
            for s in ordered {
                result.push(s.edge.clone())
            }
            continue;
        };
        let (x, y) = (node.x.unwrap_or(0.0), node.y.unwrap_or(0.0));
        let hints: Vec<Point> = ordered
            .iter()
            .flat_map(|s| [s.edge.edge.v.as_str(), s.edge.edge.w.as_str()])
            .filter(|n| *n != node_id)
            .filter_map(|n| g.node(n))
            .filter_map(|n| Some(Point::new(n.x?, n.y?)))
            .collect();
        let center = if hints.is_empty() {
            Point::new(x, y)
        } else {
            Point::new(
                hints.iter().map(|p| p.x).sum::<f64>() / hints.len() as f64,
                hints.iter().map(|p| p.y).sum::<f64>() / hints.len() as f64,
            )
        };
        let dx = center.x - x;
        let dy = center.y - y;
        let rankdir = g.graph().and_then(|a| a.rankdir.as_deref()).unwrap_or("TB");
        let side = if hints.is_empty() {
            match rankdir {
                "BT" => "bottom",
                "LR" => "right",
                "RL" => "left",
                _ => "top",
            }
        } else if dx.abs() > dy.abs() {
            if dx > 0.0 { "right" } else { "left" }
        } else if dy > 0.0 {
            "bottom"
        } else {
            "top"
        };
        let span = ordered[1]
            .edge
            .label
            .width()
            .max(node.width * 0.35)
            .clamp(36.0, node.width.mul_add(0.8, 0.0).clamp(36.0, 100.0));
        let depth = (node.width.min(node.height) * 0.45).clamp(24.0, 48.0);
        let hw = node.width / 2.0;
        let hh = node.height / 2.0;
        let pts = match side {
            "bottom" => vec![
                Point::new(x - span / 2.0, y + hh),
                Point::new(x - span / 2.0, y + hh + depth),
                Point::new(x + span / 2.0, y + hh + depth),
                Point::new(x + span / 2.0, y + hh),
            ],
            "right" => vec![
                Point::new(x + hw, y - span / 2.0),
                Point::new(x + hw + depth, y - span / 2.0),
                Point::new(x + hw + depth, y + span / 2.0),
                Point::new(x + hw, y + span / 2.0),
            ],
            "left" => vec![
                Point::new(x - hw, y - span / 2.0),
                Point::new(x - hw - depth, y - span / 2.0),
                Point::new(x - hw - depth, y + span / 2.0),
                Point::new(x - hw, y + span / 2.0),
            ],
            _ => vec![
                Point::new(x - span / 2.0, y - hh),
                Point::new(x - span / 2.0, y - hh - depth),
                Point::new(x + span / 2.0, y - hh - depth),
                Point::new(x + span / 2.0, y - hh),
            ],
        };
        let mut merged = ordered[1].edge.clone();
        merged.edge = Edge::new(&ordered[0].edge.edge.v, &ordered[2].edge.edge.w);
        merged.label = ordered[1].edge.label.clone();
        merged.label.points = Some(
            pts.iter()
                .map(|p| Point::new(p.x, p.y + y_offset))
                .collect(),
        );
        merged.label.x = Some(match side {
            "right" => {
                pts.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max)
                    + merged.label.width() / 2.0
                    + 4.0
            }
            "left" => {
                pts.iter().map(|p| p.x).fold(f64::INFINITY, f64::min)
                    - merged.label.width() / 2.0
                    - 4.0
            }
            _ => x,
        });
        merged.label.y = Some(match side {
            "bottom" => {
                pts.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max)
                    + merged.label.height() / 2.0
                    + 4.0
            }
            "right" => y,
            "left" => y,
            _ => {
                pts.iter().map(|p| p.y).fold(f64::INFINITY, f64::min)
                    - merged.label.height() / 2.0
                    - 4.0
            }
        });
        merged.edge.name = Some(id.into());
        result.push(merged)
    }
    result
}

pub fn get_render_edges(
    g: &LayeredGraph,
    index: &ClusterIndex,
    loops: &[SelfLoopSegment],
    y_offset: f64,
    merge_self_loops: bool,
) -> Vec<RenderEdge> {
    let mut edges = get_edges_to_render(g, loops, y_offset, merge_self_loops);
    for edge in &mut edges {
        if let Some((from, to)) = index.edge_cluster_origins.get(&edge.edge) {
            edge.from_cluster = from.clone();
            edge.to_cluster = to.clone();
        }
    }
    edges
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutResult {
    pub nodes: Vec<(String, PositionedNode)>,
    pub edges: Vec<RenderEdge>,
}
pub fn apply_layout_result(
    g: &LayeredGraph,
    index: &ClusterIndex,
    loops: &[SelfLoopSegment],
    margins: TitleMargins,
    merge_self_loops: bool,
) -> LayoutResult {
    let mut result = LayoutResult::default();
    for id in sort_nodes_by_hierarchy(g) {
        let Some(node) = g.node(&id) else { continue };
        let cluster = !g.children(&id).is_empty() || index.extracted.contains_key(&id);
        let width = node.width;
        let height = node.height + if cluster { margins.total() } else { 0.0 };
        let diff = 0.0;
        let position = position_dagre_node(node, diff, cluster, margins)
            .unwrap_or(Point::new(node.x.unwrap_or(0.0), node.y.unwrap_or(0.0)));
        result.nodes.push((
            id,
            PositionedNode {
                x: position.x,
                y: position.y,
                width,
                height,
                diff,
            },
        ));
    }
    result.edges = get_render_edges(g, index, loops, margins.total() / 2.0, merge_self_loops);
    result
}

#[derive(Clone, Debug, PartialEq)]
pub struct SelfLoopSegment {
    pub logical_id: String,
    pub order: u8,
    pub edge: RenderEdge,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TitleMargins {
    pub top: f64,
    pub bottom: f64,
}
impl TitleMargins {
    pub fn total(self) -> f64 {
        self.top + self.bottom
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PositionedNode {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub diff: f64,
}
impl PositionedNode {
    pub fn transform(self, is_cluster: bool) -> Point {
        if is_cluster {
            Point::new(
                self.x + self.diff - self.width / 2.0,
                self.y - self.height / 2.0 - 8.0,
            )
        } else {
            Point::new(self.x, self.y)
        }
    }
}

/// Failure while preparing the compound graph or applying its final coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum LayoutError {
    MissingSize(String),
    MissingNode(String),
    Dagre(String),
}
impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSize(id) => write!(f, "Missing dimensions for node {id}"),
            Self::MissingNode(id) => write!(f, "Graph does not have node: {id}"),
            Self::Dagre(e) => f.write_str(e),
        }
    }
}
impl std::error::Error for LayoutError {}

fn descendants(g: &LayeredGraph, id: &str, out: &mut Vec<String>) {
    for child in g.children(id) {
        out.push(child.clone());
        descendants(g, &child, out);
    }
}
pub fn extract_descendants(g: &LayeredGraph, id: &str) -> Vec<String> {
    let mut result = Vec::new();
    descendants(g, id, &mut result);
    result
}
fn leaf(g: &LayeredGraph, id: &str) -> Option<String> {
    let children = g.children(id);
    if children.is_empty() {
        Some(id.to_string())
    } else {
        children.into_iter().find_map(|c| leaf(g, &c))
    }
}
fn common_edges(g: &LayeredGraph, id1: &str, id2: &str) -> Vec<(String, String)> {
    let left: Vec<_> = g
        .edges()
        .into_iter()
        .filter(|e| e.v == id1 || e.w == id1)
        .map(|e| {
            (
                if e.v == id1 { id2.to_string() } else { e.v },
                if e.w == id1 { id1.to_string() } else { e.w },
            )
        })
        .collect();
    let right: Vec<_> = g
        .edges()
        .into_iter()
        .filter(|e| e.v == id2 || e.w == id2)
        .map(|e| (e.v, e.w))
        .collect();
    left.into_iter().filter(|e| right.contains(e)).collect()
}
/// Upstream `findNonClusterChild`, preserving its first unconnected descendant preference.
pub fn find_non_cluster_child(g: &LayeredGraph, id: &str, cluster_id: &str) -> Option<String> {
    let children = g.children(id);
    if children.is_empty() {
        return Some(id.to_string());
    }
    let mut reserve = None;
    for child in children {
        if let Some(candidate) = find_non_cluster_child(g, &child, cluster_id) {
            if common_edges(g, cluster_id, &candidate).is_empty() {
                return Some(candidate);
            }
            reserve = Some(candidate)
        }
    }
    reserve
}
fn is_in_extractable(g: &LayeredGraph, index: &ClusterIndex, node: &str, root: &str) -> bool {
    let mut parent = g.parent(node);
    while let Some(p) = parent {
        if p == root {
            break;
        }
        if index
            .clusters
            .get(p)
            .is_some_and(|c| !c.external_connections)
        {
            return true;
        }
        parent = g.parent(p)
    }
    false
}
fn safe_anchor(
    g: &LayeredGraph,
    index: &ClusterIndex,
    cluster: &str,
    excluded: Option<&str>,
) -> Option<String> {
    for child in g.children(cluster) {
        if excluded == Some(child.as_str())
            || excluded.is_some_and(|e| {
                index
                    .clusters
                    .get(e)
                    .is_some_and(|c| c.descendants.contains(&child))
            })
        {
            continue;
        }
        if let Some(candidate) = find_non_cluster_child(g, &child, cluster)
            && !is_in_extractable(g, index, &candidate, cluster)
        {
            return Some(candidate);
        }
    }
    None
}
fn is_descendant(info: &ClusterIndex, node: &str, ancestor: &str) -> bool {
    info.clusters
        .get(ancestor)
        .is_some_and(|c| c.descendants.iter().any(|d| d == node))
}

/// Identify clusters, re-anchor edges on leaf nodes, then extract isolated nested clusters.
pub fn adjust_clusters_and_edges(g: &mut LayeredGraph) -> ClusterIndex {
    let mut index = ClusterIndex::default();
    let ids = g.nodes();
    for id in &ids {
        if !g.children(id).is_empty() {
            let mut ds = Vec::new();
            descendants(g, id, &mut ds);
            let anchor = find_non_cluster_child(g, id, id).or_else(|| leaf(g, id));
            let data = g.node(id).cloned().unwrap_or_default();
            index.clusters.insert(
                id.clone(),
                ClusterInfo {
                    anchor,
                    external_connections: false,
                    descendants: ds,
                    data,
                },
            );
        }
    }
    for id in &ids {
        if index.clusters.contains_key(id) {
            let outside = g.edges().iter().any(|e| {
                let v_inside = is_descendant(&index, &e.v, id);
                let w_inside = is_descendant(&index, &e.w, id);
                v_inside ^ w_inside
            });
            if let Some(c) = index.clusters.get_mut(id) {
                c.external_connections = outside;
            }
        }
    }
    let cluster_ids: Vec<_> = index.clusters.keys().cloned().collect();
    for id in cluster_ids {
        let anchor = index.clusters[&id].anchor.clone();
        let parent = anchor.as_deref().and_then(|a| g.parent(a));
        if parent != Some(id.as_str())
            && parent.is_some_and(|p| {
                index
                    .clusters
                    .get(p)
                    .is_some_and(|c| !c.external_connections)
            })
            && let Some(c) = index.clusters.get_mut(&id)
        {
            c.anchor = parent.map(str::to_string)
        }
        if g.edges().iter().any(|e| e.v == id)
            && index.clusters[&id].external_connections
            && let Some(a) = anchor.as_deref()
            && is_in_extractable(g, &index, a, &id)
            && let Some(candidate) = safe_anchor(g, &index, &id, g.parent(a))
            && let Some(c) = index.clusters.get_mut(&id)
        {
            c.anchor = Some(candidate)
        }
    }
    let edges = g.edges();
    for e in edges {
        let v = index
            .clusters
            .get(&e.v)
            .filter(|c| c.external_connections)
            .and_then(|c| c.anchor.clone())
            .unwrap_or_else(|| e.v.clone());
        let w = index
            .clusters
            .get(&e.w)
            .filter(|c| c.external_connections)
            .and_then(|c| c.anchor.clone())
            .unwrap_or_else(|| e.w.clone());
        if (v != e.v || w != e.w)
            && let Some(label) = g.edge_object(&e).cloned()
        {
            g.remove_edge_object(&e);
            let rewritten = Edge {
                v,
                w,
                name: e.name.clone(),
            };
            index.edge_cluster_origins.insert(
                rewritten.clone(),
                (
                    if rewritten.v != e.v {
                        Some(e.v.clone())
                    } else {
                        None
                    },
                    if rewritten.w != e.w {
                        Some(e.w.clone())
                    } else {
                        None
                    },
                ),
            );
            let _ = g.set_edge_object_with(&rewritten, label);
        }
    }
    let _ = extractor(g, &mut index, 0);
    index
}

/// Copy the subgraph rooted at `root` to a scratch graph and remove copied leaves upstream-style.
pub fn copy_subgraph(root: &str, g: &mut LayeredGraph) -> LayeredGraph {
    let mut out = LayeredGraph::with_options(GraphOptions {
        multigraph: true,
        compound: true,
        ..GraphOptions::default()
    });
    fn visit(id: &str, g: &mut LayeredGraph, out: &mut LayeredGraph, root: &str) {
        let mut children = g.children(id);
        if id != root {
            children.push(id.to_string())
        }
        for child in children {
            if !g.children(&child).is_empty() {
                visit(&child, g, out, root);
                continue;
            }
            if let Some(n) = g.node(&child).cloned() {
                out.set_node_with(child.clone(), n);
            }
            if let Some(parent) = g.parent(&child)
                && parent != root
            {
                let _ = out.set_parent(&child, Some(parent));
            }
            let descendant_set = {
                let mut ds = Vec::new();
                descendants(g, root, &mut ds);
                ds
            };
            for e in g.edges() {
                if e.v != root
                    && e.w != root
                    && (descendant_set.contains(&e.v) || descendant_set.contains(&e.w))
                    && let Some(l) = g.edge_object(&e).cloned()
                {
                    let _ = out.set_edge_object_with(&e, l);
                }
            }
            g.remove_node(&child);
        }
    }
    visit(root, g, &mut out, root);
    out
}

/// Recursively extract isolated clusters into independent compound layout graphs (depth cap 10).
pub fn extractor(
    g: &mut LayeredGraph,
    index: &mut ClusterIndex,
    depth: usize,
) -> Result<(), LayoutError> {
    extractor_owned(g, index, depth, None)
}
fn extractor_owned(
    g: &mut LayeredGraph,
    index: &mut ClusterIndex,
    depth: usize,
    owner: Option<String>,
) -> Result<(), LayoutError> {
    if depth > 10 {
        return Ok(());
    }
    let ids = g.nodes();
    if !ids.iter().any(|id| !g.children(id).is_empty()) {
        return Ok(());
    }
    for id in ids {
        let Some(info) = index.clusters.get(&id).cloned() else {
            continue;
        };
        if info.external_connections || g.children(&id).is_empty() {
            continue;
        }
        // The scratch graph keeps the cluster itself as its compound root; the recursive pass
        // must not extract that root again.
        if owner.as_deref() == Some(id.as_str()) {
            continue;
        }
        let rankdir = g
            .graph()
            .and_then(|x| x.rankdir.as_deref())
            .map(|d| if d == "TB" { "LR" } else { "TB" })
            .unwrap_or("LR");
        // Upstream widens the spacing of an extracted cluster by 25 ranks and keeps the
        // parent's node spacing.
        let parent = g.graph();
        let ranksep = parent.and_then(|x| x.ranksep).unwrap_or(50.0) + 25.0;
        let nodesep = parent.and_then(|x| x.nodesep).unwrap_or(50.0);
        let mut child = copy_subgraph(&id, g);
        // Keep the cluster itself as the compound root of its scratch graph, as the recursive
        // renderer does: dagre then sizes it from its border nodes (children plus node and rank
        // spacing and every edge label dummy), which is the box the composite is painted in.
        child.set_node_with(id.clone(), info.data.clone());
        for node in child.nodes() {
            if node != id && child.parent(&node).is_none() {
                let _ = child.set_parent(&node, Some(id.as_str()));
            }
        }
        let mut label = GraphLabel {
            rankdir: Some(info.data.rankdir.clone().unwrap_or_else(|| rankdir.into())),
            nodesep: Some(nodesep),
            ranksep: Some(ranksep),
            marginx: Some(8.0),
            marginy: Some(8.0),
            ..GraphLabel::default()
        };
        child.set_graph(label.clone());
        label.rankdir = None;
        index.extracted.insert(id.clone(), child);
        index.extracted_parent.insert(id.clone(), owner.clone());
        g.set_node_with(id.clone(), info.data);
    }
    let recurse: Vec<_> = index
        .extracted
        .keys()
        .filter(|id| index.extracted_parent.get(*id) == Some(&owner))
        .cloned()
        .collect();
    for id in recurse {
        if let Some(mut child) = index.extracted.remove(&id) {
            extractor_owned(&mut child, index, depth + 1, Some(id.clone()))?;
            index.extracted.insert(id, child);
        }
    }
    Ok(())
}

/// Breadth by parent preorder, matching Mermaid's recursive `sorter` order.
pub fn sort_nodes_by_hierarchy(g: &LayeredGraph) -> Vec<String> {
    fn walk(g: &LayeredGraph, ids: Vec<String>, out: &mut Vec<String>) {
        for id in ids {
            out.push(id.clone());
            walk(g, g.children(&id), out)
        }
    }
    let mut out = Vec::new();
    walk(g, g.children("\0"), &mut out);
    out
}

/// Mermaid graph options: explicit top-level values, diagram values, then flowchart defaults.
pub fn configure_graph(g: &mut LayeredGraph, direction: &str, config: &DagreConfig) {
    let choose = |a: Option<f64>, b: Option<f64>, c: Option<f64>| a.or(b).or(c);
    g.set_graph(GraphLabel {
        rankdir: Some(direction.into()),
        nodesep: Some(
            choose(
                config.configured_node_spacing,
                config.node_spacing,
                config.flowchart_node_spacing,
            )
            .unwrap_or(50.0),
        ),
        ranksep: Some(
            choose(
                config.configured_rank_spacing,
                config.rank_spacing,
                config.flowchart_rank_spacing,
            )
            .unwrap_or(50.0),
        ),
        edgesep: config.edge_spacing,
        marginx: Some(config.margin_x),
        marginy: Some(config.margin_y),
        ..GraphLabel::default()
    });
}

/// Turn edge labels into Dagre dummy nodes without modifying their original logical edge data.
pub fn add_edge_label_dummy(g: &mut LayeredGraph, edge: &Edge, label: &EdgeLabel) -> bool {
    if label.width.unwrap_or(0.0) == 0.0 && label.height.unwrap_or(0.0) == 0.0 {
        return false;
    }
    let Some(edge) = g.edge_object_mut(edge) else {
        return false;
    };
    edge.width = label.width;
    edge.height = label.height;
    edge.labelpos = label.labelpos.clone();
    edge.labeloffset = label.labeloffset;
    true
}

/// Add Dagre-compatible three segment self-loop dummies; rendering can merge by logical id.
pub fn add_self_loop(
    g: &mut LayeredGraph,
    node: &str,
    id: &str,
    label: &EdgeLabel,
) -> Vec<SelfLoopSegment> {
    let mut out = Vec::new();
    let dummy1 = format!("{node}---{node}---1");
    let dummy2 = format!("{node}---{node}---2");
    for dummy in [&dummy1, &dummy2] {
        g.set_node_with(
            dummy.clone(),
            NodeLabel {
                width: 10.0,
                height: 10.0,
                dummy: Some("selfedge".into()),
                ..NodeLabel::default()
            },
        );
    }
    for order in 0..3u8 {
        let (v, w) = match order {
            0 => (node.to_string(), dummy1.clone()),
            1 => (dummy1.clone(), dummy2.clone()),
            _ => (dummy2.clone(), node.to_string()),
        };
        let mut edge_label = label.clone();
        if order != 1 {
            edge_label.width = Some(0.0);
            edge_label.height = Some(0.0)
        }
        let e = Edge::named(v, w, format!("{id}-{}", order));
        let _ = g.set_edge_object_with(&e, edge_label.clone());
        out.push(SelfLoopSegment {
            logical_id: id.into(),
            order,
            edge: RenderEdge {
                edge: e,
                label: edge_label,
                from_cluster: None,
                to_cluster: None,
            },
        })
    }
    out
}

/// Validate Dagre's invariant that only leaves can be endpoints of edges.
pub fn validate_clusters(g: &LayeredGraph) -> bool {
    g.edges()
        .iter()
        .all(|e| g.children(&e.v).is_empty() && g.children(&e.w).is_empty())
}

/// Build a compound Dagre graph using measured dimensions and Mermaid's cluster parents.
pub fn prepare_layout(
    g: &mut LayeredGraph,
    sizes: &NodeSizes,
) -> Result<ClusterIndex, LayoutError> {
    for id in g.nodes() {
        let (w, h) = sizes
            .get(&id)
            .copied()
            .ok_or_else(|| LayoutError::MissingSize(id.clone()))?;
        let n = g
            .node_mut(&id)
            .ok_or_else(|| LayoutError::MissingNode(id.clone()))?;
        n.width = w;
        n.height = h;
    }
    Ok(adjust_clusters_and_edges(g))
}

/// Recursively run Dagre on the graph and every compound subgraph. The core Dagre port itself
/// recursively positions cluster contents; this wrapper exposes the Mermaid call boundary.
pub fn run_layout(g: &mut LayeredGraph) -> Result<(), LayoutError> {
    run_layout_recursive(g, &mut ClusterIndex::default(), 0, None)
}
pub fn run_prepared_layout(
    g: &mut LayeredGraph,
    index: &mut ClusterIndex,
) -> Result<(), LayoutError> {
    run_layout_recursive(g, index, 0, None)
}

fn run_layout_recursive(
    g: &mut LayeredGraph,
    index: &mut ClusterIndex,
    depth: usize,
    owner: Option<String>,
) -> Result<(), LayoutError> {
    if depth > 10 {
        return Ok(());
    }
    let ids: Vec<_> = index
        .extracted
        .keys()
        .filter(|id| index.extracted_parent.get(*id) == Some(&owner))
        .cloned()
        .collect();
    for id in ids {
        if let Some(mut child) = index.extracted.remove(&id) {
            run_layout_recursive(&mut child, index, depth + 1, Some(id.clone()))?;
            let mut bounds: Option<(f64, f64, f64, f64)> = None;
            for n in child.nodes() {
                if let Some(n) = child.node(&n)
                    && let (Some(x), Some(y)) = (n.x, n.y)
                {
                    let b = (
                        x - n.width / 2.0,
                        y - n.height / 2.0,
                        x + n.width / 2.0,
                        y + n.height / 2.0,
                    );
                    bounds = Some(bounds.map_or(b, |(x0, y0, x1, y1)| {
                        (x0.min(b.0), y0.min(b.1), x1.max(b.2), y1.max(b.3))
                    }));
                }
            }
            if let Some((x1, y1, x2, y2)) = bounds
                && let Some(n) = g.node_mut(&id)
            {
                // The scratch graph's root node is the cluster itself: dagre sized it from its
                // border nodes, which is the box the composite is painted in.
                if let Some(root) = child.node(&id) {
                    // The painted composite's bounding box: the cluster rect dagre sized, plus
                    // the two pixels the SVG title sits above it (`y + 1 - 3` in
                    // `roundedWithTitle`). A concurrency region is a `divider`, which paints no
                    // title and keeps the bare rect.
                    n.width = root.width;
                    let titled = index.shapes.get(&id).map(String::as_str) != Some("divider");
                    n.height = root.height + if titled { 2.0 } else { 0.0 };
                } else {
                    n.width = x2 - x1 + 16.0;
                    n.height = y2 - y1 + 16.0;
                }
            }
            index.extracted.insert(id, child);
        }
    }
    dagre::layout(g).map_err(|e| LayoutError::Dagre(e.to_string()))
}

/// Translate Dagre's center coordinates to Mermaid's render coordinates. Cluster y positions
/// reserve title space; edge points use the same offset so endpoints remain aligned.
pub fn position_node(n: &NodeLabel, y_offset: f64, is_cluster: bool) -> Option<Point> {
    Some(Point::new(
        n.x?,
        n.y? + if is_cluster { y_offset } else { y_offset / 2.0 },
    ))
}
pub fn position_cluster(n: &NodeLabel, title_height: f64) -> Option<Point> {
    position_node(n, title_height, true)
}
pub fn position_dagre_node(
    n: &NodeLabel,
    diff: f64,
    is_cluster: bool,
    margins: TitleMargins,
) -> Option<Point> {
    let positioned = PositionedNode {
        x: n.x?,
        y: n.y?
            + if is_cluster {
                margins.total()
            } else {
                margins.total() / 2.0
            },
        width: n.width,
        height: n.height + if is_cluster { margins.total() } else { 0.0 },
        diff,
    };
    Some(positioned.transform(is_cluster))
}
pub fn offset_edge_points(edge: &mut EdgeLabel, y_offset: f64) {
    if let Some(points) = edge.points.as_mut() {
        for p in points {
            p.y += y_offset;
        }
    }
    if let Some(y) = edge.y.as_mut() {
        *y += y_offset;
    }
}

/// Make a configured compound graph with dimensions supplied by `sizes`.
pub fn graph_from_sizes(sizes: &NodeSizes, rankdir: &str) -> LayeredGraph {
    let mut g = LayeredGraph::with_options(GraphOptions::compound());
    let mut gl = GraphLabel::default();
    gl.rankdir = Some(rankdir.into());
    g.set_graph_label(Some(gl));
    for (id, (width, height)) in sizes {
        g.set_node_with(
            id.clone(),
            NodeLabel {
                width: *width,
                height: *height,
                ..NodeLabel::default()
            },
        );
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;
    const UPSTREAM_FIXTURES: &str = r#"[{"d":0,"n":[["f0A",67.5,154],["f0B",67.5,219],["f0a",67.5,37],["f0b",68,188],["f0c",68,249],["f0X",169.5,98]],"e":[["f0e1",[[67.5,46],[67.5,66],[152,92.51]]],["f0e2",[[152,104.207],[68,134],[68,158],[68,178]]],["f0e3",[[68,198],[68,218],[68,238]]]]},{"d":1,"n":[["f1A",174,66.25],["f1B",251.5,66.25],["f1a",41,66.25],["f1b",217,68],["f1c",285,68],["f1X",112,162]],"e":[["f1e1",[[53.5,66.25],[74,66.25],[107.238,150]]],["f1e2",[[117.681,150],[156.5,68],[183.5,68],[204,68]]],["f1e3",[[230,68],[250.5,68],[271,68]]]]},{"d":2,"n":[["f2A",68,160],["f2B",68,227],["f2a",68,38],["f2b",68,195],["f2c",68,259],["f2X",171.5,101]],"e":[["f2e1",[[68,47],[68,68],[154,95.42]]],["f2e2",[[154,107.256],[68,138],[68,163],[68,184]]],["f2e3",[[68,206],[68,227],[68,248]]]]},{"d":3,"n":[["f3A",181,67],["f3B",262.5,67],["f3a",43,67],["f3b",227,68],["f3c",297,68],["f3X",117,164.5]],"e":[["f3e1",[[56.5,67],[78,67],[112.2,152.5]]],["f3e2",[[122.782,152.5],[163.5,68],[192.5,68],[214,68]]],["f3e3",[[240,68],[261.5,68],[283,68]]]]},{"d":4,"n":[["f4A",69,166],["f4B",69,235],["f4a",69,39],["f4b",69,202],["f4c",69,269],["f4X",174.5,104]],"e":[["f4e1",[[69,48],[69,70],[157,98.36]]],["f4e2",[[157,110.303],[69,142],[69,168],[69,190]]],["f4e3",[[69,214],[69,236],[69,258]]]]},{"d":5,"n":[["f5A",188,68],["f5B",273.5,68],["f5a",45,68],["f5b",237,68],["f5c",309,68],["f5X",122,167.5]],"e":[["f5e1",[[59.5,68],[82,68],[117.176,155.5]]],["f5e2",[[127.849,155.5],[170.5,68],[201.5,68],[224,68]]],["f5e3",[[250,68],[272.5,68],[295,68]]]]},{"d":6,"n":[["f6A",70,172],["f6B",70,243],["f6a",70,40],["f6b",70,209],["f6c",70,279],["f6X",177.5,107]],"e":[["f6e1",[[70,49],[70,72],[160,101.302]]],["f6e2",[[160,113.349],[70,146],[70,173],[70,196]]],["f6e3",[[70,222],[70,245],[70,268]]]]},{"d":7,"n":[["f7A",195,70],["f7B",284.5,70],["f7a",47,70],["f7b",247,70],["f7c",321,70],["f7X",127,172.5]],"e":[["f7e1",[[62.5,70],[86,70],[122.2,160.5]]],["f7e2",[[132.912,160.5],[177.5,70],[210.5,70],[234,70]]],["f7e3",[[260,70],[283.5,70],[307,70]]]]},{"d":8,"n":[["f8A",71,178],["f8B",71,251],["f8a",71,41],["f8b",71,216],["f8c",71,289],["f8X",180.5,110]],"e":[["f8e1",[[71,50],[71,74],[163,104.247]]],["f8e2",[[163,116.393],[71,150],[71,178],[71,202]]],["f8e3",[[71,230],[71,254],[71,278]]]]},{"d":9,"n":[["f9A",202,72],["f9B",295.5,72],["f9a",49,72],["f9b",257,72],["f9c",333,72],["f9X",132,177.5]],"e":[["f9e1",[[65.5,72],[90,72],[127.223,165.5]]],["f9e2",[[137.972,165.5],[184.5,72],[219.5,72],[244,72]]],["f9e3",[[270,72],[294.5,72],[319,72]]]]}]"#;
    #[test]
    fn prepare_requires_every_dimension() {
        let mut g = graph_from_sizes(&HashMap::new(), "TB");
        g.set_node("A");
        assert!(
            matches!(prepare_layout(&mut g, &NodeSizes::new()), Err(LayoutError::MissingSize(id)) if id == "A")
        );
    }
    #[test]
    fn external_cluster_edges_are_reanchored() {
        let mut g = graph_from_sizes(
            &[
                ("c".into(), (80., 60.)),
                ("a".into(), (20., 20.)),
                ("b".into(), (20., 20.)),
                ("x".into(), (20., 20.)),
            ]
            .into(),
            "TB",
        );
        g.set_parent("a", Some("c")).unwrap();
        g.set_parent("b", Some("c")).unwrap();
        g.set_edge_with("a", "x", EdgeLabel::default());
        let info = adjust_clusters_and_edges(&mut g);
        assert!(info.clusters["c"].external_connections);
        assert_eq!(g.edges()[0].v, "a");
    }
    #[test]
    fn edge_point_fix_applies_offset() {
        let mut e = EdgeLabel {
            points: Some(vec![Point::new(1., 2.)]),
            y: Some(3.),
            ..EdgeLabel::default()
        };
        offset_edge_points(&mut e, 5.);
        assert_eq!(e.points.unwrap()[0].y, 7.);
        assert_eq!(e.y, Some(8.));
    }
    #[test]
    fn positions_apply_title_margin() {
        let n = NodeLabel {
            x: Some(4.),
            y: Some(8.),
            ..NodeLabel::default()
        };
        assert_eq!(position_cluster(&n, 6.), Some(Point::new(4., 14.)));
    }

    /// Ten nested flowcharts captured from Mermaid 12.3.0, which lays them out with
    /// dagre-d3-es 7.0.14: the node and edge positions must match the capture.
    #[test]
    fn upstream_node_fixtures_match_ten_nested_flowcharts() {
        let fixtures: serde_json::Value = serde_json::from_str(UPSTREAM_FIXTURES).unwrap();
        for fixture in fixtures.as_array().unwrap() {
            let k = fixture["d"].as_u64().unwrap() as usize;
            let p = format!("f{k}");
            let mut graph = LayeredGraph::with_options(GraphOptions {
                multigraph: true,
                compound: true,
                ..GraphOptions::default()
            });
            graph.set_graph(GraphLabel {
                rankdir: Some(if k % 2 == 1 { "LR" } else { "TB" }.into()),
                nodesep: Some(30.0 + k as f64),
                ranksep: Some(40.0 + k as f64),
                marginx: Some(8.0),
                marginy: Some(8.0),
                ..GraphLabel::default()
            });
            for (id, w, h) in [
                (format!("{p}A"), 55.0 + k as f64, 40.0),
                (format!("{p}B"), 48.0, 36.0),
                (format!("{p}a"), 24.0 + k as f64, 18.0),
                (format!("{p}b"), 26.0, 20.0 + k as f64),
                (format!("{p}c"), 28.0, 22.0),
                (format!("{p}X"), 35.0, 24.0),
            ] {
                graph.set_node_with(
                    id,
                    NodeLabel {
                        width: w,
                        height: h,
                        ..NodeLabel::default()
                    },
                );
            }
            graph
                .set_parent(format!("{p}B").as_str(), Some(format!("{p}A").as_str()))
                .unwrap();
            graph
                .set_parent(format!("{p}a").as_str(), Some(format!("{p}A").as_str()))
                .unwrap();
            graph
                .set_parent(format!("{p}b").as_str(), Some(format!("{p}B").as_str()))
                .unwrap();
            graph
                .set_parent(format!("{p}c").as_str(), Some(format!("{p}B").as_str()))
                .unwrap();
            for (v, w, name, width, height) in [
                (format!("{p}a"), format!("{p}X"), format!("{p}e1"), 0.0, 0.0),
                (
                    format!("{p}X"),
                    format!("{p}b"),
                    format!("{p}e2"),
                    12.0 + k as f64,
                    8.0,
                ),
                (format!("{p}b"), format!("{p}c"), format!("{p}e3"), 0.0, 0.0),
            ] {
                graph
                    .set_edge_object_with(
                        &Edge::named(v, w, name),
                        EdgeLabel {
                            width: Some(width),
                            height: Some(height),
                            ..EdgeLabel::default()
                        },
                    )
                    .unwrap();
            }
            adjust_clusters_and_edges(&mut graph);
            dagre::layout(&mut graph).unwrap();
            for expected in fixture["n"].as_array().unwrap() {
                let id = expected[0].as_str().unwrap();
                let n = graph.node(id).unwrap();
                assert!(
                    (n.x.unwrap() - expected[1].as_f64().unwrap()).abs() < 0.01,
                    "fixture {k} node {id} x: actual {:?}, expected {:?}",
                    n.x,
                    expected[1]
                );
                assert!(
                    (n.y.unwrap() - expected[2].as_f64().unwrap()).abs() < 0.01,
                    "fixture {k} node {id} y"
                );
            }
            for expected in fixture["e"].as_array().unwrap() {
                let name = expected[0].as_str().unwrap();
                let edge = graph
                    .edges()
                    .into_iter()
                    .find(|e| e.name.as_deref() == Some(name))
                    .unwrap();
                let actual = graph.edge_object(&edge).unwrap().points.as_ref().unwrap();
                let wanted = expected[1].as_array().unwrap();
                assert_eq!(
                    actual.len(),
                    wanted.len(),
                    "fixture {k} edge {name} point count"
                );
                for (a, b) in actual.iter().zip(wanted) {
                    assert!(
                        (a.x - b[0].as_f64().unwrap()).abs() < 0.01,
                        "fixture {k} edge {name} x"
                    );
                    assert!(
                        (a.y - b[1].as_f64().unwrap()).abs() < 0.01,
                        "fixture {k} edge {name} y"
                    );
                }
            }
        }
    }
}
