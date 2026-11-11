//! Port of `graphlib/test/json-test.ts` (MIT, Copyright (c) 2012-2014 Chris Pettitt).

use pretty_assertions::assert_eq;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::json::{read, write};
use super::{Edge, Graph, GraphOptions};

fn round_trip<G, N, E>(graph: &Graph<G, N, E>) -> Graph<G, N, E>
where
    G: Serialize + DeserializeOwned,
    N: Serialize + DeserializeOwned,
    E: Serialize + DeserializeOwned,
{
    read(&write(graph).unwrap()).unwrap()
}

#[test]
fn preserves_the_graph_options() {
    let directed: Graph = Graph::with_options(GraphOptions {
        directed: true,
        ..Default::default()
    });
    assert!(round_trip(&directed).is_directed());

    let undirected: Graph = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    assert!(!round_trip(&undirected).is_directed());

    let multigraph: Graph = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    assert!(round_trip(&multigraph).is_multigraph());

    let simple: Graph = Graph::with_options(GraphOptions {
        multigraph: false,
        ..Default::default()
    });
    assert!(!round_trip(&simple).is_multigraph());

    let compound: Graph = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    assert!(round_trip(&compound).is_compound());

    let flat: Graph = Graph::with_options(GraphOptions {
        compound: false,
        ..Default::default()
    });
    assert!(!round_trip(&flat).is_compound());
}

#[test]
fn preserves_the_graph_value() {
    let mut graph: Graph<Value> = Graph::new();
    graph.set_graph(json!(1));
    assert_eq!(round_trip(&graph).graph(), Some(&json!(1)));

    let mut graph: Graph<Value> = Graph::new();
    graph.set_graph(json!({"foo": "bar"}));
    assert_eq!(round_trip(&graph).graph(), Some(&json!({"foo": "bar"})));

    let graph: Graph = Graph::new();
    assert_eq!(round_trip(&graph).graph(), None);
}

#[test]
fn preserves_nodes() {
    let mut graph: Graph = Graph::new();
    graph.set_node("a");
    assert!(round_trip(&graph).has_node("a"));
    assert_eq!(round_trip(&graph).node("a"), None);

    let mut graph: Graph<Value, Value, Value> = Graph::new();
    graph.set_node_with("a", json!(1));
    assert_eq!(round_trip(&graph).node("a"), Some(&json!(1)));

    let mut graph: Graph<Value, Value, Value> = Graph::new();
    graph.set_node_with("a", json!({"foo": "bar"}));
    assert_eq!(round_trip(&graph).node("a"), Some(&json!({"foo": "bar"})));
}

#[test]
fn preserves_simple_edges() {
    let mut graph: Graph = Graph::new();
    graph.set_edge("a", "b");
    assert!(round_trip(&graph).has_edge("a", "b"));
    assert_eq!(round_trip(&graph).edge("a", "b"), None);

    let mut graph: Graph<Value, Value, Value> = Graph::new();
    graph.set_edge_with("a", "b", json!(1));
    assert_eq!(round_trip(&graph).edge("a", "b"), Some(&json!(1)));

    let mut graph: Graph<Value, Value, Value> = Graph::new();
    graph.set_edge_with("a", "b", json!({"foo": "bar"}));
    assert_eq!(
        round_trip(&graph).edge("a", "b"),
        Some(&json!({"foo": "bar"}))
    );
}

#[test]
fn preserves_multi_edges() {
    let mut graph: Graph<Value, Value, Value> = Graph::with_options(GraphOptions::multigraph());

    graph
        .set_edge_object(&Edge::named("a", "b", "foo"))
        .unwrap();
    assert!(round_trip(&graph).has_edge_label("a", "b", Some("foo")));

    graph
        .set_edge_object(&Edge::named("a", "b", "foo"))
        .unwrap();
    assert_eq!(round_trip(&graph).edge_label("a", "b", Some("foo")), None);

    graph
        .set_edge_object_with(&Edge::named("a", "b", "foo"), json!(1))
        .unwrap();
    assert_eq!(
        round_trip(&graph).edge_label("a", "b", Some("foo")),
        Some(&json!(1))
    );

    graph
        .set_edge_object_with(&Edge::named("a", "b", "foo"), json!({"foo": "bar"}))
        .unwrap();
    assert_eq!(
        round_trip(&graph).edge_label("a", "b", Some("foo")),
        Some(&json!({"foo": "bar"}))
    );
}

#[test]
fn preserves_an_empty_string_parent() {
    let mut graph: Graph = Graph::with_options(GraphOptions::compound());
    graph.set_parent("child", Some("")).unwrap();
    let serialized = write(&graph).unwrap();
    let parsed: Value = serde_json::from_str(&serialized.to_string()).unwrap();
    let restored: Graph = read(&parsed).unwrap();

    assert_eq!(restored.parent("child"), Some(""));
    assert_eq!(restored.children(""), ["child"]);
    assert_eq!(restored.root_children(), [""]);
}

#[test]
fn preserves_parent_child_relationships() {
    let mut graph: Graph = Graph::with_options(GraphOptions::compound());
    graph.set_node("a");
    assert_eq!(round_trip(&graph).parent("a"), None);

    let mut graph: Graph = Graph::with_options(GraphOptions::compound());
    graph.set_parent("a", Some("parent")).unwrap();
    assert_eq!(round_trip(&graph).parent("a"), Some("parent"));
}
