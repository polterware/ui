//! Port of `phase3.ordering.ts`: lane-aware median sweeps plus adjacent transpose
//! improvements.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use super::helpers::{Edge, Graph, Layering, NodeId, OrderedLayers, locale_compare};
use super::phase0_helpers::{build_layer_index, count_inversions};
use super::phase2_options::{TopLaneResolver, create_top_lane_resolver, resolve_top_lane_order};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SweepDirection {
    Down,
    Up,
}

fn median(mut values: Vec<f64>) -> f64 {
    let n = values.len();
    if n == 0 {
        return f64::INFINITY;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    if n % 2 == 1 {
        return values[(n - 1) / 2];
    }
    0.5 * (values[n / 2 - 1] + values[n / 2])
}

fn barycenter(values: &[f64]) -> f64 {
    if values.is_empty() {
        return f64::INFINITY;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

fn neighbor_positions_for(
    target_nodes: &[NodeId],
    fixed_index: &HashMap<NodeId, usize>,
    edges: &[Edge],
    direction: SweepDirection,
) -> HashMap<NodeId, Vec<f64>> {
    let mut neighbor_positions: HashMap<NodeId, Vec<f64>> = HashMap::new();
    for node in target_nodes {
        neighbor_positions.insert(node.clone(), Vec::new());
    }
    for edge in edges {
        match direction {
            SweepDirection::Down => {
                if let Some(fixed) = fixed_index.get(&edge.src)
                    && let Some(positions) = neighbor_positions.get_mut(&edge.dst)
                {
                    positions.push(*fixed as f64);
                }
            }
            SweepDirection::Up => {
                if let Some(fixed) = fixed_index.get(&edge.dst)
                    && let Some(positions) = neighbor_positions.get_mut(&edge.src)
                {
                    positions.push(*fixed as f64);
                }
            }
        }
    }
    neighbor_positions
}

fn current_order_tie_break(
    a: &str,
    b: &str,
    current_layer_index: &HashMap<NodeId, usize>,
) -> Ordering {
    let ia = current_layer_index.get(a).copied().unwrap_or(0);
    let ib = current_layer_index.get(b).copied().unwrap_or(0);
    if ia != ib {
        ia.cmp(&ib)
    } else {
        locale_compare(a, b)
    }
}

fn count_crossings_between_adjacent(upper: &[NodeId], lower: &[NodeId], edges: &[Edge]) -> usize {
    let upper_set: HashSet<&str> = upper.iter().map(String::as_str).collect();
    let lower_set: HashSet<&str> = lower.iter().map(String::as_str).collect();
    let upper_index = build_layer_index(upper);
    let lower_index = build_layer_index(lower);
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for edge in edges {
        if upper_set.contains(edge.src.as_str()) && lower_set.contains(edge.dst.as_str()) {
            pairs.push((
                upper_index.get(&edge.src).copied().unwrap_or(0),
                lower_index.get(&edge.dst).copied().unwrap_or(0),
            ));
        }
    }
    pairs.sort_by(|a, b| {
        if a.0 == b.0 {
            a.1.cmp(&b.1)
        } else {
            a.0.cmp(&b.0)
        }
    });
    let mut values: Vec<f64> = pairs.iter().map(|pair| pair.1 as f64).collect();
    count_inversions(&mut values)
}

pub fn total_crossings(layers: &[Vec<NodeId>], edges: &[Edge]) -> usize {
    let mut sum = 0;
    for index in 0..layers.len().saturating_sub(1) {
        sum += count_crossings_between_adjacent(&layers[index], &layers[index + 1], edges);
    }
    sum
}

/// Sorts a subset of nodes by its median score against a fixed layer.
fn sort_by_heuristic(
    nodes: &[NodeId],
    neighbor_positions: &HashMap<NodeId, Vec<f64>>,
    current_layer_index: &HashMap<NodeId, usize>,
) -> Vec<NodeId> {
    let mut sorted = nodes.to_vec();
    sorted.sort_by(|a, b| {
        let sa = median(neighbor_positions.get(a).cloned().unwrap_or_default());
        let sb = median(neighbor_positions.get(b).cloned().unwrap_or_default());
        if sa == sb {
            return current_order_tie_break(a, b, current_layer_index);
        }
        if !sa.is_finite() {
            return Ordering::Greater;
        }
        if !sb.is_finite() {
            return Ordering::Less;
        }
        sa.partial_cmp(&sb).unwrap_or(Ordering::Equal)
    });
    sorted
}

fn reorder_layer(
    fixed_layer: &[NodeId],
    target_layer: &[NodeId],
    edges: &[Edge],
    direction: SweepDirection,
    top_lane_of: Option<&TopLaneResolver>,
    lane_order: Option<&[String]>,
) -> Vec<NodeId> {
    let fixed_index = build_layer_index(fixed_layer);
    let curr_index = build_layer_index(target_layer);
    let neighbor_positions = neighbor_positions_for(target_layer, &fixed_index, edges, direction);

    let (Some(top_lane_of), Some(lane_order)) = (top_lane_of, lane_order) else {
        return sort_by_heuristic(target_layer, &neighbor_positions, &curr_index);
    };
    if lane_order.is_empty() {
        return sort_by_heuristic(target_layer, &neighbor_positions, &curr_index);
    }

    // Partition the target layer's nodes by lane.
    let mut by_lane: HashMap<Option<String>, Vec<NodeId>> = HashMap::new();
    for id in target_layer {
        by_lane
            .entry(top_lane_of.lane_of(id))
            .or_default()
            .push(id.clone());
    }

    let mut result: Vec<NodeId> = Vec::new();
    for lane in lane_order {
        let Some(nodes_in_lane) = by_lane.get(&Some(lane.clone())) else {
            continue;
        };
        if nodes_in_lane.is_empty() {
            continue;
        }
        result.extend(sort_by_heuristic(
            nodes_in_lane,
            &neighbor_positions,
            &curr_index,
        ));
    }

    // Null-lane nodes: long-edge dummies without a parent. Each goes next to the lane
    // whose fixed-layer positions best match its barycenter.
    if let Some(null_nodes) = by_lane.get(&None)
        && !null_nodes.is_empty()
    {
        let sorted = sort_by_heuristic(null_nodes, &neighbor_positions, &curr_index);
        for nid in sorted {
            let bc = barycenter(
                neighbor_positions
                    .get(&nid)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
            );
            let mut best_index = result.len();
            if bc.is_finite() {
                for (index, rid) in result.iter().enumerate() {
                    let r_bc = barycenter(
                        neighbor_positions
                            .get(rid)
                            .map(Vec::as_slice)
                            .unwrap_or(&[]),
                    );
                    if bc < r_bc {
                        best_index = index;
                        break;
                    }
                }
            }
            result.insert(best_index, nid);
        }
    }

    result
}

fn crossing_score(
    upper: &[NodeId],
    order: &[NodeId],
    edges_in: &[Edge],
    next: Option<&[NodeId]>,
    edges_out: Option<&[Edge]>,
) -> usize {
    let mut score = count_crossings_between_adjacent(upper, order, edges_in);
    if let (Some(next), Some(edges_out)) = (next, edges_out) {
        score += count_crossings_between_adjacent(order, next, edges_out);
    }
    score
}

fn transpose_improve(
    upper: &[NodeId],
    current: &[NodeId],
    edges: &[Edge],
    next: Option<&[NodeId]>,
    top_lane_of: Option<&TopLaneResolver>,
) -> Vec<NodeId> {
    let mut best = current.to_vec();
    let upper_set: HashSet<&str> = upper.iter().map(String::as_str).collect();
    let layer_set: HashSet<&str> = current.iter().map(String::as_str).collect();
    let next_set: Option<HashSet<&str>> =
        next.map(|next| next.iter().map(String::as_str).collect());

    let edges_in: Vec<Edge> = edges
        .iter()
        .filter(|edge| {
            upper_set.contains(edge.src.as_str()) && layer_set.contains(edge.dst.as_str())
        })
        .cloned()
        .collect();
    let edges_out: Option<Vec<Edge>> = next_set.as_ref().map(|next_set| {
        edges
            .iter()
            .filter(|edge| {
                layer_set.contains(edge.src.as_str()) && next_set.contains(edge.dst.as_str())
            })
            .cloned()
            .collect()
    });

    let lane_of: Option<HashMap<NodeId, Option<String>>> = top_lane_of.map(|resolver| {
        current
            .iter()
            .map(|id| (id.clone(), resolver.lane_of(id)))
            .collect()
    });

    let mut improved = true;
    let mut best_score = crossing_score(upper, &best, &edges_in, next, edges_out.as_deref());
    while improved {
        improved = false;
        for index in 0..best.len().saturating_sub(1) {
            if let Some(lane_of) = lane_of.as_ref() {
                let lane_a = lane_of.get(&best[index]).cloned().flatten();
                let lane_b = lane_of.get(&best[index + 1]).cloned().flatten();
                if lane_a != lane_b {
                    continue; // never swap across lane boundaries
                }
            }
            let previous = best_score;
            best.swap(index, index + 1);
            let next_score = crossing_score(upper, &best, &edges_in, next, edges_out.as_deref());
            if next_score < previous {
                best_score = next_score;
                improved = true;
            } else {
                best.swap(index, index + 1);
            }
        }
    }
    best
}

#[derive(Clone, Debug, Default)]
pub struct OrderOptions {
    pub lane_order: Option<Vec<String>>,
}

pub fn order_layers(
    layering: &Layering,
    g_with_dummies: &Graph,
    opts: Option<OrderOptions>,
) -> OrderedLayers {
    let mut layers = layering.layers.clone();
    let edges = &g_with_dummies.edges;

    let top_lane_of = create_top_lane_resolver(g_with_dummies);
    let lane_order = resolve_top_lane_order(
        g_with_dummies,
        opts.as_ref().and_then(|opts| opts.lane_order.as_deref()),
    );

    // Top-down / bottom-up sweeps.
    for _ in 0..3 {
        for index in 1..layers.len() {
            let upper = layers[index - 1].clone();
            let current = layers[index].clone();
            let next = layers.get(index + 1).cloned();
            let reordered = reorder_layer(
                &upper,
                &current,
                edges,
                SweepDirection::Down,
                Some(&top_lane_of),
                Some(&lane_order),
            );
            layers[index] = transpose_improve(
                &upper,
                &reordered,
                edges,
                next.as_deref(),
                Some(&top_lane_of),
            );
        }
        for index in (0..layers.len().saturating_sub(1)).rev() {
            let lower = layers[index + 1].clone();
            let current = layers[index].clone();
            let previous = index
                .checked_sub(1)
                .map(|previous| layers[previous].clone());
            let reordered = reorder_layer(
                &lower,
                &current,
                edges,
                SweepDirection::Up,
                Some(&top_lane_of),
                Some(&lane_order),
            );
            layers[index] = transpose_improve(
                &lower,
                &reordered,
                edges,
                previous.as_deref(),
                Some(&top_lane_of),
            );
        }
    }

    OrderedLayers { layers }
}
