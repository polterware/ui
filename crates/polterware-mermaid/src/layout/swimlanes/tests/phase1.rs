use crate::layout::swimlanes::helpers::Graph;
use crate::layout::swimlanes::phase0_helpers::is_acyclic;
use crate::layout::swimlanes::phase1_cycles::remove_cycles_dfs;

fn mk(nodes: &[&str], pairs: &[(&str, &str)]) -> Graph {
    super::mk_graph(nodes, pairs)
}

fn directions(g: &Graph) -> Vec<String> {
    g.edges
        .iter()
        .map(|edge| format!("{}->{}", edge.src, edge.dst))
        .collect()
}

#[test]
fn no_cycles_returns_same_edges() {
    let g = mk(&["A", "B", "C"], &[("A", "B"), ("B", "C")]);
    let result = remove_cycles_dfs(&g);
    assert!(is_acyclic(&result.acyclic));
    assert!(result.reversed.is_empty());
    assert_eq!(directions(&result.acyclic), vec!["A->B", "B->C"]);
}

#[test]
fn single_back_edge_is_reversed() {
    let g = mk(&["A", "B", "C"], &[("A", "B"), ("B", "C"), ("C", "A")]);
    let result = remove_cycles_dfs(&g);
    assert!(is_acyclic(&result.acyclic));
    assert_eq!(result.reversed.len(), 1);
    assert!(
        result
            .acyclic
            .edges
            .iter()
            .any(|edge| edge.src == "A" && edge.dst == "C")
    );
}

#[test]
fn two_node_cycle_reverses_exactly_one_edge() {
    let g = mk(&["A", "B"], &[("A", "B"), ("B", "A")]);
    let result = remove_cycles_dfs(&g);
    assert!(is_acyclic(&result.acyclic));
    assert_eq!(result.reversed.len(), 1);
    assert_eq!(result.reversed[0].src, "B");
    assert_eq!(result.reversed[0].dst, "A");
}

#[test]
fn cycle_in_one_component_leaves_dag_untouched() {
    let g = mk(
        &["A", "B", "C", "X", "Y"],
        &[("A", "B"), ("B", "A"), ("X", "Y")],
    );
    let result = remove_cycles_dfs(&g);
    assert!(is_acyclic(&result.acyclic));
    let xy = result
        .acyclic
        .edges
        .iter()
        .find(|edge| edge.id == "e2")
        .unwrap();
    assert_eq!(xy.src, "X");
    assert_eq!(xy.dst, "Y");
}

#[test]
fn dense_small_graph_becomes_acyclic() {
    let nodes = ["A", "B", "C", "D"];
    let mut pairs = Vec::new();
    for i in 0..nodes.len() {
        for j in 0..nodes.len() {
            if i != j {
                pairs.push((nodes[i], nodes[j]));
            }
        }
    }
    let g = mk(&nodes, &pairs);
    let result = remove_cycles_dfs(&g);
    assert!(is_acyclic(&result.acyclic));
}

#[test]
fn seeds_dfs_from_true_sources_not_alphabetical_order() {
    let g = mk(
        &["start", "task", "fix", "decision"],
        &[
            ("start", "task"),
            ("task", "decision"),
            ("decision", "fix"),
            ("fix", "task"),
        ],
    );
    let result = remove_cycles_dfs(&g);
    assert!(is_acyclic(&result.acyclic));
    let reversed: Vec<String> = result
        .reversed
        .iter()
        .map(|edge| format!("{}->{}", edge.src, edge.dst))
        .collect();
    assert_eq!(reversed, vec!["fix->task"]);
    let sources: Vec<&str> = result
        .acyclic
        .nodes
        .iter()
        .filter(|node| !result.acyclic.edges.iter().any(|edge| &edge.dst == *node))
        .map(String::as_str)
        .collect();
    assert_eq!(sources, vec!["start"]);
}

#[test]
fn falls_back_to_declaration_order_when_no_source_exists() {
    let g = mk(&["B", "A"], &[("B", "A"), ("A", "B")]);
    let result = remove_cycles_dfs(&g);
    assert!(is_acyclic(&result.acyclic));
    let reversed: Vec<String> = result
        .reversed
        .iter()
        .map(|edge| format!("{}->{}", edge.src, edge.dst))
        .collect();
    assert_eq!(reversed, vec!["A->B"]);
}
