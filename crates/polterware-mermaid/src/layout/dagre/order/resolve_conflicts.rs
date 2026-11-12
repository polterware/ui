//! Port of `dagre-d3-es/src/dagre/order/resolve-conflicts.js` (MIT, Copyright (c) 2012-2014
//! Chris Pettitt).
//!
//! Coalesces the barycenter entries that violate an ordering constraint into one
//! super-node, following Forster's heuristic.

use std::collections::HashMap;

use super::super::super::graphlib::Graph;
use super::barycenter::BarycenterEntry;

/// A resolved entry: either one node or a constraint-respecting super-node.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedEntry {
    /// The nodes, in their resolved order.
    pub vs: Vec<String>,
    /// The lowest original index of the entries merged here.
    pub i: usize,
    /// The weighted mean barycenter.
    pub barycenter: Option<f64>,
    /// The sum of the weights.
    pub weight: Option<f64>,
}

#[derive(Clone, Debug, Default)]
struct MappedEntry {
    indegree: usize,
    incoming: Vec<usize>,
    outgoing: Vec<usize>,
    vs: Vec<String>,
    i: usize,
    barycenter: Option<f64>,
    weight: Option<f64>,
    merged: bool,
}

/// Resolves the conflicts between `entries` and `constraint_graph`.
pub fn resolve_conflicts(
    entries: &[BarycenterEntry],
    constraint_graph: &Graph<(), (), ()>,
) -> Vec<ResolvedEntry> {
    let mut mapped: Vec<MappedEntry> = entries
        .iter()
        .enumerate()
        .map(|(i, entry)| MappedEntry {
            indegree: 0,
            incoming: Vec::new(),
            outgoing: Vec::new(),
            vs: vec![entry.v.clone()],
            i,
            barycenter: entry.barycenter,
            weight: entry.weight,
            merged: false,
        })
        .collect();
    let indices: HashMap<String, usize> = entries
        .iter()
        .enumerate()
        .map(|(i, entry)| (entry.v.clone(), i))
        .collect();

    for edge in constraint_graph.edges() {
        let (Some(&v), Some(&w)) = (indices.get(&edge.v), indices.get(&edge.w)) else {
            continue;
        };
        mapped[w].indegree += 1;
        mapped[v].outgoing.push(w);
    }

    let mut source_set: Vec<usize> = mapped
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.indegree == 0)
        .map(|(i, _)| i)
        .collect();

    let mut resolved: Vec<usize> = Vec::new();
    while let Some(entry) = source_set.pop() {
        resolved.push(entry);

        let incoming = mapped[entry].incoming.clone();
        for u in incoming.into_iter().rev() {
            handle_in(&mut mapped, entry, u);
        }
        let outgoing = mapped[entry].outgoing.clone();
        for w in outgoing {
            mapped[w].incoming.push(entry);
            mapped[w].indegree -= 1;
            if mapped[w].indegree == 0 {
                source_set.push(w);
            }
        }
    }

    resolved
        .into_iter()
        .filter(|entry| !mapped[*entry].merged)
        .map(|entry| ResolvedEntry {
            vs: mapped[entry].vs.clone(),
            i: mapped[entry].i,
            barycenter: mapped[entry].barycenter,
            weight: mapped[entry].weight,
        })
        .collect()
}

fn handle_in(mapped: &mut [MappedEntry], v: usize, u: usize) {
    if mapped[u].merged {
        return;
    }
    if mapped[u].barycenter.is_none()
        || mapped[v].barycenter.is_none()
        || mapped[u].barycenter >= mapped[v].barycenter
    {
        merge_entries(mapped, v, u);
    }
}

fn merge_entries(mapped: &mut [MappedEntry], target: usize, source: usize) {
    let mut sum = 0.0;
    let mut weight = 0.0;

    if let Some(target_weight) = mapped[target].weight.filter(|weight| *weight != 0.0) {
        sum += mapped[target].barycenter.unwrap_or(f64::NAN) * target_weight;
        weight += target_weight;
    }
    if let Some(source_weight) = mapped[source].weight.filter(|weight| *weight != 0.0) {
        sum += mapped[source].barycenter.unwrap_or(f64::NAN) * source_weight;
        weight += source_weight;
    }

    let mut vs = mapped[source].vs.clone();
    vs.extend(mapped[target].vs.clone());
    mapped[target].vs = vs;
    mapped[target].barycenter = Some(sum / weight);
    mapped[target].weight = Some(weight);
    mapped[target].i = mapped[source].i.min(mapped[target].i);
    mapped[source].merged = true;
}

#[cfg(test)]
mod tests {
    use super::super::super::super::graphlib::Graph;
    use super::super::barycenter::BarycenterEntry;
    use super::ResolvedEntry;

    fn entry(v: &str, barycenter: Option<f64>, weight: Option<f64>) -> BarycenterEntry {
        BarycenterEntry {
            v: v.to_string(),
            barycenter,
            weight,
        }
    }

    fn resolved(
        vs: &[&str],
        i: usize,
        barycenter: Option<f64>,
        weight: Option<f64>,
    ) -> ResolvedEntry {
        ResolvedEntry {
            vs: vs.iter().map(|v| v.to_string()).collect(),
            i,
            barycenter,
            weight,
        }
    }

    #[test]
    fn returns_nodes_unchanged_without_constraints() {
        let mut graph: Graph<(), (), ()> = Graph::new();
        let input = vec![
            entry("a", Some(2.0), Some(3.0)),
            entry("b", Some(1.0), Some(2.0)),
        ];
        let mut results = super::resolve_conflicts(&input, &graph);
        results.sort_by(|a, b| a.vs[0].cmp(&b.vs[0]));
        assert_eq!(
            results,
            vec![
                resolved(&["a"], 0, Some(2.0), Some(3.0)),
                resolved(&["b"], 1, Some(1.0), Some(2.0)),
            ]
        );
        graph.set_edge("b", "a");
    }

    #[test]
    fn returns_nodes_unchanged_without_conflicts() {
        let mut graph: Graph<(), (), ()> = Graph::new();
        graph.set_edge("b", "a");
        let input = vec![
            entry("a", Some(2.0), Some(3.0)),
            entry("b", Some(1.0), Some(2.0)),
        ];
        let mut results = super::resolve_conflicts(&input, &graph);
        results.sort_by(|a, b| a.vs[0].cmp(&b.vs[0]));
        assert_eq!(
            results,
            vec![
                resolved(&["a"], 0, Some(2.0), Some(3.0)),
                resolved(&["b"], 1, Some(1.0), Some(2.0)),
            ]
        );
    }

    #[test]
    fn coalesces_nodes_on_a_conflict() {
        let mut graph: Graph<(), (), ()> = Graph::new();
        graph.set_edge("a", "b");
        let input = vec![
            entry("a", Some(2.0), Some(3.0)),
            entry("b", Some(1.0), Some(2.0)),
        ];
        assert_eq!(
            super::resolve_conflicts(&input, &graph),
            vec![resolved(
                &["a", "b"],
                0,
                Some((3.0 * 2.0 + 2.0 * 1.0) / 5.0),
                Some(5.0)
            )]
        );
    }

    #[test]
    fn coalesces_nodes_on_a_chain_of_conflicts() {
        let mut graph: Graph<(), (), ()> = Graph::new();
        graph.set_path(&["a", "b", "c", "d"]);
        let input = vec![
            entry("a", Some(4.0), Some(1.0)),
            entry("b", Some(3.0), Some(1.0)),
            entry("c", Some(2.0), Some(1.0)),
            entry("d", Some(1.0), Some(1.0)),
        ];
        assert_eq!(
            super::resolve_conflicts(&input, &graph),
            vec![resolved(&["a", "b", "c", "d"], 0, Some(2.5), Some(4.0))]
        );
    }

    #[test]
    fn handles_multiple_constraints_for_the_same_target_1() {
        let mut graph: Graph<(), (), ()> = Graph::new();
        graph.set_edge("a", "c");
        graph.set_edge("b", "c");
        let input = vec![
            entry("a", Some(4.0), Some(1.0)),
            entry("b", Some(3.0), Some(1.0)),
            entry("c", Some(2.0), Some(1.0)),
        ];
        let results = super::resolve_conflicts(&input, &graph);
        assert_eq!(results.len(), 1);
        let result = &results[0];
        let a = result.vs.iter().position(|v| v == "a").unwrap();
        let b = result.vs.iter().position(|v| v == "b").unwrap();
        let c = result.vs.iter().position(|v| v == "c").unwrap();
        assert!(c > a);
        assert!(c > b);
        assert_eq!(result.i, 0);
        assert_eq!(result.barycenter, Some(3.0));
        assert_eq!(result.weight, Some(3.0));
    }

    #[test]
    fn handles_multiple_constraints_for_the_same_target_2() {
        let mut graph: Graph<(), (), ()> = Graph::new();
        graph.set_edge("a", "c");
        graph.set_edge("a", "d");
        graph.set_edge("b", "c");
        graph.set_edge("c", "d");
        let input = vec![
            entry("a", Some(4.0), Some(1.0)),
            entry("b", Some(3.0), Some(1.0)),
            entry("c", Some(2.0), Some(1.0)),
            entry("d", Some(1.0), Some(1.0)),
        ];
        let results = super::resolve_conflicts(&input, &graph);
        assert_eq!(results.len(), 1);
        let result = &results[0];
        let a = result.vs.iter().position(|v| v == "a").unwrap();
        let b = result.vs.iter().position(|v| v == "b").unwrap();
        let c = result.vs.iter().position(|v| v == "c").unwrap();
        let d = result.vs.iter().position(|v| v == "d").unwrap();
        assert!(c > a && c > b && d > c);
        assert_eq!(result.i, 0);
        assert_eq!(result.barycenter, Some(2.5));
        assert_eq!(result.weight, Some(4.0));
    }

    #[test]
    fn does_nothing_to_a_node_without_barycenter_or_constraint() {
        let graph: Graph<(), (), ()> = Graph::new();
        let input = vec![entry("a", None, None), entry("b", Some(1.0), Some(2.0))];
        let mut results = super::resolve_conflicts(&input, &graph);
        results.sort_by(|a, b| a.vs[0].cmp(&b.vs[0]));
        assert_eq!(
            results,
            vec![
                resolved(&["a"], 0, None, None),
                resolved(&["b"], 1, Some(1.0), Some(2.0))
            ]
        );
    }

    #[test]
    fn treats_a_node_without_barycenter_as_violating_constraints_1() {
        let mut graph: Graph<(), (), ()> = Graph::new();
        graph.set_edge("a", "b");
        let input = vec![entry("a", None, None), entry("b", Some(1.0), Some(2.0))];
        assert_eq!(
            super::resolve_conflicts(&input, &graph),
            vec![resolved(&["a", "b"], 0, Some(1.0), Some(2.0))]
        );
    }

    #[test]
    fn treats_a_node_without_barycenter_as_violating_constraints_2() {
        let mut graph: Graph<(), (), ()> = Graph::new();
        graph.set_edge("b", "a");
        let input = vec![entry("a", None, None), entry("b", Some(1.0), Some(2.0))];
        assert_eq!(
            super::resolve_conflicts(&input, &graph),
            vec![resolved(&["b", "a"], 0, Some(1.0), Some(2.0))]
        );
    }

    #[test]
    fn ignores_edges_unrelated_to_entries() {
        let mut graph: Graph<(), (), ()> = Graph::new();
        graph.set_edge("c", "d");
        let input = vec![
            entry("a", Some(2.0), Some(3.0)),
            entry("b", Some(1.0), Some(2.0)),
        ];
        let mut results = super::resolve_conflicts(&input, &graph);
        results.sort_by(|a, b| a.vs[0].cmp(&b.vs[0]));
        assert_eq!(
            results,
            vec![
                resolved(&["a"], 0, Some(2.0), Some(3.0)),
                resolved(&["b"], 1, Some(1.0), Some(2.0)),
            ]
        );
    }
}
