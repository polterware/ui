use crate::layout::swimlanes::helpers::Graph;
use crate::layout::swimlanes::phase3_ordering::total_crossings;
use crate::layout::swimlanes::pipeline::{LayoutOptions, sugiyama_layout};

fn mk(nodes: &[&str], pairs: &[(&str, &str)]) -> Graph {
    super::mk_graph(nodes, pairs)
}

#[test]
fn chain_yields_layered_y_and_no_reversed_edges() {
    let g = mk(&["A", "B", "C", "D"], &[("A", "B"), ("B", "C"), ("C", "D")]);
    let result = sugiyama_layout(
        &g,
        Some(LayoutOptions {
            node_gap: Some(40.0),
            layer_gap: Some(80.0),
            ignore_cross_lane_edges: Some(false),
            ..Default::default()
        }),
    );
    assert_eq!(result.reversed.len(), 0);
    assert_eq!(result.coordinates.y["A"], 0.0);
    assert_eq!(result.coordinates.y["B"], 80.0);
    assert_eq!(result.coordinates.y["C"], 160.0);
    assert_eq!(result.coordinates.y["D"], 240.0);
}

#[test]
fn two_node_cycle_gets_one_reversed_edge() {
    let g = mk(&["A", "B"], &[("A", "B"), ("B", "A")]);
    let result = sugiyama_layout(&g, None);
    assert_eq!(result.reversed.len(), 1);
}

#[test]
fn two_layer_bipartite_ordering_reaches_zero_crossings() {
    let g = mk(
        &["A", "B", "C", "X", "Y", "Z"],
        &[("A", "X"), ("B", "Y"), ("C", "Z")],
    );
    let result = sugiyama_layout(&g, None);
    assert_eq!(total_crossings(&result.ordered.layers, &g.edges), 0);
}
