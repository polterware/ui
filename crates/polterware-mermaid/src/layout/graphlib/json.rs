//! Port of `graphlib/lib/json.ts` (MIT, Copyright (c) 2012-2014 Chris Pettitt).
//!
//! [`write`] produces the same JSON shape as upstream: `options`, `nodes`, `edges`, and an
//! optional graph `value`. [`read`] rebuilds the graph, treating a missing or `null` value
//! as upstream's `undefined`.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{Edge, Graph, GraphError, GraphOptions};

/// A failure while writing or reading a graph.
#[derive(Debug)]
pub enum JsonError {
    /// The JSON did not match the graph shape or a label was not the expected type.
    Json(serde_json::Error),
    /// The JSON described a graph the graph cannot hold (for example, a parent cycle).
    Graph(GraphError),
}

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(error) => error.fmt(f),
            Self::Graph(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for JsonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Graph(error) => Some(error),
        }
    }
}

impl From<serde_json::Error> for JsonError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<GraphError> for JsonError {
    fn from(error: GraphError) -> Self {
        Self::Graph(error)
    }
}

/// Creates the JSON representation of `graph`, upstream's `write`.
pub fn write<G, N, E>(graph: &Graph<G, N, E>) -> Result<Value, JsonError>
where
    G: Serialize,
    N: Serialize,
    E: Serialize,
{
    let mut nodes = Vec::new();
    for v in graph.nodes() {
        let mut node = Map::new();
        node.insert("v".to_string(), Value::String(v.clone()));
        if let Some(label) = graph.node(&v) {
            node.insert("value".to_string(), serde_json::to_value(label)?);
        }
        if let Some(parent) = graph.parent(&v) {
            node.insert("parent".to_string(), Value::String(parent.to_string()));
        }
        nodes.push(Value::Object(node));
    }

    let mut edges = Vec::new();
    for edge in graph.edges() {
        let mut entry = Map::new();
        entry.insert("v".to_string(), Value::String(edge.v.clone()));
        entry.insert("w".to_string(), Value::String(edge.w.clone()));
        if let Some(name) = edge.name() {
            entry.insert("name".to_string(), Value::String(name.to_string()));
        }
        if let Some(label) = graph.edge_object(&edge) {
            entry.insert("value".to_string(), serde_json::to_value(label)?);
        }
        edges.push(Value::Object(entry));
    }

    let mut options = Map::new();
    options.insert("directed".to_string(), Value::Bool(graph.is_directed()));
    options.insert("multigraph".to_string(), Value::Bool(graph.is_multigraph()));
    options.insert("compound".to_string(), Value::Bool(graph.is_compound()));

    let mut root = Map::new();
    root.insert("options".to_string(), Value::Object(options));
    root.insert("nodes".to_string(), Value::Array(nodes));
    root.insert("edges".to_string(), Value::Array(edges));
    if let Some(label) = graph.graph() {
        root.insert("value".to_string(), serde_json::to_value(label)?);
    }
    Ok(Value::Object(root))
}

/// Rebuilds a graph from the JSON representation produced by [`write`].
///
/// Upstream distinguishes a missing `value` (`undefined`) from an explicit `null`; the Rust
/// labels are options, so both read as no label.
pub fn read<G, N, E>(json: &Value) -> Result<Graph<G, N, E>, JsonError>
where
    G: DeserializeOwned,
    N: DeserializeOwned,
    E: DeserializeOwned,
{
    let raw: RawGraph = serde_json::from_value(json.clone())?;
    let mut graph = Graph::with_options(GraphOptions {
        directed: raw.options.directed,
        multigraph: raw.options.multigraph,
        compound: raw.options.compound,
    });

    if let Some(value) = raw.value {
        graph.set_graph(serde_json::from_value(value)?);
    }

    for entry in raw.nodes {
        let RawNode { v, value, parent } = entry;
        let label = value.map(serde_json::from_value).transpose()?;
        graph.set_node_label(v.clone(), label);
        if let Some(parent) = parent {
            graph.set_parent(&v, Some(&parent))?;
        }
    }

    for entry in raw.edges {
        let RawEdge { v, w, name, value } = entry;
        let label = value.map(serde_json::from_value).transpose()?;
        graph.set_edge_object_label(&Edge { v, w, name }, label)?;
    }

    Ok(graph)
}

#[derive(Deserialize)]
struct RawGraph {
    #[serde(default)]
    options: RawOptions,
    nodes: Vec<RawNode>,
    edges: Vec<RawEdge>,
    #[serde(default)]
    value: Option<Value>,
}

#[derive(Deserialize)]
struct RawOptions {
    #[serde(default = "default_true")]
    directed: bool,
    #[serde(default)]
    multigraph: bool,
    #[serde(default)]
    compound: bool,
}

impl Default for RawOptions {
    fn default() -> Self {
        Self {
            directed: true,
            multigraph: false,
            compound: false,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
struct RawNode {
    v: String,
    #[serde(default)]
    value: Option<Value>,
    #[serde(default)]
    parent: Option<String>,
}

#[derive(Deserialize)]
struct RawEdge {
    v: String,
    w: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    value: Option<Value>,
}
