//! Port of `dagre-d3-es/src/dagre/greedy-fas.js` (MIT, Copyright (c) 2012-2014 Chris Pettitt).
//!
//! Eades, Lin, and Smyth's greedy feedback arc set, adjusted for weighted edges.
//! Buckets hold entry handles in upstream's order: sources are at the last bucket,
//! sinks at the first, and entries are enqueued at the front and dequeued from the
//! back.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::super::graphlib::{Edge, Graph};
use super::LayeredGraph;
use super::data::list::{List, ListNode};

/// The weight of an edge, upstream's `WeightFunction`.
pub type WeightFn<'a> = dyn Fn(&Edge) -> f64 + 'a;

struct Counters {
    v: String,
    incoming: f64,
    outgoing: f64,
}

type Handle = Rc<ListNode<RefCell<Counters>>>;

/// The FAS graph, its entry handles, the buckets, and the zero index.
type FasState = (
    Graph<(), (), f64>,
    HashMap<String, Handle>,
    Vec<List<RefCell<Counters>>>,
    f64,
);

/// A feedback arc set of `graph`; with `weight_fn` absent every edge weighs 1.
pub fn greedy_fas(graph: &LayeredGraph, weight_fn: Option<&WeightFn<'_>>) -> Vec<Edge> {
    let weight_fn = weight_fn.unwrap_or(&default_weight);
    if graph.node_count() <= 1 {
        return Vec::new();
    }
    let (mut fas_graph, handles, buckets, zero_idx) = build_state(graph, weight_fn);
    let results = do_greedy_fas(&mut fas_graph, &handles, &buckets, zero_idx);
    results
        .into_iter()
        .flat_map(|edge| graph.out_edges(&edge.v, Some(&edge.w)).unwrap_or_default())
        .collect()
}

fn default_weight(_: &Edge) -> f64 {
    1.0
}

fn do_greedy_fas(
    graph: &mut Graph<(), (), f64>,
    handles: &HashMap<String, Handle>,
    buckets: &[List<RefCell<Counters>>],
    zero_idx: f64,
) -> Vec<Edge> {
    let mut results = Vec::new();
    let sources = buckets.len() - 1;
    let sinks = 0;

    while graph.node_count() > 0 {
        while let Some(entry) = buckets[sinks].dequeue() {
            remove_node(graph, handles, buckets, zero_idx, entry, false);
        }
        while let Some(entry) = buckets[sources].dequeue() {
            remove_node(graph, handles, buckets, zero_idx, entry, false);
        }
        if graph.node_count() > 0 {
            let mut removed = false;
            for i in (1..buckets.len() - 1).rev() {
                if let Some(entry) = buckets[i].dequeue() {
                    if let Some(collected) =
                        remove_node(graph, handles, buckets, zero_idx, entry, true)
                    {
                        results.extend(collected);
                    }
                    removed = true;
                    break;
                }
            }
            if !removed {
                // Upstream would spin here; only fractional or negative weights can
                // produce it, and they cannot come from a layout graph.
                break;
            }
        }
    }

    results
}

fn remove_node(
    graph: &mut Graph<(), (), f64>,
    handles: &HashMap<String, Handle>,
    buckets: &[List<RefCell<Counters>>],
    zero_idx: f64,
    entry: Handle,
    collect_predecessors: bool,
) -> Option<Vec<Edge>> {
    let v = entry.value()?.borrow().v.clone();
    let mut collected = Vec::new();

    for edge in graph.in_edges(&v, None).unwrap_or_default() {
        let weight = graph.edge_object(&edge).copied().unwrap_or(0.0);
        if collect_predecessors {
            collected.push(Edge::new(edge.v.clone(), edge.w.clone()));
        }
        if let Some(u_entry) = handles.get(&edge.v) {
            u_entry.value().unwrap().borrow_mut().outgoing -= weight;
            assign_bucket(buckets, zero_idx, u_entry);
        }
    }

    for edge in graph.out_edges(&v, None).unwrap_or_default() {
        let weight = graph.edge_object(&edge).copied().unwrap_or(0.0);
        if let Some(w_entry) = handles.get(&edge.w) {
            w_entry.value().unwrap().borrow_mut().incoming -= weight;
            assign_bucket(buckets, zero_idx, w_entry);
        }
    }

    graph.remove_node(&v);

    if collect_predecessors {
        Some(collected)
    } else {
        None
    }
}

fn build_state(graph: &LayeredGraph, weight_fn: &WeightFn<'_>) -> FasState {
    let mut fas_graph: Graph<(), (), f64> = Graph::new();
    let mut handles: HashMap<String, Handle> = HashMap::new();
    let mut max_in = 0.0f64;
    let mut max_out = 0.0f64;

    for v in graph.nodes() {
        fas_graph.set_node(&v);
        handles.insert(
            v.clone(),
            ListNode::new(RefCell::new(Counters {
                v,
                incoming: 0.0,
                outgoing: 0.0,
            })),
        );
    }

    // Aggregate the weights of multi-edges into a single edge of the FAS graph.
    for edge in graph.edges() {
        let previous = fas_graph.edge(&edge.v, &edge.w).copied().unwrap_or(0.0);
        let weight = weight_fn(&edge);
        fas_graph.set_edge_with(&edge.v, &edge.w, previous + weight);
        if let Some(v_node) = handles.get(&edge.v) {
            v_node.value().unwrap().borrow_mut().outgoing += weight;
            max_out = max_out.max(v_node.value().unwrap().borrow().outgoing);
        }
        if let Some(w_node) = handles.get(&edge.w) {
            w_node.value().unwrap().borrow_mut().incoming += weight;
            max_in = max_in.max(w_node.value().unwrap().borrow().incoming);
        }
    }

    let bucket_count = (max_out + max_in + 3.0).ceil() as usize;
    let buckets: Vec<List<RefCell<Counters>>> =
        (0..bucket_count.max(1)).map(|_| List::new()).collect();
    let zero_idx = max_in + 1.0;

    for v in fas_graph.nodes() {
        if let Some(handle) = handles.get(&v) {
            assign_bucket(&buckets, zero_idx, handle);
        }
    }

    (fas_graph, handles, buckets, zero_idx)
}

fn assign_bucket(buckets: &[List<RefCell<Counters>>], zero_idx: f64, entry: &Handle) {
    let counters = entry
        .value()
        .expect("a bucket entry is not the sentinel")
        .borrow();
    let index = if counters.outgoing == 0.0 {
        0
    } else if counters.incoming == 0.0 {
        buckets.len() - 1
    } else {
        let index = (counters.outgoing - counters.incoming + zero_idx) as isize;
        index.clamp(0, buckets.len() as isize - 1) as usize
    };
    drop(counters);
    buckets[index].enqueue(entry.clone());
}

#[cfg(test)]
mod tests {
    use super::super::super::graphlib::alg::find_cycles;
    use super::super::test_support::{graph, weighted};
    use super::super::{EdgeLabel, LayeredGraph};
    use super::*;

    fn check_fas(graph: &mut LayeredGraph, fas: Vec<Edge>) {
        let node_count = graph.node_count();
        let edge_count = graph.edge_count();
        for edge in &fas {
            graph.remove_edge_object(edge);
        }
        assert!(find_cycles(graph).is_empty());
        assert!(
            fas.len() <= edge_count / 2 - node_count / 6,
            "the feedback arc set is small"
        );
    }

    fn weight_fn(graph: &LayeredGraph) -> impl Fn(&Edge) -> f64 + '_ {
        move |edge| graph.edge_object(edge).map_or(0.0, EdgeLabel::weight)
    }

    #[test]
    fn returns_the_empty_set_for_empty_graphs() {
        let graph = graph();
        assert!(greedy_fas(&graph, None).is_empty());
    }

    #[test]
    fn returns_the_empty_set_for_single_node_graphs() {
        let mut graph = graph();
        graph.set_node("a");
        assert!(greedy_fas(&graph, None).is_empty());
    }

    #[test]
    fn returns_the_empty_set_for_acyclic_graphs() {
        let mut graph = graph();
        graph.set_edge("a", "b");
        graph.set_edge("b", "c");
        graph.set_edge("b", "d");
        graph.set_edge("a", "e");
        assert!(greedy_fas(&graph, None).is_empty());
    }

    #[test]
    fn breaks_a_simple_cycle() {
        let mut graph = graph();
        graph.set_edge("a", "b");
        graph.set_edge("b", "a");
        let fas = greedy_fas(&graph, None);
        check_fas(&mut graph, fas);
    }

    #[test]
    fn breaks_a_four_node_cycle() {
        let mut graph = graph();
        graph.set_edge("n1", "n2");
        graph.set_path(&["n2", "n3", "n4", "n5", "n2"]);
        graph.set_edge("n3", "n5");
        graph.set_edge("n4", "n2");
        graph.set_edge("n4", "n6");
        let fas = greedy_fas(&graph, None);
        check_fas(&mut graph, fas);
    }

    #[test]
    fn breaks_two_four_node_cycles() {
        let mut graph = graph();
        graph.set_edge("n1", "n2");
        graph.set_path(&["n2", "n3", "n4", "n5", "n2"]);
        graph.set_edge("n3", "n5");
        graph.set_edge("n4", "n2");
        graph.set_edge("n4", "n6");
        graph.set_path(&["n6", "n7", "n8", "n9", "n6"]);
        graph.set_edge("n7", "n9");
        graph.set_edge("n8", "n6");
        graph.set_edge("n8", "n10");
        let fas = greedy_fas(&graph, None);
        check_fas(&mut graph, fas);
    }

    #[test]
    fn works_with_arbitrarily_weighted_edges() {
        let mut first = graph();
        first.set_edge_with("n1", "n2", weighted(2.0, 1.0));
        first.set_edge_with("n2", "n1", weighted(1.0, 1.0));
        let weight = weight_fn(&first);
        assert_eq!(
            greedy_fas(&first, Some(&weight)),
            vec![Edge::new("n2", "n1")]
        );

        let mut second = graph();
        second.set_edge_with("n1", "n2", weighted(1.0, 1.0));
        second.set_edge_with("n2", "n1", weighted(2.0, 1.0));
        let weight = weight_fn(&second);
        assert_eq!(
            greedy_fas(&second, Some(&weight)),
            vec![Edge::new("n1", "n2")]
        );
    }

    #[test]
    fn works_for_multigraphs() {
        let mut graph = super::super::test_support::multigraph();
        graph
            .set_edge_full("a", "b", Some(weighted(5.0, 1.0)), Some("foo"))
            .unwrap();
        graph
            .set_edge_full("b", "a", Some(weighted(2.0, 1.0)), Some("bar"))
            .unwrap();
        graph
            .set_edge_full("b", "a", Some(weighted(2.0, 1.0)), Some("baz"))
            .unwrap();
        let weight = weight_fn(&graph);
        let mut fas = greedy_fas(&graph, Some(&weight));
        fas.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(
            fas,
            vec![Edge::named("b", "a", "bar"), Edge::named("b", "a", "baz"),]
        );
    }
}
