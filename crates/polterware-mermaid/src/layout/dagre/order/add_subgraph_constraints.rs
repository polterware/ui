//! Port of `dagre-d3-es/src/dagre/order/add-subgraph-constraints.js` (MIT, Copyright (c)
//! 2012-2014 Chris Pettitt).
//!
//! When two adjacent nodes in a layer belong to different subgraphs, the subgraphs
//! must keep that order, so this adds a constraint between their nearest differing
//! ancestors.

use super::super::super::graphlib::Graph;
use super::super::LayeredGraph;

/// Adds subgraph order constraints for the layer ordered as `vs`.
pub fn add_subgraph_constraints(
    graph: &LayeredGraph,
    constraint_graph: &mut Graph<(), (), ()>,
    vs: &[String],
) {
    let mut prev: Vec<(String, String)> = Vec::new();
    let mut root_prev: Option<String> = None;

    for v in vs {
        let mut child = graph.parent(v).map(str::to_string);
        while let Some(current) = child.clone() {
            let parent = graph.parent(&current).map(str::to_string);
            let prev_child = match &parent {
                Some(parent) => {
                    let previous = prev
                        .iter()
                        .find(|(id, _)| id == parent)
                        .map(|(_, child)| child.clone());
                    match prev.iter_mut().find(|(id, _)| id == parent) {
                        Some(entry) => entry.1 = current.clone(),
                        None => prev.push((parent.clone(), current.clone())),
                    }
                    previous
                }
                None => {
                    let previous = root_prev.clone();
                    root_prev = Some(current.clone());
                    previous
                }
            };

            if let Some(prev_child) = prev_child
                && prev_child != current
            {
                constraint_graph.set_edge(&prev_child, &current);
                break;
            }
            child = parent;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::super::graphlib::Graph;
    use super::super::super::test_support::compound;

    #[test]
    fn does_not_change_the_constraint_graph_for_flat_nodes() {
        let mut graph = compound();
        let mut constraints: Graph<(), (), ()> = Graph::new();
        let vs: Vec<String> = ["a", "b", "c", "d"].iter().map(|v| v.to_string()).collect();
        for v in &vs {
            graph.set_node(v.clone());
        }
        super::add_subgraph_constraints(&graph, &mut constraints, &vs);
        assert_eq!(constraints.node_count(), 0);
        assert_eq!(constraints.edge_count(), 0);
    }

    #[test]
    fn does_not_constrain_contiguous_subgraph_nodes() {
        let mut graph = compound();
        let mut constraints: Graph<(), (), ()> = Graph::new();
        let vs: Vec<String> = ["a", "b", "c"].iter().map(|v| v.to_string()).collect();
        for v in &vs {
            graph.set_parent(v, Some("sg")).unwrap();
        }
        super::add_subgraph_constraints(&graph, &mut constraints, &vs);
        assert_eq!(constraints.node_count(), 0);
        assert_eq!(constraints.edge_count(), 0);
    }

    #[test]
    fn adds_a_constraint_for_different_adjacent_parents() {
        let mut graph = compound();
        let mut constraints: Graph<(), (), ()> = Graph::new();
        graph.set_parent("a", Some("sg1")).unwrap();
        graph.set_parent("b", Some("sg2")).unwrap();
        super::add_subgraph_constraints(
            &graph,
            &mut constraints,
            &["a".to_string(), "b".to_string()],
        );
        assert_eq!(
            constraints.edges(),
            vec![super::super::super::super::graphlib::Edge::new(
                "sg1", "sg2"
            )]
        );
    }

    #[test]
    fn works_for_multiple_levels() {
        let mut graph = compound();
        let mut constraints: Graph<(), (), ()> = Graph::new();
        let vs: Vec<String> = ["a", "b", "c", "d", "e", "f", "g", "h"]
            .iter()
            .map(|v| v.to_string())
            .collect();
        for v in &vs {
            graph.set_node(v.clone());
        }
        graph.set_parent("b", Some("sg2")).unwrap();
        graph.set_parent("sg2", Some("sg1")).unwrap();
        graph.set_parent("c", Some("sg1")).unwrap();
        graph.set_parent("d", Some("sg3")).unwrap();
        graph.set_parent("sg3", Some("sg1")).unwrap();
        graph.set_parent("f", Some("sg4")).unwrap();
        graph.set_parent("g", Some("sg5")).unwrap();
        graph.set_parent("sg5", Some("sg4")).unwrap();
        super::add_subgraph_constraints(&graph, &mut constraints, &vs);
        let mut edges: Vec<(String, String)> = constraints
            .edges()
            .into_iter()
            .map(|edge| (edge.v, edge.w))
            .collect();
        edges.sort();
        assert_eq!(
            edges,
            vec![
                ("sg1".to_string(), "sg4".to_string()),
                ("sg2".to_string(), "sg3".to_string()),
            ]
        );
    }
}
