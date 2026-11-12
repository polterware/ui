//! Port of the graphlib that dagre-d3-es 7.0.14 bundles
//! (`src/graphlib/` in
//! `research/mermaid-port/upstream/mermaid/node_modules/.pnpm/dagre-d3-es@7.0.14`):
//! a directed multigraph with compound (parent) nodes.
//!
//! Follows its `graph.js`, `json.js`, and `alg/` (MIT, Copyright (c) 2012-2014 Chris
//! Pettitt), which match `@dagrejs/graphlib`, the upstream checkout this port earlier
//! followed; no behavior differs between the two. Node ids stay strings and graph,
//! node, and edge labels are generics.
//! Iteration keeps upstream's insertion order (JavaScript object key order), which dagre's
//! layout depends on. Algorithms live in [`alg`] and JSON serialization in [`json`].
//!
//! Where JavaScript throws, the fallible methods return [`GraphError`]; the conditions are
//! the same. `undefined` labels become `Option::None`.

pub mod alg;
pub mod json;
mod ordered_map;
mod priority_queue;

#[cfg(test)]
mod alg_tests;
#[cfg(test)]
mod graph_tests;
#[cfg(test)]
mod json_tests;

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use ordered_map::OrderedMap;

/// A factory for the default node label, upstream's `NodeLabelFactory`.
type NodeLabelFn<N> = dyn Fn(&str) -> Option<N>;

/// A factory for the default edge label, upstream's `EdgeLabelFactory`.
type EdgeLabelFn<E> = dyn Fn(&str, &str, Option<&str>) -> Option<E>;

/// Sentinel name of the edge without a name, from upstream's `DEFAULT_EDGE_NAME`.
const DEFAULT_EDGE_NAME: &str = "\x00";
/// Sentinel id of the root node in a compound graph, from upstream's `GRAPH_NODE`.
const GRAPH_NODE: &str = "\x00";

/// Options for [`Graph::with_options`]. Upstream's constructor defaults each flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphOptions {
    /// Whether edges have an orientation. Defaults to `true`.
    pub directed: bool,
    /// Whether a pair of nodes can have multiple edges. Defaults to `false`.
    pub multigraph: bool,
    /// Whether a node can have subnodes. Defaults to `false`.
    pub compound: bool,
}

impl Default for GraphOptions {
    fn default() -> Self {
        Self {
            directed: true,
            multigraph: false,
            compound: false,
        }
    }
}

impl GraphOptions {
    /// A simple directed graph, upstream's default.
    pub fn directed() -> Self {
        Self::default()
    }

    /// An undirected graph.
    pub fn undirected() -> Self {
        Self {
            directed: false,
            ..Self::default()
        }
    }

    /// A directed multigraph.
    pub fn multigraph() -> Self {
        Self {
            multigraph: true,
            ..Self::default()
        }
    }

    /// A directed compound graph.
    pub fn compound() -> Self {
        Self {
            compound: true,
            ..Self::default()
        }
    }
}

/// An edge descriptor: enough to identify an edge. Mirrors upstream's `Edge`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Edge {
    /// Source node id.
    pub v: String,
    /// Target node id. For an undirected graph the pair is in canonical (sorted) order.
    pub w: String,
    /// Name of the edge. Only multigraphs use names.
    pub name: Option<String>,
}

impl Edge {
    /// An unnamed edge from `v` to `w`.
    pub fn new(v: impl Into<String>, w: impl Into<String>) -> Self {
        Self {
            v: v.into(),
            w: w.into(),
            name: None,
        }
    }

    /// A named edge from `v` to `w`.
    pub fn named(v: impl Into<String>, w: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            v: v.into(),
            w: w.into(),
            name: Some(name.into()),
        }
    }

    /// The edge name, if any.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

/// A failure of a graph operation. The variants mirror the exceptions upstream throws.
#[derive(Clone, Debug, PartialEq)]
pub enum GraphError {
    /// `setParent` on a graph created without the compound option.
    NonCompoundParent,
    /// A parent assignment that would make a node its own ancestor.
    ParentCycle { node: String, parent: String },
    /// A named edge on a graph created without the multigraph option.
    NamedEdgeInSimpleGraph,
    /// A traversal root that is not in the graph.
    GraphDoesNotHaveNode { node: String },
    /// A minimum spanning tree over a disconnected graph.
    NotConnected,
    /// Upstream's `CycleException`: a topological order does not exist.
    Cycle,
    /// Dijkstra encountered a negative edge weight.
    NegativeEdgeWeight { edge: Edge, weight: f64 },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonCompoundParent => f.write_str("Cannot set parent in a non-compound graph"),
            Self::ParentCycle { node, parent } => {
                write!(
                    f,
                    "Setting {parent} as parent of {node} would create a cycle"
                )
            }
            Self::NamedEdgeInSimpleGraph => {
                f.write_str("Cannot set a named edge when isMultigraph = false")
            }
            Self::GraphDoesNotHaveNode { node } => write!(f, "Graph does not have node: {node}"),
            Self::NotConnected => f.write_str("Input graph is not connected"),
            Self::Cycle => f.write_str("CycleException"),
            Self::NegativeEdgeWeight { edge, weight } => write!(
                f,
                "dijkstra does not allow negative edge weights. Bad edge: {} -> {} Weight: {weight}",
                edge.v, edge.w
            ),
        }
    }
}

impl std::error::Error for GraphError {}

/// A graph with string node ids and generic graph, node, and edge labels.
pub struct Graph<G = (), N = (), E = ()> {
    directed: bool,
    multigraph: bool,
    compound: bool,
    label: Option<G>,
    nodes: OrderedMap<Option<N>>,
    /// v -> edge id -> edge.
    in_edges: OrderedMap<OrderedMap<Edge>>,
    /// u -> v -> count.
    preds: OrderedMap<OrderedMap<usize>>,
    /// v -> edge id -> edge.
    out_edges: OrderedMap<OrderedMap<Edge>>,
    /// v -> w -> count.
    sucs: OrderedMap<OrderedMap<usize>>,
    /// edge id -> edge.
    edge_objs: OrderedMap<Edge>,
    /// edge id -> label.
    edge_labels: OrderedMap<Option<E>>,
    node_count: usize,
    edge_count: usize,
    /// v -> parent id (the root uses the `GRAPH_NODE` sentinel).
    parent: Option<OrderedMap<String>>,
    /// v -> child -> true.
    children: Option<OrderedMap<OrderedMap<bool>>>,
    default_node_label: Box<NodeLabelFn<N>>,
    default_edge_label: Box<EdgeLabelFn<E>>,
}

impl<G, N, E> Default for Graph<G, N, E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<G, N, E> Graph<G, N, E> {
    /// Creates a simple directed graph.
    pub fn new() -> Self {
        Self::with_options(GraphOptions::default())
    }

    /// Creates a graph with the given options.
    pub fn with_options(options: GraphOptions) -> Self {
        let children = options.compound.then(|| {
            let mut children = OrderedMap::new();
            children.insert(GRAPH_NODE.to_string(), OrderedMap::new());
            children
        });
        Self {
            directed: options.directed,
            multigraph: options.multigraph,
            compound: options.compound,
            label: None,
            nodes: OrderedMap::new(),
            in_edges: OrderedMap::new(),
            preds: OrderedMap::new(),
            out_edges: OrderedMap::new(),
            sucs: OrderedMap::new(),
            edge_objs: OrderedMap::new(),
            edge_labels: OrderedMap::new(),
            node_count: 0,
            edge_count: 0,
            parent: options.compound.then(OrderedMap::new),
            children,
            default_node_label: Box::new(|_: &str| None),
            default_edge_label: Box::new(|_: &str, _: &str, _: Option<&str>| None),
        }
    }

    /// Whether the graph edges have an orientation.
    pub fn is_directed(&self) -> bool {
        self.directed
    }

    /// Whether a pair of nodes can have multiple edges.
    pub fn is_multigraph(&self) -> bool {
        self.multigraph
    }

    /// Whether a node can have subnodes.
    pub fn is_compound(&self) -> bool {
        self.compound
    }

    // === Graph functions =====================================================

    /// Sets the label of the graph.
    pub fn set_graph(&mut self, label: G) -> &mut Self {
        self.label = Some(label);
        self
    }

    /// Sets or clears the label of the graph.
    pub fn set_graph_label(&mut self, label: Option<G>) -> &mut Self {
        self.label = label;
        self
    }

    /// The graph label, if one was set.
    pub fn graph(&self) -> Option<&G> {
        self.label.as_ref()
    }

    /// The graph label, for in-place updates.
    pub fn graph_mut(&mut self) -> Option<&mut G> {
        self.label.as_mut()
    }

    /// Sets the default node label assigned to nodes created without one.
    pub fn set_default_node_label(&mut self, label: N) -> &mut Self
    where
        N: Clone + 'static,
    {
        self.default_node_label = Box::new(move |_| Some(label.clone()));
        self
    }

    /// Sets a factory for the default node label.
    pub fn set_default_node_label_fn<F>(&mut self, factory: F) -> &mut Self
    where
        F: Fn(&str) -> N + 'static,
        N: 'static,
    {
        self.default_node_label = Box::new(move |v| Some(factory(v)));
        self
    }

    /// Sets a factory for the default node label that can produce `None` (upstream's
    /// `undefined`).
    pub fn set_default_node_label_option_fn<F>(&mut self, factory: F) -> &mut Self
    where
        F: Fn(&str) -> Option<N> + 'static,
        N: 'static,
    {
        self.default_node_label = Box::new(factory);
        self
    }

    /// The number of nodes.
    pub fn node_count(&self) -> usize {
        self.node_count
    }

    // === Node functions ======================================================

    /// All node ids, in insertion order. Subnodes of a compound graph are included.
    pub fn nodes(&self) -> Vec<String> {
        self.nodes.keys_vec()
    }

    /// Nodes without in-edges, in insertion order.
    pub fn sources(&self) -> Vec<String> {
        self.nodes()
            .into_iter()
            .filter(|v| self.in_edges.get(v).is_none_or(OrderedMap::is_empty))
            .collect()
    }

    /// Nodes without out-edges, in insertion order.
    pub fn sinks(&self) -> Vec<String> {
        self.nodes()
            .into_iter()
            .filter(|v| self.out_edges.get(v).is_none_or(OrderedMap::is_empty))
            .collect()
    }

    /// Creates each name in `names` with the default label.
    pub fn set_nodes<S: AsRef<str>>(&mut self, names: &[S]) -> &mut Self {
        for name in names {
            self.set_node(name.as_ref());
        }
        self
    }

    /// Creates each name in `names` with `label`. Each node receives a clone of the label;
    /// JavaScript shares one object across the nodes.
    pub fn set_nodes_with<S: AsRef<str>>(&mut self, names: &[S], label: N) -> &mut Self
    where
        N: Clone,
    {
        for name in names {
            self.set_node_with(name.as_ref(), label.clone());
        }
        self
    }

    /// Creates or updates the node `v`, assigning the default label when it is new.
    pub fn set_node(&mut self, v: impl Into<String>) -> &mut Self {
        self.upsert_node(v.into(), None, false);
        self
    }

    /// Creates or updates the node `v` with `label`.
    pub fn set_node_with(&mut self, v: impl Into<String>, label: N) -> &mut Self {
        self.upsert_node(v.into(), Some(label), true);
        self
    }

    /// Creates or updates the node `v` with an explicit optional label. `None` clears the
    /// label, like upstream's `setNode(v, undefined)`.
    pub fn set_node_label(&mut self, v: impl Into<String>, label: Option<N>) -> &mut Self {
        self.upsert_node(v.into(), label, true);
        self
    }

    /// The label of node `v`, or `None` when the node is absent or has no label.
    pub fn node(&self, v: &str) -> Option<&N> {
        self.nodes.get(v).and_then(Option::as_ref)
    }

    /// The label of node `v`, for in-place updates.
    pub fn node_mut(&mut self, v: &str) -> Option<&mut N> {
        self.nodes.get_mut(v).and_then(Option::as_mut)
    }

    /// Whether the graph has a node `v`.
    pub fn has_node(&self, v: &str) -> bool {
        self.nodes.contains_key(v)
    }

    /// Removes `v` and its incident edges. Does nothing when `v` is absent.
    pub fn remove_node(&mut self, v: &str) -> &mut Self {
        if !self.nodes.contains_key(v) {
            return self;
        }
        self.nodes.remove(v);
        if self.compound {
            self.remove_from_parents_child_list(v);
            if let Some(parent) = &mut self.parent {
                parent.remove(v);
            }
            let children = self.children(v);
            for child in children {
                self.set_parent(&child, None)
                    .expect("a child of a removed node can always be re-parented to the root");
            }
            if let Some(children) = &mut self.children {
                children.remove(v);
            }
        }
        let in_edges = self
            .in_edges
            .get(v)
            .map(OrderedMap::keys_vec)
            .unwrap_or_default();
        for id in in_edges {
            self.remove_edge_by_id(&id);
        }
        self.in_edges.remove(v);
        self.preds.remove(v);
        let out_edges = self
            .out_edges
            .get(v)
            .map(OrderedMap::keys_vec)
            .unwrap_or_default();
        for id in out_edges {
            self.remove_edge_by_id(&id);
        }
        self.out_edges.remove(v);
        self.sucs.remove(v);
        self.node_count -= 1;
        self
    }

    /// Makes `parent` the parent of `v`; `None` moves `v` to the root. Fails on a non-compound
    /// graph or when the assignment would create a cycle.
    pub fn set_parent(&mut self, v: &str, parent: Option<&str>) -> Result<&mut Self, GraphError> {
        if !self.compound {
            return Err(GraphError::NonCompoundParent);
        }
        let parent = match parent {
            Some(parent) => {
                let parent = parent.to_string();
                let mut ancestor = Some(parent.clone());
                while let Some(current) = ancestor {
                    if current == v {
                        return Err(GraphError::ParentCycle {
                            node: v.to_string(),
                            parent,
                        });
                    }
                    ancestor = self.parent(&current).map(str::to_string);
                }
                self.set_node(parent.clone());
                Some(parent)
            }
            None => None,
        };
        self.set_node(v);
        self.remove_from_parents_child_list(v);
        let key = parent.unwrap_or_else(|| GRAPH_NODE.to_string());
        if let Some(parents) = &mut self.parent {
            parents.insert(v.to_string(), key.clone());
        }
        if let Some(children) = &mut self.children
            && let Some(list) = children.get_mut(&key)
        {
            list.insert(v.to_string(), true);
        }
        Ok(self)
    }

    /// The parent of `v`, if the graph is compound and `v` is not at the root.
    pub fn parent(&self, v: &str) -> Option<&str> {
        if !self.compound {
            return None;
        }
        match self.parent.as_ref()?.get(v) {
            Some(parent) if parent != GRAPH_NODE => Some(parent),
            _ => None,
        }
    }

    /// The direct children of `v`, in insertion order. Empty when `v` is unknown or, for a
    /// non-compound graph, when `v` is not the root.
    pub fn children(&self, v: &str) -> Vec<String> {
        if self.compound {
            self.children
                .as_ref()
                .and_then(|children| children.get(v))
                .map(OrderedMap::keys_vec)
                .unwrap_or_default()
        } else if v == GRAPH_NODE {
            self.nodes()
        } else {
            Vec::new()
        }
    }

    /// The nodes without a parent, in insertion order. Upstream's `children()` with no
    /// argument.
    pub fn root_children(&self) -> Vec<String> {
        self.children(GRAPH_NODE)
    }

    /// Predecessors of `v`, or `None` when `v` is not in the graph.
    pub fn predecessors(&self, v: &str) -> Option<Vec<String>> {
        self.preds.get(v).map(OrderedMap::keys_vec)
    }

    /// Successors of `v`, or `None` when `v` is not in the graph.
    pub fn successors(&self, v: &str) -> Option<Vec<String>> {
        self.sucs.get(v).map(OrderedMap::keys_vec)
    }

    /// Predecessors and successors of `v`, or `None` when `v` is not in the graph.
    pub fn neighbors(&self, v: &str) -> Option<Vec<String>> {
        let predecessors = self.predecessors(v)?;
        let mut seen: HashSet<String> = predecessors.iter().cloned().collect();
        let mut neighbors = predecessors;
        for successor in self.successors(v).unwrap_or_default() {
            if seen.insert(successor.clone()) {
                neighbors.push(successor);
            }
        }
        Some(neighbors)
    }

    /// Whether `v` has no successors in a directed graph or no neighbors in an undirected
    /// one.
    pub fn is_leaf(&self, v: &str) -> bool {
        if self.directed {
            self.successors(v)
                .is_none_or(|successors| successors.is_empty())
        } else {
            self.neighbors(v)
                .is_none_or(|neighbors| neighbors.is_empty())
        }
    }

    /// A copy of the graph that keeps only the nodes accepted by `filter`, together with the
    /// edges between them. In a compound graph a kept node moves to the nearest kept ancestor.
    pub fn filter_nodes(&self, filter: impl Fn(&str) -> bool) -> Self
    where
        G: Clone,
        N: Clone,
        E: Clone,
    {
        let mut copy = Self::with_options(GraphOptions {
            directed: self.directed,
            multigraph: self.multigraph,
            compound: self.compound,
        });
        if let Some(label) = &self.label {
            copy.set_graph(label.clone());
        }
        for v in self.nodes() {
            if filter(&v) {
                copy.set_node_label(v.clone(), self.node(&v).cloned());
            }
        }
        for edge in self.edges() {
            if copy.has_node(&edge.v) && copy.has_node(&edge.w) {
                copy.set_edge_object_label(&edge, self.edge_object(&edge).cloned())
                    .expect("the copy keeps the source's multigraph option");
            }
        }
        if self.compound {
            let mut parents = HashMap::new();
            for v in copy.nodes() {
                let parent = find_parent(self, &copy, &mut parents, &v);
                copy.set_parent(&v, parent.as_deref())
                    .expect("a kept parent cannot create a cycle in the copy");
            }
        }
        copy
    }

    /// Sets the default edge label assigned to edges created without one.
    pub fn set_default_edge_label(&mut self, label: E) -> &mut Self
    where
        E: Clone + 'static,
    {
        self.default_edge_label = Box::new(move |_, _, _| Some(label.clone()));
        self
    }

    /// Sets a factory for the default edge label.
    pub fn set_default_edge_label_fn<F>(&mut self, factory: F) -> &mut Self
    where
        F: Fn(&str, &str, Option<&str>) -> E + 'static,
        E: 'static,
    {
        self.default_edge_label = Box::new(move |v, w, name| Some(factory(v, w, name)));
        self
    }

    /// Sets a factory for the default edge label that can produce `None` (upstream's
    /// `undefined`).
    pub fn set_default_edge_label_option_fn<F>(&mut self, factory: F) -> &mut Self
    where
        F: Fn(&str, &str, Option<&str>) -> Option<E> + 'static,
        E: 'static,
    {
        self.default_edge_label = Box::new(factory);
        self
    }

    /// The number of edges.
    pub fn edge_count(&self) -> usize {
        self.edge_count
    }

    /// All edges, in insertion order.
    pub fn edges(&self) -> Vec<Edge> {
        self.edge_objs.values().cloned().collect()
    }

    // === Edge functions ======================================================

    /// Creates a path of edges over `nodes`, assigning the default edge label. An empty
    /// path is a no-op here; upstream's `reduce` throws on it.
    pub fn set_path<S: AsRef<str>>(&mut self, nodes: &[S]) -> &mut Self {
        for pair in nodes.windows(2) {
            self.set_edge(pair[0].as_ref(), pair[1].as_ref());
        }
        self
    }

    /// Creates a path of edges over `nodes`, assigning `label` to each edge. Each edge
    /// receives a clone of the label; JavaScript shares one object across the edges.
    pub fn set_path_with<S: AsRef<str>>(&mut self, nodes: &[S], label: E) -> &mut Self
    where
        E: Clone,
    {
        for pair in nodes.windows(2) {
            self.set_edge_with(pair[0].as_ref(), pair[1].as_ref(), label.clone());
        }
        self
    }

    /// Creates or updates the unnamed edge `v -> w` with the default label.
    pub fn set_edge(&mut self, v: &str, w: &str) -> &mut Self {
        self.set_edge_core(v, w, None, false, None)
            .expect("an unnamed edge is valid in every graph");
        self
    }

    /// Creates or updates the unnamed edge `v -> w` with `label`.
    pub fn set_edge_with(&mut self, v: &str, w: &str, label: E) -> &mut Self {
        self.set_edge_core(v, w, Some(label), true, None)
            .expect("an unnamed edge is valid in every graph");
        self
    }

    /// Creates or updates the unnamed edge `v -> w` with an explicit optional label. `None`
    /// clears the label, like upstream's `setEdge(v, w, undefined)`.
    pub fn set_edge_label(&mut self, v: &str, w: &str, label: Option<E>) -> &mut Self {
        self.set_edge_core(v, w, label, true, None)
            .expect("an unnamed edge is valid in every graph");
        self
    }

    /// The faithful form of upstream's `setEdge(v, w, value, name)`: creates or updates the
    /// edge, optionally named, with an explicit optional label. Fails when a name is given
    /// on a non-multigraph.
    pub fn set_edge_full(
        &mut self,
        v: &str,
        w: &str,
        label: Option<E>,
        name: Option<&str>,
    ) -> Result<&mut Self, GraphError> {
        self.set_edge_core(v, w, label, true, name)
    }

    /// Creates or updates the edge described by `edge` with the default label.
    pub fn set_edge_object(&mut self, edge: &Edge) -> Result<&mut Self, GraphError> {
        self.set_edge_core(&edge.v, &edge.w, None, false, edge.name())
    }

    /// Creates or updates the edge described by `edge` with `label`.
    pub fn set_edge_object_with(&mut self, edge: &Edge, label: E) -> Result<&mut Self, GraphError> {
        self.set_edge_core(&edge.v, &edge.w, Some(label), true, edge.name())
    }

    /// Creates or updates the edge described by `edge` with an explicit optional label.
    pub fn set_edge_object_label(
        &mut self,
        edge: &Edge,
        label: Option<E>,
    ) -> Result<&mut Self, GraphError> {
        self.set_edge_core(&edge.v, &edge.w, label, true, edge.name())
    }

    /// The label of the unnamed edge `v -> w`.
    pub fn edge(&self, v: &str, w: &str) -> Option<&E> {
        self.edge_label(v, w, None)
    }

    /// The label of the named (or, with `None`, unnamed) edge `v -> w`.
    pub fn edge_label(&self, v: &str, w: &str, name: Option<&str>) -> Option<&E> {
        let id = self.edge_id(v, w, name);
        self.edge_labels.get(&id).and_then(Option::as_ref)
    }

    /// The label of the edge described by `edge`.
    pub fn edge_object(&self, edge: &Edge) -> Option<&E> {
        self.edge_label(&edge.v, &edge.w, edge.name())
    }

    /// The label of the unnamed edge `v -> w`, for in-place updates.
    pub fn edge_mut(&mut self, v: &str, w: &str) -> Option<&mut E> {
        self.edge_label_mut(v, w, None)
    }

    /// The label of the named (or, with `None`, unnamed) edge `v -> w`, for in-place updates.
    pub fn edge_label_mut(&mut self, v: &str, w: &str, name: Option<&str>) -> Option<&mut E> {
        let id = self.edge_id(v, w, name);
        self.edge_labels.get_mut(&id).and_then(Option::as_mut)
    }

    /// The label of the edge described by `edge`, for in-place updates.
    pub fn edge_object_mut(&mut self, edge: &Edge) -> Option<&mut E> {
        self.edge_label_mut(&edge.v, &edge.w, edge.name())
    }

    /// Whether the unnamed edge `v -> w` exists.
    pub fn has_edge(&self, v: &str, w: &str) -> bool {
        self.has_edge_label(v, w, None)
    }

    /// Whether the named (or, with `None`, unnamed) edge `v -> w` exists.
    pub fn has_edge_label(&self, v: &str, w: &str, name: Option<&str>) -> bool {
        let id = self.edge_id(v, w, name);
        self.edge_labels.contains_key(&id)
    }

    /// Whether the edge described by `edge` exists.
    pub fn has_edge_object(&self, edge: &Edge) -> bool {
        self.has_edge_label(&edge.v, &edge.w, edge.name())
    }

    /// Removes the unnamed edge `v -> w`.
    pub fn remove_edge(&mut self, v: &str, w: &str) -> &mut Self {
        let id = self.edge_id(v, w, None);
        self.remove_edge_by_id(&id);
        self
    }

    /// Removes the named (or, with `None`, unnamed) edge `v -> w`.
    pub fn remove_edge_label(&mut self, v: &str, w: &str, name: Option<&str>) -> &mut Self {
        let id = self.edge_id(v, w, name);
        self.remove_edge_by_id(&id);
        self
    }

    /// Removes the edge described by `edge`.
    pub fn remove_edge_object(&mut self, edge: &Edge) -> &mut Self {
        let id = self.edge_id(&edge.v, &edge.w, edge.name());
        self.remove_edge_by_id(&id);
        self
    }

    /// Edges that point at `v`, optionally only those involving `w`. `None` when `v` is not
    /// in the graph.
    pub fn in_edges(&self, v: &str, w: Option<&str>) -> Option<Vec<Edge>> {
        if self.directed {
            self.filter_edges(self.in_edges.get(v), v, w)
        } else {
            self.node_edges(v, w)
        }
    }

    /// Edges that point away from `v`, optionally only those involving `w`. `None` when `v`
    /// is not in the graph.
    pub fn out_edges(&self, v: &str, w: Option<&str>) -> Option<Vec<Edge>> {
        if self.directed {
            self.filter_edges(self.out_edges.get(v), v, w)
        } else {
            self.node_edges(v, w)
        }
    }

    /// All edges incident to `v`, regardless of direction, optionally only those involving
    /// `w`. `None` when `v` is not in the graph.
    pub fn node_edges(&self, v: &str, w: Option<&str>) -> Option<Vec<Edge>> {
        if !self.nodes.contains_key(v) {
            return None;
        }
        let mut merged = self.in_edges.get(v).cloned().unwrap_or_default();
        if let Some(out_edges) = self.out_edges.get(v) {
            for (id, edge) in out_edges.iter() {
                merged.insert(id.to_string(), edge.clone());
            }
        }
        self.filter_edges(Some(&merged), v, w)
    }

    // === Internals ===========================================================

    fn upsert_node(&mut self, name: String, label: Option<N>, specified: bool) {
        if self.nodes.contains_key(&name) {
            if specified {
                self.nodes.insert(name, label);
            }
            return;
        }
        let label = if specified {
            label
        } else {
            (self.default_node_label)(&name)
        };
        self.nodes.insert(name.clone(), label);
        if self.compound {
            if let Some(parents) = &mut self.parent {
                parents.insert(name.clone(), GRAPH_NODE.to_string());
            }
            if let Some(children) = &mut self.children {
                children.insert(name.clone(), OrderedMap::new());
                if let Some(root) = children.get_mut(GRAPH_NODE) {
                    root.insert(name.clone(), true);
                }
            }
        }
        self.in_edges.insert(name.clone(), OrderedMap::new());
        self.preds.insert(name.clone(), OrderedMap::new());
        self.out_edges.insert(name.clone(), OrderedMap::new());
        self.sucs.insert(name.clone(), OrderedMap::new());
        self.node_count += 1;
    }

    fn remove_from_parents_child_list(&mut self, v: &str) {
        let Some(parent) = self
            .parent
            .as_ref()
            .and_then(|parents| parents.get(v))
            .cloned()
        else {
            return;
        };
        if let Some(children) = &mut self.children
            && let Some(list) = children.get_mut(&parent)
        {
            list.remove(v);
        }
    }

    fn edge_id(&self, v: &str, w: &str, name: Option<&str>) -> String {
        let (v, w) = if !self.directed && js_less_than(w, v) {
            (w, v)
        } else {
            (v, w)
        };
        format!("{v}\x01{w}\x01{}", name.unwrap_or(DEFAULT_EDGE_NAME))
    }

    fn edge_object_for(&self, v: &str, w: &str, name: Option<&str>) -> Edge {
        let (v, w) = if !self.directed && js_less_than(w, v) {
            (w, v)
        } else {
            (v, w)
        };
        Edge {
            v: v.to_string(),
            w: w.to_string(),
            name: name.filter(|name| !name.is_empty()).map(str::to_string),
        }
    }

    fn set_edge_core(
        &mut self,
        v: &str,
        w: &str,
        label: Option<E>,
        specified: bool,
        name: Option<&str>,
    ) -> Result<&mut Self, GraphError> {
        let id = self.edge_id(v, w, name);
        if self.edge_labels.contains_key(&id) {
            if specified {
                self.edge_labels.insert(id, label);
            }
            return Ok(self);
        }
        if name.is_some() && !self.multigraph {
            return Err(GraphError::NamedEdgeInSimpleGraph);
        }
        self.set_node(v.to_string());
        self.set_node(w.to_string());
        let label = if specified {
            label
        } else {
            (self.default_edge_label)(v, w, name)
        };
        self.edge_labels.insert(id.clone(), label);
        let edge = self.edge_object_for(v, w, name);
        let source = edge.v.clone();
        let target = edge.w.clone();
        self.edge_objs.insert(id.clone(), edge.clone());
        increment(self.preds.get_mut(&target), &source);
        increment(self.sucs.get_mut(&source), &target);
        if let Some(edges) = self.in_edges.get_mut(&target) {
            edges.insert(id.clone(), edge.clone());
        }
        if let Some(edges) = self.out_edges.get_mut(&source) {
            edges.insert(id, edge);
        }
        self.edge_count += 1;
        Ok(self)
    }

    fn remove_edge_by_id(&mut self, id: &str) -> bool {
        let Some(edge) = self.edge_objs.get(id).cloned() else {
            return false;
        };
        self.edge_labels.remove(id);
        self.edge_objs.remove(id);
        decrement(self.preds.get_mut(&edge.w), &edge.v);
        decrement(self.sucs.get_mut(&edge.v), &edge.w);
        if let Some(edges) = self.in_edges.get_mut(&edge.w) {
            edges.remove(id);
        }
        if let Some(edges) = self.out_edges.get_mut(&edge.v) {
            edges.remove(id);
        }
        self.edge_count -= 1;
        true
    }

    fn filter_edges(
        &self,
        edges: Option<&OrderedMap<Edge>>,
        local: &str,
        remote: Option<&str>,
    ) -> Option<Vec<Edge>> {
        let edges = edges?;
        let all = edges.values().cloned().collect::<Vec<_>>();
        match remote.filter(|remote| !remote.is_empty()) {
            None => Some(all),
            Some(remote) => Some(
                all.into_iter()
                    .filter(|edge| {
                        (edge.v == local && edge.w == remote)
                            || (edge.v == remote && edge.w == local)
                    })
                    .collect(),
            ),
        }
    }
}

fn increment(counts: Option<&mut OrderedMap<usize>>, key: &str) {
    if let Some(counts) = counts {
        let next = counts.get(key).copied().unwrap_or(0) + 1;
        counts.insert(key.to_string(), next);
    }
}

fn decrement(counts: Option<&mut OrderedMap<usize>>, key: &str) {
    if let Some(counts) = counts
        && let Some(count) = counts.get(key).copied()
    {
        if count <= 1 {
            counts.remove(key);
        } else {
            counts.insert(key.to_string(), count - 1);
        }
    }
}

/// Upstream compares edge endpoints with JavaScript's `<`, which orders UTF-16 code units.
fn js_less_than(a: &str, b: &str) -> bool {
    a.encode_utf16().cmp(b.encode_utf16()) == Ordering::Less
}

fn find_parent<G, N, E>(
    source: &Graph<G, N, E>,
    copy: &Graph<G, N, E>,
    parents: &mut HashMap<String, Option<String>>,
    v: &str,
) -> Option<String> {
    match source.parent(v) {
        None => {
            parents.insert(v.to_string(), None);
            None
        }
        // Upstream tests `!parent`, so an empty-string parent is kept even when the copy
        // does not have it (and `setParent` then creates it).
        Some(parent) if parent.is_empty() || copy.has_node(parent) => {
            parents.insert(v.to_string(), Some(parent.to_string()));
            Some(parent.to_string())
        }
        Some(parent) => match parents.get(parent) {
            Some(cached) => cached.clone(),
            None => find_parent(source, copy, parents, parent),
        },
    }
}
