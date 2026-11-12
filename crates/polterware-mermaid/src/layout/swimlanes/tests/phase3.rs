use std::collections::HashMap;

use crate::layout::swimlanes::helpers::{Graph, Layering};
use crate::layout::swimlanes::phase3_ordering::{order_layers, total_crossings};

fn mk(nodes: &[&str], pairs: &[(&str, &str)]) -> Graph {
    super::mk_graph(nodes, pairs)
}

fn layering(layers: Vec<Vec<&str>>, ranks: &[(&str, i64)]) -> Layering {
    Layering {
        layers: layers
            .into_iter()
            .map(|layer| layer.into_iter().map(str::to_string).collect())
            .collect(),
        rank_of: ranks
            .iter()
            .map(|(id, rank)| ((*id).to_string(), *rank))
            .collect::<HashMap<_, _>>(),
        dummy: Default::default(),
    }
}

#[test]
fn two_layer_bipartite_zero_crossing_order_is_achieved() {
    let nodes = ["U1", "U2", "L1", "L2"];
    let g = mk(&nodes, &[("U1", "L1"), ("U2", "L2")]);
    let layering = layering(
        vec![vec!["U1", "U2"], vec!["L2", "L1"]],
        &[("U1", 0), ("U2", 0), ("L1", 1), ("L2", 1)],
    );
    let ordered = order_layers(&layering, &g, None);
    assert_eq!(ordered.layers[0], vec!["U1", "U2"]);
    assert_eq!(ordered.layers[1], vec!["L1", "L2"]);
    assert_eq!(total_crossings(&ordered.layers, &g.edges), 0);
}

#[test]
fn transpose_reduces_crossings_on_a_crafted_case() {
    let nodes = ["A", "B", "C", "d", "e", "f"];
    let g = mk(&nodes, &[("A", "e"), ("B", "d"), ("C", "f")]);
    let layering = layering(
        vec![vec!["A", "B", "C"], vec!["d", "e", "f"]],
        &[("A", 0), ("B", 0), ("C", 0), ("d", 1), ("e", 1), ("f", 1)],
    );
    let before = total_crossings(&layering.layers, &g.edges);
    let ordered = order_layers(&layering, &g, None);
    assert!(total_crossings(&ordered.layers, &g.edges) <= before);
}

#[test]
fn results_are_deterministic_on_repeated_runs() {
    let nodes = ["U1", "U2", "U3", "L1", "L2", "L3"];
    let g = mk(&nodes, &[("U1", "L2"), ("U2", "L1"), ("U3", "L3")]);
    let layering = layering(
        vec![vec!["U1", "U2", "U3"], vec!["L3", "L2", "L1"]],
        &[
            ("U1", 0),
            ("U2", 0),
            ("U3", 0),
            ("L1", 1),
            ("L2", 1),
            ("L3", 1),
        ],
    );
    assert_eq!(
        order_layers(&layering, &g, None).layers,
        order_layers(&layering, &g, None).layers
    );
}

#[test]
fn transpose_considers_crossings_against_the_next_layer() {
    let nodes = ["I", "J", "K", "L", "E"];
    let g = mk(&nodes, &[("I", "J"), ("I", "K"), ("J", "E"), ("K", "L")]);
    let layering = layering(
        vec![vec!["I"], vec!["J", "K"], vec!["L", "E"]],
        &[("I", 0), ("J", 1), ("K", 1), ("L", 2), ("E", 2)],
    );
    let before = total_crossings(&layering.layers, &g.edges);
    let ordered = order_layers(&layering, &g, None);
    let after = total_crossings(&ordered.layers, &g.edges);
    assert_eq!(ordered.layers[1], vec!["K", "J"]);
    assert!(after < before);
}
