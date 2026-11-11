//! Port of `dagre/lib/order/cross-count.ts` (MIT, Copyright (c) 2012-2014 Chris
//! Pettitt).
//!
//! Weighted two-layer crossing count, after Barth, et al., "Bilayer Cross Counting."

use super::super::LayeredGraph;
use super::super::util::LayerMatrix;

/// The weighted number of edge crossings in `layering`.
pub fn cross_count(graph: &LayeredGraph, layering: &LayerMatrix) -> f64 {
    let mut cross_count = 0.0;
    for i in 1..layering.len() {
        cross_count += two_layer_cross_count(graph, &layering[i - 1], &layering[i]);
    }
    cross_count
}

fn two_layer_cross_count(
    graph: &LayeredGraph,
    north_layer: &[String],
    south_layer: &[String],
) -> f64 {
    let mut south_entries: Vec<(usize, f64)> = Vec::new();
    for v in north_layer {
        let edges = graph.out_edges(v, None).unwrap_or_default();
        let mut entries: Vec<(usize, f64)> = edges
            .iter()
            .filter_map(|edge| {
                let position = south_layer.iter().position(|w| w == &edge.w)?;
                Some((
                    position,
                    graph.edge_object(edge).map_or(0.0, |label| label.weight()),
                ))
            })
            .collect();
        entries.sort_by_key(|(position, _)| *position);
        south_entries.extend(entries);
    }

    let mut first_index = 1usize;
    while first_index < south_layer.len() {
        first_index <<= 1;
    }
    let tree_size = 2 * first_index - 1;
    let first_index = first_index - 1;
    let mut tree = vec![0.0; tree_size];

    let mut cross_count = 0.0;
    for (position, weight) in south_entries {
        let mut index = position + first_index;
        tree[index] += weight;
        let mut weight_sum = 0.0;
        while index > 0 {
            if index % 2 == 1 {
                weight_sum += tree[index + 1];
            }
            index = (index - 1) >> 1;
            tree[index] += weight;
        }
        cross_count += weight * weight_sum;
    }

    cross_count
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::{graph, weighted};
    use super::super::super::util::LayerMatrix;

    fn layering(layers: &[&[&str]]) -> LayerMatrix {
        layers
            .iter()
            .map(|layer| layer.iter().map(|v| v.to_string()).collect())
            .collect()
    }

    #[test]
    fn returns_zero_for_an_empty_layering() {
        let graph = graph();
        assert_eq!(super::cross_count(&graph, &Vec::<Vec<String>>::new()), 0.0);
    }

    #[test]
    fn returns_zero_without_crossings() {
        let mut graph = graph();
        graph.set_edge("a1", "b1");
        graph.set_edge("a2", "b2");
        assert_eq!(
            super::cross_count(&graph, &layering(&[&["a1", "a2"], &["b1", "b2"]])),
            0.0
        );
    }

    #[test]
    fn counts_one_crossing() {
        let mut graph = graph();
        graph.set_edge("a1", "b1");
        graph.set_edge("a2", "b2");
        assert_eq!(
            super::cross_count(&graph, &layering(&[&["a1", "a2"], &["b2", "b1"]])),
            1.0
        );
    }

    #[test]
    fn counts_weighted_crossings() {
        let mut graph = graph();
        graph.set_edge_with("a1", "b1", weighted(2.0, 1.0));
        graph.set_edge_with("a2", "b2", weighted(3.0, 1.0));
        assert_eq!(
            super::cross_count(&graph, &layering(&[&["a1", "a2"], &["b2", "b1"]])),
            6.0
        );
    }

    #[test]
    fn calculates_crossings_across_layers() {
        let mut graph = graph();
        graph.set_path(&["a1", "b1", "c1"]);
        graph.set_path(&["a2", "b2", "c2"]);
        assert_eq!(
            super::cross_count(
                &graph,
                &layering(&[&["a1", "a2"], &["b2", "b1"], &["c1", "c2"]])
            ),
            2.0
        );
    }

    #[test]
    fn works_for_graph_one() {
        let mut graph = graph();
        graph.set_path(&["a", "b", "c"]);
        graph.set_path(&["d", "e", "c"]);
        graph.set_path(&["a", "f", "i"]);
        graph.set_edge("a", "e");
        assert_eq!(
            super::cross_count(
                &graph,
                &layering(&[&["a", "d"], &["b", "e", "f"], &["c", "i"]])
            ),
            1.0
        );
        assert_eq!(
            super::cross_count(
                &graph,
                &layering(&[&["d", "a"], &["e", "b", "f"], &["c", "i"]])
            ),
            0.0
        );
    }
}
