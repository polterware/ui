//! Port of `graphlib/lib/alg` (MIT, Copyright (c) 2012-2014 Chris Pettitt).
//!
//! The algorithms keep upstream's traversal order: container order follows node insertion
//! order, and where upstream iterates a JavaScript object the result order is the same.
//! Upstream's `''` predecessor sentinel is [`Option::None`], and its
//! `Number.POSITIVE_INFINITY` is [`f64::INFINITY`]. Upstream's `CycleException` is
//! [`GraphError::Cycle`]. Not ported here: `bellmanFord`, `shortestPaths`, and
//! `extractPath`.

use std::collections::{HashMap, HashSet};

use super::priority_queue::PriorityQueue;
use super::{Edge, Graph, GraphError};

/// A weight for an edge; upstream's `WeightFunction`.
pub type WeightFn<'a> = dyn Fn(&Edge) -> f64 + 'a;

/// The edges incident to a node; upstream's `EdgeFunction`.
pub type EdgeFn<'a> = dyn Fn(&str) -> Vec<Edge> + 'a;

/// One shortest path entry: the sum of weights from the source and the previous node.
/// Upstream stores `''` for "no predecessor"; this port stores `None`.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    /// The sum of edge weights from the source, or infinity when unreachable.
    pub distance: f64,
    /// The previous node on the shortest path, when there is one.
    pub predecessor: Option<String>,
}

impl Path {
    /// A path entry with `distance` and an optional predecessor.
    pub fn new(distance: f64, predecessor: Option<&str>) -> Self {
        Self {
            distance,
            predecessor: predecessor.map(str::to_string),
        }
    }
}

/// Shortest paths from one source, in the graph's node order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Paths(Vec<(String, Path)>);

impl Paths {
    /// The entry for `node`.
    pub fn get(&self, node: &str) -> Option<&Path> {
        self.0
            .iter()
            .find(|(id, _)| id == node)
            .map(|(_, path)| path)
    }

    /// The entries in order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Path)> {
        self.0.iter().map(|(id, path)| (id.as_str(), path))
    }

    /// The node ids in order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|(id, _)| id.as_str())
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no entries.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl FromIterator<(String, Path)> for Paths {
    fn from_iter<T: IntoIterator<Item = (String, Path)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// Shortest paths from every source, in the graph's node order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AllPaths(Vec<(String, Paths)>);

impl AllPaths {
    /// The paths that start at `source`.
    pub fn get(&self, source: &str) -> Option<&Paths> {
        self.0
            .iter()
            .find(|(id, _)| id == source)
            .map(|(_, paths)| paths)
    }

    /// The entries in order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Paths)> {
        self.0.iter().map(|(id, paths)| (id.as_str(), paths))
    }

    /// The number of sources.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no sources.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Pre-order or post-order traversal, upstream's `"pre" | "post"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Traversal {
    /// Visit a node before its successors.
    Pre,
    /// Visit a node after its successors.
    Post,
}

fn unit_weight(_: &Edge) -> f64 {
    1.0
}

/// Connected components of the graph, each component in visit order.
pub fn components<G, N, E>(graph: &Graph<G, N, E>) -> Vec<Vec<String>> {
    fn dfs<G, N, E>(
        graph: &Graph<G, N, E>,
        v: &str,
        visited: &mut HashSet<String>,
        component: &mut Vec<String>,
    ) {
        if !visited.insert(v.to_string()) {
            return;
        }
        component.push(v.to_string());
        for w in graph.successors(v).unwrap_or_default() {
            dfs(graph, &w, visited, component);
        }
        for w in graph.predecessors(v).unwrap_or_default() {
            dfs(graph, &w, visited, component);
        }
    }

    let mut visited = HashSet::new();
    let mut components = Vec::new();
    for v in graph.nodes() {
        let mut component = Vec::new();
        dfs(graph, &v, &mut visited, &mut component);
        if !component.is_empty() {
            components.push(component);
        }
    }
    components
}

/// Dijkstra's algorithm: shortest paths from `source` to every node. Fails on a negative
/// edge weight that the traversal reaches.
pub fn dijkstra<G, N, E>(
    graph: &Graph<G, N, E>,
    source: &str,
    weight_fn: Option<&WeightFn<'_>>,
    edge_fn: Option<&EdgeFn<'_>>,
) -> Result<Paths, GraphError> {
    let weight_fn = weight_fn.unwrap_or(&unit_weight);
    let default_edge_fn = |v: &str| graph.out_edges(v, None).unwrap_or_default();
    let edge_fn = edge_fn.unwrap_or(&default_edge_fn);
    run_dijkstra(graph, source, weight_fn, edge_fn)
}

fn run_dijkstra<G, N, E>(
    graph: &Graph<G, N, E>,
    source: &str,
    weight_fn: &WeightFn<'_>,
    edge_fn: &EdgeFn<'_>,
) -> Result<Paths, GraphError> {
    let mut results: Vec<(String, Path)> = Vec::new();
    let mut positions: HashMap<String, usize> = HashMap::new();
    let mut queue = PriorityQueue::new();

    for v in graph.nodes() {
        let distance = if v == source { 0.0 } else { f64::INFINITY };
        positions.insert(v.clone(), results.len());
        results.push((
            v.clone(),
            Path {
                distance,
                predecessor: None,
            },
        ));
        queue.add(&v, distance);
    }

    while queue.size() > 0 {
        let v = queue.remove_min();
        let Some(&v_position) = positions.get(&v) else {
            continue;
        };
        let v_distance = results[v_position].1.distance;
        if v_distance == f64::INFINITY {
            break;
        }
        for edge in edge_fn(&v) {
            let w = if edge.v != v { &edge.v } else { &edge.w };
            let Some(&w_position) = positions.get(w) else {
                continue;
            };
            let w_distance = results[w_position].1.distance;
            let weight = weight_fn(&edge);
            let distance = v_distance + weight;
            if weight < 0.0 {
                return Err(GraphError::NegativeEdgeWeight { edge, weight });
            }
            if distance < w_distance {
                results[w_position].1.distance = distance;
                results[w_position].1.predecessor = Some(v.clone());
                queue.decrease(w, distance);
            }
        }
    }

    Ok(results.into_iter().collect())
}

/// Dijkstra's algorithm from every node.
pub fn dijkstra_all<G, N, E>(
    graph: &Graph<G, N, E>,
    weight_fn: Option<&WeightFn<'_>>,
    edge_fn: Option<&EdgeFn<'_>>,
) -> Result<AllPaths, GraphError> {
    let mut all = Vec::new();
    for source in graph.nodes() {
        let paths = dijkstra(graph, &source, weight_fn, edge_fn)?;
        all.push((source, paths));
    }
    Ok(AllPaths(all))
}

/// All nodes that are part of a cycle, one strongly connected component per cycle.
pub fn find_cycles<G, N, E>(graph: &Graph<G, N, E>) -> Vec<Vec<String>> {
    tarjan(graph)
        .into_iter()
        .filter(|component| match component.first() {
            None => false,
            Some(first) => {
                component.len() > 1
                    || graph
                        .out_edges(first, Some(first))
                        .is_some_and(|edges| !edges.is_empty())
            }
        })
        .collect()
}

/// Floyd-Warshall: shortest paths from every node to every other node, including negative
/// weights.
#[allow(clippy::needless_range_loop)] // The three indices drive three parallel matrices.
pub fn floyd_warshall<G, N, E>(
    graph: &Graph<G, N, E>,
    weight_fn: Option<&WeightFn<'_>>,
    edge_fn: Option<&EdgeFn<'_>>,
) -> Result<AllPaths, GraphError> {
    let weight_fn = weight_fn.unwrap_or(&unit_weight);
    let default_edge_fn = |v: &str| graph.out_edges(v, None).unwrap_or_default();
    let edge_fn = edge_fn.unwrap_or(&default_edge_fn);

    let nodes = graph.nodes();
    let index: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(position, node)| (node.as_str(), position))
        .collect();
    let size = nodes.len();
    let mut distance = vec![vec![f64::INFINITY; size]; size];
    let mut predecessor: Vec<Vec<Option<String>>> = vec![vec![None; size]; size];

    for (i, v) in nodes.iter().enumerate() {
        distance[i][i] = 0.0;
        for edge in edge_fn(v) {
            let w = if edge.v == *v { &edge.w } else { &edge.v };
            if let Some(&j) = index.get(w.as_str()) {
                distance[i][j] = weight_fn(&edge);
                predecessor[i][j] = Some(v.clone());
            }
        }
    }

    for k in 0..size {
        for i in 0..size {
            for j in 0..size {
                let alternative = distance[i][k] + distance[k][j];
                if alternative < distance[i][j] {
                    distance[i][j] = alternative;
                    predecessor[i][j] = predecessor[k][j].clone();
                }
            }
        }
    }

    let all = nodes
        .iter()
        .enumerate()
        .map(|(i, source)| {
            let paths = nodes
                .iter()
                .enumerate()
                .map(|(j, target)| {
                    (
                        target.clone(),
                        Path {
                            distance: distance[i][j],
                            predecessor: predecessor[i][j].clone(),
                        },
                    )
                })
                .collect();
            (source.clone(), paths)
        })
        .collect();
    Ok(AllPaths(all))
}

/// Whether the graph has no cycles.
pub fn is_acyclic<G, N, E>(graph: &Graph<G, N, E>) -> bool {
    topsort(graph).is_ok()
}

/// Post-order depth-first traversal from `roots`. Fails when a root is not in the graph.
pub fn postorder<G, N, E>(
    graph: &Graph<G, N, E>,
    roots: &[&str],
) -> Result<Vec<String>, GraphError> {
    reduce(graph, roots, Traversal::Post, Vec::new(), |mut acc, v| {
        acc.push(v.to_string());
        acc
    })
}

/// Pre-order depth-first traversal from `roots`. Fails when a root is not in the graph.
pub fn preorder<G, N, E>(
    graph: &Graph<G, N, E>,
    roots: &[&str],
) -> Result<Vec<String>, GraphError> {
    reduce(graph, roots, Traversal::Pre, Vec::new(), |mut acc, v| {
        acc.push(v.to_string());
        acc
    })
}

/// Minimum spanning tree of a connected undirected graph. Fails when the graph is not
/// connected.
pub fn prim<G, N, E>(
    graph: &Graph<G, N, E>,
    weight_fn: &WeightFn<'_>,
) -> Result<Graph<(), (), ()>, GraphError> {
    let mut result = Graph::new();
    let mut parents: HashMap<String, String> = HashMap::new();
    let mut queue = PriorityQueue::new();

    if graph.node_count() == 0 {
        return Ok(result);
    }

    let nodes = graph.nodes();
    for v in &nodes {
        queue.add(v, f64::INFINITY);
        result.set_node(v.clone());
    }

    let first = &nodes[0];
    queue.decrease(first, 0.0);

    let mut initialized = false;
    while queue.size() > 0 {
        let v = queue.remove_min();
        if let Some(parent) = parents.get(&v).cloned() {
            result.set_edge(&v, &parent);
        } else if initialized {
            return Err(GraphError::NotConnected);
        } else {
            initialized = true;
        }

        for edge in graph.node_edges(&v, None).unwrap_or_default() {
            let w = if edge.v == v { &edge.w } else { &edge.v };
            if let Some(priority) = queue.priority(w) {
                let weight = weight_fn(&edge);
                if weight < priority {
                    parents.insert(w.clone(), v.clone());
                    queue.decrease(w, weight);
                }
            }
        }
    }

    Ok(result)
}

/// A pre- or post-order traversal that folds the visited nodes into an accumulator. Fails
/// when a root is not in the graph.
pub fn reduce<G, N, E, T>(
    graph: &Graph<G, N, E>,
    roots: &[&str],
    order: Traversal,
    initial: T,
    mut visit: impl FnMut(T, &str) -> T,
) -> Result<T, GraphError> {
    let mut visited = HashSet::new();
    let mut accumulator = initial;
    for &root in roots {
        if !graph.has_node(root) {
            return Err(GraphError::GraphDoesNotHaveNode {
                node: root.to_string(),
            });
        }
        accumulator = reduce_from(graph, root, order, &mut visited, &mut visit, accumulator);
    }
    Ok(accumulator)
}

fn reduce_from<G, N, E, T>(
    graph: &Graph<G, N, E>,
    v: &str,
    order: Traversal,
    visited: &mut HashSet<String>,
    visit: &mut dyn FnMut(T, &str) -> T,
    mut accumulator: T,
) -> T {
    if visited.insert(v.to_string()) {
        if order == Traversal::Pre {
            accumulator = visit(accumulator, v);
        }
        let navigation = if graph.is_directed() {
            graph.successors(v)
        } else {
            graph.neighbors(v)
        };
        for w in navigation.unwrap_or_default() {
            accumulator = reduce_from(graph, &w, order, visited, visit, accumulator);
        }
        if order == Traversal::Post {
            accumulator = visit(accumulator, v);
        }
    }
    accumulator
}

/// Tarjan's algorithm: the strongly connected components of the graph.
pub fn tarjan<G, N, E>(graph: &Graph<G, N, E>) -> Vec<Vec<String>> {
    #[derive(Clone, Copy)]
    struct Visited {
        on_stack: bool,
        lowlink: usize,
        index: usize,
    }

    struct State {
        index: usize,
        stack: Vec<String>,
        visited: HashMap<String, Visited>,
        results: Vec<Vec<String>>,
    }

    fn dfs<G, N, E>(graph: &Graph<G, N, E>, v: &str, state: &mut State) {
        let entry_index = state.index;
        state.index += 1;
        state.visited.insert(
            v.to_string(),
            Visited {
                on_stack: true,
                lowlink: entry_index,
                index: entry_index,
            },
        );
        state.stack.push(v.to_string());

        for w in graph.successors(v).unwrap_or_default() {
            if !state.visited.contains_key(&w) {
                dfs(graph, &w, state);
                let w_lowlink = state.visited[&w].lowlink;
                let entry = state
                    .visited
                    .get_mut(v)
                    .expect("the current node is visited");
                entry.lowlink = entry.lowlink.min(w_lowlink);
            } else if state.visited[&w].on_stack {
                let w_index = state.visited[&w].index;
                let entry = state
                    .visited
                    .get_mut(v)
                    .expect("the current node is visited");
                entry.lowlink = entry.lowlink.min(w_index);
            }
        }

        let entry = state.visited[v];
        if entry.lowlink == entry.index {
            let mut component = Vec::new();
            loop {
                let w = state.stack.pop().expect("the component is on the stack");
                if let Some(w_entry) = state.visited.get_mut(&w) {
                    w_entry.on_stack = false;
                }
                let done = w == v;
                component.push(w);
                if done {
                    break;
                }
            }
            state.results.push(component);
        }
    }

    let mut state = State {
        index: 0,
        stack: Vec::new(),
        visited: HashMap::new(),
        results: Vec::new(),
    };
    for v in graph.nodes() {
        if !state.visited.contains_key(&v) {
            dfs(graph, &v, &mut state);
        }
    }
    state.results
}

/// A topological order: for every edge `u -> v`, `u` comes before `v`. Fails with
/// [`GraphError::Cycle`] when the graph has a cycle.
pub fn topsort<G, N, E>(graph: &Graph<G, N, E>) -> Result<Vec<String>, GraphError> {
    fn visit<G, N, E>(
        graph: &Graph<G, N, E>,
        node: &str,
        visited: &mut HashSet<String>,
        stack: &mut HashSet<String>,
        results: &mut Vec<String>,
    ) -> Result<(), GraphError> {
        if stack.contains(node) {
            return Err(GraphError::Cycle);
        }
        if !visited.contains(node) {
            stack.insert(node.to_string());
            visited.insert(node.to_string());
            for predecessor in graph.predecessors(node).unwrap_or_default() {
                visit(graph, &predecessor, visited, stack, results)?;
            }
            stack.remove(node);
            results.push(node.to_string());
        }
        Ok(())
    }

    let mut visited = HashSet::new();
    let mut stack = HashSet::new();
    let mut results = Vec::new();
    for sink in graph.sinks() {
        visit(graph, &sink, &mut visited, &mut stack, &mut results)?;
    }
    if visited.len() != graph.node_count() {
        return Err(GraphError::Cycle);
    }
    Ok(results)
}
