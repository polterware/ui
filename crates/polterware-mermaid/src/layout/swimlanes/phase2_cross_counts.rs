//! Port of `phase2.crossCounts.ts`: approximate subtree crossing counts, with a binary-lifting
//! LCA over the driving tree.

use std::collections::{HashMap, HashSet, VecDeque};

use super::driving_tree::DrivingTree;
use super::helpers::{Graph, NodeId};

#[allow(clippy::needless_range_loop)]
pub fn compute_subtree_cross_counts(
    g: &Graph,
    rank_of: &HashMap<NodeId, i64>,
    tree: &DrivingTree,
) -> HashMap<NodeId, HashMap<NodeId, i64>> {
    let nodes = &g.nodes;
    let mut index_of: HashMap<&str, usize> = HashMap::new();
    for (index, node) in nodes.iter().enumerate() {
        index_of.insert(node.as_str(), index);
    }
    let n = nodes.len();

    let mut parent_idx: Vec<i64> = vec![-1; n];
    let mut depth: Vec<i64> = vec![0; n];

    let mut queue: VecDeque<&str> = VecDeque::new();
    let mut seen: HashSet<&str> = HashSet::new();

    // Seed BFS from roots.
    for node in nodes {
        let parent_id = tree.parent.get(node).cloned().flatten();
        let Some(&idx) = index_of.get(node.as_str()) else {
            continue;
        };
        if parent_id.is_none() {
            parent_idx[idx] = -1;
            depth[idx] = 0;
            if seen.insert(node.as_str()) {
                queue.push_back(node.as_str());
            }
        }
    }

    while let Some(current) = queue.pop_front() {
        let Some(&current_idx) = index_of.get(current) else {
            continue;
        };
        let Some(child_list) = tree.children.get(current) else {
            continue;
        };
        for child in child_list {
            if seen.contains(child.as_str()) {
                continue;
            }
            let Some(&child_idx) = index_of.get(child.as_str()) else {
                continue;
            };
            parent_idx[child_idx] = current_idx as i64;
            depth[child_idx] = depth[current_idx] + 1;
            seen.insert(child.as_str());
            queue.push_back(child.as_str());
        }
    }

    // Ensure all nodes are represented even if disconnected in the tree.
    for node in nodes {
        if seen.contains(node.as_str()) {
            continue;
        }
        let Some(&idx) = index_of.get(node.as_str()) else {
            continue;
        };
        parent_idx[idx] = -1;
        depth[idx] = 0;
        seen.insert(node.as_str());
    }

    let max_log = 1.max(((n.max(1) as f64).log2().ceil() as usize) + 1);
    let mut up: Vec<Vec<i64>> = vec![vec![-1; n]; max_log];
    up[0].copy_from_slice(&parent_idx);
    for k in 1..max_log {
        for i in 0..n {
            let prev = up[k - 1][i];
            up[k][i] = if prev == -1 {
                -1
            } else {
                up[k - 1][prev as usize]
            };
        }
    }

    let lca_index = |mut a_idx: i64, mut b_idx: i64| -> i64 {
        if a_idx == -1 || b_idx == -1 {
            return -1;
        }
        if depth[a_idx as usize] < depth[b_idx as usize] {
            std::mem::swap(&mut a_idx, &mut b_idx);
        }
        let diff = depth[a_idx as usize] - depth[b_idx as usize];
        for k in 0..max_log {
            if ((diff >> k) & 1) == 1 {
                a_idx = up[k][a_idx as usize];
                if a_idx == -1 {
                    return -1;
                }
            }
        }
        if a_idx == b_idx {
            return a_idx;
        }
        for k in (0..max_log).rev() {
            let up_a = up[k][a_idx as usize];
            let up_b = up[k][b_idx as usize];
            if up_a == -1 || up_b == -1 {
                continue;
            }
            if up_a != up_b {
                a_idx = up_a;
                b_idx = up_b;
            }
        }
        up[0][a_idx as usize]
    };

    let mut own_counts: Vec<HashMap<i64, i64>> = vec![HashMap::new(); n];

    // Count, for each LCA, how many edges pass through each layer below it.
    for edge in &g.edges {
        let (Some(&ru), Some(&rv)) = (rank_of.get(&edge.src), rank_of.get(&edge.dst)) else {
            continue;
        };
        if ru > rv {
            let (upper, lower) = (&edge.dst, &edge.src);
            let upper_idx = index_of.get(upper.as_str()).copied();
            let lower_idx = index_of.get(lower.as_str()).copied();
            let (Some(upper_idx), Some(lower_idx)) = (upper_idx, lower_idx) else {
                continue;
            };
            let lca = lca_index(upper_idx as i64, lower_idx as i64);
            if lca == -1 {
                continue;
            }
            let bucket = &mut own_counts[lca as usize];
            for layer in rv..ru {
                *bucket.entry(layer).or_insert(0) += 1;
            }
        } else {
            if ru == rv {
                continue;
            }
            let upper_idx = index_of.get(edge.src.as_str()).copied();
            let lower_idx = index_of.get(edge.dst.as_str()).copied();
            let (Some(upper_idx), Some(lower_idx)) = (upper_idx, lower_idx) else {
                continue;
            };
            let lca = lca_index(upper_idx as i64, lower_idx as i64);
            if lca == -1 {
                continue;
            }
            let bucket = &mut own_counts[lca as usize];
            for layer in ru..rv {
                *bucket.entry(layer).or_insert(0) += 1;
            }
        }
    }

    let mut cross_counts: HashMap<NodeId, HashMap<NodeId, i64>> = HashMap::new();
    let mut visited: HashSet<NodeId> = HashSet::new();

    fn dfs(
        node: &str,
        tree: &DrivingTree,
        rank_of: &HashMap<NodeId, i64>,
        index_of: &HashMap<&str, usize>,
        own_counts: &[HashMap<i64, i64>],
        visited: &mut HashSet<NodeId>,
        cross_counts: &mut HashMap<NodeId, HashMap<NodeId, i64>>,
    ) -> HashMap<i64, i64> {
        visited.insert(node.to_string());
        let base = index_of
            .get(node)
            .map(|index| own_counts[*index].clone())
            .unwrap_or_default();
        let mut accumulator = base;
        if let Some(child_list) = tree.children.get(node) {
            for child in child_list {
                let child_map = dfs(
                    child,
                    tree,
                    rank_of,
                    index_of,
                    own_counts,
                    visited,
                    cross_counts,
                );
                let parent_layer = rank_of.get(node).copied();
                if let Some(parent_layer) = parent_layer {
                    let entry = cross_counts.entry(node.to_string()).or_default();
                    let mut value = child_map.get(&parent_layer).copied().unwrap_or(0);
                    let child_layer = rank_of.get(child).copied();
                    if child_layer.is_some_and(|child_layer| child_layer > parent_layer) {
                        value += 1;
                    }
                    entry.insert(child.clone(), value);
                }
                for (layer, value) in child_map {
                    *accumulator.entry(layer).or_insert(0) += value;
                }
            }
        }
        accumulator
    }

    for root in &tree.roots {
        if !visited.contains(root) {
            dfs(
                root,
                tree,
                rank_of,
                &index_of,
                &own_counts,
                &mut visited,
                &mut cross_counts,
            );
        }
    }
    for node in nodes {
        if !visited.contains(node) {
            dfs(
                node,
                tree,
                rank_of,
                &index_of,
                &own_counts,
                &mut visited,
                &mut cross_counts,
            );
        }
    }

    cross_counts
}
