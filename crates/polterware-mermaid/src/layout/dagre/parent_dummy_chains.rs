//! Port of `dagre/lib/parent-dummy-chains.ts` (MIT, Copyright (c) 2012-2014 Chris
//! Pettitt).
//!
//! After ordering, puts each dummy of a long edge inside the deepest subgraph that
//! contains the whole edge, using the lowest common ancestor of its endpoints.

use std::collections::HashMap;

use super::LayeredGraph;

#[derive(Clone, Copy)]
struct PostorderNum {
    low: usize,
    lim: usize,
}

struct PathData {
    path: Vec<Option<String>>,
    lca: Option<String>,
}

/// Assigns parents to the dummies of long edges.
pub fn run(graph: &mut LayeredGraph) {
    let postorder_nums = postorder(graph);
    let Some(chains) = graph.graph().and_then(|label| label.dummy_chains.clone()) else {
        return;
    };

    for start in chains {
        let Some(node) = graph.node(&start).cloned() else {
            continue;
        };
        let Some(edge_obj) = node.edge_obj.clone() else {
            continue;
        };
        let path_data = find_path(graph, &postorder_nums, &edge_obj.v, &edge_obj.w);
        let path = path_data.path;
        let lca = path_data.lca;
        let mut path_idx = 0usize;
        let mut path_v = path.first().cloned().flatten();
        let mut ascending = true;
        let mut v = start;

        while v != edge_obj.w {
            let Some(node) = graph.node(&v).cloned() else {
                break;
            };

            if ascending {
                loop {
                    path_v = path.get(path_idx).cloned().flatten();
                    if path_v == lca {
                        break;
                    }
                    let Some(candidate) = &path_v else {
                        break;
                    };
                    let max_rank = graph
                        .node(candidate)
                        .and_then(|candidate| candidate.max_rank)
                        .unwrap_or(0.0);
                    let rank = node.rank.unwrap_or(0.0);
                    if max_rank < rank {
                        path_idx += 1;
                    } else {
                        break;
                    }
                }
                if path_v == lca {
                    ascending = false;
                }
            }

            if !ascending {
                while path_idx < path.len().saturating_sub(1)
                    && graph
                        .node(path[path_idx + 1].as_deref().unwrap_or_default())
                        .and_then(|candidate| candidate.min_rank)
                        .unwrap_or(0.0)
                        <= node.rank.unwrap_or(0.0)
                {
                    path_idx += 1;
                }
                path_v = path.get(path_idx).cloned().flatten();
            }

            if let Some(path_v) = &path_v {
                graph
                    .set_parent(&v, Some(path_v.as_str()))
                    .expect("the layout graph is compound");
            }
            let Some(next) = graph.successors(&v).and_then(|next| next.first().cloned()) else {
                break;
            };
            v = next;
        }
    }
}

fn find_path(
    graph: &LayeredGraph,
    postorder_nums: &HashMap<String, PostorderNum>,
    v: &str,
    w: &str,
) -> PathData {
    let mut v_path: Vec<Option<String>> = Vec::new();
    let mut w_path: Vec<Option<String>> = Vec::new();
    let low = postorder_nums
        .get(v)
        .map_or(0, |num| num.low)
        .min(postorder_nums.get(w).map_or(0, |num| num.low));
    let lim = postorder_nums
        .get(v)
        .map_or(0, |num| num.lim)
        .max(postorder_nums.get(w).map_or(0, |num| num.lim));

    let mut parent = Some(v.to_string());
    loop {
        let Some(current) = parent.clone() else {
            v_path.push(None);
            break;
        };
        parent = graph.parent(&current).map(str::to_string);
        v_path.push(parent.clone());
        let Some(parent) = parent.clone() else {
            break;
        };
        let parent_num = postorder_nums.get(&parent).copied();
        let Some(parent_num) = parent_num else {
            break;
        };
        if parent_num.low > low || lim > parent_num.lim {
            continue;
        }
        break;
    }
    let lca = parent;

    let mut w_parent = w.to_string();
    loop {
        let Some(parent) = graph.parent(&w_parent).map(str::to_string) else {
            if lca.is_some() {
                w_path.push(None);
            }
            break;
        };
        w_parent = parent;
        if Some(w_parent.clone()) == lca {
            break;
        }
        w_path.push(Some(w_parent.clone()));
    }

    w_path.reverse();
    v_path.extend(w_path);
    PathData { path: v_path, lca }
}

fn postorder(graph: &LayeredGraph) -> HashMap<String, PostorderNum> {
    fn dfs(
        graph: &LayeredGraph,
        v: &str,
        lim: &mut usize,
        result: &mut HashMap<String, PostorderNum>,
    ) {
        let low = *lim;
        for child in graph.children(v) {
            dfs(graph, &child, lim, result);
        }
        result.insert(v.to_string(), PostorderNum { low, lim: *lim });
        *lim += 1;
    }

    let mut result = HashMap::new();
    let mut lim = 0;
    for v in graph.root_children() {
        dfs(graph, &v, &mut lim, &mut result);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::super::graphlib::Edge;
    use super::super::test_support::compound;
    use super::super::{LayeredGraph, NodeLabel};

    fn node(
        edge_obj: Option<Edge>,
        rank: Option<f64>,
        min_rank: Option<f64>,
        max_rank: Option<f64>,
    ) -> NodeLabel {
        NodeLabel {
            edge_obj,
            rank,
            min_rank,
            max_rank,
            ..NodeLabel::default()
        }
    }

    fn chain(graph: &mut LayeredGraph, chain: &[&str]) {
        graph.set_graph_label(Some(super::super::GraphLabel {
            dummy_chains: Some(vec![chain[1].to_string()]),
            ..Default::default()
        }));
        let path: Vec<&str> = chain.to_vec();
        graph.set_path(&path);
    }

    #[test]
    fn does_not_set_a_parent_if_neither_end_has_one() {
        let mut graph = compound();
        graph.set_node_with("a", NodeLabel::default());
        graph.set_node_with("b", NodeLabel::default());
        graph.set_node_with("d1", node(Some(Edge::new("a", "b")), None, None, None));
        chain(&mut graph, &["a", "d1", "b"]);
        super::run(&mut graph);
        assert_eq!(graph.parent("d1"), None);
    }

    #[test]
    fn uses_the_tail_parent_for_the_first_node() {
        let mut graph = compound();
        graph.set_node_with("sg1", node(None, None, Some(0.0), Some(2.0)));
        graph.set_node_with("a", NodeLabel::default());
        graph.set_parent("a", Some("sg1")).unwrap();
        graph.set_node_with("b", NodeLabel::default());
        graph.set_node_with("d1", node(Some(Edge::new("a", "b")), Some(2.0), None, None));
        chain(&mut graph, &["a", "d1", "b"]);
        super::run(&mut graph);
        assert_eq!(graph.parent("d1"), Some("sg1"));
    }

    #[test]
    fn uses_the_head_parent_if_the_tail_is_at_the_root() {
        let mut graph = compound();
        graph.set_node_with("sg1", node(None, None, Some(1.0), Some(3.0)));
        graph.set_node_with("a", NodeLabel::default());
        graph.set_node_with("b", NodeLabel::default());
        graph.set_parent("b", Some("sg1")).unwrap();
        graph.set_node_with("d1", node(Some(Edge::new("a", "b")), Some(1.0), None, None));
        chain(&mut graph, &["a", "d1", "b"]);
        super::run(&mut graph);
        assert_eq!(graph.parent("d1"), Some("sg1"));
    }

    #[test]
    fn handles_a_long_chain_starting_in_a_subgraph() {
        let mut graph = compound();
        graph.set_node_with("sg1", node(None, None, Some(0.0), Some(2.0)));
        graph.set_node_with("a", NodeLabel::default());
        graph.set_parent("a", Some("sg1")).unwrap();
        graph.set_node_with("b", NodeLabel::default());
        graph.set_node_with("d1", node(Some(Edge::new("a", "b")), Some(2.0), None, None));
        graph.set_node_with("d2", node(None, Some(3.0), None, None));
        graph.set_node_with("d3", node(None, Some(4.0), None, None));
        chain(&mut graph, &["a", "d1", "d2", "d3", "b"]);
        super::run(&mut graph);
        assert_eq!(graph.parent("d1"), Some("sg1"));
        assert_eq!(graph.parent("d2"), None);
        assert_eq!(graph.parent("d3"), None);
    }

    #[test]
    fn handles_a_long_chain_ending_in_a_subgraph() {
        let mut graph = compound();
        graph.set_node_with("a", NodeLabel::default());
        graph.set_node_with("sg1", node(None, None, Some(3.0), Some(5.0)));
        graph.set_node_with("b", NodeLabel::default());
        graph.set_parent("b", Some("sg1")).unwrap();
        graph.set_node_with("d1", node(Some(Edge::new("a", "b")), Some(1.0), None, None));
        graph.set_node_with("d2", node(None, Some(2.0), None, None));
        graph.set_node_with("d3", node(None, Some(3.0), None, None));
        chain(&mut graph, &["a", "d1", "d2", "d3", "b"]);
        super::run(&mut graph);
        assert_eq!(graph.parent("d1"), None);
        assert_eq!(graph.parent("d2"), None);
        assert_eq!(graph.parent("d3"), Some("sg1"));
    }

    #[test]
    fn handles_nested_subgraphs() {
        let mut graph = compound();
        graph.set_node_with("sg1", node(None, None, Some(0.0), Some(4.0)));
        graph.set_node_with("sg2", node(None, None, Some(1.0), Some(3.0)));
        graph.set_parent("sg2", Some("sg1")).unwrap();
        graph.set_node_with("a", NodeLabel::default());
        graph.set_parent("a", Some("sg2")).unwrap();
        graph.set_node_with("sg3", node(None, None, Some(6.0), Some(10.0)));
        graph.set_node_with("sg4", node(None, None, Some(7.0), Some(9.0)));
        graph.set_parent("sg4", Some("sg3")).unwrap();
        graph.set_node_with("b", NodeLabel::default());
        graph.set_parent("b", Some("sg4")).unwrap();
        for i in 0..5 {
            graph.set_node_with(
                format!("d{}", i + 1),
                node(None, Some((i + 3) as f64), None, None),
            );
        }
        graph.node_mut("d1").unwrap().edge_obj = Some(Edge::new("a", "b"));
        chain(&mut graph, &["a", "d1", "d2", "d3", "d4", "d5", "b"]);
        super::run(&mut graph);
        assert_eq!(graph.parent("d1"), Some("sg2"));
        assert_eq!(graph.parent("d2"), Some("sg1"));
        assert_eq!(graph.parent("d3"), None);
        assert_eq!(graph.parent("d4"), Some("sg3"));
        assert_eq!(graph.parent("d5"), Some("sg4"));
    }

    #[test]
    fn handles_overlapping_rank_ranges() {
        let mut graph = compound();
        graph.set_node_with("sg1", node(None, None, Some(0.0), Some(3.0)));
        graph.set_node_with("a", NodeLabel::default());
        graph.set_parent("a", Some("sg1")).unwrap();
        graph.set_node_with("sg2", node(None, None, Some(2.0), Some(6.0)));
        graph.set_node_with("b", NodeLabel::default());
        graph.set_parent("b", Some("sg2")).unwrap();
        graph.set_node_with("d1", node(Some(Edge::new("a", "b")), Some(2.0), None, None));
        graph.set_node_with("d2", node(None, Some(3.0), None, None));
        graph.set_node_with("d3", node(None, Some(4.0), None, None));
        chain(&mut graph, &["a", "d1", "d2", "d3", "b"]);
        super::run(&mut graph);
        assert_eq!(graph.parent("d1"), Some("sg1"));
        assert_eq!(graph.parent("d2"), Some("sg1"));
        assert_eq!(graph.parent("d3"), Some("sg2"));
    }

    #[test]
    fn handles_an_lca_that_is_not_the_root_1() {
        let mut graph = compound();
        graph.set_node_with("sg1", node(None, None, Some(0.0), Some(6.0)));
        graph.set_node_with("sg2", node(None, None, Some(3.0), Some(5.0)));
        graph.set_parent("sg2", Some("sg1")).unwrap();
        graph.set_node_with("a", NodeLabel::default());
        graph.set_parent("a", Some("sg1")).unwrap();
        graph.set_node_with("b", NodeLabel::default());
        graph.set_parent("b", Some("sg2")).unwrap();
        graph.set_node_with("d1", node(Some(Edge::new("a", "b")), Some(2.0), None, None));
        graph.set_node_with("d2", node(None, Some(3.0), None, None));
        chain(&mut graph, &["a", "d1", "d2", "b"]);
        super::run(&mut graph);
        assert_eq!(graph.parent("d1"), Some("sg1"));
        assert_eq!(graph.parent("d2"), Some("sg2"));
    }

    #[test]
    fn handles_an_lca_that_is_not_the_root_2() {
        let mut graph = compound();
        graph.set_node_with("sg1", node(None, None, Some(0.0), Some(6.0)));
        graph.set_node_with("sg2", node(None, None, Some(1.0), Some(3.0)));
        graph.set_parent("sg2", Some("sg1")).unwrap();
        graph.set_node_with("a", NodeLabel::default());
        graph.set_parent("a", Some("sg2")).unwrap();
        graph.set_node_with("b", NodeLabel::default());
        graph.set_parent("b", Some("sg1")).unwrap();
        graph.set_node_with("d1", node(Some(Edge::new("a", "b")), Some(3.0), None, None));
        graph.set_node_with("d2", node(None, Some(4.0), None, None));
        chain(&mut graph, &["a", "d1", "d2", "b"]);
        super::run(&mut graph);
        assert_eq!(graph.parent("d1"), Some("sg2"));
        assert_eq!(graph.parent("d2"), Some("sg1"));
    }
}
