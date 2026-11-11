//! Port of `graphlib/test/graph-test.ts` (MIT, Copyright (c) 2012-2014 Chris Pettitt).
//!
//! Every test is ported; the JavaScript-only assertions (`toBe(g)` chaining identity and
//! `edgeAsObj`, which cannot be expressed without `Any`) become chain calls or are dropped.

use pretty_assertions::assert_eq;

use super::{Edge, Graph, GraphOptions};

fn simple() -> Graph {
    Graph::new()
}

fn compound() -> Graph {
    Graph::with_options(GraphOptions::compound())
}

fn sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}

fn sort_edges(edges: &mut [Edge]) {
    edges.sort_by(|a, b| match (a.name.as_deref(), b.name.as_deref()) {
        (Some(a_name), Some(b_name)) => a_name
            .cmp(b_name)
            .then_with(|| a.v.cmp(&b.v))
            .then_with(|| a.w.cmp(&b.w)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.v.cmp(&b.v).then_with(|| a.w.cmp(&b.w)),
    });
}

#[test]
fn has_no_nodes() {
    assert_eq!(simple().node_count(), 0);
}

#[test]
fn has_no_edges() {
    assert_eq!(simple().edge_count(), 0);
}

#[test]
fn has_no_label() {
    assert_eq!(simple().graph(), None);
}

#[test]
fn defaults_to_a_simple_directed_graph() {
    let g = simple();
    assert!(g.is_directed());
    assert!(!g.is_compound());
    assert!(!g.is_multigraph());
}

#[test]
fn can_be_set_to_undirected() {
    let g: Graph = Graph::with_options(GraphOptions::undirected());
    assert!(!g.is_directed());
    assert!(!g.is_compound());
    assert!(!g.is_multigraph());
}

#[test]
fn can_be_set_to_a_compound_graph() {
    let g: Graph = Graph::with_options(GraphOptions::compound());
    assert!(g.is_directed());
    assert!(g.is_compound());
    assert!(!g.is_multigraph());
}

#[test]
fn can_be_set_to_a_multigraph() {
    let g: Graph = Graph::with_options(GraphOptions::multigraph());
    assert!(g.is_directed());
    assert!(!g.is_compound());
    assert!(g.is_multigraph());
}

#[test]
fn set_graph_stores_the_label() {
    let mut g: Graph<&str> = Graph::new();
    g.set_graph("foo");
    assert_eq!(g.graph(), Some(&"foo"));
}

#[test]
fn set_graph_is_chainable() {
    let mut g: Graph<&str> = Graph::new();
    g.set_graph("foo").set_graph("bar");
    assert_eq!(g.graph(), Some(&"bar"));
}

#[test]
fn nodes_are_empty_without_nodes() {
    assert!(simple().nodes().is_empty());
}

#[test]
fn nodes_returns_the_node_ids() {
    let mut g = simple();
    g.set_node("a");
    g.set_node("b");
    assert_eq!(sorted(g.nodes()), ["a", "b"]);
}

#[test]
fn sources_are_nodes_without_in_edges() {
    let mut g = simple();
    g.set_path(&["a", "b", "c"]);
    g.set_node("d");
    assert_eq!(sorted(g.sources()), ["a", "d"]);
}

#[test]
fn sinks_are_nodes_without_out_edges() {
    let mut g = simple();
    g.set_path(&["a", "b", "c"]);
    g.set_node("d");
    assert_eq!(sorted(g.sinks()), ["c", "d"]);
}

#[test]
fn filter_nodes_returns_an_identical_graph_when_the_filter_selects_everything() {
    let mut g: Graph<&str, i32, i32> = Graph::new();
    g.set_graph("graph label");
    g.set_node_with("a", 123);
    g.set_path(&["a", "b", "c"]);
    g.set_edge_with("a", "c", 456);
    let filtered = g.filter_nodes(|_| true);
    assert_eq!(sorted(filtered.nodes()), ["a", "b", "c"]);
    assert_eq!(sorted(filtered.successors("a").unwrap()), ["b", "c"]);
    assert_eq!(filtered.successors("b"), Some(vec!["c".to_string()]));
    assert_eq!(filtered.node("a"), Some(&123));
    assert_eq!(filtered.edge("a", "c"), Some(&456));
    assert_eq!(filtered.graph(), Some(&"graph label"));
}

#[test]
fn filter_nodes_returns_an_empty_graph_when_the_filter_selects_nothing() {
    let mut g = simple();
    g.set_path(&["a", "b", "c"]);
    let filtered = g.filter_nodes(|_| false);
    assert!(filtered.nodes().is_empty());
    assert!(filtered.edges().is_empty());
}

#[test]
fn filter_nodes_only_includes_selected_nodes() {
    let mut g = simple();
    g.set_nodes(&["a", "b"]);
    let filtered = g.filter_nodes(|v| v == "a");
    assert_eq!(filtered.nodes(), ["a"]);
}

#[test]
fn filter_nodes_removes_edges_connected_to_removed_nodes() {
    let mut g = simple();
    g.set_edge("a", "b");
    let filtered = g.filter_nodes(|v| v == "a");
    assert_eq!(filtered.nodes(), ["a"]);
    assert!(filtered.edges().is_empty());
}

#[test]
fn filter_nodes_preserves_the_directed_option() {
    let directed: Graph = Graph::with_options(GraphOptions {
        directed: true,
        ..Default::default()
    });
    assert!(directed.filter_nodes(|_| true).is_directed());

    let undirected: Graph = Graph::with_options(GraphOptions {
        directed: false,
        ..Default::default()
    });
    assert!(!undirected.filter_nodes(|_| true).is_directed());
}

#[test]
fn filter_nodes_preserves_the_multigraph_option() {
    let multigraph: Graph = Graph::with_options(GraphOptions {
        multigraph: true,
        ..Default::default()
    });
    assert!(multigraph.filter_nodes(|_| true).is_multigraph());

    let simple: Graph = Graph::with_options(GraphOptions {
        multigraph: false,
        ..Default::default()
    });
    assert!(!simple.filter_nodes(|_| true).is_multigraph());
}

#[test]
fn filter_nodes_preserves_the_compound_option() {
    let compound: Graph = Graph::with_options(GraphOptions {
        compound: true,
        ..Default::default()
    });
    assert!(compound.filter_nodes(|_| true).is_compound());

    let simple: Graph = Graph::with_options(GraphOptions {
        compound: false,
        ..Default::default()
    });
    assert!(!simple.filter_nodes(|_| true).is_compound());
}

#[test]
fn filter_nodes_includes_subgraphs() {
    let mut g = compound();
    g.set_parent("a", Some("parent")).unwrap();
    let filtered = g.filter_nodes(|_| true);
    assert_eq!(filtered.parent("a"), Some("parent"));
}

#[test]
fn filter_nodes_includes_multi_level_subgraphs() {
    let mut g = compound();
    g.set_parent("a", Some("parent")).unwrap();
    g.set_parent("parent", Some("root")).unwrap();
    let filtered = g.filter_nodes(|_| true);
    assert_eq!(filtered.parent("a"), Some("parent"));
    assert_eq!(filtered.parent("parent"), Some("root"));
}

#[test]
fn filter_nodes_promotes_a_node_whose_parent_is_excluded() {
    let mut g = compound();
    g.set_parent("a", Some("parent")).unwrap();
    g.set_parent("parent", Some("root")).unwrap();
    let filtered = g.filter_nodes(|v| v != "parent");
    assert_eq!(filtered.parent("a"), Some("root"));
}

#[test]
fn filter_nodes_keeps_an_empty_string_parent() {
    let mut g = compound();
    g.set_parent("a", Some("")).unwrap();
    let filtered = g.filter_nodes(|v| !v.is_empty());
    assert_eq!(filtered.parent("a"), Some(""));
}

#[test]
fn set_nodes_creates_multiple_nodes() {
    let mut g = simple();
    g.set_nodes(&["a", "b", "c"]);
    assert!(g.has_node("a"));
    assert!(g.has_node("b"));
    assert!(g.has_node("c"));
}

#[test]
fn set_nodes_can_set_a_value_for_all_nodes() {
    let mut g: Graph<(), &str, ()> = Graph::new();
    g.set_nodes_with(&["a", "b", "c"], "foo");
    assert_eq!(g.node("a"), Some(&"foo"));
    assert_eq!(g.node("b"), Some(&"foo"));
    assert_eq!(g.node("c"), Some(&"foo"));
}

#[test]
fn set_nodes_is_chainable() {
    let mut g = simple();
    g.set_nodes(&["a"]).set_nodes(&["b"]);
    assert_eq!(sorted(g.nodes()), ["a", "b"]);
}

#[test]
fn set_node_creates_the_node() {
    let mut g = simple();
    g.set_node("a");
    assert!(g.has_node("a"));
    assert_eq!(g.node("a"), None);
    assert_eq!(g.node_count(), 1);
}

#[test]
fn set_node_can_set_a_value() {
    let mut g: Graph<(), &str, ()> = Graph::new();
    g.set_node_with("a", "foo");
    assert_eq!(g.node("a"), Some(&"foo"));
}

#[test]
fn set_node_does_not_change_the_value_with_a_one_argument_call() {
    let mut g: Graph<(), &str, ()> = Graph::new();
    g.set_node_with("a", "foo");
    g.set_node("a");
    assert_eq!(g.node("a"), Some(&"foo"));
}

#[test]
fn set_node_can_remove_the_value_with_none() {
    let mut g: Graph<(), &str, ()> = Graph::new();
    g.set_node_label("a", None);
    assert_eq!(g.node("a"), None);
}

#[test]
fn set_node_is_idempotent() {
    let mut g: Graph<(), &str, ()> = Graph::new();
    g.set_node_with("a", "foo");
    g.set_node_with("a", "foo");
    assert_eq!(g.node("a"), Some(&"foo"));
    assert_eq!(g.node_count(), 1);
}

#[test]
fn set_node_uses_the_stringified_id() {
    let mut g = simple();
    g.set_node("1");
    assert!(g.has_node("1"));
    assert_eq!(g.nodes(), ["1"]);
}

#[test]
fn set_node_is_chainable() {
    let mut g = simple();
    g.set_node("a").set_node("b");
    assert_eq!(sorted(g.nodes()), ["a", "b"]);
}

#[test]
fn set_default_node_label_applies_to_new_nodes() {
    let mut g: Graph<(), &str, ()> = Graph::new();
    g.set_default_node_label("foo");
    g.set_node("a");
    assert_eq!(g.node("a"), Some(&"foo"));
}

#[test]
fn set_default_node_label_does_not_change_existing_nodes() {
    let mut g: Graph<(), &str, ()> = Graph::new();
    g.set_node("a");
    g.set_default_node_label("foo");
    assert_eq!(g.node("a"), None);
}

#[test]
fn set_default_node_label_is_not_used_for_an_explicit_value() {
    let mut g: Graph<(), &str, ()> = Graph::new();
    g.set_default_node_label("foo");
    g.set_node_with("a", "bar");
    assert_eq!(g.node("a"), Some(&"bar"));
}

#[test]
fn set_default_node_label_can_take_a_function() {
    let mut g: Graph<(), &str, ()> = Graph::new();
    g.set_default_node_label_fn(|_| "foo");
    g.set_node("a");
    assert_eq!(g.node("a"), Some(&"foo"));
}

#[test]
fn set_default_node_label_can_take_a_function_of_the_name() {
    let mut g: Graph<(), String, ()> = Graph::new();
    g.set_default_node_label_fn(|v| format!("{v}-foo"));
    g.set_node("a");
    assert_eq!(g.node("a"), Some(&"a-foo".to_string()));
}

#[test]
fn set_default_node_label_is_chainable() {
    let mut g: Graph<(), &str, ()> = Graph::new();
    g.set_default_node_label("foo")
        .set_default_node_label("bar");
    g.set_node("a");
    assert_eq!(g.node("a"), Some(&"bar"));
}

#[test]
fn node_is_undefined_for_an_absent_node() {
    assert_eq!(simple().node("a"), None);
}

#[test]
fn node_returns_the_value_for_a_present_node() {
    let mut g: Graph<(), &str, ()> = Graph::new();
    g.set_node_with("a", "foo");
    assert_eq!(g.node("a"), Some(&"foo"));
}

#[test]
fn remove_node_does_nothing_for_an_absent_node() {
    let mut g = simple();
    assert_eq!(g.node_count(), 0);
    g.remove_node("a");
    assert!(!g.has_node("a"));
    assert_eq!(g.node_count(), 0);
}

#[test]
fn remove_node_removes_a_present_node() {
    let mut g = simple();
    g.set_node("a");
    g.remove_node("a");
    assert!(!g.has_node("a"));
    assert_eq!(g.node_count(), 0);
}

#[test]
fn remove_node_is_idempotent() {
    let mut g = simple();
    g.set_node("a");
    g.remove_node("a");
    g.remove_node("a");
    assert!(!g.has_node("a"));
    assert_eq!(g.node_count(), 0);
}

#[test]
fn remove_node_removes_incident_edges() {
    let mut g = simple();
    g.set_edge("a", "b");
    g.set_edge("b", "c");
    g.remove_node("b");
    assert_eq!(g.edge_count(), 0);
}

#[test]
fn remove_node_removes_parent_child_relationships() {
    let mut g = compound();
    g.set_parent("c", Some("b")).unwrap();
    g.set_parent("b", Some("a")).unwrap();
    g.remove_node("b");
    assert_eq!(g.parent("b"), None);
    assert!(g.children("b").is_empty());
    assert!(!g.children("a").contains(&"b".to_string()));
    assert_eq!(g.parent("c"), None);
}

#[test]
fn remove_node_is_chainable() {
    let mut g = simple();
    g.set_node("a").remove_node("a").set_node("b");
    assert_eq!(g.nodes(), ["b"]);
}

#[test]
fn set_parent_throws_on_a_non_compound_graph() {
    let mut g = simple();
    assert!(g.set_parent("a", Some("parent")).is_err());
}

#[test]
fn set_parent_creates_the_parent() {
    let mut g = compound();
    g.set_node("a");
    g.set_parent("a", Some("parent")).unwrap();
    assert!(g.has_node("parent"));
    assert_eq!(g.parent("a"), Some("parent"));
}

#[test]
fn set_parent_creates_the_child() {
    let mut g = compound();
    g.set_node("parent");
    g.set_parent("a", Some("parent")).unwrap();
    assert!(g.has_node("a"));
    assert_eq!(g.parent("a"), Some("parent"));
}

#[test]
fn parent_is_undefined_when_never_assigned() {
    let mut g = compound();
    g.set_node("a");
    assert_eq!(g.parent("a"), None);
}

#[test]
fn set_parent_moves_the_node_from_the_previous_parent() {
    let mut g = compound();
    g.set_parent("a", Some("parent")).unwrap();
    g.set_parent("a", Some("parent2")).unwrap();
    assert_eq!(g.parent("a"), Some("parent2"));
    assert!(g.children("parent").is_empty());
    assert_eq!(g.children("parent2"), ["a"]);
}

#[test]
fn set_parent_removes_the_parent_with_none() {
    let mut g = compound();
    g.set_parent("a", Some("parent")).unwrap();
    g.set_parent("a", None).unwrap();
    assert_eq!(g.parent("a"), None);
    assert_eq!(sorted(g.root_children()), ["a", "parent"]);
}

#[test]
fn set_parent_removes_the_parent_when_omitted() {
    let mut g = compound();
    g.set_parent("a", Some("parent")).unwrap();
    g.set_parent("a", None).unwrap();
    assert_eq!(g.parent("a"), None);
    assert_eq!(sorted(g.root_children()), ["a", "parent"]);
}

#[test]
fn set_parent_is_idempotent_when_removing_a_parent() {
    let mut g = compound();
    g.set_parent("a", Some("parent")).unwrap();
    g.set_parent("a", None).unwrap();
    g.set_parent("a", None).unwrap();
    assert_eq!(g.parent("a"), None);
    assert_eq!(sorted(g.root_children()), ["a", "parent"]);
}

#[test]
fn set_parent_uses_string_ids() {
    let mut g = compound();
    g.set_parent("2", Some("1")).unwrap();
    g.set_parent("3", Some("2")).unwrap();
    assert_eq!(g.parent("2"), Some("1"));
    assert_eq!(g.parent("3"), Some("2"));
}

#[test]
fn set_parent_preserves_the_tree_invariant() {
    let mut g = compound();
    g.set_parent("c", Some("b")).unwrap();
    g.set_parent("b", Some("a")).unwrap();
    assert!(g.set_parent("a", Some("c")).is_err());
}

#[test]
fn set_parent_is_chainable() {
    let mut g = compound();
    g.set_parent("a", Some("parent"))
        .unwrap()
        .set_parent("b", Some("parent"))
        .unwrap();
    assert_eq!(g.parent("a"), Some("parent"));
    assert_eq!(g.parent("b"), Some("parent"));
}

#[test]
fn parent_is_undefined_on_a_non_compound_graph() {
    let g: Graph = Graph::with_options(GraphOptions {
        compound: false,
        ..Default::default()
    });
    assert_eq!(g.parent("a"), None);
}

#[test]
fn parent_is_undefined_for_an_absent_node() {
    assert_eq!(compound().parent("a"), None);
}

#[test]
fn parent_defaults_to_undefined_for_new_nodes() {
    let mut g = compound();
    g.set_node("a");
    assert_eq!(g.parent("a"), None);
}

#[test]
fn parent_returns_the_current_assignment() {
    let mut g = compound();
    g.set_node("a");
    g.set_node("parent");
    g.set_parent("a", Some("parent")).unwrap();
    assert_eq!(g.parent("a"), Some("parent"));
}

#[test]
fn children_is_empty_for_an_absent_node() {
    assert!(compound().children("a").is_empty());
}

#[test]
fn children_defaults_to_an_empty_list_for_new_nodes() {
    let mut g = compound();
    g.set_node("a");
    assert!(g.children("a").is_empty());
}

#[test]
fn children_is_empty_for_a_non_compound_graph_without_the_node() {
    let g = simple();
    assert!(g.children("a").is_empty());
}

#[test]
fn children_is_empty_for_a_non_compound_graph_with_the_node() {
    let mut g = simple();
    g.set_node("a");
    assert!(g.children("a").is_empty());
}

#[test]
fn children_returns_all_nodes_for_the_root_of_a_non_compound_graph() {
    let mut g = simple();
    g.set_node("a");
    g.set_node("b");
    assert_eq!(sorted(g.root_children()), ["a", "b"]);
}

#[test]
fn children_returns_children_for_the_node() {
    let mut g = compound();
    g.set_parent("a", Some("parent")).unwrap();
    g.set_parent("b", Some("parent")).unwrap();
    assert_eq!(sorted(g.children("parent")), ["a", "b"]);
}

#[test]
fn children_returns_all_nodes_without_a_parent() {
    let mut g = compound();
    g.set_node("a");
    g.set_node("b");
    g.set_node("c");
    g.set_node("parent");
    g.set_parent("a", Some("parent")).unwrap();
    assert_eq!(sorted(g.root_children()), ["b", "c", "parent"]);
}

#[test]
fn predecessors_are_undefined_for_an_absent_node() {
    assert_eq!(simple().predecessors("a"), None);
}

#[test]
fn predecessors_returns_the_predecessors() {
    let mut g = simple();
    g.set_edge("a", "b");
    g.set_edge("b", "c");
    g.set_edge("a", "a");
    assert_eq!(sorted(g.predecessors("a").unwrap()), ["a"]);
    assert_eq!(sorted(g.predecessors("b").unwrap()), ["a"]);
    assert_eq!(sorted(g.predecessors("c").unwrap()), ["b"]);
}

#[test]
fn successors_are_undefined_for_an_absent_node() {
    assert_eq!(simple().successors("a"), None);
}

#[test]
fn successors_returns_the_successors() {
    let mut g = simple();
    g.set_edge("a", "b");
    g.set_edge("b", "c");
    g.set_edge("a", "a");
    assert_eq!(sorted(g.successors("a").unwrap()), ["a", "b"]);
    assert_eq!(sorted(g.successors("b").unwrap()), ["c"]);
    assert_eq!(sorted(g.successors("c").unwrap()), Vec::<String>::new());
}

#[test]
fn neighbors_are_undefined_for_an_absent_node() {
    assert_eq!(simple().neighbors("a"), None);
}

#[test]
fn neighbors_returns_the_neighbors() {
    let mut g = simple();
    g.set_edge("a", "b");
    g.set_edge("b", "c");
    g.set_edge("a", "a");
    assert_eq!(sorted(g.neighbors("a").unwrap()), ["a", "b"]);
    assert_eq!(sorted(g.neighbors("b").unwrap()), ["a", "c"]);
    assert_eq!(sorted(g.neighbors("c").unwrap()), ["b"]);
}

#[test]
fn is_leaf_is_false_for_a_connected_node_in_an_undirected_graph() {
    let mut g: Graph = Graph::with_options(GraphOptions::undirected());
    g.set_node("a");
    g.set_node("b");
    g.set_edge("a", "b");
    assert!(!g.is_leaf("b"));
}

#[test]
fn is_leaf_is_true_for_an_unconnected_node_in_an_undirected_graph() {
    let mut g: Graph = Graph::with_options(GraphOptions::undirected());
    g.set_node("a");
    assert!(g.is_leaf("a"));
}

#[test]
fn is_leaf_is_true_for_an_unconnected_node_in_a_directed_graph() {
    let mut g = simple();
    g.set_node("a");
    assert!(g.is_leaf("a"));
}

#[test]
fn is_leaf_is_false_for_a_predecessor_in_a_directed_graph() {
    let mut g = simple();
    g.set_node("a");
    g.set_node("b");
    g.set_edge("a", "b");
    assert!(!g.is_leaf("a"));
}

#[test]
fn is_leaf_is_true_for_a_successor_in_a_directed_graph() {
    let mut g = simple();
    g.set_node("a");
    g.set_node("b");
    g.set_edge("a", "b");
    assert!(g.is_leaf("b"));
}

#[test]
fn edges_are_empty_without_edges() {
    assert!(simple().edges().is_empty());
}

#[test]
fn edges_returns_the_edge_objects() {
    let mut g = simple();
    g.set_edge("a", "b");
    g.set_edge("b", "c");
    let mut edges = g.edges();
    sort_edges(&mut edges);
    assert_eq!(edges, [Edge::new("a", "b"), Edge::new("b", "c")]);
}

#[test]
fn set_path_creates_a_path_of_multiple_edges() {
    let mut g = simple();
    g.set_path(&["a", "b", "c"]);
    assert!(g.has_edge("a", "b"));
    assert!(g.has_edge("b", "c"));
}

#[test]
fn set_path_can_set_a_value_for_all_edges() {
    let mut g: Graph<(), (), &str> = Graph::new();
    g.set_path_with(&["a", "b", "c"], "foo");
    assert_eq!(g.edge("a", "b"), Some(&"foo"));
    assert_eq!(g.edge("b", "c"), Some(&"foo"));
}

#[test]
fn set_path_is_chainable() {
    let mut g = simple();
    g.set_path(&["a", "b"]).set_path(&["b", "c"]);
    assert!(g.has_edge("b", "c"));
}

#[test]
fn set_edge_creates_the_edge() {
    let mut g = simple();
    g.set_node("a");
    g.set_node("b");
    g.set_edge("a", "b");
    assert_eq!(g.edge("a", "b"), None);
    assert!(g.has_edge("a", "b"));
    assert!(g.has_edge_object(&Edge::new("a", "b")));
    assert_eq!(g.edge_count(), 1);
}

#[test]
fn set_edge_creates_its_nodes() {
    let mut g = simple();
    g.set_edge("a", "b");
    assert!(g.has_node("a"));
    assert!(g.has_node("b"));
    assert_eq!(g.node_count(), 2);
}

#[test]
fn set_edge_creates_a_multi_edge() {
    let mut g: Graph = Graph::with_options(GraphOptions::multigraph());
    g.set_edge_full("a", "b", None, Some("name")).unwrap();
    assert!(!g.has_edge("a", "b"));
    assert!(g.has_edge_label("a", "b", Some("name")));
}

#[test]
fn set_edge_throws_for_a_multi_edge_on_a_non_multigraph() {
    let mut g = simple();
    assert!(g.set_edge_full("a", "b", None, Some("name")).is_err());
}

#[test]
fn set_edge_changes_an_existing_value() {
    let mut g: Graph<(), (), &str> = Graph::new();
    g.set_edge_with("a", "b", "foo");
    g.set_edge_with("a", "b", "bar");
    assert_eq!(g.edge("a", "b"), Some(&"bar"));
}

#[test]
fn set_edge_deletes_the_value_when_none() {
    let mut g: Graph<(), (), &str> = Graph::new();
    g.set_edge_with("a", "b", "foo");
    g.set_edge_label("a", "b", None);
    assert_eq!(g.edge("a", "b"), None);
    assert!(g.has_edge("a", "b"));
}

#[test]
fn set_edge_changes_a_multi_edge_value() {
    let mut g: Graph<(), (), &str> = Graph::with_options(GraphOptions::multigraph());
    g.set_edge_full("a", "b", Some("value"), Some("name"))
        .unwrap();
    g.set_edge_full("a", "b", None, Some("name")).unwrap();
    assert_eq!(g.edge_label("a", "b", Some("name")), None);
    assert!(g.has_edge_label("a", "b", Some("name")));
}

#[test]
fn set_edge_can_take_an_edge_object() {
    let mut g: Graph<(), (), &str> = Graph::new();
    g.set_edge_object_with(&Edge::new("a", "b"), "value")
        .unwrap();
    assert_eq!(g.edge("a", "b"), Some(&"value"));
}

#[test]
fn set_edge_can_take_a_multi_edge_object() {
    let mut g: Graph<(), (), &str> = Graph::with_options(GraphOptions::multigraph());
    g.set_edge_object_with(&Edge::named("a", "b", "name"), "value")
        .unwrap();
    assert_eq!(g.edge_label("a", "b", Some("name")), Some(&"value"));
}

#[test]
fn set_edge_uses_the_stringified_id() {
    let mut g: Graph<(), (), &str> = Graph::new();
    g.set_edge_with("1", "2", "foo");
    assert_eq!(g.edges(), [Edge::new("1", "2")]);
    assert_eq!(g.edge("1", "2"), Some(&"foo"));
}

#[test]
fn set_edge_uses_the_stringified_id_with_a_name() {
    let mut g: Graph<(), (), &str> = Graph::with_options(GraphOptions::multigraph());
    g.set_edge_full("1", "2", Some("foo"), Some("3")).unwrap();
    assert_eq!(g.edge_label("1", "2", Some("3")), Some(&"foo"));
    assert_eq!(g.edges(), [Edge::named("1", "2", "3")]);
}

#[test]
fn set_edge_treats_opposite_directions_as_distinct() {
    let mut g = simple();
    g.set_edge("a", "b");
    assert!(g.has_edge("a", "b"));
    assert!(!g.has_edge("b", "a"));
}

#[test]
fn set_edge_handles_undirected_graphs() {
    let mut g: Graph<(), (), &str> = Graph::with_options(GraphOptions::undirected());
    g.set_edge_with("a", "b", "foo");
    assert_eq!(g.edge("a", "b"), Some(&"foo"));
    assert_eq!(g.edge("b", "a"), Some(&"foo"));
}

#[test]
fn set_edge_handles_undirected_edges_whose_ids_have_another_order() {
    let mut g: Graph<(), (), &str> = Graph::with_options(GraphOptions::undirected());
    g.set_edge_with("9", "10", "foo");
    assert!(g.has_edge("9", "10"));
    assert!(g.has_edge("10", "9"));
    assert_eq!(g.edge("9", "10"), Some(&"foo"));
}

#[test]
fn set_edge_is_chainable() {
    let mut g = simple();
    g.set_edge("a", "b").set_edge("b", "c");
    assert!(g.has_edge("b", "c"));
}

#[test]
fn set_default_edge_label_applies_to_new_edges() {
    let mut g: Graph<(), (), &str> = Graph::new();
    g.set_default_edge_label("foo");
    g.set_edge("a", "b");
    assert_eq!(g.edge("a", "b"), Some(&"foo"));
}

#[test]
fn set_default_edge_label_does_not_change_existing_edges() {
    let mut g: Graph<(), (), &str> = Graph::new();
    g.set_edge("a", "b");
    g.set_default_edge_label("foo");
    assert_eq!(g.edge("a", "b"), None);
}

#[test]
fn set_default_edge_label_is_not_used_for_an_explicit_value() {
    let mut g: Graph<(), (), &str> = Graph::new();
    g.set_default_edge_label("foo");
    g.set_edge_with("a", "b", "bar");
    assert_eq!(g.edge("a", "b"), Some(&"bar"));
}

#[test]
fn set_default_edge_label_can_take_a_function() {
    let mut g: Graph<(), (), &str> = Graph::new();
    g.set_default_edge_label_fn(|_, _, _| "foo");
    g.set_edge("a", "b");
    assert_eq!(g.edge("a", "b"), Some(&"foo"));
}

#[test]
fn set_default_edge_label_can_take_a_function_of_the_endpoints_and_name() {
    let mut g: Graph<(), (), String> = Graph::with_options(GraphOptions::multigraph());
    g.set_default_edge_label_fn(|v, w, name| format!("{v}-{w}-{}-foo", name.unwrap_or("")));
    g.set_edge_object(&Edge::named("a", "b", "name")).unwrap();
    assert_eq!(
        g.edge_label("a", "b", Some("name")),
        Some(&"a-b-name-foo".to_string())
    );
}

#[test]
fn set_default_edge_label_does_not_override_an_existing_multi_edge() {
    let mut g: Graph<(), (), &str> = Graph::with_options(GraphOptions::multigraph());
    g.set_edge_full("a", "b", Some("old"), Some("name"))
        .unwrap();
    g.set_default_edge_label_fn(|_, _, _| "should not set this");
    g.set_edge_object(&Edge::named("a", "b", "name")).unwrap();
    assert_eq!(g.edge_label("a", "b", Some("name")), Some(&"old"));
}

#[test]
fn set_default_edge_label_is_chainable() {
    let mut g: Graph<(), (), &str> = Graph::new();
    g.set_default_edge_label("foo")
        .set_default_edge_label("bar");
    g.set_edge("a", "b");
    assert_eq!(g.edge("a", "b"), Some(&"bar"));
}

#[test]
fn edge_is_undefined_for_an_absent_edge() {
    let g: Graph = simple();
    assert_eq!(g.edge("a", "b"), None);
    assert_eq!(g.edge_object(&Edge::new("a", "b")), None);
    assert_eq!(g.edge_label("a", "b", Some("foo")), None);
}

#[test]
fn edge_returns_the_value_for_a_present_edge() {
    let mut g: Graph<(), (), &str> = Graph::new();
    g.set_edge_with("a", "b", "bar");
    assert_eq!(g.edge("a", "b"), Some(&"bar"));
    assert_eq!(g.edge_object(&Edge::new("a", "b")), Some(&"bar"));
    assert_eq!(g.edge("b", "a"), None);
}

#[test]
fn edge_returns_the_value_of_a_multi_edge() {
    let mut g: Graph<(), (), &str> = Graph::with_options(GraphOptions::multigraph());
    g.set_edge_full("a", "b", Some("baz"), Some("foo")).unwrap();
    assert_eq!(g.edge_label("a", "b", Some("foo")), Some(&"baz"));
    assert_eq!(g.edge("a", "b"), None);
}

#[test]
fn edge_returns_an_edge_in_either_direction_in_an_undirected_graph() {
    let mut g: Graph<(), (), &str> = Graph::with_options(GraphOptions::undirected());
    g.set_edge_with("a", "b", "bar");
    assert_eq!(g.edge("a", "b"), Some(&"bar"));
    assert_eq!(g.edge("b", "a"), Some(&"bar"));
}

#[test]
fn remove_edge_has_no_effect_for_an_absent_edge() {
    let mut g = simple();
    g.remove_edge("a", "b");
    assert!(!g.has_edge("a", "b"));
    assert_eq!(g.edge_count(), 0);
}

#[test]
fn remove_edge_can_remove_an_edge_by_object() {
    let mut g: Graph = Graph::with_options(GraphOptions::multigraph());
    g.set_edge_object(&Edge::named("a", "b", "foo")).unwrap();
    g.remove_edge_object(&Edge::named("a", "b", "foo"));
    assert!(!g.has_edge_label("a", "b", Some("foo")));
    assert_eq!(g.edge_count(), 0);
}

#[test]
fn remove_edge_can_remove_an_edge_by_separate_ids() {
    let mut g: Graph = Graph::with_options(GraphOptions::multigraph());
    g.set_edge_object(&Edge::named("a", "b", "foo")).unwrap();
    g.remove_edge_label("a", "b", Some("foo"));
    assert!(!g.has_edge_label("a", "b", Some("foo")));
    assert_eq!(g.edge_count(), 0);
}

#[test]
fn remove_edge_correctly_removes_neighbors() {
    let mut g = simple();
    g.set_edge("a", "b");
    g.remove_edge("a", "b");
    assert_eq!(g.successors("a"), Some(Vec::new()));
    assert_eq!(g.neighbors("a"), Some(Vec::new()));
    assert_eq!(g.predecessors("b"), Some(Vec::new()));
    assert_eq!(g.neighbors("b"), Some(Vec::new()));
}

#[test]
fn remove_edge_correctly_decrements_neighbor_counts() {
    let mut g: Graph = Graph::with_options(GraphOptions::multigraph());
    g.set_edge("a", "b");
    g.set_edge_object(&Edge::named("a", "b", "foo")).unwrap();
    g.remove_edge("a", "b");
    assert!(g.has_edge_label("a", "b", Some("foo")));
    assert_eq!(g.successors("a"), Some(vec!["b".to_string()]));
    assert_eq!(g.neighbors("a"), Some(vec!["b".to_string()]));
    assert_eq!(g.predecessors("b"), Some(vec!["a".to_string()]));
    assert_eq!(g.neighbors("b"), Some(vec!["a".to_string()]));
}

#[test]
fn remove_edge_works_with_undirected_graphs() {
    let mut g: Graph = Graph::with_options(GraphOptions::undirected());
    g.set_edge("h", "g");
    g.remove_edge("g", "h");
    assert_eq!(g.neighbors("g"), Some(Vec::new()));
    assert_eq!(g.neighbors("h"), Some(Vec::new()));
}

#[test]
fn remove_edge_is_chainable() {
    let mut g = simple();
    g.set_edge("a", "b")
        .remove_edge("a", "b")
        .set_edge("b", "c");
    assert!(g.has_edge("b", "c"));
}

#[test]
fn in_edges_is_undefined_for_an_absent_node() {
    assert_eq!(simple().in_edges("a", None), None);
}

#[test]
fn in_edges_returns_edges_that_point_at_the_node() {
    let mut g = simple();
    g.set_edge("a", "b");
    g.set_edge("b", "c");
    assert_eq!(g.in_edges("a", None), Some(Vec::new()));
    assert_eq!(g.in_edges("b", None), Some(vec![Edge::new("a", "b")]));
    assert_eq!(g.in_edges("c", None), Some(vec![Edge::new("b", "c")]));
}

#[test]
fn in_edges_works_for_multigraphs() {
    let mut g: Graph = Graph::with_options(GraphOptions::multigraph());
    g.set_edge("a", "b");
    g.set_edge_full("a", "b", None, Some("bar")).unwrap();
    g.set_edge_full("a", "b", None, Some("foo")).unwrap();
    assert_eq!(g.in_edges("a", None), Some(Vec::new()));
    let mut edges = g.in_edges("b", None).unwrap();
    sort_edges(&mut edges);
    assert_eq!(
        edges,
        [
            Edge::named("a", "b", "bar"),
            Edge::named("a", "b", "foo"),
            Edge::new("a", "b")
        ]
    );
}

#[test]
fn in_edges_can_return_only_edges_from_a_node() {
    let mut g: Graph = Graph::with_options(GraphOptions::multigraph());
    g.set_edge("a", "b");
    g.set_edge_full("a", "b", None, Some("foo")).unwrap();
    g.set_edge("a", "c");
    g.set_edge("b", "c");
    g.set_edge("z", "a");
    g.set_edge("z", "b");
    assert_eq!(g.in_edges("a", Some("b")), Some(Vec::new()));
    let mut edges = g.in_edges("b", Some("a")).unwrap();
    sort_edges(&mut edges);
    assert_eq!(edges, [Edge::named("a", "b", "foo"), Edge::new("a", "b")]);
}

#[test]
fn out_edges_is_undefined_for_an_absent_node() {
    assert_eq!(simple().out_edges("a", None), None);
}

#[test]
fn out_edges_returns_edges_that_the_node_points_at() {
    let mut g = simple();
    g.set_edge("a", "b");
    g.set_edge("b", "c");
    assert_eq!(g.out_edges("a", None), Some(vec![Edge::new("a", "b")]));
    assert_eq!(g.out_edges("b", None), Some(vec![Edge::new("b", "c")]));
    assert_eq!(g.out_edges("c", None), Some(Vec::new()));
}

#[test]
fn out_edges_works_for_multigraphs() {
    let mut g: Graph = Graph::with_options(GraphOptions::multigraph());
    g.set_edge("a", "b");
    g.set_edge_full("a", "b", None, Some("bar")).unwrap();
    g.set_edge_full("a", "b", None, Some("foo")).unwrap();
    let mut edges = g.out_edges("a", None).unwrap();
    sort_edges(&mut edges);
    assert_eq!(
        edges,
        [
            Edge::named("a", "b", "bar"),
            Edge::named("a", "b", "foo"),
            Edge::new("a", "b")
        ]
    );
    assert_eq!(g.out_edges("b", None), Some(Vec::new()));
}

#[test]
fn out_edges_can_return_only_edges_to_a_node() {
    let mut g: Graph = Graph::with_options(GraphOptions::multigraph());
    g.set_edge("a", "b");
    g.set_edge_full("a", "b", None, Some("foo")).unwrap();
    g.set_edge("a", "c");
    g.set_edge("b", "c");
    g.set_edge("z", "a");
    g.set_edge("z", "b");
    let mut edges = g.out_edges("a", Some("b")).unwrap();
    sort_edges(&mut edges);
    assert_eq!(edges, [Edge::named("a", "b", "foo"), Edge::new("a", "b")]);
    assert_eq!(g.out_edges("b", Some("a")), Some(Vec::new()));
}

#[test]
fn node_edges_is_undefined_for_an_absent_node() {
    assert_eq!(simple().node_edges("a", None), None);
}

#[test]
fn node_edges_returns_all_incident_edges() {
    let mut g = simple();
    g.set_edge("a", "b");
    g.set_edge("b", "c");
    assert_eq!(g.node_edges("a", None), Some(vec![Edge::new("a", "b")]));
    let mut edges = g.node_edges("b", None).unwrap();
    sort_edges(&mut edges);
    assert_eq!(edges, [Edge::new("a", "b"), Edge::new("b", "c")]);
    assert_eq!(g.node_edges("c", None), Some(vec![Edge::new("b", "c")]));
}

#[test]
fn node_edges_works_for_multigraphs() {
    let mut g: Graph = Graph::with_options(GraphOptions::multigraph());
    g.set_edge("a", "b");
    g.set_edge_object(&Edge::named("a", "b", "bar")).unwrap();
    g.set_edge_object(&Edge::named("a", "b", "foo")).unwrap();
    let mut edges = g.node_edges("a", None).unwrap();
    sort_edges(&mut edges);
    assert_eq!(
        edges,
        [
            Edge::named("a", "b", "bar"),
            Edge::named("a", "b", "foo"),
            Edge::new("a", "b")
        ]
    );
    let mut edges = g.node_edges("b", None).unwrap();
    sort_edges(&mut edges);
    assert_eq!(
        edges,
        [
            Edge::named("a", "b", "bar"),
            Edge::named("a", "b", "foo"),
            Edge::new("a", "b")
        ]
    );
}

#[test]
fn node_edges_can_return_only_edges_between_specific_nodes() {
    let mut g: Graph = Graph::with_options(GraphOptions::multigraph());
    g.set_edge("a", "b");
    g.set_edge_object(&Edge::named("a", "b", "foo")).unwrap();
    g.set_edge("a", "c");
    g.set_edge("b", "c");
    g.set_edge("z", "a");
    g.set_edge("z", "b");
    let mut edges = g.node_edges("a", Some("b")).unwrap();
    sort_edges(&mut edges);
    assert_eq!(edges, [Edge::named("a", "b", "foo"), Edge::new("a", "b")]);
    let mut edges = g.node_edges("b", Some("a")).unwrap();
    sort_edges(&mut edges);
    assert_eq!(edges, [Edge::named("a", "b", "foo"), Edge::new("a", "b")]);
}

#[test]
fn insertion_order_is_kept_for_nodes_and_edges() {
    let mut g = simple();
    for name in ["c", "a", "b"] {
        g.set_node(name);
    }
    assert_eq!(g.nodes(), ["c", "a", "b"]);
    g.set_edge("c", "a");
    g.set_edge("b", "a");
    assert_eq!(g.edges(), [Edge::new("c", "a"), Edge::new("b", "a")]);
    g.remove_node("a");
    g.set_node("a");
    assert_eq!(g.nodes(), ["c", "b", "a"]);
}
