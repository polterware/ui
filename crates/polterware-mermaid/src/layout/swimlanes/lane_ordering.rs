//! Port of `laneOrdering.ts`: a deterministic weighted linear arrangement search over the
//! diagram's top-level lanes.

use std::collections::HashMap;

use super::helpers::{Graph, locale_compare};
use super::phase2_options::{build_top_lane_order, create_top_lane_resolver};

pub const AUTOMATIC_LANE_ORDERING_RESTARTS: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeightedLaneEdge {
    pub a: String,
    pub b: String,
    pub weight: i64,
}

#[derive(Clone, Debug)]
struct CandidateOrder {
    order: Vec<String>,
    cost: f64,
    source_distance: f64,
}

fn hash_string(input: &str) -> u32 {
    let mut hash: u32 = 2_166_136_261;
    for unit in input.encode_utf16() {
        hash ^= unit as u32;
        hash = (hash as i32).wrapping_mul(16_777_619) as u32;
    }
    hash
}

fn mulberry32(seed: u32) -> impl FnMut() -> f64 {
    let mut state = seed;
    move || {
        state = state.wrapping_add(0x6d2b_79f5);
        let mut t = state;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        (t ^ (t >> 14)) as f64 / 4_294_967_296.0
    }
}

fn deterministic_shuffle(order: &[String], seed: u32) -> Vec<String> {
    let mut shuffled = order.to_vec();
    let mut random = mulberry32(seed);
    for i in (1..shuffled.len()).rev() {
        let j = (random() * (i + 1) as f64).floor() as usize;
        shuffled.swap(i, j);
    }
    shuffled
}

fn source_distance(order: &[String], source_index: &HashMap<String, usize>) -> f64 {
    let mut distance = 0.0;
    for (index, lane_id) in order.iter().enumerate() {
        let source = source_index.get(lane_id).copied().unwrap_or(index);
        distance += (index as f64 - source as f64).abs();
    }
    distance
}

pub fn lane_arrangement_cost(order: &[String], weights: &[WeightedLaneEdge]) -> f64 {
    let mut position: HashMap<&str, usize> = HashMap::new();
    for (index, lane_id) in order.iter().enumerate() {
        position.insert(lane_id.as_str(), index);
    }

    let mut cost = 0.0;
    for WeightedLaneEdge { a, b, weight } in weights {
        let (Some(&ai), Some(&bi)) = (position.get(a.as_str()), position.get(b.as_str())) else {
            continue;
        };
        cost += *weight as f64 * (ai as f64 - bi as f64).abs();
    }
    cost
}

pub fn build_weighted_lane_edges(g: &Graph) -> Vec<WeightedLaneEdge> {
    let source_order = build_top_lane_order(g);
    if source_order.len() < 2 {
        return Vec::new();
    }

    let source_index: HashMap<String, usize> = source_order
        .iter()
        .enumerate()
        .map(|(index, lane_id)| (lane_id.clone(), index))
        .collect();
    let top_lane_of = create_top_lane_resolver(g);
    let mut weights: Vec<WeightedLaneEdge> = Vec::new();
    let mut weight_index: HashMap<String, usize> = HashMap::new();

    for edge in &g.layout.edges {
        if edge.is_layout_only.unwrap_or(false) {
            continue;
        }
        let (Some(src), Some(dst)) = (edge.start.as_deref(), edge.end.as_deref()) else {
            continue;
        };
        if !g.node_by_id.contains_key(src) || !g.node_by_id.contains_key(dst) {
            continue;
        }

        let lane_a = top_lane_of.lane_of(src);
        let lane_b = top_lane_of.lane_of(dst);
        let (Some(lane_a), Some(lane_b)) = (lane_a, lane_b) else {
            continue;
        };
        if lane_a == lane_b {
            continue;
        }

        let (Some(&ia), Some(&ib)) = (source_index.get(&lane_a), source_index.get(&lane_b)) else {
            continue;
        };

        let (a, b) = if ia <= ib {
            (lane_a, lane_b)
        } else {
            (lane_b, lane_a)
        };
        let key = format!("{}\0{}", a, b);
        if let Some(index) = weight_index.get(&key) {
            weights[*index].weight += 1;
        } else {
            weight_index.insert(key, weights.len());
            weights.push(WeightedLaneEdge { a, b, weight: 1 });
        }
    }

    weights
}

fn greedy_switch(
    start_order: &[String],
    weights: &[WeightedLaneEdge],
    source_index: &HashMap<String, usize>,
) -> CandidateOrder {
    let mut order = start_order.to_vec();
    let mut cost = lane_arrangement_cost(&order, weights);
    let mut changed = true;
    let mut sweeps = 0;
    let max_sweeps = order.len().max(1);

    while changed && sweeps < max_sweeps {
        changed = false;
        sweeps += 1;
        for i in 0..order.len().saturating_sub(1) {
            order.swap(i, i + 1);
            let next_cost = lane_arrangement_cost(&order, weights);
            if next_cost < cost {
                cost = next_cost;
                changed = true;
            } else {
                order.swap(i, i + 1);
            }
        }
    }

    CandidateOrder {
        source_distance: source_distance(&order, source_index),
        order,
        cost,
    }
}

fn is_better_candidate(candidate: &CandidateOrder, best: &CandidateOrder) -> bool {
    if candidate.cost != best.cost {
        return candidate.cost < best.cost;
    }
    candidate.source_distance < best.source_distance
}

fn seed_for_restart(
    source_order: &[String],
    weights: &[WeightedLaneEdge],
    restart_index: usize,
) -> u32 {
    let mut sorted = weights.to_vec();
    sorted.sort_by(|a, b| {
        if a.a == b.a {
            locale_compare(&a.b, &b.b)
        } else {
            locale_compare(&a.a, &b.a)
        }
    });
    let weight_signature = sorted
        .iter()
        .map(|WeightedLaneEdge { a, b, weight }| format!("{}:{}:{}", a, b, weight))
        .collect::<Vec<_>>()
        .join("|");
    hash_string(&format!(
        "{}#{}#{}",
        source_order.join("|"),
        weight_signature,
        restart_index
    ))
}

pub fn optimize_top_lane_order(g: &Graph, restarts: Option<usize>) -> Vec<String> {
    let source_order = build_top_lane_order(g);
    if source_order.len() < 2 {
        return source_order;
    }

    let weights = build_weighted_lane_edges(g);
    if weights.is_empty() {
        return source_order;
    }

    let source_index: HashMap<String, usize> = source_order
        .iter()
        .enumerate()
        .map(|(index, lane_id)| (lane_id.clone(), index))
        .collect();
    let mut best = greedy_switch(&source_order, &weights, &source_index);
    let restarts = restarts.unwrap_or(AUTOMATIC_LANE_ORDERING_RESTARTS);

    for index in 0..restarts {
        let seed = seed_for_restart(&source_order, &weights, index);
        let start = deterministic_shuffle(&source_order, seed);
        let candidate = greedy_switch(&start, &weights, &source_index);
        if is_better_candidate(&candidate, &best) {
            best = candidate;
        }
    }

    best.order
}
