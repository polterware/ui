//! Port of `dagre-d3-es/src/dagre/util.js` (MIT, Copyright (c) 2012-2014 Chris Pettitt).
//!
//! The helpers keep upstream's iteration order. `applyWithChunking` only exists to
//! stay under JavaScript's argument limit; here the same folds run directly.
//! Upstream's `time` is logging instrumentation and is not ported.

use super::{
    super::graphlib::{Graph, GraphOptions},
    DummyKind, EdgeLabel, GraphLabel, LayeredGraph, LayoutError, NodeLabel, Point,
};

/// A rank by rank matrix of node ids, upstream's `buildLayerMatrix` result.
pub type LayerMatrix = Vec<Vec<String>>;

/// Adds a dummy node of `kind` and returns its id. Starts at `name` with a unique
/// suffix and repeats until the id is free, like upstream's `addDummyNode`.
pub fn add_dummy_node(
    graph: &mut LayeredGraph,
    kind: &DummyKind,
    mut label: NodeLabel,
    name: &str,
) -> String {
    let mut v;
    loop {
        v = unique_id(name);
        if !graph.has_node(&v) {
            break;
        }
    }
    label.dummy = Some(kind.to_string());
    graph.set_node_with(v.clone(), label);
    v
}

/// A new graph with only simple edges, aggregating the labels of multi-edges.
pub fn simplify(graph: &LayeredGraph) -> LayeredGraph {
    let mut simplified = LayeredGraph::new();
    simplified.set_graph_label(graph.graph().cloned());
    for v in graph.nodes() {
        simplified.set_node_label(v.clone(), graph.node(&v).cloned());
    }
    for e in graph.edges() {
        let label = graph.edge_object(&e).cloned().unwrap_or_default();
        let (previous_weight, previous_minlen) = match simplified.edge(&e.v, &e.w) {
            Some(previous) => (previous.weight(), previous.minlen()),
            None => (0.0, 1.0),
        };
        simplified.set_edge_with(
            &e.v,
            &e.w,
            EdgeLabel {
                weight: Some(previous_weight + label.weight()),
                minlen: Some(previous_minlen.max(label.minlen())),
                ..EdgeLabel::default()
            },
        );
    }
    simplified
}

/// A copy of `graph` without compound nodes: nodes that have children are left out.
pub fn as_non_compound_graph(graph: &LayeredGraph) -> LayeredGraph {
    let mut simplified = Graph::with_options(GraphOptions {
        multigraph: graph.is_multigraph(),
        ..GraphOptions::default()
    });
    simplified.set_graph_label(graph.graph().cloned());
    for v in graph.nodes() {
        if graph.children(&v).is_empty() {
            simplified.set_node_label(v.clone(), graph.node(&v).cloned());
        }
    }
    for e in graph.edges() {
        simplified
            .set_edge_object_label(&e, graph.edge_object(&e).cloned())
            .expect("the copy keeps the source's multigraph option");
    }
    simplified
}

/// For each node, the weights of the edges to its successors.
pub fn successor_weights(graph: &LayeredGraph) -> Vec<(String, Vec<(String, f64)>)> {
    graph
        .nodes()
        .into_iter()
        .map(|v| {
            let mut weights: Vec<(String, f64)> = Vec::new();
            for e in graph.out_edges(&v, None).unwrap_or_default() {
                add_weight(
                    &mut weights,
                    &e.w,
                    graph.edge_object(&e).map_or(0.0, EdgeLabel::weight),
                );
            }
            (v, weights)
        })
        .collect()
}

/// For each node, the weights of the edges from its predecessors.
pub fn predecessor_weights(graph: &LayeredGraph) -> Vec<(String, Vec<(String, f64)>)> {
    graph
        .nodes()
        .into_iter()
        .map(|v| {
            let mut weights: Vec<(String, f64)> = Vec::new();
            for e in graph.in_edges(&v, None).unwrap_or_default() {
                add_weight(
                    &mut weights,
                    &e.v,
                    graph.edge_object(&e).map_or(0.0, EdgeLabel::weight),
                );
            }
            (v, weights)
        })
        .collect()
}

fn add_weight(weights: &mut Vec<(String, f64)>, key: &str, weight: f64) {
    if let Some(entry) = weights.iter_mut().find(|(id, _)| id == key) {
        entry.1 += weight;
    } else {
        weights.push((key.to_string(), weight));
    }
}

/// Where a line from `point` to the rectangle's center crosses the rectangle.
///
/// Fails when `point` is the center itself, as upstream throws.
pub fn intersect_rect(rect: &NodeLabel, point: Point) -> Result<Point, LayoutError> {
    let x = rect.x.unwrap_or(0.0);
    let y = rect.y.unwrap_or(0.0);
    let dx = point.x - x;
    let dy = point.y - y;
    let mut w = rect.width / 2.0;
    let mut h = rect.height / 2.0;

    if dx == 0.0 && dy == 0.0 {
        return Err(LayoutError::NoIntersection);
    }

    let (sx, sy);
    if dy.abs() * w > dx.abs() * h {
        if dy < 0.0 {
            h = -h;
        }
        sx = h * dx / dy;
        sy = h;
    } else {
        if dx < 0.0 {
            w = -w;
        }
        sx = w;
        sy = w * dy / dx;
    }

    Ok(Point::new(x + sx, y + sy))
}

/// The matrix of node ids by rank, indexed by each node's `order`.
///
/// Upstream writes `layering[rank][order] = v` into JavaScript arrays, so a later
/// node with the same order replaces an earlier one; this keeps that overwrite and
/// drops the empty slots.
pub fn build_layer_matrix(graph: &LayeredGraph) -> LayerMatrix {
    let ranks = max_rank(graph);
    if !ranks.is_finite() || ranks < 0.0 {
        return Vec::new();
    }
    let mut layering: Vec<Vec<Option<String>>> = vec![Vec::new(); ranks as usize + 1];
    for v in graph.nodes() {
        let Some(node) = graph.node(&v) else { continue };
        let (Some(rank), Some(order)) = (node.rank, node.order) else {
            continue;
        };
        if !rank.is_finite() || rank < 0.0 {
            continue;
        }
        let rank = rank as usize;
        if rank >= layering.len() {
            layering.resize(rank + 1, Vec::new());
        }
        if layering[rank].len() <= order {
            layering[rank].resize(order + 1, None);
        }
        layering[rank][order] = Some(v);
    }
    layering
        .into_iter()
        .map(|layer| layer.into_iter().flatten().collect())
        .collect()
}

/// Shifts every rank so the lowest rank is 0.
pub fn normalize_ranks(graph: &mut LayeredGraph) {
    let node_ranks: Vec<f64> = graph
        .nodes()
        .iter()
        .map(|v| graph.node(v).and_then(|n| n.rank).unwrap_or(f64::MAX))
        .collect();
    let min = node_ranks.into_iter().fold(f64::INFINITY, f64::min);
    for v in graph.nodes() {
        if let Some(node) = graph.node_mut(&v)
            && node.rank.is_some()
        {
            node.rank = node.rank.map(|rank| rank - min);
        }
    }
}

/// Removes the empty ranks that only separate subgraph borders.
pub fn remove_empty_ranks(graph: &mut LayeredGraph) {
    let node_ranks: Vec<f64> = graph
        .nodes()
        .iter()
        .filter_map(|v| graph.node(v).and_then(|n| n.rank))
        .collect();
    let offset = node_ranks.iter().copied().fold(f64::INFINITY, f64::min);

    let mut layers: Vec<Option<Vec<String>>> = Vec::new();
    for v in graph.nodes() {
        let Some(rank) = graph.node(&v).and_then(|n| n.rank) else {
            continue;
        };
        let rank = rank - offset;
        if rank < 0.0 || !rank.is_finite() {
            continue;
        }
        let rank = rank as usize;
        if rank >= layers.len() {
            layers.resize(rank + 1, None);
        }
        layers[rank].get_or_insert_with(Vec::new).push(v);
    }

    let factor = graph
        .graph()
        .and_then(|graph| graph.node_rank_factor)
        .unwrap_or(f64::NAN);
    let mut delta: f64 = 0.0;
    for (i, layer) in layers.iter().enumerate() {
        match layer {
            None if i as f64 % factor != 0.0 => delta -= 1.0,
            Some(vs) if delta != 0.0 => {
                for v in vs {
                    if let Some(node) = graph.node_mut(v)
                        && let Some(rank) = node.rank
                    {
                        node.rank = Some(rank + delta);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Adds a border dummy node to `graph`, upstream's `addBorderNode`.
pub fn add_border_node(
    graph: &mut LayeredGraph,
    prefix: &str,
    rank: Option<f64>,
    order: Option<usize>,
) -> String {
    let mut node = NodeLabel {
        width: 0.0,
        height: 0.0,
        ..NodeLabel::default()
    };
    if rank.is_some() && order.is_some() {
        node.rank = rank;
        node.order = order;
    }
    add_dummy_node(graph, "border", node, prefix)
}

/// The highest rank in the graph, or JavaScript's `Number.MIN_VALUE` when no node
/// has one.
pub fn max_rank(graph: &LayeredGraph) -> f64 {
    graph.nodes().iter().fold(5e-324, |max, v| {
        let rank = graph.node(v).and_then(|n| n.rank).unwrap_or(5e-324);
        max.max(rank)
    })
}

/// Splits `collection` into the entries matching `predicate` and the rest.
pub fn partition<T>(collection: Vec<T>, predicate: impl Fn(&T) -> bool) -> (Vec<T>, Vec<T>) {
    let mut lhs = Vec::new();
    let mut rhs = Vec::new();
    for value in collection {
        if predicate(&value) {
            lhs.push(value);
        } else {
            rhs.push(value);
        }
    }
    (lhs, rhs)
}

/// A process-wide counter for dummy ids, upstream's module-level `idCounter`.
static UNIQUE_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A fresh id with `prefix`, upstream's `uniqueId`.
pub fn unique_id(prefix: &str) -> String {
    let id = UNIQUE_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    format!("{prefix}{id}")
}

/// The integers from `start` (inclusive) while `step` keeps them on `limit`'s side.
pub fn range(start: isize, limit: isize, step: isize) -> Vec<isize> {
    let mut range = Vec::new();
    let mut i = start;
    if step < 0 {
        while limit < i {
            range.push(i);
            i += step;
        }
    } else {
        while i < limit {
            range.push(i);
            i += step;
        }
    }
    range
}

/// The integers below `limit`.
pub fn range_to(limit: isize) -> Vec<isize> {
    range(0, limit, 1)
}

/// Compares two numbers the way a JavaScript comparator does, treating NaN as equal.
pub fn cmp_f64(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}

/// The graph label with layout defaults filled in, upstream's `graphDefaults`.
///
/// `rankdir` defaults to lower-case `tb` and no `rankalign` is read, as in
/// dagre-d3-es; `makeSpaceForEdgeLabels` compares the direction case-sensitively, so
/// an absent direction takes the horizontal branch there.
pub fn graph_label_with_defaults(label: Option<&GraphLabel>) -> GraphLabel {
    let mut graph = label.cloned().unwrap_or_default();
    graph.ranksep.get_or_insert(50.0);
    graph.edgesep.get_or_insert(20.0);
    graph.nodesep.get_or_insert(50.0);
    graph.rankdir.get_or_insert_with(|| "tb".to_string());
    graph
}

#[cfg(test)]
mod tests {
    use super::super::super::graphlib::GraphOptions;
    use super::super::test_support::{close, compound, graph, multigraph, ranked};
    use super::super::{EdgeLabel, GraphLabel, LayeredGraph, NodeLabel};
    use super::*;

    fn has_rank(graph: &LayeredGraph, v: &str, rank: f64) {
        close(rank, graph.node(v).and_then(|node| node.rank).unwrap());
    }

    #[test]
    fn simplify_copies_a_graph_without_multi_edges() {
        let mut graph = multigraph();
        graph.set_edge_with("a", "b", super::super::test_support::weighted(1.0, 1.0));
        let simplified = simplify(&graph);
        let label = simplified.edge("a", "b").unwrap();
        assert_eq!(label.weight(), 1.0);
        assert_eq!(label.minlen(), 1.0);
        assert_eq!(simplified.edge_count(), 1);
    }

    #[test]
    fn simplify_collapses_multi_edges() {
        let mut graph = multigraph();
        graph.set_edge_with("a", "b", super::super::test_support::weighted(1.0, 1.0));
        graph
            .set_edge_full(
                "a",
                "b",
                Some(super::super::test_support::weighted(2.0, 2.0)),
                Some("multi"),
            )
            .unwrap();
        let simplified = simplify(&graph);
        assert!(!simplified.is_multigraph());
        let label = simplified.edge("a", "b").unwrap();
        assert_eq!(label.weight(), 3.0);
        assert_eq!(label.minlen(), 2.0);
        assert_eq!(simplified.edge_count(), 1);
    }

    #[test]
    fn simplify_copies_the_graph_label() {
        let mut graph = multigraph();
        graph.set_graph(GraphLabel {
            rankdir: Some("LR".to_string()),
            ..GraphLabel::default()
        });
        assert_eq!(
            simplify(&graph).graph().unwrap().rankdir.as_deref(),
            Some("LR")
        );
    }

    #[test]
    fn as_non_compound_graph_copies_all_nodes() {
        let mut graph: LayeredGraph = super::super::test_support::layering();
        graph.set_node_with(
            "a",
            NodeLabel {
                dummy: Some("x".to_string()),
                ..NodeLabel::default()
            },
        );
        graph.set_node("b");
        let copy = as_non_compound_graph(&graph);
        assert_eq!(copy.node("a").unwrap().dummy.as_deref(), Some("x"));
        assert!(copy.has_node("b"));
    }

    #[test]
    fn as_non_compound_graph_copies_all_edges() {
        let mut graph: LayeredGraph = super::super::test_support::layering();
        graph.set_edge_with(
            "a",
            "b",
            EdgeLabel {
                width: Some(1.0),
                ..EdgeLabel::default()
            },
        );
        graph
            .set_edge_full(
                "a",
                "b",
                Some(EdgeLabel {
                    width: Some(2.0),
                    ..EdgeLabel::default()
                }),
                Some("multi"),
            )
            .unwrap();
        let copy = as_non_compound_graph(&graph);
        assert_eq!(copy.edge("a", "b").unwrap().width(), 1.0);
        assert_eq!(
            copy.edge_label("a", "b", Some("multi")).unwrap().width(),
            2.0
        );
    }

    #[test]
    fn as_non_compound_graph_drops_compound_nodes() {
        let mut graph: LayeredGraph = super::super::test_support::layering();
        graph.set_parent("a", Some("sg1")).unwrap();
        let copy = as_non_compound_graph(&graph);
        assert_eq!(copy.parent("sg1"), None);
        assert_eq!(copy.parent("a"), None);
        assert!(!copy.is_compound());
    }

    #[test]
    fn as_non_compound_graph_copies_the_graph_label() {
        let mut graph: LayeredGraph = super::super::test_support::layering();
        graph.set_graph(GraphLabel {
            rankdir: Some("TB".to_string()),
            ..GraphLabel::default()
        });
        assert_eq!(
            as_non_compound_graph(&graph)
                .graph()
                .unwrap()
                .rankdir
                .as_deref(),
            Some("TB")
        );
    }

    #[test]
    fn successor_weights_maps_nodes_to_successors() {
        let mut graph: LayeredGraph = multigraph();
        graph.set_edge_with(
            "a",
            "b",
            EdgeLabel {
                weight: Some(2.0),
                ..EdgeLabel::default()
            },
        );
        graph.set_edge_with(
            "b",
            "c",
            EdgeLabel {
                weight: Some(1.0),
                ..EdgeLabel::default()
            },
        );
        graph
            .set_edge_full(
                "b",
                "c",
                Some(EdgeLabel {
                    weight: Some(2.0),
                    ..EdgeLabel::default()
                }),
                Some("multi"),
            )
            .unwrap();
        graph
            .set_edge_full(
                "b",
                "d",
                Some(EdgeLabel {
                    weight: Some(1.0),
                    ..EdgeLabel::default()
                }),
                Some("multi"),
            )
            .unwrap();
        let weights = successor_weights(&graph);
        let find = |v: &str, w: &str| {
            weights
                .iter()
                .find(|(id, _)| id == v)
                .and_then(|(_, list)| list.iter().find(|(id, _)| id == w))
                .map(|(_, weight)| *weight)
        };
        assert_eq!(find("a", "b"), Some(2.0));
        assert_eq!(find("b", "c"), Some(3.0));
        assert_eq!(find("b", "d"), Some(1.0));
    }

    #[test]
    fn predecessor_weights_maps_nodes_to_predecessors() {
        let mut graph: LayeredGraph = multigraph();
        graph.set_edge_with(
            "a",
            "b",
            EdgeLabel {
                weight: Some(2.0),
                ..EdgeLabel::default()
            },
        );
        graph.set_edge_with(
            "b",
            "c",
            EdgeLabel {
                weight: Some(1.0),
                ..EdgeLabel::default()
            },
        );
        graph
            .set_edge_full(
                "b",
                "c",
                Some(EdgeLabel {
                    weight: Some(2.0),
                    ..EdgeLabel::default()
                }),
                Some("multi"),
            )
            .unwrap();
        let weights = predecessor_weights(&graph);
        assert_eq!(
            weights
                .iter()
                .find(|(id, _)| id == "c")
                .unwrap()
                .1
                .first()
                .unwrap()
                .1,
            3.0
        );
    }

    #[test]
    fn intersect_rect_keeps_the_slope_through_the_center() {
        let rect = NodeLabel {
            x: Some(0.0),
            y: Some(0.0),
            width: 1.0,
            height: 1.0,
            ..NodeLabel::default()
        };
        for point in [
            Point::new(2.0, 6.0),
            Point::new(2.0, -6.0),
            Point::new(6.0, 2.0),
            Point::new(-6.0, 2.0),
            Point::new(5.0, 0.0),
            Point::new(0.0, 5.0),
        ] {
            let cross = intersect_rect(&rect, point).unwrap();
            if cross.x != point.x {
                let slope = (cross.y - point.y) / (cross.x - point.x);
                close(
                    cross.y - rect.y.unwrap(),
                    slope * (cross.x - rect.x.unwrap()),
                );
            }
        }
    }

    #[test]
    fn intersect_rect_touches_the_border() {
        let rect = NodeLabel {
            x: Some(0.0),
            y: Some(0.0),
            width: 1.0,
            height: 1.0,
            ..NodeLabel::default()
        };
        for point in [
            Point::new(2.0, 6.0),
            Point::new(2.0, -6.0),
            Point::new(6.0, 2.0),
            Point::new(-6.0, 2.0),
            Point::new(5.0, 0.0),
            Point::new(0.0, 5.0),
        ] {
            let cross = intersect_rect(&rect, point).unwrap();
            if (rect.x.unwrap() - cross.x).abs() != rect.width / 2.0 {
                close((rect.y.unwrap() - cross.y).abs(), rect.height / 2.0);
            }
        }
    }

    #[test]
    fn intersect_rect_rejects_the_center() {
        let rect = NodeLabel {
            x: Some(0.0),
            y: Some(0.0),
            width: 1.0,
            height: 1.0,
            ..NodeLabel::default()
        };
        assert_eq!(
            intersect_rect(&rect, Point::new(0.0, 0.0)),
            Err(LayoutError::NoIntersection)
        );
    }

    #[test]
    fn build_layer_matrix_orders_by_rank_then_order() {
        let mut graph = graph();
        super::super::test_support::ordered(&mut graph, "a", 0.0, 0);
        super::super::test_support::ordered(&mut graph, "b", 0.0, 1);
        super::super::test_support::ordered(&mut graph, "c", 1.0, 0);
        super::super::test_support::ordered(&mut graph, "d", 1.0, 1);
        super::super::test_support::ordered(&mut graph, "e", 2.0, 0);
        assert_eq!(
            build_layer_matrix(&graph),
            vec![
                vec!["a".to_string(), "b".to_string()],
                vec!["c".to_string(), "d".to_string()],
                vec!["e".to_string()],
            ]
        );
    }

    #[test]
    fn normalize_ranks_shifts_to_zero() {
        let mut graph = graph();
        graph.set_node_with("a", ranked(3.0));
        graph.set_node_with("b", ranked(2.0));
        graph.set_node_with("c", ranked(4.0));
        normalize_ranks(&mut graph);
        has_rank(&graph, "a", 1.0);
        has_rank(&graph, "b", 0.0);
        has_rank(&graph, "c", 2.0);
    }

    #[test]
    fn normalize_ranks_handles_negative_ranks() {
        let mut graph = graph();
        graph.set_node_with("a", ranked(-3.0));
        graph.set_node_with("b", ranked(-2.0));
        normalize_ranks(&mut graph);
        has_rank(&graph, "a", 0.0);
        has_rank(&graph, "b", 1.0);
    }

    #[test]
    fn normalize_ranks_skips_nodes_without_a_rank() {
        let mut graph = compound();
        graph.set_node_with("a", ranked(0.0));
        graph.set_node_with("sg", NodeLabel::default());
        graph.set_parent("a", Some("sg")).unwrap();
        normalize_ranks(&mut graph);
        assert_eq!(graph.node("sg").unwrap().rank, None);
        has_rank(&graph, "a", 0.0);
    }

    #[test]
    fn remove_empty_ranks_removes_border_ranks() {
        let mut graph = graph();
        graph.set_graph(GraphLabel {
            node_rank_factor: Some(4.0),
            ..GraphLabel::default()
        });
        graph.set_node_with("a", ranked(0.0));
        graph.set_node_with("b", ranked(4.0));
        remove_empty_ranks(&mut graph);
        has_rank(&graph, "a", 0.0);
        has_rank(&graph, "b", 1.0);
    }

    #[test]
    fn remove_empty_ranks_keeps_non_border_ranks() {
        let mut graph = graph();
        graph.set_graph(GraphLabel {
            node_rank_factor: Some(4.0),
            ..GraphLabel::default()
        });
        graph.set_node_with("a", ranked(0.0));
        graph.set_node_with("b", ranked(8.0));
        remove_empty_ranks(&mut graph);
        has_rank(&graph, "a", 0.0);
        has_rank(&graph, "b", 2.0);
    }

    #[test]
    fn remove_empty_ranks_handles_parents_without_ranks() {
        let mut graph = compound();
        graph.set_graph(GraphLabel {
            node_rank_factor: Some(3.0),
            ..GraphLabel::default()
        });
        graph.set_node_with("a", ranked(0.0));
        graph.set_node_with("b", ranked(6.0));
        graph.set_node_with("sg", NodeLabel::default());
        graph.set_parent("a", Some("sg")).unwrap();
        remove_empty_ranks(&mut graph);
        has_rank(&graph, "a", 0.0);
        has_rank(&graph, "b", 2.0);
        assert_eq!(graph.node("sg").unwrap().rank, None);
    }

    #[test]
    fn range_builds_a_range_to_the_limit() {
        assert_eq!(range_to(4), vec![0, 1, 2, 3]);
    }

    #[test]
    fn range_builds_a_range_with_a_start() {
        assert_eq!(range(2, 4, 1), vec![2, 3]);
    }

    #[test]
    fn range_builds_a_range_with_a_negative_step() {
        let values = range(5, -1, -1);
        assert_eq!(values.len(), 6);
        assert_eq!(values[0], 5);
        assert_eq!(values[5], 0);
    }

    #[test]
    fn max_rank_is_the_highest_rank() {
        let mut graph = graph();
        graph.set_node_with("a", ranked(0.0));
        graph.set_node_with("b", ranked(7.0));
        assert_eq!(max_rank(&graph), 7.0);
    }

    #[test]
    fn unique_ids_are_distinct_and_patterned() {
        let first = unique_id("_root");
        let second = unique_id("_root");
        assert!(first.starts_with("_root"));
        assert_ne!(first, second);
        assert!(first[5..].chars().all(|c| c.is_ascii_digit()));
        assert!(second[5..].chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn add_dummy_node_avoids_existing_ids() {
        let mut graph = graph();
        graph.set_node("_d");
        let first = add_dummy_node(&mut graph, "edge", NodeLabel::default(), "_d");
        assert_ne!(first, "_d");
        assert_eq!(graph.node(&first).unwrap().dummy.as_deref(), Some("edge"));
    }

    #[test]
    fn add_border_node_sets_rank_and_order_only_when_given() {
        let mut graph = graph();
        let plain = add_border_node(&mut graph, "_bt", None, None);
        assert_eq!(graph.node(&plain).unwrap().rank, None);
        let ranked_node = add_border_node(&mut graph, "_bl", Some(2.0), Some(3));
        assert_eq!(graph.node(&ranked_node).unwrap().rank, Some(2.0));
        assert_eq!(graph.node(&ranked_node).unwrap().order, Some(3));
    }

    #[test]
    fn graph_label_with_defaults_fills_the_layout_defaults() {
        let label = graph_label_with_defaults(None);
        assert_eq!(label.ranksep, Some(50.0));
        assert_eq!(label.edgesep, Some(20.0));
        assert_eq!(label.nodesep, Some(50.0));
        assert_eq!(label.rankdir.as_deref(), Some("tb"));
    }

    #[test]
    fn graph_options_helpers_are_consistent() {
        let options = GraphOptions::undirected();
        assert!(!options.directed);
    }
}
