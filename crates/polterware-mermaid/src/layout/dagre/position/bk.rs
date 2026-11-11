//! Port of `dagre/lib/position/bk.ts` (MIT, Copyright (c) 2012-2014 Chris Pettitt).
//!
//! Brandes and Köpf, "Fast and Simple Horizontal Coordinate Assignment." Four
//! alignments (up-left, up-right, down-left, down-right) are compacted, aligned to
//! the narrowest one, and balanced per node.

use std::collections::{HashMap, HashSet};

use super::super::super::graphlib::Graph;
use super::super::util::{self, LayerMatrix};
use super::super::{LayeredGraph, NodeLabel, js_greater_than};

/// Pairs of nodes whose edges must not be aligned, upstream's `Conflicts`.
pub type Conflicts = HashMap<String, HashSet<String>>;

/// A node id to coordinate map.
pub type PositionMap = HashMap<String, f64>;

/// The root and alignment maps [`vertical_alignment`] returns.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlignmentResult {
    /// The block root of every node.
    pub root: HashMap<String, String>,
    /// The node every node is aligned with.
    pub align: HashMap<String, String>,
}

/// Marks every type-1 conflict: a non-inner segment crossing an inner segment.
pub fn find_type1_conflicts(graph: &LayeredGraph, layering: &LayerMatrix) -> Conflicts {
    let mut conflicts = Conflicts::new();

    fn visit_layer(
        graph: &LayeredGraph,
        prev_layer: &[String],
        layer: &[String],
        conflicts: &mut Conflicts,
    ) {
        let mut k0 = 0usize;
        let mut scan_pos = 0usize;
        let prev_layer_length = prev_layer.len();
        let last_node = layer.last();

        for (i, v) in layer.iter().enumerate() {
            let w = find_other_inner_segment_node(graph, v);
            let k1 = w
                .as_ref()
                .and_then(|w| graph.node(w).and_then(|node| node.order))
                .unwrap_or(prev_layer_length);

            if w.is_some() || Some(v) == last_node {
                for scan_node in &layer[scan_pos..=i] {
                    for u in graph.predecessors(scan_node).unwrap_or_default() {
                        let u_label = graph.node(&u).cloned().unwrap_or_default();
                        let u_pos = u_label.order.unwrap_or(0);
                        if (u_pos < k0 || k1 < u_pos)
                            && !(u_label.dummy.is_some()
                                && graph
                                    .node(scan_node)
                                    .is_some_and(|node| node.dummy.is_some()))
                        {
                            add_conflict(conflicts, &u, scan_node);
                        }
                    }
                }
                scan_pos = i + 1;
                k0 = k1;
            }
        }
    }

    for i in 1..layering.len() {
        visit_layer(graph, &layering[i - 1], &layering[i], &mut conflicts);
    }
    conflicts
}

/// Marks every type-2 conflict: a non-inner segment crossing a border dummy.
pub fn find_type2_conflicts(graph: &LayeredGraph, layering: &LayerMatrix) -> Conflicts {
    let mut conflicts = Conflicts::new();
    let has_border_dummy = graph.nodes().iter().any(|v| {
        graph
            .node(v)
            .is_some_and(|node| node.dummy.as_deref() == Some("border"))
    });
    if !has_border_dummy {
        return conflicts;
    }

    fn scan(
        graph: &LayeredGraph,
        conflicts: &mut Conflicts,
        south: &[String],
        south_pos: usize,
        south_end: usize,
        prev_north_border: isize,
        next_north_border: isize,
    ) {
        for i in south_pos..south_end {
            let Some(v) = south.get(i) else { continue };
            if graph.node(v).is_some_and(|node| node.dummy.is_some()) {
                for u in graph.predecessors(v).unwrap_or_default() {
                    let u_node = graph.node(&u).cloned().unwrap_or_default();
                    let u_order = u_node.order.unwrap_or(0) as isize;
                    if u_node.dummy.is_some()
                        && (u_order < prev_north_border || u_order > next_north_border)
                    {
                        add_conflict(conflicts, &u, v);
                    }
                }
            }
        }
    }

    fn visit_layer(
        graph: &LayeredGraph,
        north: &[String],
        south: &[String],
        conflicts: &mut Conflicts,
    ) {
        let mut prev_north_pos: isize = -1;
        let mut next_north_pos: isize = -1;
        let mut south_pos = 0usize;

        for (south_lookahead, v) in south.iter().enumerate() {
            if graph
                .node(v)
                .is_some_and(|node| node.dummy.as_deref() == Some("border"))
            {
                let predecessors = graph.predecessors(v).unwrap_or_default();
                if let Some(first_pred) = predecessors.first() {
                    next_north_pos = graph
                        .node(first_pred)
                        .and_then(|node| node.order)
                        .unwrap_or(0) as isize;
                    scan(
                        graph,
                        conflicts,
                        south,
                        south_pos,
                        south_lookahead,
                        prev_north_pos,
                        next_north_pos,
                    );
                    south_pos = south_lookahead;
                    prev_north_pos = next_north_pos;
                }
            }
            scan(
                graph,
                conflicts,
                south,
                south_pos,
                south.len(),
                next_north_pos,
                north.len() as isize,
            );
        }
    }

    for i in 1..layering.len() {
        visit_layer(graph, &layering[i - 1], &layering[i], &mut conflicts);
    }
    conflicts
}

fn find_other_inner_segment_node(graph: &LayeredGraph, v: &str) -> Option<String> {
    if graph.node(v).is_some_and(|node| node.dummy.is_some()) {
        return graph
            .predecessors(v)
            .unwrap_or_default()
            .into_iter()
            .find(|u| graph.node(u).is_some_and(|node| node.dummy.is_some()));
    }
    None
}

/// Records a conflict between `v` and `w`, in canonical order.
pub fn add_conflict(conflicts: &mut Conflicts, v: &str, w: &str) {
    let (v, w) = if js_greater_than(v, w) {
        (w, v)
    } else {
        (v, w)
    };
    conflicts
        .entry(v.to_string())
        .or_default()
        .insert(w.to_string());
}

/// Whether `v` and `w` conflict.
pub fn has_conflict(conflicts: &Conflicts, v: &str, w: &str) -> bool {
    let (v, w) = if js_greater_than(v, w) {
        (w, v)
    } else {
        (v, w)
    };
    conflicts.get(v).is_some_and(|targets| targets.contains(w))
}

/// Aligns nodes with their median neighbours, skipping conflicted segments.
pub fn vertical_alignment(
    graph: &LayeredGraph,
    layering: &LayerMatrix,
    conflicts: &Conflicts,
    neighbor_fn: &dyn Fn(&str) -> Vec<String>,
    core_path: Option<&[String]>,
) -> AlignmentResult {
    let mut root: HashMap<String, String> = HashMap::new();
    let mut align: HashMap<String, String> = HashMap::new();
    let mut pos: HashMap<String, usize> = HashMap::new();

    for layer in layering {
        for (order, v) in layer.iter().enumerate() {
            root.insert(v.clone(), v.clone());
            align.insert(v.clone(), v.clone());
            pos.insert(v.clone(), order);
        }
    }

    for layer in layering {
        let mut prev_idx: isize = -1;
        let mut already_aligned_index: isize = -1;
        let mut has_shifted = false;
        let core_node_index = layer.iter().position(|v| {
            core_path.is_some_and(|path| path.contains(v)) || is_in_core_path(v, graph, core_path)
        });
        let mut modified_layer = layer.clone();
        if let Some(core_node_index) = core_node_index
            && core_node_index > 0
        {
            modified_layer = std::iter::once(layer[core_node_index].clone())
                .chain(layer[..core_node_index].iter().cloned())
                .chain(layer[core_node_index + 1..].iter().cloned())
                .collect();
            has_shifted = true;
        }

        for v in &modified_layer {
            let mut ws: Vec<String> = neighbor_fn(v);
            if ws.is_empty() {
                continue;
            }
            if core_path.is_some_and(|path| path.contains(v)) {
                ws.retain(|w| is_in_core_path(w, graph, core_path));
            }
            ws.sort_by(|a, b| {
                util::cmp_f64(
                    pos.get(a).copied().unwrap_or(0) as f64,
                    pos.get(b).copied().unwrap_or(0) as f64,
                )
            });
            let mp = (ws.len() as f64 - 1.0) / 2.0;
            let first = mp.floor() as usize;
            let last = mp.ceil() as usize;
            for w in &ws[first..=last] {
                let pos_w = pos.get(w).copied().unwrap_or(0) as isize;
                if align.get(v) == Some(v)
                    && prev_idx < pos_w
                    && pos_w != already_aligned_index
                    && !has_conflict(conflicts, v, w)
                {
                    let Some(root_w) = root.get(w).cloned() else {
                        continue;
                    };
                    align.insert(w.clone(), v.clone());
                    align.insert(v.clone(), root_w.clone());
                    root.insert(v.clone(), root_w);
                    prev_idx = pos_w;
                    if has_shifted {
                        prev_idx = -1;
                        already_aligned_index = pos_w;
                        has_shifted = false;
                    }
                }
            }
        }
    }

    AlignmentResult { root, align }
}

fn is_in_core_path(v: &str, graph: &LayeredGraph, core_path: Option<&[String]>) -> bool {
    let Some(core_path) = core_path else {
        return false;
    };
    let Some(node) = graph.node(v) else {
        return false;
    };
    let Some(edge) = &node.edge_obj else {
        return false;
    };
    if node.edge_label.as_ref().is_some_and(|label| label.reversed) {
        return false;
    }
    let source_index = core_path.iter().position(|id| id == &edge.v);
    let target_index = core_path.iter().position(|id| id == &edge.w);
    let (Some(source_index), Some(target_index)) = (source_index, target_index) else {
        return false;
    };
    let length = core_path.len() as isize;
    source_index as isize == (target_index as isize + 1) % length
        || source_index as isize == (target_index as isize - 1) % length
}

/// Packs blocks horizontally, first to the smallest then to the greatest coordinates.
pub fn horizontal_compaction(
    graph: &LayeredGraph,
    layering: &LayerMatrix,
    root: &HashMap<String, String>,
    align: &HashMap<String, String>,
    reverse_sep: bool,
) -> PositionMap {
    let mut xs = PositionMap::new();
    let block_graph = build_block_graph(graph, layering, root, reverse_sep);
    let border_type = if reverse_sep {
        "borderLeft"
    } else {
        "borderRight"
    };

    fn iterate(
        block_graph: &Graph<(), (), f64>,
        mut set_xs: impl FnMut(&str),
        next_nodes: impl Fn(&str) -> Vec<String>,
    ) {
        let mut stack: Vec<String> = block_graph.nodes();
        let mut visited: HashSet<String> = HashSet::new();
        while let Some(elem) = stack.pop() {
            if visited.contains(&elem) {
                set_xs(&elem);
            } else {
                visited.insert(elem.clone());
                stack.push(elem.clone());
                for next in next_nodes(&elem) {
                    stack.push(next);
                }
            }
        }
    }

    iterate(
        &block_graph,
        |elem| {
            let in_edges = block_graph.in_edges(elem, None).unwrap_or_default();
            let value = in_edges.into_iter().fold(0.0f64, |accumulator, edge| {
                let xs_v = xs.get(&edge.v).copied().unwrap_or(0.0);
                let weight = block_graph.edge_object(&edge).copied().unwrap_or(0.0);
                accumulator.max(xs_v + weight)
            });
            xs.insert(elem.to_string(), value);
        },
        |elem| block_graph.predecessors(elem).unwrap_or_default(),
    );

    iterate(
        &block_graph,
        |elem| {
            let out_edges = block_graph.out_edges(elem, None).unwrap_or_default();
            let min = out_edges
                .into_iter()
                .fold(f64::INFINITY, |accumulator, edge| {
                    let xs_w = xs.get(&edge.w).copied().unwrap_or(0.0);
                    let weight = block_graph.edge_object(&edge).copied().unwrap_or(0.0);
                    accumulator.min(xs_w - weight)
                });
            let node = graph.node(elem).cloned().unwrap_or_default();
            if min != f64::INFINITY && node.border_type.as_deref() != Some(border_type) {
                let value = xs.get(elem).copied().unwrap_or(0.0).max(min);
                xs.insert(elem.to_string(), value);
            }
        },
        |elem| block_graph.successors(elem).unwrap_or_default(),
    );

    for v in align.keys() {
        let root_v = root.get(v);
        if let Some(root_v) = root_v {
            xs.insert(v.clone(), xs.get(root_v).copied().unwrap_or(0.0));
        }
    }

    xs
}

fn build_block_graph(
    graph: &LayeredGraph,
    layering: &LayerMatrix,
    root: &HashMap<String, String>,
    reverse_sep: bool,
) -> Graph<(), (), f64> {
    let mut block_graph: Graph<(), (), f64> = Graph::new();
    let graph_label = graph.graph().cloned().unwrap_or_default();
    let node_sep = graph_label.nodesep.unwrap_or(0.0);
    let edge_sep = graph_label.edgesep.unwrap_or(0.0);

    for layer in layering {
        let mut u: Option<String> = None;
        for v in layer {
            let Some(v_root) = root.get(v) else {
                continue;
            };
            block_graph.set_node(v_root.clone());
            if let Some(u) = &u
                && let Some(u_root) = root.get(u)
            {
                let previous_max = block_graph.edge(u_root, v_root).copied();
                let separation = sep(graph, node_sep, edge_sep, reverse_sep, v, u);
                block_graph.set_edge_with(
                    u_root,
                    v_root,
                    separation.max(previous_max.unwrap_or(0.0)),
                );
            }
            u = Some(v.clone());
        }
    }

    block_graph
}

fn sep(
    graph: &LayeredGraph,
    node_sep: f64,
    edge_sep: f64,
    reverse_sep: bool,
    v: &str,
    w: &str,
) -> f64 {
    let v_label = graph.node(v).cloned().unwrap_or_default();
    let w_label = graph.node(w).cloned().unwrap_or_default();
    let mut sum = 0.0;

    sum += v_label.width / 2.0;
    let delta = label_delta(&v_label, true);
    if delta != 0.0 {
        sum += if reverse_sep { delta } else { -delta };
    }

    sum += (if v_label.dummy.is_some() {
        edge_sep
    } else {
        node_sep
    }) / 2.0;
    sum += (if w_label.dummy.is_some() {
        edge_sep
    } else {
        node_sep
    }) / 2.0;

    sum += w_label.width / 2.0;
    let delta = label_delta(&w_label, false);
    if delta != 0.0 {
        sum += if reverse_sep { delta } else { -delta };
    }

    sum
}

fn label_delta(node: &NodeLabel, is_source: bool) -> f64 {
    let Some(labelpos) = node.labelpos.as_deref() else {
        return 0.0;
    };
    match labelpos.to_lowercase().as_str() {
        "l" => {
            if is_source {
                -node.width / 2.0
            } else {
                node.width / 2.0
            }
        }
        "r" => {
            if is_source {
                node.width / 2.0
            } else {
                -node.width / 2.0
            }
        }
        _ => 0.0,
    }
}

/// The alignment with the smallest total width.
pub fn find_smallest_width_alignment(
    graph: &LayeredGraph,
    xss: &[(String, PositionMap)],
) -> Option<(String, PositionMap)> {
    let mut best: Option<(String, PositionMap)> = None;
    let mut best_width = f64::INFINITY;
    for (key, xs) in xss {
        let mut max = f64::NEG_INFINITY;
        let mut min = f64::INFINITY;
        for (v, x) in xs {
            let half_width = graph.node(v).map_or(0.0, |node| node.width) / 2.0;
            max = max.max(x + half_width);
            min = min.min(x - half_width);
        }
        let width = max - min;
        if width < best_width {
            best_width = width;
            best = Some((key.clone(), xs.clone()));
        }
    }
    best
}

/// Shifts every alignment so it starts or ends where the narrowest one does.
pub fn align_coordinates(xss: &mut [(String, PositionMap)], align_to: &str) {
    let Some((_, align_to_map)) = xss.iter().find(|(key, _)| key == align_to) else {
        return;
    };
    let align_to_min = align_to_map.values().copied().fold(f64::INFINITY, f64::min);
    let align_to_max = align_to_map
        .values()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);

    for vertical in ["u", "d"] {
        for horizontal in ["l", "r"] {
            let key = format!("{vertical}{horizontal}");
            if key == align_to {
                continue;
            }
            let Some((_, xs)) = xss.iter().find(|(id, _)| id == &key) else {
                continue;
            };
            let xs_min = xs.values().copied().fold(f64::INFINITY, f64::min);
            let xs_max = xs.values().copied().fold(f64::NEG_INFINITY, f64::max);
            let mut delta = align_to_min - xs_min;
            if horizontal != "l" {
                delta = align_to_max - xs_max;
            }
            if delta != 0.0
                && let Some((_, xs)) = xss.iter_mut().find(|(id, _)| id == &key)
            {
                for value in xs.values_mut() {
                    *value += delta;
                }
            }
        }
    }
}

/// The per-node average of the two middle alignments, or the requested alignment.
pub fn balance(xss: &[(String, PositionMap)], align: Option<&str>) -> PositionMap {
    let Some((_, ul_map)) = xss.iter().find(|(id, _)| id == "ul") else {
        return PositionMap::new();
    };
    let mut balanced = PositionMap::new();
    for v in ul_map.keys() {
        if let Some(align) = align {
            let key = align.to_lowercase();
            if let Some((_, alignment)) = xss.iter().find(|(id, _)| id == &key)
                && let Some(value) = alignment.get(v)
            {
                balanced.insert(v.clone(), *value);
                continue;
            }
        }
        let mut values: Vec<f64> = xss
            .iter()
            .map(|(_, xs)| xs.get(v).copied().unwrap_or(0.0))
            .collect();
        values.sort_by(|a, b| util::cmp_f64(*a, *b));
        let lower = values.get(1).copied().unwrap_or(0.0);
        let upper = values.get(2).copied().unwrap_or(0.0);
        balanced.insert(v.clone(), (lower + upper) / 2.0);
    }
    balanced
}

/// Computes the x coordinate of every node in `graph`.
pub fn position_x(graph: &LayeredGraph, core_path: Option<&[String]>) -> PositionMap {
    let layering = util::build_layer_matrix(graph);
    let mut conflicts = find_type1_conflicts(graph, &layering);
    // Upstream's `Object.assign` replaces a node's type-1 set with its type-2 set.
    for (v, targets) in find_type2_conflicts(graph, &layering) {
        conflicts.insert(v, targets);
    }

    let mut xss: Vec<(String, PositionMap)> = Vec::new();
    for vertical in ["u", "d"] {
        let mut adjusted_layering = if vertical == "u" {
            layering.clone()
        } else {
            layering.iter().rev().cloned().collect()
        };
        for horizontal in ["l", "r"] {
            if horizontal == "r" {
                adjusted_layering = adjusted_layering
                    .iter()
                    .map(|inner| inner.iter().rev().cloned().collect())
                    .collect();
            }

            let neighbor_fn = |v: &str| {
                if vertical == "u" {
                    graph.predecessors(v)
                } else {
                    graph.successors(v)
                }
                .unwrap_or_default()
            };
            let alignment = vertical_alignment(
                graph,
                &adjusted_layering,
                &conflicts,
                &neighbor_fn,
                core_path,
            );
            let mut xs = horizontal_compaction(
                graph,
                &adjusted_layering,
                &alignment.root,
                &alignment.align,
                horizontal == "r",
            );
            if horizontal == "r" {
                for value in xs.values_mut() {
                    *value = -*value;
                }
            }
            xss.push((format!("{vertical}{horizontal}"), xs));
        }
    }

    let smallest = find_smallest_width_alignment(graph, &xss);
    if let Some((key, _)) = &smallest {
        align_coordinates(&mut xss, key);
    }
    balance(&xss, graph.graph().and_then(|label| label.align.as_deref()))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::super::super::test_support::{close, graph};
    use super::super::super::util::build_layer_matrix;
    use super::super::super::{LayeredGraph, NodeLabel};
    use super::*;

    fn node(rank: f64, order: usize, width: f64) -> NodeLabel {
        NodeLabel {
            rank: Some(rank),
            order: Some(order),
            width,
            ..NodeLabel::default()
        }
    }

    fn with_dummy(rank: f64, order: usize, width: f64, dummy: &str) -> NodeLabel {
        NodeLabel {
            dummy: Some(dummy.to_string()),
            ..node(rank, order, width)
        }
    }

    fn conflicts_layer() -> LayeredGraph {
        let mut graph = graph();
        graph.set_node_with("a", node(0.0, 0, 0.0));
        graph.set_node_with("b", node(0.0, 1, 0.0));
        graph.set_node_with("c", node(1.0, 0, 0.0));
        graph.set_node_with("d", node(1.0, 1, 0.0));
        graph.set_edge("a", "d");
        graph.set_edge("b", "c");
        graph
    }

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn positions(pairs: &[(&str, f64)]) -> PositionMap {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), *value))
            .collect()
    }

    fn xss() -> Vec<(String, PositionMap)> {
        vec![
            ("ul".to_string(), positions(&[("a", 50.0), ("b", 1000.0)])),
            ("ur".to_string(), positions(&[("a", 100.0), ("b", 900.0)])),
            ("dl".to_string(), positions(&[("a", 150.0), ("b", 800.0)])),
            ("dr".to_string(), positions(&[("a", 200.0), ("b", 700.0)])),
        ]
    }

    #[test]
    fn type1_does_not_mark_edges_without_conflicts() {
        let mut graph = conflicts_layer();
        graph.remove_edge("a", "d");
        graph.remove_edge("b", "c");
        graph.set_edge("a", "c");
        graph.set_edge("b", "d");
        let layering = build_layer_matrix(&graph);
        let conflicts = find_type1_conflicts(&graph, &layering);
        assert!(!has_conflict(&conflicts, "a", "c"));
        assert!(!has_conflict(&conflicts, "b", "d"));
    }

    #[test]
    fn type1_does_not_mark_type_zero_conflicts() {
        let graph = conflicts_layer();
        let layering = build_layer_matrix(&graph);
        let conflicts = find_type1_conflicts(&graph, &layering);
        assert!(!has_conflict(&conflicts, "a", "d"));
        assert!(!has_conflict(&conflicts, "b", "c"));
    }

    #[test]
    fn type1_does_not_mark_with_one_dummy() {
        for v in ["a", "b", "c", "d"] {
            let mut graph = conflicts_layer();
            graph.node_mut(v).unwrap().dummy = Some("edge".to_string());
            let layering = build_layer_matrix(&graph);
            let conflicts = find_type1_conflicts(&graph, &layering);
            assert!(!has_conflict(&conflicts, "a", "d"), "{v}");
            assert!(!has_conflict(&conflicts, "b", "c"), "{v}");
        }
    }

    #[test]
    fn type1_marks_with_one_non_dummy() {
        for v in ["a", "b", "c", "d"] {
            let mut graph = conflicts_layer();
            for w in ["a", "b", "c", "d"] {
                if v != w {
                    graph.node_mut(w).unwrap().dummy = Some("edge".to_string());
                }
            }
            let layering = build_layer_matrix(&graph);
            let conflicts = find_type1_conflicts(&graph, &layering);
            if v == "a" || v == "d" {
                assert!(has_conflict(&conflicts, "a", "d"));
                assert!(!has_conflict(&conflicts, "b", "c"));
            } else {
                assert!(!has_conflict(&conflicts, "a", "d"));
                assert!(has_conflict(&conflicts, "b", "c"));
            }
        }
    }

    #[test]
    fn type1_does_not_mark_with_all_dummies() {
        let mut graph = conflicts_layer();
        for v in ["a", "b", "c", "d"] {
            graph.node_mut(v).unwrap().dummy = Some("edge".to_string());
        }
        let layering = build_layer_matrix(&graph);
        let conflicts = find_type1_conflicts(&graph, &layering);
        assert!(!has_conflict(&conflicts, "a", "d"));
        assert!(!has_conflict(&conflicts, "b", "c"));
    }

    #[test]
    fn type2_marks_favoring_border_segments_1() {
        let mut graph = conflicts_layer();
        for v in ["a", "d"] {
            graph.node_mut(v).unwrap().dummy = Some("edge".to_string());
        }
        for v in ["b", "c"] {
            graph.node_mut(v).unwrap().dummy = Some("border".to_string());
        }
        let layering = build_layer_matrix(&graph);
        let conflicts = find_type2_conflicts(&graph, &layering);
        assert!(has_conflict(&conflicts, "a", "d"));
        assert!(!has_conflict(&conflicts, "b", "c"));
    }

    #[test]
    fn type2_marks_favoring_border_segments_2() {
        let mut graph = conflicts_layer();
        for v in ["b", "c"] {
            graph.node_mut(v).unwrap().dummy = Some("edge".to_string());
        }
        for v in ["a", "d"] {
            graph.node_mut(v).unwrap().dummy = Some("border".to_string());
        }
        let layering = build_layer_matrix(&graph);
        let conflicts = find_type2_conflicts(&graph, &layering);
        assert!(!has_conflict(&conflicts, "a", "d"));
        assert!(has_conflict(&conflicts, "b", "c"));
    }

    #[test]
    fn type2_returns_no_conflicts_without_border_dummies() {
        let mut graph = conflicts_layer();
        for v in ["a", "b", "c", "d"] {
            graph.node_mut(v).unwrap().dummy = Some("edge".to_string());
        }
        let layering = build_layer_matrix(&graph);
        assert!(find_type2_conflicts(&graph, &layering).is_empty());
    }

    #[test]
    fn has_conflict_works_regardless_of_orientation() {
        let mut conflicts = Conflicts::new();
        add_conflict(&mut conflicts, "b", "a");
        assert!(has_conflict(&conflicts, "a", "b"));
        assert!(has_conflict(&conflicts, "b", "a"));
    }

    #[test]
    fn has_conflict_works_for_multiple_conflicts_with_one_node() {
        let mut conflicts = Conflicts::new();
        add_conflict(&mut conflicts, "a", "b");
        add_conflict(&mut conflicts, "a", "c");
        assert!(has_conflict(&conflicts, "a", "b"));
        assert!(has_conflict(&conflicts, "a", "c"));
    }

    fn alignment(
        layering: &LayerMatrix,
        conflicts: &Conflicts,
        graph: &LayeredGraph,
    ) -> AlignmentResult {
        vertical_alignment(
            graph,
            layering,
            conflicts,
            &|v: &str| graph.predecessors(v).unwrap_or_default(),
            None,
        )
    }

    #[test]
    fn aligns_with_itself_without_adjacencies() {
        let mut graph = graph();
        graph.set_node_with("a", node(0.0, 0, 0.0));
        graph.set_node_with("b", node(1.0, 0, 0.0));
        let layering = build_layer_matrix(&graph);
        let result = alignment(&layering, &Conflicts::new(), &graph);
        assert_eq!(result.root, map(&[("a", "a"), ("b", "b")]));
        assert_eq!(result.align, map(&[("a", "a"), ("b", "b")]));
    }

    #[test]
    fn aligns_with_its_sole_adjacency() {
        let mut graph = graph();
        graph.set_node_with("a", node(0.0, 0, 0.0));
        graph.set_node_with("b", node(1.0, 0, 0.0));
        graph.set_edge("a", "b");
        let layering = build_layer_matrix(&graph);
        let result = alignment(&layering, &Conflicts::new(), &graph);
        assert_eq!(result.root, map(&[("a", "a"), ("b", "a")]));
        assert_eq!(result.align, map(&[("a", "b"), ("b", "a")]));
    }

    #[test]
    fn aligns_with_the_left_median() {
        let mut graph = graph();
        graph.set_node_with("a", node(0.0, 0, 0.0));
        graph.set_node_with("b", node(0.0, 1, 0.0));
        graph.set_node_with("c", node(1.0, 0, 0.0));
        graph.set_edge("a", "c");
        graph.set_edge("b", "c");
        let layering = build_layer_matrix(&graph);
        let result = alignment(&layering, &Conflicts::new(), &graph);
        assert_eq!(result.root, map(&[("a", "a"), ("b", "b"), ("c", "a")]));
        assert_eq!(result.align, map(&[("a", "c"), ("b", "b"), ("c", "a")]));
    }

    #[test]
    fn aligns_regardless_of_insertion_order() {
        let mut graph = graph();
        graph.set_node_with("b", node(0.0, 1, 0.0));
        graph.set_node_with("c", node(1.0, 0, 0.0));
        graph.set_node_with("z", node(0.0, 0, 0.0));
        graph.set_edge("z", "c");
        graph.set_edge("b", "c");
        let layering = build_layer_matrix(&graph);
        let result = alignment(&layering, &Conflicts::new(), &graph);
        assert_eq!(result.root, map(&[("z", "z"), ("b", "b"), ("c", "z")]));
        assert_eq!(result.align, map(&[("z", "c"), ("b", "b"), ("c", "z")]));
    }

    #[test]
    fn aligns_with_the_right_median_when_left_is_unavailable() {
        let mut graph = graph();
        graph.set_node_with("a", node(0.0, 0, 0.0));
        graph.set_node_with("b", node(0.0, 1, 0.0));
        graph.set_node_with("c", node(1.0, 0, 0.0));
        graph.set_edge("a", "c");
        graph.set_edge("b", "c");
        let layering = build_layer_matrix(&graph);
        let mut conflicts = Conflicts::new();
        add_conflict(&mut conflicts, "a", "c");
        let result = alignment(&layering, &conflicts, &graph);
        assert_eq!(result.root, map(&[("a", "a"), ("b", "b"), ("c", "b")]));
        assert_eq!(result.align, map(&[("a", "a"), ("b", "c"), ("c", "b")]));
    }

    #[test]
    fn aligns_with_neither_median_when_both_are_unavailable() {
        let mut graph = graph();
        graph.set_node_with("a", node(0.0, 0, 0.0));
        graph.set_node_with("b", node(0.0, 1, 0.0));
        graph.set_node_with("c", node(1.0, 0, 0.0));
        graph.set_node_with("d", node(1.0, 1, 0.0));
        graph.set_edge("a", "d");
        graph.set_edge("b", "c");
        graph.set_edge("b", "d");
        let layering = build_layer_matrix(&graph);
        let result = alignment(&layering, &Conflicts::new(), &graph);
        assert_eq!(
            result.root,
            map(&[("a", "a"), ("b", "b"), ("c", "b"), ("d", "d")])
        );
        assert_eq!(
            result.align,
            map(&[("a", "a"), ("b", "c"), ("c", "b"), ("d", "d")])
        );
    }

    #[test]
    fn aligns_with_the_single_median_for_odd_adjacencies() {
        let mut graph = graph();
        graph.set_node_with("a", node(0.0, 0, 0.0));
        graph.set_node_with("b", node(0.0, 1, 0.0));
        graph.set_node_with("c", node(0.0, 2, 0.0));
        graph.set_node_with("d", node(1.0, 0, 0.0));
        graph.set_edge("a", "d");
        graph.set_edge("b", "d");
        graph.set_edge("c", "d");
        let layering = build_layer_matrix(&graph);
        let result = alignment(&layering, &Conflicts::new(), &graph);
        assert_eq!(
            result.root,
            map(&[("a", "a"), ("b", "b"), ("c", "c"), ("d", "b")])
        );
        assert_eq!(
            result.align,
            map(&[("a", "a"), ("b", "d"), ("c", "c"), ("d", "b")])
        );
    }

    #[test]
    fn aligns_blocks_across_multiple_layers() {
        let mut graph = graph();
        graph.set_node_with("a", node(0.0, 0, 0.0));
        graph.set_node_with("b", node(1.0, 0, 0.0));
        graph.set_node_with("c", node(1.0, 1, 0.0));
        graph.set_node_with("d", node(2.0, 0, 0.0));
        graph.set_path(&["a", "b", "d"]);
        graph.set_path(&["a", "c", "d"]);
        let layering = build_layer_matrix(&graph);
        let result = alignment(&layering, &Conflicts::new(), &graph);
        assert_eq!(
            result.root,
            map(&[("a", "a"), ("b", "a"), ("c", "c"), ("d", "a")])
        );
        assert_eq!(
            result.align,
            map(&[("a", "b"), ("b", "d"), ("c", "c"), ("d", "a")])
        );
    }

    #[test]
    fn compaction_places_a_single_node_at_the_origin() {
        let mut graph = graph();
        graph.set_node_with("a", node(0.0, 0, 0.0));
        let xs = horizontal_compaction(
            &graph,
            &build_layer_matrix(&graph),
            &map(&[("a", "a")]),
            &map(&[("a", "a")]),
            false,
        );
        close(xs["a"], 0.0);
    }

    #[test]
    fn compaction_separates_adjacent_nodes() {
        let mut graph = graph();
        graph.set_graph(super::super::super::GraphLabel {
            nodesep: Some(100.0),
            ..Default::default()
        });
        graph.set_node_with("a", node(0.0, 0, 100.0));
        graph.set_node_with("b", node(0.0, 1, 200.0));
        let xs = horizontal_compaction(
            &graph,
            &build_layer_matrix(&graph),
            &map(&[("a", "a"), ("b", "b")]),
            &map(&[("a", "a"), ("b", "b")]),
            false,
        );
        close(xs["a"], 0.0);
        close(xs["b"], 50.0 + 100.0 + 100.0);
    }

    #[test]
    fn compaction_separates_adjacent_edges() {
        let mut graph = graph();
        graph.set_graph(super::super::super::GraphLabel {
            edgesep: Some(20.0),
            ..Default::default()
        });
        graph.set_node_with("a", with_dummy(0.0, 0, 100.0, "edge"));
        graph.set_node_with("b", with_dummy(0.0, 1, 200.0, "edge"));
        let xs = horizontal_compaction(
            &graph,
            &build_layer_matrix(&graph),
            &map(&[("a", "a"), ("b", "b")]),
            &map(&[("a", "a"), ("b", "b")]),
            false,
        );
        close(xs["b"], 50.0 + 20.0 + 100.0);
    }

    #[test]
    fn compaction_aligns_centers_in_the_same_block() {
        let mut graph = graph();
        graph.set_node_with("a", node(0.0, 0, 100.0));
        graph.set_node_with("b", node(1.0, 0, 200.0));
        let xs = horizontal_compaction(
            &graph,
            &build_layer_matrix(&graph),
            &map(&[("a", "a"), ("b", "a")]),
            &map(&[("a", "b"), ("b", "a")]),
            false,
        );
        close(xs["a"], 0.0);
        close(xs["b"], 0.0);
    }

    #[test]
    fn compaction_separates_blocks() {
        let mut graph = graph();
        graph.set_graph(super::super::super::GraphLabel {
            nodesep: Some(75.0),
            ..Default::default()
        });
        graph.set_node_with("a", node(0.0, 0, 100.0));
        graph.set_node_with("b", node(1.0, 1, 200.0));
        graph.set_node_with("c", node(1.0, 0, 50.0));
        let xs = horizontal_compaction(
            &graph,
            &build_layer_matrix(&graph),
            &map(&[("a", "a"), ("b", "a"), ("c", "c")]),
            &map(&[("a", "b"), ("b", "a"), ("c", "c")]),
            false,
        );
        close(xs["a"], 25.0 + 75.0 + 100.0);
        close(xs["b"], 25.0 + 75.0 + 100.0);
        close(xs["c"], 0.0);
    }

    #[test]
    fn compaction_separates_classes() {
        let mut graph = graph();
        graph.set_graph(super::super::super::GraphLabel {
            nodesep: Some(75.0),
            ..Default::default()
        });
        graph.set_node_with("a", node(0.0, 0, 100.0));
        graph.set_node_with("b", node(0.0, 1, 200.0));
        graph.set_node_with("c", node(1.0, 0, 50.0));
        graph.set_node_with("d", node(1.0, 1, 80.0));
        let xs = horizontal_compaction(
            &graph,
            &build_layer_matrix(&graph),
            &map(&[("a", "a"), ("b", "b"), ("c", "c"), ("d", "b")]),
            &map(&[("a", "a"), ("b", "d"), ("c", "c"), ("d", "b")]),
            false,
        );
        close(xs["a"], 0.0);
        close(xs["b"], 50.0 + 75.0 + 100.0);
        close(xs["c"], 50.0 + 75.0 + 100.0 - 40.0 - 75.0 - 25.0);
        close(xs["d"], 50.0 + 75.0 + 100.0);
    }

    #[test]
    fn compaction_shifts_classes_by_max_separation_1() {
        let mut graph = graph();
        graph.set_graph(super::super::super::GraphLabel {
            nodesep: Some(75.0),
            ..Default::default()
        });
        graph.set_node_with("a", node(0.0, 0, 50.0));
        graph.set_node_with("b", node(0.0, 1, 150.0));
        graph.set_node_with("c", node(1.0, 0, 60.0));
        graph.set_node_with("d", node(1.0, 1, 70.0));
        let xs = horizontal_compaction(
            &graph,
            &build_layer_matrix(&graph),
            &map(&[("a", "a"), ("b", "b"), ("c", "a"), ("d", "b")]),
            &map(&[("a", "c"), ("b", "d"), ("c", "a"), ("d", "b")]),
            false,
        );
        close(xs["a"], 0.0);
        close(xs["b"], 25.0 + 75.0 + 75.0);
        close(xs["c"], 0.0);
        close(xs["d"], 25.0 + 75.0 + 75.0);
    }

    #[test]
    fn compaction_shifts_classes_by_max_separation_2() {
        let mut graph = graph();
        graph.set_graph(super::super::super::GraphLabel {
            nodesep: Some(75.0),
            ..Default::default()
        });
        graph.set_node_with("a", node(0.0, 0, 50.0));
        graph.set_node_with("b", node(0.0, 1, 70.0));
        graph.set_node_with("c", node(1.0, 0, 60.0));
        graph.set_node_with("d", node(1.0, 1, 150.0));
        let xs = horizontal_compaction(
            &graph,
            &build_layer_matrix(&graph),
            &map(&[("a", "a"), ("b", "b"), ("c", "a"), ("d", "b")]),
            &map(&[("a", "c"), ("b", "d"), ("c", "a"), ("d", "b")]),
            false,
        );
        close(xs["a"], 0.0);
        close(xs["b"], 30.0 + 75.0 + 75.0);
        close(xs["c"], 0.0);
        close(xs["d"], 30.0 + 75.0 + 75.0);
    }

    #[test]
    fn compaction_cascades_class_shift() {
        let mut graph = graph();
        graph.set_graph(super::super::super::GraphLabel {
            nodesep: Some(75.0),
            ..Default::default()
        });
        for (v, rank, order) in [
            ("a", 0.0, 0),
            ("b", 0.0, 1),
            ("c", 1.0, 0),
            ("d", 1.0, 1),
            ("e", 1.0, 2),
            ("f", 2.0, 0),
            ("g", 2.0, 1),
        ] {
            graph.set_node_with(v, node(rank, order, 50.0));
        }
        let xs = horizontal_compaction(
            &graph,
            &build_layer_matrix(&graph),
            &map(&[
                ("a", "a"),
                ("b", "b"),
                ("c", "c"),
                ("d", "d"),
                ("e", "b"),
                ("f", "f"),
                ("g", "d"),
            ]),
            &map(&[
                ("a", "a"),
                ("b", "e"),
                ("c", "c"),
                ("d", "g"),
                ("e", "b"),
                ("f", "f"),
                ("g", "d"),
            ]),
            false,
        );
        close(xs["a"], xs["b"] - 25.0 - 75.0 - 25.0);
        close(xs["b"], xs["e"]);
        close(xs["c"], xs["f"]);
        close(xs["d"], xs["c"] + 25.0 + 75.0 + 25.0);
        close(xs["e"], xs["d"] + 25.0 + 75.0 + 25.0);
        close(xs["g"], xs["f"] + 25.0 + 75.0 + 25.0);
    }

    #[test]
    fn compaction_handles_labelpos_left() {
        let mut graph = graph();
        graph.set_graph(super::super::super::GraphLabel {
            edgesep: Some(50.0),
            ..Default::default()
        });
        graph.set_node_with("a", with_dummy(0.0, 0, 100.0, "edge"));
        graph.set_node_with(
            "b",
            NodeLabel {
                dummy: Some("edge-label".to_string()),
                labelpos: Some("l".to_string()),
                ..node(0.0, 1, 200.0)
            },
        );
        graph.set_node_with("c", with_dummy(0.0, 2, 300.0, "edge"));
        let xs = horizontal_compaction(
            &graph,
            &build_layer_matrix(&graph),
            &map(&[("a", "a"), ("b", "b"), ("c", "c")]),
            &map(&[("a", "a"), ("b", "b"), ("c", "c")]),
            false,
        );
        close(xs["a"], 0.0);
        close(xs["b"], 50.0 + 50.0 + 200.0);
        close(xs["c"], xs["b"] + 50.0 + 150.0);
    }

    #[test]
    fn compaction_handles_labelpos_center() {
        let mut graph = graph();
        graph.set_graph(super::super::super::GraphLabel {
            edgesep: Some(50.0),
            ..Default::default()
        });
        graph.set_node_with("a", with_dummy(0.0, 0, 100.0, "edge"));
        graph.set_node_with(
            "b",
            NodeLabel {
                dummy: Some("edge-label".to_string()),
                labelpos: Some("c".to_string()),
                ..node(0.0, 1, 200.0)
            },
        );
        graph.set_node_with("c", with_dummy(0.0, 2, 300.0, "edge"));
        let xs = horizontal_compaction(
            &graph,
            &build_layer_matrix(&graph),
            &map(&[("a", "a"), ("b", "b"), ("c", "c")]),
            &map(&[("a", "a"), ("b", "b"), ("c", "c")]),
            false,
        );
        close(xs["b"], 50.0 + 50.0 + 100.0);
        close(xs["c"], xs["b"] + 100.0 + 50.0 + 150.0);
    }

    #[test]
    fn compaction_handles_labelpos_right() {
        let mut graph = graph();
        graph.set_graph(super::super::super::GraphLabel {
            edgesep: Some(50.0),
            ..Default::default()
        });
        graph.set_node_with("a", with_dummy(0.0, 0, 100.0, "edge"));
        graph.set_node_with(
            "b",
            NodeLabel {
                dummy: Some("edge-label".to_string()),
                labelpos: Some("r".to_string()),
                ..node(0.0, 1, 200.0)
            },
        );
        graph.set_node_with("c", with_dummy(0.0, 2, 300.0, "edge"));
        let xs = horizontal_compaction(
            &graph,
            &build_layer_matrix(&graph),
            &map(&[("a", "a"), ("b", "b"), ("c", "c")]),
            &map(&[("a", "a"), ("b", "b"), ("c", "c")]),
            false,
        );
        close(xs["b"], 50.0 + 50.0);
        close(xs["c"], xs["b"] + 200.0 + 50.0 + 150.0);
    }

    #[test]
    fn align_coordinates_aligns_a_single_node() {
        let mut xss = vec![
            ("ul".to_string(), positions(&[("a", 50.0)])),
            ("ur".to_string(), positions(&[("a", 100.0)])),
            ("dl".to_string(), positions(&[("a", 50.0)])),
            ("dr".to_string(), positions(&[("a", 200.0)])),
        ];
        align_coordinates(&mut xss, "ul");
        for (_, xs) in &xss {
            close(xs["a"], 50.0);
        }
    }

    #[test]
    fn align_coordinates_aligns_multiple_nodes() {
        let mut xss = xss();
        align_coordinates(&mut xss, "ul");
        let find = |key: &str| xss.iter().find(|(id, _)| id == key).unwrap().1.clone();
        assert_eq!(find("ul"), positions(&[("a", 50.0), ("b", 1000.0)]));
        assert_eq!(find("ur"), positions(&[("a", 200.0), ("b", 1000.0)]));
        assert_eq!(find("dl"), positions(&[("a", 50.0), ("b", 700.0)]));
        assert_eq!(find("dr"), positions(&[("a", 500.0), ("b", 1000.0)]));
    }

    #[test]
    fn finds_the_smallest_width_alignment() {
        let mut graph = graph();
        graph.set_node_with(
            "a",
            NodeLabel {
                width: 50.0,
                ..NodeLabel::default()
            },
        );
        graph.set_node_with(
            "b",
            NodeLabel {
                width: 50.0,
                ..NodeLabel::default()
            },
        );
        let xss = vec![
            ("ul".to_string(), positions(&[("a", 0.0), ("b", 1000.0)])),
            ("ur".to_string(), positions(&[("a", -5.0), ("b", 1000.0)])),
            ("dl".to_string(), positions(&[("a", 5.0), ("b", 2000.0)])),
            ("dr".to_string(), positions(&[("a", 0.0), ("b", 200.0)])),
        ];
        assert_eq!(find_smallest_width_alignment(&graph, &xss).unwrap().0, "dr");
    }

    #[test]
    fn smallest_width_takes_node_width_into_account() {
        let mut graph = graph();
        for (v, width) in [("a", 50.0), ("b", 50.0), ("c", 200.0)] {
            graph.set_node_with(
                v,
                NodeLabel {
                    width,
                    ..NodeLabel::default()
                },
            );
        }
        let xss = vec![
            (
                "ul".to_string(),
                positions(&[("a", 0.0), ("b", 100.0), ("c", 75.0)]),
            ),
            (
                "ur".to_string(),
                positions(&[("a", 0.0), ("b", 100.0), ("c", 80.0)]),
            ),
            (
                "dl".to_string(),
                positions(&[("a", 0.0), ("b", 100.0), ("c", 85.0)]),
            ),
            (
                "dr".to_string(),
                positions(&[("a", 0.0), ("b", 100.0), ("c", 90.0)]),
            ),
        ];
        assert_eq!(find_smallest_width_alignment(&graph, &xss).unwrap().0, "ul");
    }

    #[test]
    fn balances_a_single_node_to_the_shared_median() {
        let xss = vec![
            ("ul".to_string(), positions(&[("a", 0.0)])),
            ("ur".to_string(), positions(&[("a", 100.0)])),
            ("dl".to_string(), positions(&[("a", 100.0)])),
            ("dr".to_string(), positions(&[("a", 200.0)])),
        ];
        assert_eq!(balance(&xss, None), positions(&[("a", 100.0)]));
    }

    #[test]
    fn balances_a_single_node_to_the_average_of_medians() {
        let xss = vec![
            ("ul".to_string(), positions(&[("a", 0.0)])),
            ("ur".to_string(), positions(&[("a", 75.0)])),
            ("dl".to_string(), positions(&[("a", 125.0)])),
            ("dr".to_string(), positions(&[("a", 200.0)])),
        ];
        assert_eq!(balance(&xss, None), positions(&[("a", 100.0)]));
    }

    #[test]
    fn balances_multiple_nodes() {
        let xss = vec![
            ("ul".to_string(), positions(&[("a", 0.0), ("b", 50.0)])),
            ("ur".to_string(), positions(&[("a", 75.0), ("b", 0.0)])),
            ("dl".to_string(), positions(&[("a", 125.0), ("b", 60.0)])),
            ("dr".to_string(), positions(&[("a", 200.0), ("b", 75.0)])),
        ];
        assert_eq!(balance(&xss, None), positions(&[("a", 100.0), ("b", 55.0)]));
    }

    #[test]
    fn position_x_places_one_node_at_the_origin() {
        let mut graph = graph();
        graph.set_node_with("a", node(0.0, 0, 100.0));
        assert_eq!(position_x(&graph, None), positions(&[("a", 0.0)]));
    }

    #[test]
    fn position_x_places_a_block_at_the_origin() {
        let mut graph = graph();
        graph.set_node_with("a", node(0.0, 0, 100.0));
        graph.set_node_with("b", node(1.0, 0, 100.0));
        graph.set_edge("a", "b");
        assert_eq!(
            position_x(&graph, None),
            positions(&[("a", 0.0), ("b", 0.0)])
        );
    }

    #[test]
    fn position_x_places_different_sizes_at_the_origin() {
        let mut graph = graph();
        graph.set_node_with("a", node(0.0, 0, 40.0));
        graph.set_node_with("b", node(1.0, 0, 500.0));
        graph.set_node_with("c", node(2.0, 0, 20.0));
        graph.set_path(&["a", "b", "c"]);
        assert_eq!(
            position_x(&graph, None),
            positions(&[("a", 0.0), ("b", 0.0), ("c", 0.0)])
        );
    }

    #[test]
    fn position_x_centers_a_predecessor_of_two_equal_nodes() {
        let mut graph = graph();
        graph.set_graph(super::super::super::GraphLabel {
            nodesep: Some(10.0),
            ..Default::default()
        });
        graph.set_node_with("a", node(0.0, 0, 20.0));
        graph.set_node_with("b", node(1.0, 0, 50.0));
        graph.set_node_with("c", node(1.0, 1, 50.0));
        graph.set_edge("a", "b");
        graph.set_edge("a", "c");
        let pos = position_x(&graph, None);
        let a = pos["a"];
        close(pos["b"], a - 30.0);
        close(pos["c"], a + 30.0);
    }

    #[test]
    fn position_x_shifts_blocks_on_both_sides() {
        let mut graph = graph();
        graph.set_graph(super::super::super::GraphLabel {
            nodesep: Some(10.0),
            ..Default::default()
        });
        graph.set_node_with("a", node(0.0, 0, 50.0));
        graph.set_node_with("b", node(0.0, 1, 60.0));
        graph.set_node_with("c", node(1.0, 0, 70.0));
        graph.set_node_with("d", node(1.0, 1, 80.0));
        graph.set_edge("b", "c");
        let pos = position_x(&graph, None);
        let b = pos["b"];
        close(pos["a"], b - 30.0 - 10.0 - 25.0);
        close(pos["c"], b);
        close(pos["d"], b + 35.0 + 10.0 + 40.0);
    }

    #[test]
    fn position_x_aligns_inner_segments() {
        let mut graph = graph();
        graph.set_graph(super::super::super::GraphLabel {
            nodesep: Some(10.0),
            edgesep: Some(10.0),
            ..Default::default()
        });
        graph.set_node_with("a", with_dummy(0.0, 0, 50.0, "edge"));
        graph.set_node_with("b", node(0.0, 1, 60.0));
        graph.set_node_with("c", node(1.0, 0, 70.0));
        graph.set_node_with("d", with_dummy(1.0, 1, 80.0, "edge"));
        graph.set_edge("b", "c");
        graph.set_edge("a", "d");
        let pos = position_x(&graph, None);
        let a = pos["a"];
        close(pos["d"], a);
        close(pos["b"], a + 25.0 + 10.0 + 30.0);
        close(pos["c"], a - 35.0 - 10.0 - 40.0);
    }
}
