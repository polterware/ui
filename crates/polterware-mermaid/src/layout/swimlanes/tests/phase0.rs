use crate::layout::swimlanes::helpers::Graph;
use crate::layout::swimlanes::phase0_helpers::{
    incoming, is_acyclic, normalize_graph, outgoing, topo_sort_if_acyclic,
};

fn mk(nodes: &[&str], pairs: &[(&str, &str)]) -> Graph {
    super::mk_graph(nodes, pairs)
}

#[test]
fn handles_empty_graph() {
    let g = mk(&[], &[]);
    let gn = normalize_graph(&g);
    assert!(is_acyclic(&gn));
    assert_eq!(topo_sort_if_acyclic(&gn), Some(Vec::new()));
}

#[test]
fn handles_single_node() {
    let g = mk(&["A"], &[]);
    let gn = normalize_graph(&g);
    assert!(is_acyclic(&gn));
    assert_eq!(topo_sort_if_acyclic(&gn), Some(vec!["A".to_string()]));
}

#[test]
fn parallel_edges_remain_acyclic() {
    let g = mk(&["A", "B"], &[("A", "B"), ("A", "B")]);
    let gn = normalize_graph(&g);
    assert!(is_acyclic(&gn));
    assert_eq!(
        topo_sort_if_acyclic(&gn),
        Some(vec!["A".to_string(), "B".to_string()])
    );
    assert_eq!(incoming(&gn, "B").len(), 2);
    assert_eq!(outgoing(&gn, "A").len(), 2);
}

#[test]
fn detects_self_loop_as_cyclic() {
    let g = mk(&["A"], &[("A", "A")]);
    let gn = normalize_graph(&g);
    assert!(!is_acyclic(&gn));
    assert_eq!(topo_sort_if_acyclic(&gn), None);
}

#[test]
fn two_node_cycle_is_cyclic() {
    let g = mk(&["A", "B"], &[("A", "B"), ("B", "A")]);
    let gn = normalize_graph(&g);
    assert!(!is_acyclic(&gn));
    assert_eq!(topo_sort_if_acyclic(&gn), None);
}

#[test]
fn disconnected_dag_topological_order_includes_all_nodes() {
    let g = mk(&["A", "B", "C", "D"], &[("A", "B"), ("C", "D")]);
    let gn = normalize_graph(&g);
    let order = topo_sort_if_acyclic(&gn).expect("acyclic");
    assert_eq!(order.len(), 4);
    let pos = |id: &str| order.iter().position(|node| node == id).unwrap();
    assert!(pos("A") < pos("B"));
    assert!(pos("C") < pos("D"));
}
