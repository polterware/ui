//! Mermaid's `layout-algorithms/dagre`: compound cluster extraction, recursive layout,
//! and SVG coordinate adjustments over the ported `crate::layout::dagre` and graphlib.
//!
//! This module leaves shape measurement to its caller: supply each node's `(width, height)`.
//! TODO: port `extractor`/`copy` subgraph graph construction, `findCommonEdges`,
//! `findNonClusterChild` safe-anchor edge metadata (`fromCluster`/`toCluster`), hierarchy sort,
//! Dagre graph config precedence, edge-label insertion, and self-loop segment fixes.

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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClusterIndex {
    pub clusters: HashMap<String, ClusterInfo>,
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
fn leaf(g: &LayeredGraph, id: &str) -> Option<String> {
    let children = g.children(id);
    if children.is_empty() {
        Some(id.to_string())
    } else {
        children.into_iter().find_map(|c| leaf(g, &c))
    }
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
            let anchor = leaf(g, id);
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
                let v_inside = e.v == *id || is_descendant(&index, &e.v, id);
                let w_inside = e.w == *id || is_descendant(&index, &e.w, id);
                v_inside ^ w_inside
            });
            if let Some(c) = index.clusters.get_mut(id) {
                c.external_connections = outside;
            }
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
        if v != e.v || w != e.w {
            if let Some(label) = g.edge_object(&e).cloned() {
                g.remove_edge_object(&e);
                let _ = g.set_edge_object_with(
                    &Edge::named(v, w, e.name.clone().unwrap_or_default()),
                    label,
                );
            }
        }
    }
    index
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
    #[test]
    fn prepare_requires_every_dimension() {
        let mut g = graph_from_sizes(&HashMap::new(), "TB");
        g.set_node("A");
        assert_eq!(
            prepare_layout(&mut g, &NodeSizes::new()),
            Err(LayoutError::MissingSize("A".into()))
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
}
