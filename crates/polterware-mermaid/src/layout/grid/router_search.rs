//! Port of Mermaid 12.3.0's
//! `rendering-util/layout-algorithms/grid/routerSearch.ts`.
//!
//! The search minimizes a lexicographic tuple: Manhattan length, bends, hierarchy boundary
//! transitions, and endpoint-candidate rank. A reusable workspace keeps typed state arrays across
//! endpoint candidate pairs so one routing invocation does not allocate an object-heavy graph per
//! pair.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::layout::grid::router_errors::{GridRoutingError, GridRoutingResourceLimitError};
use crate::layout::grid::router_instrumentation::GridRoutingInstrumentation;
use crate::layout::grid::types::{
    ContainerRoutingTopology, GridOrientation, GridSearchBudget, GridSide, RouterArc, RouterPoint,
    RouterSearchArc, RouterVertex,
};

/// A* minimizes this tuple lexicographically. Length and bends dominate hierarchy transitions and
/// endpoint preference, which are deterministic tie-breakers rather than weighted approximations.
pub type RouterTupleCost = [f64; 4];

const LENGTH: usize = 0;
const BENDS: usize = 1;
const BOUNDARY_TRANSITIONS: usize = 2;
const ENDPOINT_RANK: usize = 3;

#[derive(Clone, Debug, PartialEq)]
pub struct RouterSearchResult {
    pub points: Vec<RouterPoint>,
    pub vertex_ids: Vec<usize>,
    pub cost: RouterTupleCost,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RouterSearchCaps {
    pub max_expanded_states: Option<usize>,
    pub max_invocation_expanded_states: Option<usize>,
    pub max_estimated_bytes: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RouterSearchHeuristic {
    #[default]
    BendAware,
    Zero,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RouterQueueOrder {
    #[default]
    Canonical,
    Reverse,
}

/// The graph contract the search reads. `ContainerRoutingTopology` supplies compact search arcs;
/// tests and the dense oracle can supply a plain adjacency map.
pub trait RouterSearchTopology {
    fn vertices(&self) -> &[RouterVertex];
    fn vertex_count(&self) -> usize;
    fn vertex(&self, id: usize) -> Option<&RouterVertex>;
    fn search_arcs(&self, id: usize) -> Option<&[RouterSearchArc]>;
    fn arcs(&self, id: usize) -> &[RouterArc];
}

impl RouterSearchTopology for ContainerRoutingTopology {
    fn vertices(&self) -> &[RouterVertex] {
        &self.vertices
    }

    fn vertex_count(&self) -> usize {
        self.vertex_count
    }

    fn vertex(&self, id: usize) -> Option<&RouterVertex> {
        self.get_vertex(id)
    }

    fn search_arcs(&self, id: usize) -> Option<&[RouterSearchArc]> {
        Some(self.get_search_arcs(id))
    }

    fn arcs(&self, id: usize) -> &[RouterArc] {
        self.adjacency.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// A plain vertices-plus-adjacency graph, matching the shape the upstream tests pass to
/// `findShortestRoute`.
#[derive(Clone, Debug, Default)]
pub struct AdjacencySearchTopology {
    pub vertices: Vec<RouterVertex>,
    pub adjacency: HashMap<usize, Vec<RouterArc>>,
}

impl RouterSearchTopology for AdjacencySearchTopology {
    fn vertices(&self) -> &[RouterVertex] {
        &self.vertices
    }

    fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    fn vertex(&self, id: usize) -> Option<&RouterVertex> {
        self.vertices.get(id)
    }

    fn search_arcs(&self, _id: usize) -> Option<&[RouterSearchArc]> {
        None
    }

    fn arcs(&self, id: usize) -> &[RouterArc] {
        self.adjacency.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }
}

pub struct RouterSearchOptions<'a> {
    pub metrics: Option<&'a mut GridRoutingInstrumentation>,
    pub caps: Option<RouterSearchCaps>,
    pub endpoint_candidate_rank: f64,
    pub record_outcome: Option<bool>,
    pub budget: Option<&'a mut GridSearchBudget>,
    pub initial_orientation: Option<GridOrientation>,
    pub initial_side: Option<GridSide>,
    pub initial_length: Option<f64>,
    pub target_orientation: Option<GridOrientation>,
    pub target_side: Option<GridSide>,
    pub target_length: Option<f64>,
    pub topology_validated: bool,
    pub workspace: Option<&'a mut RouterSearchWorkspace>,
    pub heuristic: RouterSearchHeuristic,
    pub queue_order: RouterQueueOrder,
    pub estimated_bytes_base: Option<f64>,
    pub arc_allowed: Option<&'a dyn Fn(RouterPoint, RouterPoint) -> bool>,
}

impl Default for RouterSearchOptions<'_> {
    fn default() -> Self {
        Self {
            metrics: None,
            caps: None,
            endpoint_candidate_rank: 0.0,
            record_outcome: None,
            budget: None,
            initial_orientation: None,
            initial_side: None,
            initial_length: None,
            target_orientation: None,
            target_side: None,
            target_length: None,
            topology_validated: false,
            workspace: None,
            heuristic: RouterSearchHeuristic::BendAware,
            queue_order: RouterQueueOrder::Canonical,
            estimated_bytes_base: None,
            arc_allowed: None,
        }
    }
}

const DEFAULT_MAX_EDGE_STATES: usize = 100_000;
const DEFAULT_MAX_INVOCATION_STATES: usize = 2_000_000;

pub struct RouterSearchWorkspace {
    // Typed arrays are retained across searches to avoid allocating object-heavy state graphs for
    // every endpoint candidate pair. State capacity grows geometrically and never shrinks.
    pub state_vertices: Vec<i32>,
    pub orientations: Vec<u8>,
    pub g_lengths: Vec<f64>,
    pub g_bends: Vec<f64>,
    pub g_boundary_transitions: Vec<f64>,
    pub f_lengths: Vec<f64>,
    pub f_bends: Vec<f64>,
    pub predecessors: Vec<i32>,
    pub depths: Vec<i32>,
    pub ancestor4: Vec<i32>,
    pub best: Vec<i32>,
    pub heap_children: Vec<i32>,
    pub heap_siblings: Vec<i32>,
    pub heap_pairs: Vec<i32>,
    pub touched_best_indices: Vec<i32>,
    /// Number of resets performed, exposed so tests can observe budget checks that happen before
    /// the workspace is touched.
    pub reset_count: usize,
}

impl Default for RouterSearchWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

impl RouterSearchWorkspace {
    pub fn new() -> Self {
        let mut best = vec![-1; 256];
        best.fill(-1);
        Self {
            state_vertices: vec![0; 256],
            orientations: vec![0; 256],
            g_lengths: vec![0.0; 256],
            g_bends: vec![0.0; 256],
            g_boundary_transitions: vec![0.0; 256],
            f_lengths: vec![0.0; 256],
            f_bends: vec![0.0; 256],
            predecessors: vec![0; 256],
            depths: vec![0; 256],
            ancestor4: vec![0; 256],
            best,
            heap_children: vec![0; 256],
            heap_siblings: vec![0; 256],
            heap_pairs: Vec::new(),
            touched_best_indices: Vec::new(),
            reset_count: 0,
        }
    }

    pub fn reset(&mut self, vertex_count: usize) {
        self.reset_count += 1;
        let best_length = vertex_count * 3;
        if self.best.len() < best_length {
            self.best = vec![-1; best_length];
        } else {
            for index in self.touched_best_indices.iter().copied() {
                if let Some(slot) = self.best.get_mut(index as usize) {
                    *slot = -1;
                }
            }
        }
        self.touched_best_indices.clear();
        self.heap_pairs.clear();
    }

    pub fn set_best(&mut self, index: i32, state: i32) {
        if self.best[index as usize] < 0 {
            self.touched_best_indices.push(index);
        }
        self.best[index as usize] = state;
    }

    pub fn best_at(&self, index: i32) -> i32 {
        self.best.get(index as usize).copied().unwrap_or(-1)
    }

    pub fn ensure_state_capacity(&mut self, required: usize) {
        if self.state_vertices.len() >= required {
            return;
        }
        let capacity = required.max(self.state_vertices.len() * 2);
        resize_with_default(&mut self.state_vertices, capacity);
        resize_with_default(&mut self.orientations, capacity);
        resize_with_default(&mut self.g_lengths, capacity);
        resize_with_default(&mut self.g_bends, capacity);
        resize_with_default(&mut self.g_boundary_transitions, capacity);
        resize_with_default(&mut self.f_lengths, capacity);
        resize_with_default(&mut self.f_bends, capacity);
        resize_with_default(&mut self.predecessors, capacity);
        resize_with_default(&mut self.depths, capacity);
        resize_with_default(&mut self.ancestor4, capacity);
        resize_with_default(&mut self.heap_children, capacity);
        resize_with_default(&mut self.heap_siblings, capacity);
    }

    pub fn estimated_bytes(&self) -> f64 {
        (self.state_vertices.len() * 4
            + self.orientations.len()
            + self.g_lengths.len() * 8
            + self.g_bends.len() * 8
            + self.g_boundary_transitions.len() * 8
            + self.f_lengths.len() * 8
            + self.f_bends.len() * 8
            + self.predecessors.len() * 4
            + self.depths.len() * 4
            + self.ancestor4.len() * 4
            + self.best.len() * 4
            + self.heap_children.len() * 4
            + self.heap_siblings.len() * 4) as f64
            + (self.heap_pairs.len() + self.touched_best_indices.len()) as f64 * 8.0
    }
}

fn resize_with_default<T: Clone + Default>(values: &mut Vec<T>, capacity: usize) {
    values.resize(capacity, T::default());
}

pub fn assert_search_budget_available(
    metrics: Option<&GridRoutingInstrumentation>,
    caps: Option<&RouterSearchCaps>,
    budget: Option<&GridSearchBudget>,
) -> Result<(), GridRoutingError> {
    let edge_cap = caps
        .and_then(|caps| caps.max_expanded_states)
        .unwrap_or(DEFAULT_MAX_EDGE_STATES);
    let invocation_cap = caps
        .and_then(|caps| caps.max_invocation_expanded_states)
        .unwrap_or(DEFAULT_MAX_INVOCATION_STATES);
    let invocation_start = budget
        .map(|budget| budget.expanded_states)
        .or_else(|| metrics.map(|metrics| metrics.expanded_states))
        .unwrap_or(0);
    if edge_cap > 0 && invocation_start < invocation_cap {
        return Ok(());
    }
    Err(GridRoutingError::ResourceLimit(
        GridRoutingResourceLimitError::new(
            crate::layout::grid::router_instrumentation::GridRoutingFallbackReason::SearchStateCap,
            "Grid routing search-state cap exceeded",
            Some(if invocation_start >= invocation_cap {
                crate::layout::grid::router_errors::SearchStateScope::Invocation
            } else {
                crate::layout::grid::router_errors::SearchStateScope::Edge
            }),
        ),
    ))
}

pub fn compare_tuple_cost(a: &RouterTupleCost, b: &RouterTupleCost) -> Ordering {
    for index in 0..4 {
        let ordering = a[index].total_cmp(&b[index]);
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    Ordering::Equal
}

fn minimum_bends(
    dx: f64,
    dy: f64,
    incoming_orientation: Option<GridOrientation>,
    target_orientation: Option<GridOrientation>,
) -> usize {
    if dx == 0.0 && dy == 0.0 {
        return match (incoming_orientation, target_orientation) {
            (Some(incoming), Some(target)) => usize::from(incoming != target),
            _ => 0,
        };
    }
    if dx == 0.0 {
        return usize::from(
            incoming_orientation.is_some_and(|orientation| orientation != GridOrientation::V),
        ) + usize::from(
            target_orientation.is_some_and(|orientation| orientation != GridOrientation::V),
        );
    }
    if dy == 0.0 {
        return usize::from(
            incoming_orientation.is_some_and(|orientation| orientation != GridOrientation::H),
        ) + usize::from(
            target_orientation.is_some_and(|orientation| orientation != GridOrientation::H),
        );
    }
    let first = usize::from(
        incoming_orientation.is_some_and(|orientation| orientation != GridOrientation::H),
    ) + 1
        + usize::from(
            target_orientation.is_some_and(|orientation| orientation != GridOrientation::V),
        );
    let second = usize::from(
        incoming_orientation.is_some_and(|orientation| orientation != GridOrientation::V),
    ) + 1
        + usize::from(
            target_orientation.is_some_and(|orientation| orientation != GridOrientation::H),
        );
    first.min(second)
}

fn orientation_ordinal(orientation: Option<GridOrientation>) -> u8 {
    match orientation {
        None => 0,
        Some(GridOrientation::H) => 1,
        Some(GridOrientation::V) => 2,
    }
}

fn moves_toward_owner(from: RouterPoint, to: RouterPoint, side: GridSide) -> bool {
    match side {
        GridSide::Left => from.y == to.y && to.x > from.x,
        GridSide::Right => from.y == to.y && to.x < from.x,
        GridSide::Top => from.x == to.x && to.y > from.y,
        GridSide::Bottom => from.x == to.x && to.y < from.y,
    }
}

fn state_index(vertex_id: usize, orientation: Option<GridOrientation>) -> i32 {
    (vertex_id * 3) as i32 + orientation_ordinal(orientation) as i32
}

fn normalize_points(points: &[RouterPoint]) -> Vec<RouterPoint> {
    let mut result: Vec<RouterPoint> = Vec::new();
    for point in points {
        let previous = result.last().copied();
        if previous.is_some_and(|previous| previous.x == point.x && previous.y == point.y) {
            continue;
        }
        let before_previous = if result.len() >= 2 {
            Some(result[result.len() - 2])
        } else {
            None
        };
        if let (Some(before_previous), Some(previous)) = (before_previous, previous)
            && ((before_previous.x == previous.x && previous.x == point.x)
                || (before_previous.y == previous.y && previous.y == point.y))
        {
            let last = result.len() - 1;
            result[last] = *point;
            continue;
        }
        result.push(*point);
    }
    result
}

fn reconstruct(
    state_id: i32,
    state_vertices: &[i32],
    predecessors: &[i32],
    cost: RouterTupleCost,
    vertex_at: impl Fn(usize) -> RouterPoint,
) -> RouterSearchResult {
    let mut vertex_ids: Vec<usize> = Vec::new();
    let mut current = state_id;
    while current >= 0 {
        vertex_ids.push(state_vertices[current as usize] as usize);
        current = predecessors[current as usize];
    }
    vertex_ids.reverse();
    let points = normalize_points(
        &vertex_ids
            .iter()
            .map(|id| vertex_at(*id))
            .collect::<Vec<_>>(),
    );
    RouterSearchResult {
        points,
        vertex_ids,
        cost,
    }
}

fn validate_search_topology(topology: &dyn RouterSearchTopology) -> Result<(), GridRoutingError> {
    let vertices = topology.vertices();
    for (index, vertex) in vertices.iter().enumerate() {
        if vertex.id != index
            || topology.vertex(vertex.id).map(|candidate| candidate.id) != Some(vertex.id)
            || !vertex.point.x.is_finite()
            || !vertex.point.y.is_finite()
        {
            return Err(GridRoutingError::geometry("Malformed grid routing vertex"));
        }
        for arc in topology.arcs(vertex.id) {
            let target = topology.vertex(arc.to);
            let aligned = target.is_some_and(|target| {
                if arc.orientation == GridOrientation::H {
                    vertex.point.y == target.point.y
                } else {
                    vertex.point.x == target.point.x
                }
            });
            let length = target
                .map(|target| {
                    (vertex.point.x - target.point.x).abs()
                        + (vertex.point.y - target.point.y).abs()
                })
                .unwrap_or(f64::NAN);
            if arc.from != vertex.id
                || !aligned
                || !arc.length.is_finite()
                || arc.length <= 0.0
                || arc.length != length
            {
                return Err(GridRoutingError::geometry("Malformed grid routing arc"));
            }
        }
    }
    Ok(())
}

struct SearchRun<'a> {
    topology: &'a dyn RouterSearchTopology,
    workspace: &'a mut RouterSearchWorkspace,
    metrics: Option<&'a mut GridRoutingInstrumentation>,
    budget: Option<&'a mut GridSearchBudget>,
    endpoint_candidate_rank: f64,
    initial_orientation: Option<GridOrientation>,
    initial_side: Option<GridSide>,
    initial_length: Option<f64>,
    target_orientation: Option<GridOrientation>,
    target_side: Option<GridSide>,
    target_length: Option<f64>,
    caps: Option<RouterSearchCaps>,
    queue_order: RouterQueueOrder,
    use_bend_heuristic: bool,
    estimated_bytes_base: f64,
    arc_allowed: Option<&'a dyn Fn(RouterPoint, RouterPoint) -> bool>,
    record_outcome: bool,
    heap_root: i32,
    heap_size: usize,
    state_count: usize,
    expanded_this_search: usize,
    edge_cap: usize,
    invocation_cap: usize,
    invocation_start: usize,
    source_id: usize,
    target_id: usize,
    best_goal_state: i32,
    best_goal_cost: Option<RouterTupleCost>,
}

impl<'a> SearchRun<'a> {
    fn vertex_at(&self, id: usize) -> Result<&RouterVertex, GridRoutingError> {
        self.topology
            .vertex(id)
            .ok_or_else(|| GridRoutingError::geometry(format!("Missing grid routing vertex {id}")))
    }

    fn depth(&self, index: i32) -> i32 {
        if index < 0 {
            -1
        } else {
            self.workspace.depths[index as usize]
        }
    }

    fn predecessor(&self, index: i32) -> i32 {
        if index < 0 {
            -1
        } else {
            self.workspace.predecessors[index as usize]
        }
    }

    fn non_cost_order(&self, a: i32, b: i32) -> Ordering {
        let ordering = self.workspace.state_vertices[a as usize]
            .cmp(&self.workspace.state_vertices[b as usize])
            .then_with(|| {
                self.workspace.orientations[a as usize]
                    .cmp(&self.workspace.orientations[b as usize])
            })
            .then_with(|| {
                self.workspace.predecessors[a as usize]
                    .cmp(&self.workspace.predecessors[b as usize])
            });
        if ordering != Ordering::Equal {
            return ordering;
        }
        a.cmp(&b)
    }

    fn compare_states(&self, a: i32, b: i32) -> Ordering {
        let ordering = self.workspace.f_lengths[a as usize]
            .total_cmp(&self.workspace.f_lengths[b as usize])
            .then_with(|| {
                self.workspace.f_bends[a as usize].total_cmp(&self.workspace.f_bends[b as usize])
            })
            .then_with(|| {
                self.workspace.g_boundary_transitions[a as usize]
                    .total_cmp(&self.workspace.g_boundary_transitions[b as usize])
            });
        if ordering != Ordering::Equal {
            return ordering;
        }
        let non_cost = self.non_cost_order(a, b);
        match self.queue_order {
            RouterQueueOrder::Canonical => non_cost,
            RouterQueueOrder::Reverse => non_cost.reverse(),
        }
    }

    fn compare_chains(&self, a: i32, b: i32) -> Ordering {
        // Equal-cost states choose the lexicographically smaller predecessor chain. Four-level jump
        // pointers reduce the common-prefix walk without retaining full paths per state.
        if a == b {
            return Ordering::Equal;
        }
        if a < 0 {
            return Ordering::Less;
        }
        if b < 0 {
            return Ordering::Greater;
        }
        let a_depth = self.workspace.depths[a as usize];
        let b_depth = self.workspace.depths[b as usize];
        let mut aligned_a = a;
        let mut aligned_b = b;
        while self.depth(aligned_a) > self.depth(aligned_b) {
            aligned_a = self.predecessor(aligned_a);
        }
        while self.depth(aligned_b) > self.depth(aligned_a) {
            aligned_b = self.predecessor(aligned_b);
        }
        if aligned_a == aligned_b {
            return a_depth.cmp(&b_depth);
        }
        while aligned_a >= 0
            && aligned_b >= 0
            && self.workspace.ancestor4[aligned_a as usize] >= 0
            && self.workspace.ancestor4[aligned_b as usize] >= 0
            && self.workspace.ancestor4[aligned_a as usize]
                != self.workspace.ancestor4[aligned_b as usize]
        {
            aligned_a = self.workspace.ancestor4[aligned_a as usize];
            aligned_b = self.workspace.ancestor4[aligned_b as usize];
        }
        while self.predecessor(aligned_a) != self.predecessor(aligned_b) {
            aligned_a = self.predecessor(aligned_a);
            aligned_b = self.predecessor(aligned_b);
        }
        if aligned_a < 0 || aligned_b < 0 {
            return aligned_a.cmp(&aligned_b);
        }
        self.workspace.state_vertices[aligned_a as usize]
            .cmp(&self.workspace.state_vertices[aligned_b as usize])
            .then_with(|| {
                self.workspace.orientations[aligned_a as usize]
                    .cmp(&self.workspace.orientations[aligned_b as usize])
            })
    }

    fn heap_meld(&mut self, first: i32, second: i32) -> i32 {
        if first < 0 {
            return second;
        }
        if second < 0 {
            return first;
        }
        if self.compare_states(first, second) != Ordering::Greater {
            self.workspace.heap_siblings[second as usize] =
                self.workspace.heap_children[first as usize];
            self.workspace.heap_children[first as usize] = second;
            return first;
        }
        self.workspace.heap_siblings[first as usize] =
            self.workspace.heap_children[second as usize];
        self.workspace.heap_children[second as usize] = first;
        second
    }

    fn heap_push(&mut self, value: i32) {
        self.workspace.heap_children[value as usize] = -1;
        self.workspace.heap_siblings[value as usize] = -1;
        self.heap_root = self.heap_meld(self.heap_root, value);
        self.heap_size += 1;
    }

    fn heap_pop(&mut self) -> Option<i32> {
        if self.heap_root < 0 {
            return None;
        }
        let first = self.heap_root;
        self.workspace.heap_pairs.clear();
        let mut child = self.workspace.heap_children[first as usize];
        while child >= 0 {
            let next = self.workspace.heap_siblings[child as usize];
            self.workspace.heap_siblings[child as usize] = -1;
            self.workspace.heap_pairs.push(child);
            child = next;
        }
        let mut index = 0usize;
        while index + 1 < self.workspace.heap_pairs.len() {
            let first_pair = self.workspace.heap_pairs[index];
            let second_pair = self.workspace.heap_pairs[index + 1];
            let melded = self.heap_meld(first_pair, second_pair);
            self.workspace.heap_pairs[index / 2] = melded;
            index += 2;
        }
        let mut pair_count = self.workspace.heap_pairs.len().div_ceil(2);
        if !self.workspace.heap_pairs.len().is_multiple_of(2) {
            let last = *self
                .workspace
                .heap_pairs
                .last()
                .expect("heap pairs are not empty");
            self.workspace.heap_pairs[pair_count - 1] = last;
        }
        self.heap_root = -1;
        while pair_count > 0 {
            pair_count -= 1;
            let value = self.workspace.heap_pairs[pair_count];
            self.heap_root = self.heap_meld(value, self.heap_root);
        }
        self.heap_size -= 1;
        Some(first)
    }

    fn record_workspace_bytes(&mut self) -> Result<(), GridRoutingError> {
        let bytes = self.workspace.estimated_bytes();
        let total_bytes = self.estimated_bytes_base + bytes;
        if let Some(metrics) = self.metrics.as_deref_mut() {
            metrics.search_workspace_bytes = metrics.search_workspace_bytes.max(bytes);
            metrics.estimated_bytes = metrics.estimated_bytes.max(total_bytes);
        }
        let cap = self
            .caps
            .and_then(|caps| caps.max_estimated_bytes)
            .unwrap_or(f64::INFINITY);
        if total_bytes > cap {
            return Err(GridRoutingError::ResourceLimit(
                GridRoutingResourceLimitError::new(
                    crate::layout::grid::router_instrumentation::GridRoutingFallbackReason::EstimatedMemoryCap,
                    "Grid routing search workspace memory cap exceeded",
                    None,
                ),
            ));
        }
        Ok(())
    }

    fn commit_expanded_states(&mut self) {
        // Charge states once on every exit path, including cap failures, so the invocation-wide
        // budget cannot be reset by trying another candidate pair.
        if let Some(budget) = self.budget.as_deref_mut() {
            budget.expanded_states += self.expanded_this_search;
        }
        if let Some(metrics) = self.metrics.as_deref_mut() {
            metrics.expanded_states += self.expanded_this_search;
        }
    }

    fn consider_goal(&mut self, state: i32) {
        let completed_cost: RouterTupleCost = [
            self.workspace.g_lengths[state as usize] + self.target_length.unwrap_or(0.0),
            self.workspace.g_bends[state as usize]
                + f64::from(
                    self.target_orientation.is_some()
                        && self.workspace.orientations[state as usize] != 0
                        && orientation_ordinal(self.target_orientation)
                            != self.workspace.orientations[state as usize],
                ),
            self.workspace.g_boundary_transitions[state as usize],
            self.endpoint_candidate_rank,
        ];
        let replace = match self.best_goal_cost {
            None => true,
            Some(best) => {
                let ordering = compare_tuple_cost(&completed_cost, &best);
                ordering == Ordering::Less
                    || (ordering == Ordering::Equal
                        && self.compare_chains(state, self.best_goal_state) == Ordering::Less)
            }
        };
        if replace {
            self.best_goal_state = state;
            self.best_goal_cost = Some(completed_cost);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn relax_arc(
        &mut self,
        current: i32,
        current_point: RouterPoint,
        to: usize,
        orientation: u8,
        arc_length: f64,
        boundary_transitions: f64,
    ) -> Result<(), GridRoutingError> {
        let next_point = self.vertex_at(to)?.point;
        if let Some(arc_allowed) = self.arc_allowed
            && !arc_allowed(current_point, next_point)
        {
            return Ok(());
        }
        if let Some(initial_side) = self.initial_side
            && self.predecessor(current) < 0
            && moves_toward_owner(current_point, next_point, initial_side)
        {
            return Ok(());
        }
        if let Some(target_side) = self.target_side
            && to == self.target_id
        {
            let target_point = self.vertex_at(self.target_id)?.point;
            if moves_toward_owner(target_point, current_point, target_side) {
                return Ok(());
            }
        }
        let index = to as i32 * 3 + orientation as i32;
        let previous = self.workspace.best_at(index);
        let length = self.workspace.g_lengths[current as usize] + arc_length;
        let bends = self.workspace.g_bends[current as usize]
            + f64::from(
                self.workspace.orientations[current as usize] != 0
                    && self.workspace.orientations[current as usize] != orientation,
            );
        let next_boundary_transitions =
            self.workspace.g_boundary_transitions[current as usize] + boundary_transitions;
        let previous_comparison = if previous >= 0 {
            let ordering = length
                .total_cmp(&self.workspace.g_lengths[previous as usize])
                .then_with(|| bends.total_cmp(&self.workspace.g_bends[previous as usize]))
                .then_with(|| {
                    next_boundary_transitions
                        .total_cmp(&self.workspace.g_boundary_transitions[previous as usize])
                });
            match ordering {
                Ordering::Greater => 1,
                Ordering::Equal => 0,
                Ordering::Less => -1,
            }
        } else {
            -1
        };
        if previous >= 0
            && (previous_comparison > 0
                || (previous_comparison == 0
                    && self.compare_chains(current, self.predecessor(previous)) != Ordering::Less))
        {
            return Ok(());
        }
        let candidate = self.state_count as i32;
        self.state_count += 1;
        if self.state_count > self.workspace.state_vertices.len() {
            self.workspace.ensure_state_capacity(self.state_count);
            self.record_workspace_bytes()?;
        }
        self.workspace.state_vertices[candidate as usize] = to as i32;
        self.workspace.orientations[candidate as usize] = orientation;
        self.workspace.g_lengths[candidate as usize] = length;
        self.workspace.g_bends[candidate as usize] = bends;
        self.workspace.g_boundary_transitions[candidate as usize] = next_boundary_transitions;
        let target_point = self.vertex_at(self.target_id)?.point;
        let candidate_dx = (next_point.x - target_point.x).abs();
        let candidate_dy = (next_point.y - target_point.y).abs();
        self.workspace.f_lengths[candidate as usize] = length
            + if self.use_bend_heuristic {
                candidate_dx + candidate_dy
            } else {
                0.0
            };
        self.workspace.f_bends[candidate as usize] = bends
            + if self.use_bend_heuristic {
                minimum_bends(
                    candidate_dx,
                    candidate_dy,
                    Some(if orientation == 1 {
                        GridOrientation::H
                    } else {
                        GridOrientation::V
                    }),
                    self.target_orientation,
                ) as f64
            } else {
                0.0
            };
        self.workspace.predecessors[candidate as usize] = current;
        self.workspace.depths[candidate as usize] = self.workspace.depths[current as usize] + 1;
        let parent2 = self.predecessor(current);
        let parent3 = self.predecessor(parent2);
        self.workspace.ancestor4[candidate as usize] = if parent3 >= 0 {
            self.predecessor(parent3)
        } else {
            -1
        };
        self.workspace.set_best(index, candidate);
        self.heap_push(candidate);
        Ok(())
    }

    fn run(mut self) -> Result<Option<RouterSearchResult>, GridRoutingError> {
        let source = self.vertex_at(self.source_id)?.point;
        let target = self.vertex_at(self.target_id)?.point;
        self.workspace.reset(self.topology.vertex_count());
        self.record_workspace_bytes()?;
        let initial_length = self.initial_length.unwrap_or(0.0);
        self.workspace.state_vertices[0] = self.source_id as i32;
        self.workspace.orientations[0] = orientation_ordinal(self.initial_orientation);
        self.workspace.g_lengths[0] = initial_length;
        self.workspace.g_bends[0] = 0.0;
        self.workspace.g_boundary_transitions[0] = 0.0;
        let initial_dx = (source.x - target.x).abs();
        let initial_dy = (source.y - target.y).abs();
        self.workspace.f_lengths[0] = initial_length
            + if self.use_bend_heuristic {
                initial_dx + initial_dy
            } else {
                0.0
            };
        self.workspace.f_bends[0] = if self.use_bend_heuristic {
            minimum_bends(
                initial_dx,
                initial_dy,
                self.initial_orientation,
                self.target_orientation,
            ) as f64
        } else {
            0.0
        };
        self.workspace.predecessors[0] = -1;
        self.workspace.depths[0] = 1;
        self.workspace.ancestor4[0] = -1;
        self.workspace
            .set_best(state_index(self.source_id, self.initial_orientation), 0);
        self.heap_push(0);
        if let Some(metrics) = self.metrics.as_deref_mut() {
            metrics.max_open_set = metrics.max_open_set.max(self.heap_size);
        }

        while self.heap_size > 0 {
            let next = self.heap_root;
            if let Some(best_goal_cost) = self.best_goal_cost {
                // The queue is ordered by the same admissible tuple prefix as the goal. Once its
                // best state is worse than the best goal, no remaining state can improve it.
                let mut difference =
                    self.workspace.f_lengths[next as usize] - best_goal_cost[LENGTH];
                if difference == 0.0 {
                    difference = self.workspace.f_bends[next as usize] - best_goal_cost[BENDS];
                }
                if difference == 0.0 {
                    difference = self.workspace.g_boundary_transitions[next as usize]
                        - best_goal_cost[BOUNDARY_TRANSITIONS];
                }
                if difference == 0.0 {
                    difference = self.endpoint_candidate_rank - best_goal_cost[ENDPOINT_RANK];
                }
                if difference > 0.0 {
                    break;
                }
            }
            let Some(current) = self.heap_pop() else {
                break;
            };
            let state_index = self.workspace.state_vertices[current as usize] * 3
                + self.workspace.orientations[current as usize] as i32;
            if self.workspace.best_at(state_index) != current {
                continue;
            }
            if self.expanded_this_search >= self.edge_cap
                || self.invocation_start + self.expanded_this_search >= self.invocation_cap
            {
                if self.best_goal_state >= 0
                    && let Some(best_goal_cost) = self.best_goal_cost
                {
                    if self.record_outcome
                        && let Some(metrics) = self.metrics.as_deref_mut()
                    {
                        metrics.routes_found += 1;
                    }
                    self.commit_expanded_states();
                    let workspace = &self.workspace;
                    return Ok(Some(reconstruct(
                        self.best_goal_state,
                        &workspace.state_vertices,
                        &workspace.predecessors,
                        best_goal_cost,
                        |id| self.topology.vertex(id).expect("validated vertex").point,
                    )));
                }
                self.commit_expanded_states();
                let scope =
                    if self.invocation_start + self.expanded_this_search >= self.invocation_cap {
                        crate::layout::grid::router_errors::SearchStateScope::Invocation
                    } else {
                        crate::layout::grid::router_errors::SearchStateScope::Edge
                    };
                return Err(GridRoutingError::ResourceLimit(
                    GridRoutingResourceLimitError::new(
                        crate::layout::grid::router_instrumentation::GridRoutingFallbackReason::SearchStateCap,
                        "Grid routing search-state cap exceeded",
                        Some(scope),
                    ),
                ));
            }
            self.expanded_this_search += 1;

            if self.workspace.state_vertices[current as usize] as usize == self.target_id {
                self.consider_goal(current);
                continue;
            }

            let vertex_id = self.workspace.state_vertices[current as usize] as usize;
            let current_point = self.vertex_at(vertex_id)?.point;
            if let Some(compact_arcs) = self.topology.search_arcs(vertex_id) {
                let arcs: Vec<RouterSearchArc> = compact_arcs.to_vec();
                for arc in arcs {
                    self.relax_arc(
                        current,
                        current_point,
                        arc.to,
                        arc.orientation_ordinal,
                        arc.length,
                        f64::from(arc.boundary_transitions),
                    )?;
                }
            } else {
                let arcs: Vec<RouterArc> = self.topology.arcs(vertex_id).to_vec();
                for arc in arcs {
                    self.relax_arc(
                        current,
                        current_point,
                        arc.to,
                        orientation_ordinal(Some(arc.orientation)),
                        arc.length,
                        if arc.kind == crate::layout::grid::types::RouterArcKind::Portal {
                            1.0
                        } else {
                            0.0
                        },
                    )?;
                }
            }
            if let Some(metrics) = self.metrics.as_deref_mut() {
                metrics.max_open_set = metrics.max_open_set.max(self.heap_size);
            }
        }

        if self.best_goal_state < 0 || self.best_goal_cost.is_none() {
            self.commit_expanded_states();
            if self.record_outcome
                && let Some(metrics) = self.metrics.as_deref_mut()
            {
                metrics.routes_impossible += 1;
            }
            return Ok(None);
        }
        if self.record_outcome
            && let Some(metrics) = self.metrics.as_deref_mut()
        {
            metrics.routes_found += 1;
        }
        self.commit_expanded_states();
        let workspace = &self.workspace;
        let best_goal_cost = self.best_goal_cost.expect("checked above");
        Ok(Some(reconstruct(
            self.best_goal_state,
            &workspace.state_vertices,
            &workspace.predecessors,
            best_goal_cost,
            |id| self.topology.vertex(id).expect("validated vertex").point,
        )))
    }
}

pub fn find_shortest_route(
    topology: &dyn RouterSearchTopology,
    source_id: usize,
    target_id: usize,
    mut options: RouterSearchOptions<'_>,
) -> Result<Option<RouterSearchResult>, GridRoutingError> {
    if !options.topology_validated {
        validate_search_topology(topology)?;
    }
    if let Some(metrics) = options.metrics.as_deref_mut() {
        metrics.searches += 1;
    }
    assert_search_budget_available(
        options.metrics.as_deref(),
        options.caps.as_ref(),
        options.budget.as_deref(),
    )?;

    let metrics = options.metrics.take();
    let mut owned_workspace;
    let workspace: &mut RouterSearchWorkspace = match options.workspace.take() {
        Some(workspace) => workspace,
        None => {
            owned_workspace = RouterSearchWorkspace::new();
            &mut owned_workspace
        }
    };
    let heap_root = -1;
    let heap_size = 0;
    let initial_side = options.initial_side;
    let initial_orientation = options.initial_orientation;
    let initial_length = options.initial_length;
    let target_length = options.target_length;
    let target_side = options.target_side;
    let target_orientation = options.target_orientation;
    let endpoint_candidate_rank = options.endpoint_candidate_rank;
    let caps = options.caps;
    let queue_order = options.queue_order;
    let use_bend_heuristic = options.heuristic != RouterSearchHeuristic::Zero;
    let estimated_bytes_base = options.estimated_bytes_base.unwrap_or(0.0);
    let arc_allowed = options.arc_allowed;
    let record_outcome = options.record_outcome != Some(false);
    let budget = options.budget.take();
    let edge_cap = caps
        .and_then(|caps| caps.max_expanded_states)
        .unwrap_or(DEFAULT_MAX_EDGE_STATES);
    let invocation_cap = caps
        .and_then(|caps| caps.max_invocation_expanded_states)
        .unwrap_or(DEFAULT_MAX_INVOCATION_STATES);
    let invocation_start = budget
        .as_ref()
        .map(|budget| budget.expanded_states)
        .or_else(|| metrics.as_ref().map(|metrics| metrics.expanded_states))
        .unwrap_or(0);

    SearchRun {
        topology,
        workspace,
        metrics,
        budget,
        endpoint_candidate_rank,
        initial_orientation,
        initial_side,
        initial_length,
        target_orientation,
        target_side,
        target_length,
        caps,
        queue_order,
        use_bend_heuristic,
        estimated_bytes_base,
        arc_allowed,
        record_outcome,
        heap_root,
        heap_size,
        state_count: 1,
        expanded_this_search: 0,
        edge_cap,
        invocation_cap,
        invocation_start,
        source_id,
        target_id,
        best_goal_state: -1,
        best_goal_cost: None,
    }
    .run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::grid::router_instrumentation::create_grid_routing_instrumentation;
    use crate::layout::grid::router_topology::{
        BuildTopologyOptions, ContainerTopologyInput, RouterObstacleInput,
        build_container_routing_topology, build_endpoint_routing_overlay, routing_point_key,
    };
    use crate::layout::grid::types::{
        ContainerRoutingTopology, ROOT_CONTAINER_ID, RouterArcKind, RouterObstacle, RouterRect,
        RouterVertexKind,
    };

    fn build(obstacles: &[(&str, f64, f64, f64, f64)]) -> ContainerRoutingTopology {
        build_container_routing_topology(
            &ContainerTopologyInput {
                container_id: ROOT_CONTAINER_ID.to_string(),
                bounds: RouterRect {
                    left: 0.0,
                    right: 100.0,
                    top: 0.0,
                    bottom: 100.0,
                },
                obstacles: obstacles
                    .iter()
                    .map(|(id, left, right, top, bottom)| RouterObstacleInput {
                        id: (*id).to_string(),
                        bounds: RouterRect {
                            left: *left,
                            right: *right,
                            top: *top,
                            bottom: *bottom,
                        },
                    })
                    .collect(),
                title_exclusions: Vec::new(),
                portal_ranges: Vec::new(),
            },
            BuildTopologyOptions::default(),
        )
        .expect("topology builds")
    }

    fn vertex_at(topology: &ContainerRoutingTopology, point: RouterPoint) -> usize {
        topology
            .vertices
            .iter()
            .find(|candidate| candidate.point.x == point.x && candidate.point.y == point.y)
            .map(|vertex| vertex.id)
            .unwrap_or_else(|| panic!("missing topology vertex at {},{}", point.x, point.y))
    }

    fn route_cost(route: &RouterSearchResult) -> [f64; 2] {
        [route.cost[0], route.cost[1]]
    }

    #[test]
    fn compares_tuple_costs_lexicographically_without_epsilon() {
        assert_eq!(
            compare_tuple_cost(&[10.0, 2.0, 0.0, 0.0], &[10.0, 3.0, 0.0, 0.0]),
            Ordering::Less
        );
        assert_eq!(
            compare_tuple_cost(&[10.0000000001, 1.0, 0.0, 0.0], &[10.0, 99.0, 0.0, 0.0]),
            Ordering::Greater
        );
    }

    #[test]
    fn finds_a_deterministic_shortest_path_with_exact_bend_accounting() {
        let topology = build(&[]);
        let source = vertex_at(&topology, RouterPoint::new(0.0, 0.0));
        let target = vertex_at(&topology, RouterPoint::new(100.0, 100.0));
        let mut metrics = create_grid_routing_instrumentation();

        let route = find_shortest_route(
            &topology,
            source,
            target,
            RouterSearchOptions {
                metrics: Some(&mut metrics),
                ..Default::default()
            },
        )
        .expect("search succeeds")
        .expect("route exists");

        assert_eq!(route_cost(&route), [200.0, 1.0]);
        assert_eq!(
            route.points,
            vec![
                RouterPoint::new(0.0, 0.0),
                RouterPoint::new(0.0, 100.0),
                RouterPoint::new(100.0, 100.0),
            ]
        );
        assert_eq!(metrics.searches, 1);
        assert_eq!(metrics.routes_found, 1);
        assert!(metrics.expanded_states > 0);
        assert!(metrics.max_open_set > 0);
    }

    #[test]
    fn matches_the_dense_oracle_along_an_obstacle_boundary() {
        let topology = build(&[("center", 40.0, 60.0, 30.0, 70.0)]);
        // The obstacle inflates to y=24. Its boundary is routable; only its interior is blocked.
        let source = vertex_at(&topology, RouterPoint::new(0.0, 24.0));
        let target = vertex_at(&topology, RouterPoint::new(100.0, 24.0));

        let reduced = find_shortest_route(&topology, source, target, Default::default())
            .expect("search succeeds")
            .expect("route exists");
        let dense = find_dense_oracle_route(&DenseOracleInput {
            bounds: topology.bounds,
            obstacles: topology.obstacles.clone(),
            source: topology.vertices[source].point,
            target: topology.vertices[target].point,
        })
        .expect("dense route exists");

        assert_eq!(route_cost(&reduced), route_cost(&dense));
        assert_eq!(route_cost(&reduced), [100.0, 0.0]);
    }

    #[test]
    fn returns_the_same_canonical_route_with_zero_heuristic_and_perturbed_queue_order() {
        let topology = build(&[
            ("center", 40.0, 60.0, 30.0, 70.0),
            ("upper-left", 15.0, 25.0, 10.0, 25.0),
        ]);
        let overlay = build_endpoint_routing_overlay(
            &topology,
            RouterPoint::new(0.0, 50.0),
            RouterPoint::new(100.0, 50.0),
            None,
            None,
            &[],
        )
        .expect("overlay builds");
        let source = vertex_at(&overlay, RouterPoint::new(0.0, 50.0));
        let target = vertex_at(&overlay, RouterPoint::new(100.0, 50.0));
        let canonical =
            find_shortest_route(&overlay, source, target, Default::default()).expect("search");
        let zero = find_shortest_route(
            &overlay,
            source,
            target,
            RouterSearchOptions {
                heuristic: RouterSearchHeuristic::Zero,
                ..Default::default()
            },
        )
        .expect("search");
        let perturbed = find_shortest_route(
            &overlay,
            source,
            target,
            RouterSearchOptions {
                queue_order: RouterQueueOrder::Reverse,
                ..Default::default()
            },
        )
        .expect("search");

        assert_eq!(zero, canonical);
        assert_eq!(perturbed, canonical);
    }

    #[test]
    fn returns_an_already_found_route_when_search_state_caps_interrupt_tie_resolution() {
        let topology = build(&[]);
        let source = vertex_at(&topology, RouterPoint::new(0.0, 0.0));
        let target = vertex_at(&topology, RouterPoint::new(100.0, 100.0));
        let canonical =
            find_shortest_route(&topology, source, target, Default::default()).expect("search");
        let mut edge_metrics = create_grid_routing_instrumentation();
        let mut invocation_metrics = create_grid_routing_instrumentation();
        let mut budget = GridSearchBudget::default();

        let edge_capped = find_shortest_route(
            &topology,
            source,
            target,
            RouterSearchOptions {
                metrics: Some(&mut edge_metrics),
                caps: Some(RouterSearchCaps {
                    max_expanded_states: Some(3),
                    ..Default::default()
                }),
                queue_order: RouterQueueOrder::Reverse,
                ..Default::default()
            },
        )
        .expect("search");
        let invocation_capped = find_shortest_route(
            &topology,
            source,
            target,
            RouterSearchOptions {
                metrics: Some(&mut invocation_metrics),
                budget: Some(&mut budget),
                caps: Some(RouterSearchCaps {
                    max_invocation_expanded_states: Some(3),
                    ..Default::default()
                }),
                queue_order: RouterQueueOrder::Reverse,
                ..Default::default()
            },
        )
        .expect("search");

        assert_eq!(
            edge_capped.as_ref().map(|route| route.cost),
            canonical.as_ref().map(|route| route.cost)
        );
        assert_eq!(
            invocation_capped.as_ref().map(|route| route.cost),
            canonical.as_ref().map(|route| route.cost)
        );
        assert_eq!(
            edge_capped.expect("edge route").points,
            vec![
                RouterPoint::new(0.0, 0.0),
                RouterPoint::new(100.0, 0.0),
                RouterPoint::new(100.0, 100.0),
            ]
        );
        assert_eq!(
            invocation_capped.expect("invocation route").points,
            vec![
                RouterPoint::new(0.0, 0.0),
                RouterPoint::new(100.0, 0.0),
                RouterPoint::new(100.0, 100.0),
            ]
        );
        assert_eq!(edge_metrics.expanded_states, 3);
        assert_eq!(edge_metrics.routes_found, 1);
        assert_eq!(invocation_metrics.expanded_states, 3);
        assert_eq!(invocation_metrics.routes_found, 1);
        assert_eq!(budget.expanded_states, 3);
    }

    #[test]
    fn accounts_for_reusable_search_workspace_memory_and_enforces_its_cap() {
        let topology = build(&[]);
        let source = vertex_at(&topology, RouterPoint::new(0.0, 0.0));
        let target = vertex_at(&topology, RouterPoint::new(100.0, 100.0));
        let mut metrics = create_grid_routing_instrumentation();

        find_shortest_route(
            &topology,
            source,
            target,
            RouterSearchOptions {
                metrics: Some(&mut metrics),
                ..Default::default()
            },
        )
        .expect("search");
        assert!(metrics.search_workspace_bytes > 0.0);
        let error = find_shortest_route(
            &topology,
            source,
            target,
            RouterSearchOptions {
                caps: Some(RouterSearchCaps {
                    max_estimated_bytes: Some(1.0),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .expect_err("memory cap rejects the search");
        match error {
            GridRoutingError::ResourceLimit(error) => {
                assert_eq!(
                    error.reason,
                    crate::layout::grid::router_instrumentation::GridRoutingFallbackReason::EstimatedMemoryCap
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn reports_deterministic_search_state_cap_exhaustion() {
        let topology = build(&[("center", 40.0, 60.0, 30.0, 70.0)]);
        let source = vertex_at(&topology, RouterPoint::new(0.0, 24.0));
        let target = vertex_at(&topology, RouterPoint::new(100.0, 76.0));
        let mut metrics = create_grid_routing_instrumentation();

        let error = find_shortest_route(
            &topology,
            source,
            target,
            RouterSearchOptions {
                metrics: Some(&mut metrics),
                caps: Some(RouterSearchCaps {
                    max_expanded_states: Some(1),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .expect_err("state cap rejects the search");
        match error {
            GridRoutingError::ResourceLimit(error) => {
                assert_eq!(
                    error.reason,
                    crate::layout::grid::router_instrumentation::GridRoutingFallbackReason::SearchStateCap
                );
                assert_eq!(
                    error.search_state_scope,
                    Some(crate::layout::grid::router_errors::SearchStateScope::Edge)
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert_eq!(metrics.expanded_states, 1);
    }

    #[test]
    fn reuses_a_workspace_after_capped_searches() {
        let topology = build(&[("center", 40.0, 60.0, 30.0, 70.0)]);
        let source = vertex_at(&topology, RouterPoint::new(0.0, 24.0));
        let target = vertex_at(&topology, RouterPoint::new(100.0, 76.0));
        let mut workspace = RouterSearchWorkspace::new();

        assert!(
            find_shortest_route(
                &topology,
                source,
                target,
                RouterSearchOptions {
                    workspace: Some(&mut workspace),
                    caps: Some(RouterSearchCaps {
                        max_expanded_states: Some(1),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .is_err()
        );

        assert!(
            find_shortest_route(
                &topology,
                source,
                target,
                RouterSearchOptions {
                    workspace: Some(&mut workspace),
                    ..Default::default()
                },
            )
            .expect("search")
            .is_some()
        );
    }

    #[test]
    fn distinguishes_invocation_search_state_exhaustion() {
        let topology = build(&[("center", 40.0, 60.0, 30.0, 70.0)]);
        let source = vertex_at(&topology, RouterPoint::new(0.0, 24.0));
        let target = vertex_at(&topology, RouterPoint::new(100.0, 76.0));
        let mut workspace = RouterSearchWorkspace::new();
        let mut budget = GridSearchBudget { expanded_states: 1 };

        let error = find_shortest_route(
            &topology,
            source,
            target,
            RouterSearchOptions {
                workspace: Some(&mut workspace),
                budget: Some(&mut budget),
                caps: Some(RouterSearchCaps {
                    max_invocation_expanded_states: Some(1),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .expect_err("invocation budget rejects the search");
        match error {
            GridRoutingError::ResourceLimit(error) => {
                assert_eq!(
                    error.reason,
                    crate::layout::grid::router_instrumentation::GridRoutingFallbackReason::SearchStateCap
                );
                assert_eq!(
                    error.search_state_scope,
                    Some(crate::layout::grid::router_errors::SearchStateScope::Invocation)
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert_eq!(workspace.reset_count, 0);
    }

    #[test]
    fn rejects_malformed_graph_invariants_without_treating_them_as_no_route() {
        let topology = build(&[]);
        let source = vertex_at(&topology, RouterPoint::new(0.0, 0.0));
        let target = vertex_at(&topology, RouterPoint::new(100.0, 100.0));
        let mut adjacency = topology.adjacency.clone();
        adjacency.insert(
            source,
            vec![RouterArc {
                from: source,
                to: target,
                orientation: GridOrientation::H,
                length: f64::NAN,
                kind: RouterArcKind::Visibility,
                interval_start: 0.0,
                interval_end: 100.0,
            }],
        );
        let malformed = AdjacencySearchTopology {
            vertices: topology.vertices.clone(),
            adjacency,
        };

        let error = find_shortest_route(&malformed, source, target, Default::default())
            .expect_err("malformed arc is rejected");
        assert_eq!(
            error,
            GridRoutingError::geometry("Malformed grid routing arc")
        );
    }

    #[test]
    fn matches_dense_dijkstra_reachability_shortest_length_and_bends_for_10000_cases() {
        let mut state: u32 = 0x5eed_1234;
        let mut random = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            f64::from(state) / 4_294_967_296.0
        };

        for case_index in 0..10_000 {
            let obstacle_count = 1 + (random() * 3.0).floor() as usize;
            let mut obstacles = Vec::new();
            for index in 0..obstacle_count {
                let column = index % 2;
                let row = index / 2;
                let left = 16.0 + column as f64 * 44.0 + (random() * 8.0).floor();
                let top = 16.0 + row as f64 * 44.0 + (random() * 8.0).floor();
                let bounds = RouterRect {
                    left,
                    right: left + 12.0 + (random() * 10.0).floor(),
                    top,
                    bottom: top + 12.0 + (random() * 10.0).floor(),
                };
                obstacles.push(RouterObstacle {
                    id: format!("obstacle-{index}"),
                    left: bounds.left,
                    right: bounds.right,
                    top: bounds.top,
                    bottom: bounds.bottom,
                });
            }
            let topology = build_container_routing_topology(
                &ContainerTopologyInput {
                    container_id: ROOT_CONTAINER_ID.to_string(),
                    bounds: RouterRect {
                        left: 0.0,
                        right: 100.0,
                        top: 0.0,
                        bottom: 100.0,
                    },
                    obstacles: obstacles
                        .iter()
                        .map(|obstacle| RouterObstacleInput {
                            id: obstacle.id.clone(),
                            bounds: obstacle.bounds(),
                        })
                        .collect(),
                    title_exclusions: Vec::new(),
                    portal_ranges: Vec::new(),
                },
                BuildTopologyOptions::default(),
            )
            .expect("topology");
            let seeds: Vec<&crate::layout::grid::types::RouterVertex> = topology
                .vertices
                .iter()
                .filter(|vertex| vertex.kind != RouterVertexKind::Projection)
                .collect();
            let source = seeds[case_index % seeds.len()];
            let target = seeds[(case_index * 17 + 7) % seeds.len()];
            let overlay = build_endpoint_routing_overlay(
                &topology,
                source.point,
                target.point,
                None,
                None,
                &[],
            )
            .expect("overlay");
            let overlay_source = vertex_at(&overlay, source.point);
            let overlay_target = vertex_at(&overlay, target.point);
            let reduced =
                find_shortest_route(&overlay, overlay_source, overlay_target, Default::default())
                    .expect("search");
            let zero = find_shortest_route(
                &overlay,
                overlay_source,
                overlay_target,
                RouterSearchOptions {
                    heuristic: RouterSearchHeuristic::Zero,
                    ..Default::default()
                },
            )
            .expect("search");
            let perturbed = find_shortest_route(
                &overlay,
                overlay_source,
                overlay_target,
                RouterSearchOptions {
                    queue_order: RouterQueueOrder::Reverse,
                    ..Default::default()
                },
            )
            .expect("search");
            let dense = find_dense_oracle_route(&DenseOracleInput {
                bounds: topology.bounds,
                obstacles: topology.obstacles.clone(),
                source: source.point,
                target: target.point,
            });

            assert_eq!(
                reduced.is_some(),
                dense.is_some(),
                "reachability case {case_index}"
            );
            assert_eq!(
                reduced.as_ref().map(route_cost),
                dense.as_ref().map(route_cost),
                "cost case {case_index}"
            );
            assert_eq!(zero, reduced, "zero heuristic canonical case {case_index}");
            assert_eq!(
                perturbed, reduced,
                "queue perturbation canonical case {case_index}"
            );
        }
    }

    #[derive(Clone)]
    struct DenseOracleInput {
        bounds: RouterRect,
        obstacles: Vec<RouterObstacle>,
        source: RouterPoint,
        target: RouterPoint,
    }

    fn is_blocked_point(point: RouterPoint, obstacles: &[RouterObstacle]) -> bool {
        obstacles.iter().any(|obstacle| {
            point.x > obstacle.left
                && point.x < obstacle.right
                && point.y > obstacle.top
                && point.y < obstacle.bottom
        })
    }

    fn segment_blocked(
        orientation: GridOrientation,
        fixed: f64,
        start: f64,
        end: f64,
        obstacles: &[RouterObstacle],
    ) -> bool {
        let low = start.min(end);
        let high = start.max(end);
        obstacles.iter().any(|obstacle| {
            if orientation == GridOrientation::H {
                fixed > obstacle.top
                    && fixed < obstacle.bottom
                    && high > obstacle.left
                    && low < obstacle.right
            } else {
                fixed > obstacle.left
                    && fixed < obstacle.right
                    && high > obstacle.top
                    && low < obstacle.bottom
            }
        })
    }

    fn find_dense_oracle_route(input: &DenseOracleInput) -> Option<RouterSearchResult> {
        let mut xs = vec![
            input.bounds.left,
            input.bounds.right,
            input.source.x,
            input.target.x,
        ];
        for obstacle in &input.obstacles {
            xs.push(obstacle.left);
            xs.push(obstacle.right);
        }
        xs.sort_by(f64::total_cmp);
        xs.dedup();
        let mut ys = vec![
            input.bounds.top,
            input.bounds.bottom,
            input.source.y,
            input.target.y,
        ];
        for obstacle in &input.obstacles {
            ys.push(obstacle.top);
            ys.push(obstacle.bottom);
        }
        ys.sort_by(f64::total_cmp);
        ys.dedup();

        let mut vertices: Vec<RouterVertex> = Vec::new();
        let mut by_point: HashMap<String, usize> = HashMap::new();
        for y in &ys {
            for x in &xs {
                let point = RouterPoint::new(*x, *y);
                if !is_blocked_point(point, &input.obstacles) {
                    let id = vertices.len();
                    vertices.push(RouterVertex {
                        id,
                        point,
                        kind: RouterVertexKind::Projection,
                        owner_id: None,
                        side: None,
                    });
                    by_point.insert(routing_point_key(point), id);
                }
            }
        }
        let mut adjacency: HashMap<usize, Vec<RouterArc>> = vertices
            .iter()
            .map(|vertex| (vertex.id, Vec::new()))
            .collect();
        let mut connect_lines = |orientation: GridOrientation| {
            let coordinates = if orientation == GridOrientation::H {
                ys.clone()
            } else {
                xs.clone()
            };
            let varying = if orientation == GridOrientation::H {
                xs.clone()
            } else {
                ys.clone()
            };
            for fixed in coordinates {
                let mut previous: Option<usize> = None;
                for value in &varying {
                    let point = if orientation == GridOrientation::H {
                        RouterPoint::new(*value, fixed)
                    } else {
                        RouterPoint::new(fixed, *value)
                    };
                    let Some(id) = by_point.get(&routing_point_key(point)).copied() else {
                        continue;
                    };
                    if let Some(previous_id) = previous {
                        let previous_vertex = &vertices[previous_id];
                        let current_vertex = &vertices[id];
                        let start = if orientation == GridOrientation::H {
                            previous_vertex.point.x
                        } else {
                            previous_vertex.point.y
                        };
                        let end = if orientation == GridOrientation::H {
                            current_vertex.point.x
                        } else {
                            current_vertex.point.y
                        };
                        if !segment_blocked(orientation, fixed, start, end, &input.obstacles) {
                            let length = end - start;
                            adjacency
                                .get_mut(&previous_id)
                                .expect("adjacency")
                                .push(RouterArc {
                                    from: previous_id,
                                    to: id,
                                    orientation,
                                    length,
                                    kind: RouterArcKind::Visibility,
                                    interval_start: start,
                                    interval_end: end,
                                });
                            adjacency.get_mut(&id).expect("adjacency").push(RouterArc {
                                from: id,
                                to: previous_id,
                                orientation,
                                length,
                                kind: RouterArcKind::Visibility,
                                interval_start: start,
                                interval_end: end,
                            });
                        }
                    }
                    previous = Some(id);
                }
            }
        };
        connect_lines(GridOrientation::H);
        connect_lines(GridOrientation::V);
        let source_id = by_point.get(&routing_point_key(input.source)).copied();
        let target_id = by_point.get(&routing_point_key(input.target)).copied();
        let (Some(source_id), Some(target_id)) = (source_id, target_id) else {
            return None;
        };
        find_shortest_route(
            &AdjacencySearchTopology {
                vertices,
                adjacency,
            },
            source_id,
            target_id,
            RouterSearchOptions {
                heuristic: RouterSearchHeuristic::Zero,
                ..Default::default()
            },
        )
        .expect("dense search")
    }
}
