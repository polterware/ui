//! Ports of `graphlib/test/alg/*-test.ts` and `graphlib/test/alg/utils/*-test.ts` (MIT,
//! Copyright (c) 2012-2014 Chris Pettitt).
//!
//! The JavaScript-only "rethrows non-CycleException errors" test for `isAcyclic` has no
//! equivalent: Rust has no `undefined` graph to pass.

use pretty_assertions::assert_eq;

use super::alg::{self, AllPaths, EdgeFn, Paths, Traversal, WeightFn};
use super::{Edge, Graph, GraphError, GraphOptions};

fn graph() -> Graph<(), (), f64> {
    Graph::new()
}

type ExpectedPath<'a> = (&'a str, f64, Option<&'a str>);

type SingleSourceAlgorithm = fn(
    &Graph<(), (), f64>,
    &str,
    Option<&WeightFn<'_>>,
    Option<&EdgeFn<'_>>,
) -> Result<Paths, GraphError>;

type AllPathsAlgorithm = fn(
    &Graph<(), (), f64>,
    Option<&WeightFn<'_>>,
    Option<&EdgeFn<'_>>,
) -> Result<AllPaths, GraphError>;

fn directed() -> Graph {
    Graph::new()
}

fn sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}

fn sorted_components(mut components: Vec<Vec<String>>) -> Vec<Vec<String>> {
    for component in &mut components {
        component.sort();
    }
    components.sort_by(|a, b| a.first().cmp(&b.first()));
    components
}

fn index_of(nodes: &[String], node: &str) -> usize {
    nodes
        .iter()
        .position(|candidate| candidate == node)
        .expect("the node was visited")
}

fn expect_paths(paths: &Paths, expected: &[ExpectedPath<'_>]) {
    assert_eq!(paths.len(), expected.len(), "path count");
    for (node, distance, predecessor) in expected {
        let path = paths
            .get(node)
            .unwrap_or_else(|| panic!("no paths entry for {node}"));
        assert_eq!(path.distance, *distance, "distance for {node}");
        assert_eq!(
            path.predecessor.as_deref(),
            *predecessor,
            "predecessor for {node}"
        );
    }
}

fn expect_all(all: &AllPaths, expected: &[(&str, &[ExpectedPath<'_>])]) {
    assert_eq!(all.len(), expected.len(), "source count");
    for (source, rows) in expected {
        let paths = all
            .get(source)
            .unwrap_or_else(|| panic!("no paths for {source}"));
        expect_paths(paths, rows);
    }
}

fn run_dijkstra(
    graph: &Graph<(), (), f64>,
    source: &str,
    weight_fn: Option<&WeightFn<'_>>,
    edge_fn: Option<&EdgeFn<'_>>,
) -> Result<Paths, GraphError> {
    alg::dijkstra(graph, source, weight_fn, edge_fn)
}

fn run_dijkstra_all(
    graph: &Graph<(), (), f64>,
    weight_fn: Option<&WeightFn<'_>>,
    edge_fn: Option<&EdgeFn<'_>>,
) -> Result<AllPaths, GraphError> {
    alg::dijkstra_all(graph, weight_fn, edge_fn)
}

fn run_floyd_warshall(
    graph: &Graph<(), (), f64>,
    weight_fn: Option<&WeightFn<'_>>,
    edge_fn: Option<&EdgeFn<'_>>,
) -> Result<AllPaths, GraphError> {
    alg::floyd_warshall(graph, weight_fn, edge_fn)
}

fn single_source_suite(algorithm: SingleSourceAlgorithm) {
    let mut g = graph();
    g.set_node("source");
    expect_paths(
        &algorithm(&g, "source", None, None).unwrap(),
        &[("source", 0.0, None)],
    );

    let mut g = graph();
    g.set_node("a");
    g.set_node("b");
    expect_paths(
        &algorithm(&g, "a", None, None).unwrap(),
        &[("a", 0.0, None), ("b", f64::INFINITY, None)],
    );

    let mut g = graph();
    g.set_path(&["a", "b", "c"]);
    g.set_edge("b", "d");
    expect_paths(
        &algorithm(&g, "a", None, None).unwrap(),
        &[
            ("a", 0.0, None),
            ("b", 1.0, Some("a")),
            ("c", 2.0, Some("b")),
            ("d", 2.0, Some("b")),
        ],
    );

    let mut g: Graph<(), (), f64> = Graph::with_options(GraphOptions::undirected());
    g.set_path(&["a", "b", "c"]);
    g.set_edge("b", "d");
    expect_paths(
        &algorithm(&g, "a", None, None).unwrap(),
        &[
            ("a", 0.0, None),
            ("b", 1.0, Some("a")),
            ("c", 2.0, Some("b")),
            ("d", 2.0, Some("b")),
        ],
    );
    expect_paths(
        &algorithm(&g, "d", None, None).unwrap(),
        &[
            ("a", 2.0, Some("b")),
            ("b", 1.0, Some("d")),
            ("c", 2.0, Some("b")),
            ("d", 0.0, None),
        ],
    );

    let mut g = graph();
    g.set_edge_with("a", "b", 1.0);
    g.set_edge_with("a", "c", 2.0);
    g.set_edge_with("b", "d", 3.0);
    g.set_edge_with("c", "d", 3.0);
    let weight = |edge: &Edge| *g.edge_object(edge).unwrap_or(&1.0);
    expect_paths(
        &algorithm(&g, "a", Some(&weight), None).unwrap(),
        &[
            ("a", 0.0, None),
            ("b", 1.0, Some("a")),
            ("c", 2.0, Some("a")),
            ("d", 4.0, Some("b")),
        ],
    );

    let mut g = graph();
    g.set_path(&["a", "c", "d"]);
    g.set_edge("b", "c");
    let in_edges: &EdgeFn<'_> = &|v: &str| g.in_edges(v, None).unwrap_or_default();
    expect_paths(
        &algorithm(&g, "d", None, Some(in_edges)).unwrap(),
        &[
            ("a", 2.0, Some("c")),
            ("b", 2.0, Some("c")),
            ("c", 1.0, Some("d")),
            ("d", 0.0, None),
        ],
    );
}

#[test]
fn dijkstra_shortest_paths() {
    single_source_suite(run_dijkstra);
}

#[test]
fn dijkstra_throws_on_a_negative_edge_weight() {
    let mut g = graph();
    g.set_edge_with("a", "b", 1.0);
    g.set_edge_with("a", "c", -2.0);
    g.set_edge_with("b", "d", 3.0);
    g.set_edge_with("c", "d", 3.0);
    let weight = |edge: &Edge| *g.edge_object(edge).unwrap_or(&1.0);
    match alg::dijkstra(&g, "a", Some(&weight), None) {
        Err(GraphError::NegativeEdgeWeight { weight, .. }) => assert_eq!(weight, -2.0),
        other => panic!("expected a negative weight error, got {other:?}"),
    }
}

fn all_paths_suite(algorithm: AllPathsAlgorithm) {
    let mut g = graph();
    g.set_node("a");
    expect_all(
        &algorithm(&g, None, None).unwrap(),
        &[("a", &[("a", 0.0, None)])],
    );

    let mut g = graph();
    g.set_edge("a", "b");
    g.set_edge("b", "c");
    expect_all(
        &algorithm(&g, None, None).unwrap(),
        &[
            (
                "a",
                &[
                    ("a", 0.0, None),
                    ("b", 1.0, Some("a")),
                    ("c", 2.0, Some("b")),
                ],
            ),
            (
                "b",
                &[
                    ("a", f64::INFINITY, None),
                    ("b", 0.0, None),
                    ("c", 1.0, Some("b")),
                ],
            ),
            (
                "c",
                &[
                    ("a", f64::INFINITY, None),
                    ("b", f64::INFINITY, None),
                    ("c", 0.0, None),
                ],
            ),
        ],
    );

    let mut g = graph();
    g.set_edge_with("a", "b", 2.0);
    g.set_edge_with("b", "c", 3.0);
    let weight = |edge: &Edge| *g.edge_object(edge).unwrap_or(&1.0);
    expect_all(
        &algorithm(&g, Some(&weight), None).unwrap(),
        &[
            (
                "a",
                &[
                    ("a", 0.0, None),
                    ("b", 2.0, Some("a")),
                    ("c", 5.0, Some("b")),
                ],
            ),
            (
                "b",
                &[
                    ("a", f64::INFINITY, None),
                    ("b", 0.0, None),
                    ("c", 3.0, Some("b")),
                ],
            ),
            (
                "c",
                &[
                    ("a", f64::INFINITY, None),
                    ("b", f64::INFINITY, None),
                    ("c", 0.0, None),
                ],
            ),
        ],
    );

    let mut g = graph();
    g.set_edge("a", "b");
    g.set_edge("b", "c");
    let in_edges: &EdgeFn<'_> = &|v: &str| g.in_edges(v, None).unwrap_or_default();
    expect_all(
        &algorithm(&g, None, Some(in_edges)).unwrap(),
        &[
            (
                "a",
                &[
                    ("a", 0.0, None),
                    ("b", f64::INFINITY, None),
                    ("c", f64::INFINITY, None),
                ],
            ),
            (
                "b",
                &[
                    ("a", 1.0, Some("b")),
                    ("b", 0.0, None),
                    ("c", f64::INFINITY, None),
                ],
            ),
            (
                "c",
                &[
                    ("a", 2.0, Some("b")),
                    ("b", 1.0, Some("c")),
                    ("c", 0.0, None),
                ],
            ),
        ],
    );

    let mut g: Graph<(), (), f64> = Graph::with_options(GraphOptions::undirected());
    g.set_edge_with("a", "b", 1.0);
    g.set_edge_with("b", "c", 2.0);
    g.set_edge_with("c", "a", 4.0);
    g.set_edge_with("b", "d", 6.0);
    let weight = |edge: &Edge| *g.edge_object(edge).unwrap_or(&1.0);
    let node_edges: &EdgeFn<'_> = &|v: &str| g.node_edges(v, None).unwrap_or_default();
    expect_all(
        &algorithm(&g, Some(&weight), Some(node_edges)).unwrap(),
        &[
            (
                "a",
                &[
                    ("a", 0.0, None),
                    ("b", 1.0, Some("a")),
                    ("c", 3.0, Some("b")),
                    ("d", 7.0, Some("b")),
                ],
            ),
            (
                "b",
                &[
                    ("a", 1.0, Some("b")),
                    ("b", 0.0, None),
                    ("c", 2.0, Some("b")),
                    ("d", 6.0, Some("b")),
                ],
            ),
            (
                "c",
                &[
                    ("a", 3.0, Some("b")),
                    ("b", 2.0, Some("c")),
                    ("c", 0.0, None),
                    ("d", 8.0, Some("b")),
                ],
            ),
            (
                "d",
                &[
                    ("a", 7.0, Some("b")),
                    ("b", 6.0, Some("d")),
                    ("c", 8.0, Some("b")),
                    ("d", 0.0, None),
                ],
            ),
        ],
    );
}

#[test]
fn dijkstra_all_shortest_paths() {
    all_paths_suite(run_dijkstra_all);
}

#[test]
fn dijkstra_all_throws_on_a_negative_edge_weight() {
    let mut g = graph();
    g.set_edge_with("a", "b", 1.0);
    g.set_edge_with("a", "c", -2.0);
    g.set_edge_with("b", "d", 3.0);
    g.set_edge_with("c", "d", 3.0);
    let weight = |edge: &Edge| *g.edge_object(edge).unwrap_or(&1.0);
    assert!(alg::dijkstra_all(&g, Some(&weight), None).is_err());
}

#[test]
fn floyd_warshall_shortest_paths() {
    all_paths_suite(run_floyd_warshall);
}

#[test]
fn floyd_warshall_handles_negative_weights() {
    let mut g = graph();
    g.set_edge_with("a", "b", 1.0);
    g.set_edge_with("a", "c", -2.0);
    g.set_edge_with("b", "d", 3.0);
    g.set_edge_with("c", "d", 3.0);
    let weight = |edge: &Edge| *g.edge_object(edge).unwrap_or(&1.0);
    expect_all(
        &alg::floyd_warshall(&g, Some(&weight), None).unwrap(),
        &[
            (
                "a",
                &[
                    ("a", 0.0, None),
                    ("b", 1.0, Some("a")),
                    ("c", -2.0, Some("a")),
                    ("d", 1.0, Some("c")),
                ],
            ),
            (
                "b",
                &[
                    ("a", f64::INFINITY, None),
                    ("b", 0.0, None),
                    ("c", f64::INFINITY, None),
                    ("d", 3.0, Some("b")),
                ],
            ),
            (
                "c",
                &[
                    ("a", f64::INFINITY, None),
                    ("b", f64::INFINITY, None),
                    ("c", 0.0, None),
                    ("d", 3.0, Some("c")),
                ],
            ),
            (
                "d",
                &[
                    ("a", f64::INFINITY, None),
                    ("b", f64::INFINITY, None),
                    ("c", f64::INFINITY, None),
                    ("d", 0.0, None),
                ],
            ),
        ],
    );
}

#[test]
fn floyd_warshall_includes_negative_weight_self_edges() {
    let mut g = graph();
    g.set_edge_with("a", "a", -1.0);
    let weight = |edge: &Edge| *g.edge_object(edge).unwrap_or(&1.0);
    expect_all(
        &alg::floyd_warshall(&g, Some(&weight), None).unwrap(),
        &[("a", &[("a", -2.0, Some("a"))])],
    );
}

#[test]
fn components_is_empty_for_an_empty_graph() {
    let g: Graph = Graph::with_options(GraphOptions::undirected());
    assert!(alg::components(&g).is_empty());
}

#[test]
fn components_returns_singletons_for_unconnected_nodes() {
    let mut g: Graph = Graph::with_options(GraphOptions::undirected());
    g.set_node("a");
    g.set_node("b");
    assert_eq!(sorted_components(alg::components(&g)), [["a"], ["b"]]);
}

#[test]
fn components_returns_a_list_of_nodes_in_a_component() {
    let mut g: Graph = Graph::with_options(GraphOptions::undirected());
    g.set_edge("a", "b");
    g.set_edge("b", "c");
    assert_eq!(sorted_components(alg::components(&g)), [["a", "b", "c"]]);
}

#[test]
fn components_returns_nodes_connected_by_neighbors_in_a_digraph() {
    let mut g: Graph = Graph::new();
    g.set_path(&["a", "b", "c", "a"]);
    g.set_edge("d", "c");
    g.set_edge("e", "f");
    assert_eq!(
        sorted_components(alg::components(&g)),
        vec![vec!["a", "b", "c", "d"], vec!["e", "f"]]
    );
}

#[test]
fn find_cycles_is_empty_for_an_empty_graph() {
    let g: Graph = Graph::new();
    assert!(alg::find_cycles(&g).is_empty());
}

#[test]
fn find_cycles_is_empty_when_there_are_no_cycles() {
    let mut g = directed();
    g.set_path(&["a", "b", "c"]);
    assert!(alg::find_cycles(&g).is_empty());
}

#[test]
fn find_cycles_finds_a_single_node_cycle() {
    let mut g = directed();
    g.set_path(&["a", "a"]);
    assert_eq!(sorted_components(alg::find_cycles(&g)), [["a"]]);
}

#[test]
fn find_cycles_finds_a_two_node_cycle() {
    let mut g = directed();
    g.set_path(&["a", "b", "a"]);
    assert_eq!(sorted_components(alg::find_cycles(&g)), [["a", "b"]]);
}

#[test]
fn find_cycles_finds_a_triangle() {
    let mut g = directed();
    g.set_path(&["a", "b", "c", "a"]);
    assert_eq!(sorted_components(alg::find_cycles(&g)), [["a", "b", "c"]]);
}

#[test]
fn find_cycles_detects_a_named_self_loop_in_a_multigraph() {
    let mut g: Graph<(), (), &str> = Graph::with_options(GraphOptions::multigraph());
    g.set_node("a");
    g.set_edge_full("a", "a", Some("label"), Some("name"))
        .unwrap();
    assert_eq!(sorted_components(alg::find_cycles(&g)), [["a"]]);
}

#[test]
fn find_cycles_returns_multiple_cycles() {
    let mut g = directed();
    g.set_path(&["a", "b", "a"]);
    g.set_path(&["c", "d", "e", "c"]);
    g.set_path(&["f", "g", "g"]);
    g.set_node("h");
    assert_eq!(
        sorted_components(alg::find_cycles(&g)),
        vec![vec!["a", "b"], vec!["c", "d", "e"], vec!["g"]]
    );
}

#[test]
fn is_acyclic_is_true_without_cycles() {
    let mut g = directed();
    g.set_path(&["a", "b", "c"]);
    assert!(alg::is_acyclic(&g));
}

#[test]
fn is_acyclic_is_false_with_a_cycle() {
    let mut g = directed();
    g.set_path(&["a", "b", "c", "a"]);
    assert!(!alg::is_acyclic(&g));
}

#[test]
fn is_acyclic_is_false_with_a_single_node_cycle() {
    let mut g = directed();
    g.set_path(&["a", "a"]);
    assert!(!alg::is_acyclic(&g));
}

#[test]
fn postorder_returns_the_root_for_a_singleton_graph() {
    let mut g = directed();
    g.set_node("a");
    assert_eq!(alg::postorder(&g, &["a"]).unwrap(), ["a"]);
}

#[test]
fn postorder_visits_each_node_once() {
    let mut g = directed();
    g.set_path(&["a", "b", "d", "e"]);
    g.set_path(&["a", "c", "d", "e"]);
    assert_eq!(
        sorted(alg::postorder(&g, &["a"]).unwrap()),
        ["a", "b", "c", "d", "e"]
    );
}

#[test]
fn postorder_works_for_a_tree() {
    let mut g = directed();
    g.set_edge("a", "b");
    g.set_path(&["a", "c", "d"]);
    g.set_edge("c", "e");
    let nodes = alg::postorder(&g, &["a"]).unwrap();
    assert!(index_of(&nodes, "b") < index_of(&nodes, "a"));
    assert!(index_of(&nodes, "c") < index_of(&nodes, "a"));
    assert!(index_of(&nodes, "d") < index_of(&nodes, "c"));
    assert!(index_of(&nodes, "e") < index_of(&nodes, "c"));
    assert_eq!(sorted(nodes), ["a", "b", "c", "d", "e"]);
}

#[test]
fn postorder_works_for_an_array_of_roots() {
    let mut g = directed();
    g.set_edge("a", "b");
    g.set_edge("c", "d");
    g.set_node("e");
    g.set_node("f");
    let nodes = alg::postorder(&g, &["a", "b", "c", "e"]).unwrap();
    assert!(index_of(&nodes, "b") < index_of(&nodes, "a"));
    assert!(index_of(&nodes, "d") < index_of(&nodes, "c"));
    assert_eq!(sorted(nodes), ["a", "b", "c", "d", "e"]);
}

#[test]
fn postorder_works_for_multiple_connected_roots() {
    let mut g = directed();
    g.set_edge("a", "b");
    g.set_edge("a", "c");
    g.set_edge("d", "c");
    let nodes = alg::postorder(&g, &["a", "d"]).unwrap();
    assert!(index_of(&nodes, "b") < index_of(&nodes, "a"));
    assert!(index_of(&nodes, "c") < index_of(&nodes, "a"));
    assert!(index_of(&nodes, "c") < index_of(&nodes, "d"));
    assert_eq!(sorted(nodes), ["a", "b", "c", "d"]);
}

#[test]
fn postorder_fails_when_a_root_is_absent() {
    let mut g = directed();
    g.set_node("a");
    assert_eq!(
        alg::postorder(&g, &["b"]),
        Err(GraphError::GraphDoesNotHaveNode {
            node: "b".to_string()
        })
    );
}

#[test]
fn preorder_returns_the_root_for_a_singleton_graph() {
    let mut g = directed();
    g.set_node("a");
    assert_eq!(alg::preorder(&g, &["a"]).unwrap(), ["a"]);
}

#[test]
fn preorder_visits_each_node_once() {
    let mut g = directed();
    g.set_path(&["a", "b", "d", "e"]);
    g.set_path(&["a", "c", "d", "e"]);
    assert_eq!(
        sorted(alg::preorder(&g, &["a"]).unwrap()),
        ["a", "b", "c", "d", "e"]
    );
}

#[test]
fn preorder_works_for_a_tree() {
    let mut g = directed();
    g.set_edge("a", "b");
    g.set_path(&["a", "c", "d"]);
    g.set_edge("c", "e");
    let nodes = alg::preorder(&g, &["a"]).unwrap();
    assert_eq!(sorted(nodes.clone()), ["a", "b", "c", "d", "e"]);
    assert!(index_of(&nodes, "b") > index_of(&nodes, "a"));
    assert!(index_of(&nodes, "c") > index_of(&nodes, "a"));
    assert!(index_of(&nodes, "d") > index_of(&nodes, "c"));
    assert!(index_of(&nodes, "e") > index_of(&nodes, "c"));
}

#[test]
fn preorder_works_for_an_array_of_roots() {
    let mut g = directed();
    g.set_edge("a", "b");
    g.set_edge("c", "d");
    g.set_node("e");
    g.set_node("f");
    let nodes = alg::preorder(&g, &["a", "c", "e"]).unwrap();
    assert_eq!(sorted(nodes.clone()), ["a", "b", "c", "d", "e"]);
    assert!(index_of(&nodes, "b") > index_of(&nodes, "a"));
    assert!(index_of(&nodes, "d") > index_of(&nodes, "c"));
}

#[test]
fn preorder_fails_when_a_root_is_absent() {
    let mut g = directed();
    g.set_node("a");
    assert_eq!(
        alg::preorder(&g, &["b"]),
        Err(GraphError::GraphDoesNotHaveNode {
            node: "b".to_string()
        })
    );
}

#[test]
fn prim_returns_an_empty_graph_for_an_empty_input() {
    let source = graph();
    let weight = |edge: &Edge| *source.edge_object(edge).unwrap_or(&1.0);
    let tree = alg::prim(&source, &weight).unwrap();
    assert_eq!(tree.node_count(), 0);
    assert_eq!(tree.edge_count(), 0);
}

#[test]
fn prim_returns_a_single_node_graph_for_a_single_node() {
    let mut source = graph();
    source.set_node("a");
    let weight = |edge: &Edge| *source.edge_object(edge).unwrap_or(&1.0);
    let tree = alg::prim(&source, &weight).unwrap();
    assert_eq!(tree.nodes(), ["a"]);
    assert_eq!(tree.edge_count(), 0);
}

#[test]
fn prim_returns_a_deterministic_result_given_an_optimal_solution() {
    let mut source = graph();
    source.set_edge_with("a", "b", 1.0);
    source.set_edge_with("b", "c", 2.0);
    source.set_edge_with("b", "d", 3.0);
    source.set_edge_with("c", "d", 20.0);
    source.set_edge_with("c", "e", 60.0);
    source.set_edge_with("d", "e", 1.0);
    let weight = |edge: &Edge| *source.edge_object(edge).unwrap_or(&1.0);
    let tree = alg::prim(&source, &weight).unwrap();
    assert_eq!(sorted(tree.neighbors("a").unwrap()), ["b"]);
    assert_eq!(sorted(tree.neighbors("b").unwrap()), ["a", "c", "d"]);
    assert_eq!(sorted(tree.neighbors("c").unwrap()), ["b"]);
    assert_eq!(sorted(tree.neighbors("d").unwrap()), ["b", "e"]);
    assert_eq!(sorted(tree.neighbors("e").unwrap()), ["d"]);
}

#[test]
fn prim_fails_for_unconnected_graphs() {
    let mut source = graph();
    source.set_node("a");
    source.set_node("b");
    let weight = |edge: &Edge| *source.edge_object(edge).unwrap_or(&1.0);
    assert!(matches!(
        alg::prim(&source, &weight),
        Err(GraphError::NotConnected)
    ));
}

#[test]
fn tarjan_is_empty_for_an_empty_graph() {
    let g = directed();
    assert!(alg::tarjan(&g).is_empty());
}

#[test]
fn tarjan_returns_singletons_for_nodes_outside_a_component() {
    let mut g = directed();
    g.set_path(&["a", "b", "c"]);
    g.set_edge("d", "c");
    assert_eq!(
        sorted_components(alg::tarjan(&g)),
        [["a"], ["b"], ["c"], ["d"]]
    );
}

#[test]
fn tarjan_returns_a_single_component_for_a_one_edge_cycle() {
    let mut g = directed();
    g.set_path(&["a", "b", "a"]);
    assert_eq!(sorted_components(alg::tarjan(&g)), [["a", "b"]]);
}

#[test]
fn tarjan_returns_a_single_component_for_a_triangle() {
    let mut g = directed();
    g.set_path(&["a", "b", "c", "a"]);
    assert_eq!(sorted_components(alg::tarjan(&g)), [["a", "b", "c"]]);
}

#[test]
fn tarjan_can_find_multiple_components() {
    let mut g = directed();
    g.set_path(&["a", "b", "a"]);
    g.set_path(&["c", "d", "e", "c"]);
    g.set_node("f");
    assert_eq!(
        sorted_components(alg::tarjan(&g)),
        vec![vec!["a", "b"], vec!["c", "d", "e"], vec!["f"]]
    );
}

#[test]
fn topsort_is_empty_for_an_empty_graph() {
    let g = directed();
    assert!(alg::topsort(&g).unwrap().is_empty());
}

#[test]
fn topsort_orders_edges_from_earlier_to_later() {
    let mut g = directed();
    g.set_path(&["b", "c", "a"]);
    assert_eq!(alg::topsort(&g).unwrap(), ["b", "c", "a"]);
}

#[test]
fn topsort_works_for_a_diamond() {
    let mut g = directed();
    g.set_path(&["a", "b", "d"]);
    g.set_path(&["a", "c", "d"]);
    let result = alg::topsort(&g).unwrap();
    assert_eq!(index_of(&result, "a"), 0);
    assert!(index_of(&result, "b") < index_of(&result, "d"));
    assert!(index_of(&result, "c") < index_of(&result, "d"));
    assert_eq!(index_of(&result, "d"), 3);
}

#[test]
fn topsort_fails_on_a_cycle() {
    let mut g = directed();
    g.set_path(&["b", "c", "a", "b"]);
    assert_eq!(alg::topsort(&g), Err(GraphError::Cycle));
}

#[test]
fn topsort_fails_on_a_cycle_with_an_outgoing_edge() {
    let mut g = directed();
    g.set_path(&["b", "c", "a", "b"]);
    g.set_edge("b", "d");
    assert_eq!(alg::topsort(&g), Err(GraphError::Cycle));
}

#[test]
fn topsort_fails_on_a_cycle_with_an_isolated_node() {
    let mut g = directed();
    g.set_path(&["b", "c", "a", "b"]);
    g.set_node("d");
    assert_eq!(alg::topsort(&g), Err(GraphError::Cycle));
}

#[test]
fn reduce_returns_the_initial_value_for_an_empty_graph() {
    let g = directed();
    let result: i32 = alg::reduce(&g, &[], Traversal::Pre, 0, |acc, _| acc).unwrap();
    assert_eq!(result, 0);
}

#[test]
fn reduce_applies_the_accumulator_to_all_nodes() {
    let mut g: Graph = Graph::with_options(GraphOptions::undirected());
    g.set_path(&["1", "2", "3", "5", "7"]);
    g.set_path(&["2", "5", "11", "13"]);
    let result = alg::reduce(&g, &["2"], Traversal::Pre, 0, |acc: i32, value| {
        acc + value.parse::<i32>().unwrap()
    })
    .unwrap();
    assert_eq!(result, 42);
}

#[test]
fn reduce_traverses_in_pre_order() {
    let mut g: Graph = Graph::with_options(GraphOptions::undirected());
    g.set_path(&["1", "2", "3", "5", "7"]);
    g.set_path(&["2", "5", "11", "13"]);
    let result = alg::reduce(&g, &["2"], Traversal::Pre, String::new(), |acc, value| {
        format!("{acc}{value}-")
    })
    .unwrap();
    assert_eq!(result, "2-1-3-5-11-13-7-");
}

#[test]
fn reduce_traverses_in_post_order() {
    let mut g: Graph = Graph::with_options(GraphOptions::undirected());
    g.set_path(&["1", "2", "3", "5", "7"]);
    g.set_path(&["2", "5", "11", "13"]);
    let result = alg::reduce(&g, &["2"], Traversal::Post, String::new(), |acc, value| {
        format!("{acc}{value}-")
    })
    .unwrap();
    assert_eq!(result, "1-13-11-7-5-3-2-");
}
