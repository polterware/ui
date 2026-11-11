//! Port of `dagre/lib/normalize.ts` (MIT, Copyright (c) 2012-2014 Chris Pettitt).
//!
//! Breaks every edge longer than one rank into one-rank segments with dummy nodes,
//! and restores the original edges after positioning. Preconditions are upstream's:
//! the graph is a DAG and every node has a rank.

use super::super::graphlib::Edge;
use super::util::add_dummy_node;
use super::{EdgeLabel, LayeredGraph, NodeLabel, Point};

/// Splits long edges and records the first dummy of each chain.
pub fn run(graph: &mut LayeredGraph) {
    if let Some(label) = graph.graph_mut() {
        label.dummy_chains = Some(Vec::new());
    }
    for edge in graph.edges() {
        normalize_edge(graph, &edge);
    }
}

fn normalize_edge(graph: &mut LayeredGraph, e: &Edge) {
    let v_rank = graph.node(&e.v).and_then(|node| node.rank).unwrap_or(0.0);
    let w_rank = graph.node(&e.w).and_then(|node| node.rank).unwrap_or(0.0);
    let name = e.name.clone();
    let mut label = graph.edge_object(e).cloned().unwrap_or_default();
    let label_rank = label.label_rank;

    if w_rank == v_rank + 1.0 {
        return;
    }

    graph.remove_edge_object(e);

    let mut v = e.v.clone();
    let mut v_rank = v_rank + 1.0;
    let mut i = 0;
    while v_rank < w_rank {
        label.points = Some(Vec::new());
        let attrs = NodeLabel {
            width: 0.0,
            height: 0.0,
            edge_label: Some(label.clone()),
            edge_obj: Some(e.clone()),
            rank: Some(v_rank),
            ..NodeLabel::default()
        };
        let dummy = add_dummy_node(graph, "edge", attrs, "_d");
        if Some(v_rank) == label_rank
            && let Some(node) = graph.node_mut(&dummy)
        {
            node.width = label.width();
            node.height = label.height();
            node.dummy = Some("edge-label".to_string());
            node.labelpos = label.labelpos.clone();
        }
        graph
            .set_edge_full(
                &v,
                &dummy,
                Some(EdgeLabel {
                    weight: Some(label.weight()),
                    ..EdgeLabel::default()
                }),
                name.as_deref(),
            )
            .expect("a segment stays inside the multigraph");
        if i == 0
            && let Some(graph_label) = graph.graph_mut()
            && let Some(chains) = &mut graph_label.dummy_chains
        {
            chains.push(dummy.clone());
        }
        v = dummy;
        i += 1;
        v_rank += 1.0;
    }

    graph
        .set_edge_full(
            &v,
            &e.w,
            Some(EdgeLabel {
                weight: Some(label.weight()),
                ..EdgeLabel::default()
            }),
            name.as_deref(),
        )
        .expect("a segment stays inside the multigraph");
}

/// Restores the edges [`run`] split, collecting their dummy coordinates as points.
pub fn undo(graph: &mut LayeredGraph) {
    let chains = graph
        .graph()
        .and_then(|label| label.dummy_chains.clone())
        .unwrap_or_default();

    for start in chains {
        let Some(node) = graph.node(&start).cloned() else {
            continue;
        };
        let (Some(mut label), Some(edge_obj)) = (node.edge_label.clone(), node.edge_obj.clone())
        else {
            continue;
        };

        let mut points = Vec::new();
        let mut v = start.clone();
        loop {
            let Some(node) = graph.node(&v).cloned() else {
                break;
            };
            if node.dummy.is_none() {
                break;
            }
            points.push(Point::new(node.x.unwrap_or(0.0), node.y.unwrap_or(0.0)));
            if node.dummy.as_deref() == Some("edge-label") {
                label.x = node.x;
                label.y = node.y;
                label.width = Some(node.width);
                label.height = Some(node.height);
            }
            let Some(next) = graph.successors(&v).and_then(|next| next.first().cloned()) else {
                break;
            };
            v = next;
        }
        label.points = Some(points);

        graph
            .set_edge_object_label(&edge_obj, Some(label))
            .expect("a restored edge stays inside the multigraph");

        let mut current = start;
        while graph
            .node(&current)
            .is_some_and(|node| node.dummy.is_some())
        {
            let next = graph
                .successors(&current)
                .and_then(|next| next.first().cloned());
            graph.remove_node(&current);
            match next {
                Some(next) => current = next,
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{close, layering, ranked};
    use super::super::{EdgeLabel, LayeredGraph, Point};

    fn graph() -> LayeredGraph {
        layering()
    }

    #[test]
    fn run_does_not_change_a_short_edge() {
        let mut graph = graph();
        graph.set_node_with("a", ranked(0.0));
        graph.set_node_with("b", ranked(1.0));
        graph.set_edge("a", "b");
        super::run(&mut graph);
        assert_eq!(
            super::super::test_support::edge_ids(&graph),
            vec![("a".to_string(), "b".to_string(), None)]
        );
        assert_eq!(graph.node("a").unwrap().rank, Some(0.0));
        assert_eq!(graph.node("b").unwrap().rank, Some(1.0));
    }

    #[test]
    fn run_splits_a_two_layer_edge_into_two_segments() {
        let mut graph = graph();
        graph.set_node_with("a", ranked(0.0));
        graph.set_node_with("b", ranked(2.0));
        graph.set_edge("a", "b");
        super::run(&mut graph);
        let successors = graph.successors("a").unwrap();
        assert_eq!(successors.len(), 1);
        let dummy = &successors[0];
        assert_eq!(graph.node(dummy).unwrap().dummy.as_deref(), Some("edge"));
        assert_eq!(graph.node(dummy).unwrap().rank, Some(1.0));
        assert_eq!(graph.successors(dummy).unwrap(), vec!["b".to_string()]);
        assert_eq!(graph.node("a").unwrap().rank, Some(0.0));
        assert_eq!(graph.node("b").unwrap().rank, Some(2.0));
        let chains = graph.graph().unwrap().dummy_chains.clone().unwrap();
        assert_eq!(chains.len(), 1);
        assert_eq!(&chains[0], dummy);
    }

    #[test]
    fn run_assigns_zero_size_to_dummy_nodes_by_default() {
        let mut graph = graph();
        graph.set_node_with("a", ranked(0.0));
        graph.set_node_with("b", ranked(2.0));
        graph.set_edge_with(
            "a",
            "b",
            EdgeLabel {
                width: Some(10.0),
                height: Some(10.0),
                ..EdgeLabel::default()
            },
        );
        super::run(&mut graph);
        let dummy = &graph.successors("a").unwrap()[0];
        assert_eq!(graph.node(dummy).unwrap().width, 0.0);
        assert_eq!(graph.node(dummy).unwrap().height, 0.0);
    }

    #[test]
    fn run_assigns_the_label_size_at_label_rank() {
        let mut graph = graph();
        graph.set_node_with("a", ranked(0.0));
        graph.set_node_with("b", ranked(4.0));
        graph.set_edge_with(
            "a",
            "b",
            EdgeLabel {
                width: Some(20.0),
                height: Some(10.0),
                label_rank: Some(2.0),
                ..EdgeLabel::default()
            },
        );
        super::run(&mut graph);
        let first = &graph.successors("a").unwrap()[0];
        let label = &graph.successors(first).unwrap()[0];
        assert_eq!(graph.node(label).unwrap().width, 20.0);
        assert_eq!(graph.node(label).unwrap().height, 10.0);
        assert_eq!(
            graph.node(label).unwrap().dummy.as_deref(),
            Some("edge-label")
        );
    }

    #[test]
    fn run_preserves_the_weight_for_the_edge() {
        let mut graph = graph();
        graph.set_node_with("a", ranked(0.0));
        graph.set_node_with("b", ranked(2.0));
        graph.set_edge_with(
            "a",
            "b",
            EdgeLabel {
                weight: Some(2.0),
                ..EdgeLabel::default()
            },
        );
        super::run(&mut graph);
        let dummy = &graph.successors("a").unwrap()[0];
        assert_eq!(graph.edge("a", dummy).unwrap().weight(), 2.0);
    }

    #[test]
    fn undo_reverses_the_run_operation() {
        let mut graph = graph();
        graph.set_node_with("a", ranked(0.0));
        graph.set_node_with("b", ranked(2.0));
        graph.set_edge("a", "b");
        super::run(&mut graph);
        super::undo(&mut graph);
        assert_eq!(
            super::super::test_support::edge_ids(&graph),
            vec![("a".to_string(), "b".to_string(), None)]
        );
        assert_eq!(graph.node("a").unwrap().rank, Some(0.0));
        assert_eq!(graph.node("b").unwrap().rank, Some(2.0));
    }

    #[test]
    fn undo_restores_previous_edge_labels() {
        let mut graph = graph();
        graph.set_node_with("a", ranked(0.0));
        graph.set_node_with("b", ranked(2.0));
        graph.set_edge_with(
            "a",
            "b",
            EdgeLabel {
                width: Some(7.0),
                labeloffset: Some(3.0),
                ..EdgeLabel::default()
            },
        );
        super::run(&mut graph);
        super::undo(&mut graph);
        let label = graph.edge("a", "b").unwrap();
        assert_eq!(label.width(), 7.0);
        assert_eq!(label.labeloffset(), 3.0);
    }

    #[test]
    fn undo_collects_assigned_coordinates_into_points() {
        let mut graph = graph();
        graph.set_node_with("a", ranked(0.0));
        graph.set_node_with("b", ranked(2.0));
        graph.set_edge("a", "b");
        super::run(&mut graph);
        let dummy = graph.neighbors("a").unwrap()[0].clone();
        let label = graph.node_mut(&dummy).unwrap();
        label.x = Some(5.0);
        label.y = Some(10.0);
        super::undo(&mut graph);
        assert_eq!(
            graph.edge("a", "b").unwrap().points,
            Some(vec![Point::new(5.0, 10.0)])
        );
    }

    #[test]
    fn undo_merges_assigned_coordinates_into_points() {
        let mut graph = graph();
        graph.set_node_with("a", ranked(0.0));
        graph.set_node_with("b", ranked(4.0));
        graph.set_edge("a", "b");
        super::run(&mut graph);
        let first = graph.neighbors("a").unwrap()[0].clone();
        let first_label = graph.node_mut(&first).unwrap();
        first_label.x = Some(5.0);
        first_label.y = Some(10.0);
        let middle = graph.successors(&first).unwrap()[0].clone();
        let middle_label = graph.node_mut(&middle).unwrap();
        middle_label.x = Some(20.0);
        middle_label.y = Some(25.0);
        let last = graph.neighbors("b").unwrap()[0].clone();
        let last_label = graph.node_mut(&last).unwrap();
        last_label.x = Some(100.0);
        last_label.y = Some(200.0);
        super::undo(&mut graph);
        assert_eq!(
            graph.edge("a", "b").unwrap().points,
            Some(vec![
                Point::new(5.0, 10.0),
                Point::new(20.0, 25.0),
                Point::new(100.0, 200.0),
            ])
        );
    }

    #[test]
    fn undo_sets_coords_and_dims_for_the_label() {
        let mut graph = graph();
        graph.set_node_with("a", ranked(0.0));
        graph.set_node_with("b", ranked(2.0));
        graph.set_edge_with(
            "a",
            "b",
            EdgeLabel {
                width: Some(10.0),
                height: Some(20.0),
                label_rank: Some(1.0),
                ..EdgeLabel::default()
            },
        );
        super::run(&mut graph);
        let label_node = graph.successors("a").unwrap()[0].clone();
        let label = graph.node_mut(&label_node).unwrap();
        label.x = Some(50.0);
        label.y = Some(60.0);
        label.width = 20.0;
        label.height = 10.0;
        super::undo(&mut graph);
        let edge = graph.edge("a", "b").unwrap();
        assert_eq!(edge.x, Some(50.0));
        assert_eq!(edge.y, Some(60.0));
        assert_eq!(edge.width, Some(20.0));
        assert_eq!(edge.height, Some(10.0));
    }

    #[test]
    fn undo_sets_coords_and_dims_for_a_long_edge_label() {
        let mut graph = graph();
        graph.set_node_with("a", ranked(0.0));
        graph.set_node_with("b", ranked(4.0));
        graph.set_edge_with(
            "a",
            "b",
            EdgeLabel {
                width: Some(10.0),
                height: Some(20.0),
                label_rank: Some(2.0),
                ..EdgeLabel::default()
            },
        );
        super::run(&mut graph);
        let first = graph.successors("a").unwrap()[0].clone();
        let label_node = graph.successors(&first).unwrap()[0].clone();
        let label = graph.node_mut(&label_node).unwrap();
        label.x = Some(50.0);
        label.y = Some(60.0);
        label.width = 20.0;
        label.height = 10.0;
        super::undo(&mut graph);
        let edge = graph.edge("a", "b").unwrap();
        close(edge.x.unwrap(), 50.0);
        close(edge.y.unwrap(), 60.0);
        assert_eq!(edge.width, Some(20.0));
        assert_eq!(edge.height, Some(10.0));
    }

    #[test]
    fn undo_restores_multi_edges() {
        let mut graph = graph();
        graph.set_node_with("a", ranked(0.0));
        graph.set_node_with("b", ranked(2.0));
        graph.set_edge_full("a", "b", None, Some("bar")).unwrap();
        graph.set_edge_full("a", "b", None, Some("foo")).unwrap();
        super::run(&mut graph);
        let mut out_edges = graph.out_edges("a", None).unwrap();
        out_edges.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(out_edges.len(), 2);
        let bar = out_edges[0].w.clone();
        let foo = out_edges[1].w.clone();
        let bar_label = graph.node_mut(&bar).unwrap();
        bar_label.x = Some(5.0);
        bar_label.y = Some(10.0);
        let foo_label = graph.node_mut(&foo).unwrap();
        foo_label.x = Some(15.0);
        foo_label.y = Some(20.0);
        super::undo(&mut graph);
        assert!(!graph.has_edge("a", "b"));
        assert_eq!(
            graph.edge_label("a", "b", Some("bar")).unwrap().points,
            Some(vec![Point::new(5.0, 10.0)])
        );
        assert_eq!(
            graph.edge_label("a", "b", Some("foo")).unwrap().points,
            Some(vec![Point::new(15.0, 20.0)])
        );
    }
}
