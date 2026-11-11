//! Port of `dagre/lib/order/sort.ts` (MIT, Copyright (c) 2012-2014 Chris Pettitt).

use super::super::util;
use super::super::{LayeredGraph, NodeLabel};
use super::resolve_conflicts::ResolvedEntry;

/// One sortable entry: a node or a resolved super-node.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SortEntry {
    /// The nodes in this entry.
    pub vs: Vec<String>,
    /// The lowest original index.
    pub i: usize,
    /// The weighted mean barycenter, when the entry has one.
    pub barycenter: Option<f64>,
    /// The sum of the weights, when the entry has one.
    pub weight: Option<f64>,
}

/// The sorted layering and its aggregate barycenter and weight.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SortResult {
    /// The nodes in order.
    pub vs: Vec<String>,
    /// The weighted mean barycenter, when there is one.
    pub barycenter: Option<f64>,
    /// The sum of the weights, when there is one.
    pub weight: Option<f64>,
}

impl From<ResolvedEntry> for SortEntry {
    fn from(entry: ResolvedEntry) -> Self {
        Self {
            vs: entry.vs,
            i: entry.i,
            barycenter: entry.barycenter,
            weight: entry.weight,
        }
    }
}

/// Sorts `entries` by barycenter, biasing left on ties, as upstream's one-argument
/// `sort` does.
pub fn sort(entries: Vec<SortEntry>) -> SortResult {
    sort_with(entries, &[], None, None, false)
}

/// Sorts `entries` by barycenter, biasing right on ties.
pub fn sort_biased(entries: Vec<SortEntry>, bias_right: bool) -> SortResult {
    sort_with(entries, &[], None, None, bias_right)
}

/// Sorts `entries`, reinserting reversed pairs and breaking ties with the previous
/// layout's order and the bias.
pub fn sort_with(
    entries: Vec<SortEntry>,
    reversed_pairs: &[(String, ResolvedEntry)],
    old_nodes: Option<&[(String, NodeLabel)]>,
    graph: Option<&LayeredGraph>,
    bias_right: bool,
) -> SortResult {
    let (mut sortable, mut unsortable) =
        util::partition(entries, |entry| entry.barycenter.is_some());
    unsortable.sort_by(|a, b| b.i.cmp(&a.i));
    let mut vs: Vec<Vec<String>> = Vec::new();
    let mut sum = 0.0;
    let mut weight = 0.0;
    let mut vs_index = 0usize;

    sortable.sort_by(|a, b| compare_with_old_order(graph, old_nodes, bias_right, a, b));

    for (key, value) in reversed_pairs {
        let key_index = sortable
            .iter()
            .position(|entry| entry.vs.first() == Some(key));
        match key_index {
            Some(index) => sortable.insert(index + 1, value.clone().into()),
            None => sortable.insert(0, value.clone().into()),
        }
    }

    vs_index = consume_unsortable(&mut vs, &mut unsortable, vs_index);

    for entry in sortable {
        vs_index += entry.vs.len();
        vs.push(entry.vs);
        sum += entry.barycenter.unwrap_or(f64::NAN) * entry.weight.unwrap_or(f64::NAN);
        weight += entry.weight.unwrap_or(f64::NAN);
        vs_index = consume_unsortable(&mut vs, &mut unsortable, vs_index);
    }

    let mut result = SortResult {
        vs: vs.into_iter().flatten().collect(),
        ..SortResult::default()
    };
    if weight != 0.0 && !weight.is_nan() {
        result.barycenter = Some(sum / weight);
        result.weight = Some(weight);
    }
    result
}

fn consume_unsortable(
    vs: &mut Vec<Vec<String>>,
    unsortable: &mut Vec<SortEntry>,
    mut index: usize,
) -> usize {
    while let Some(last) = unsortable.last() {
        if last.i > index {
            break;
        }
        let last = unsortable.pop().expect("the last entry exists");
        vs.push(last.vs);
        index += 1;
    }
    index
}

fn compare_with_old_order(
    graph: Option<&LayeredGraph>,
    old_nodes: Option<&[(String, NodeLabel)]>,
    bias_right: bool,
    entry_v: &SortEntry,
    entry_w: &SortEntry,
) -> std::cmp::Ordering {
    let barycenter_v = entry_v.barycenter.unwrap_or(f64::NAN);
    let barycenter_w = entry_w.barycenter.unwrap_or(f64::NAN);
    match util::cmp_f64(barycenter_v, barycenter_w) {
        std::cmp::Ordering::Less => return std::cmp::Ordering::Less,
        std::cmp::Ordering::Greater => return std::cmp::Ordering::Greater,
        std::cmp::Ordering::Equal => {}
    }

    if let Some(graph) = graph {
        let node_v = graph
            .node(entry_v.vs.first().map(String::as_str).unwrap_or_default())
            .cloned()
            .unwrap_or_default();
        let node_w = graph
            .node(entry_w.vs.first().map(String::as_str).unwrap_or_default())
            .cloned()
            .unwrap_or_default();
        let by_old_order = util::compare_by_old_order(old_nodes, &node_v, &node_w);
        match util::cmp_f64(by_old_order, 0.0) {
            std::cmp::Ordering::Less => return std::cmp::Ordering::Less,
            std::cmp::Ordering::Greater => return std::cmp::Ordering::Greater,
            std::cmp::Ordering::Equal => {}
        }
    }

    if bias_right {
        entry_w.i.cmp(&entry_v.i)
    } else {
        entry_v.i.cmp(&entry_w.i)
    }
}

#[cfg(test)]
mod tests {
    use super::SortEntry;

    fn entry(vs: &[&str], i: usize, barycenter: Option<f64>, weight: Option<f64>) -> SortEntry {
        SortEntry {
            vs: vs.iter().map(|v| v.to_string()).collect(),
            i,
            barycenter,
            weight,
        }
    }

    #[test]
    fn sorts_nodes_by_barycenter() {
        let result = super::sort(vec![
            entry(&["a"], 0, Some(2.0), Some(3.0)),
            entry(&["b"], 1, Some(1.0), Some(2.0)),
        ]);
        assert_eq!(result.vs, vec!["b".to_string(), "a".to_string()]);
        assert_eq!(result.barycenter, Some(8.0 / 5.0));
        assert_eq!(result.weight, Some(5.0));
    }

    #[test]
    fn can_sort_super_nodes() {
        let result = super::sort(vec![
            entry(&["a", "c", "d"], 0, Some(2.0), Some(3.0)),
            entry(&["b"], 1, Some(1.0), Some(2.0)),
        ]);
        assert_eq!(
            result.vs,
            vec![
                "b".to_string(),
                "a".to_string(),
                "c".to_string(),
                "d".to_string()
            ]
        );
    }

    #[test]
    fn biases_left_by_default() {
        let result = super::sort(vec![
            entry(&["a"], 0, Some(1.0), Some(1.0)),
            entry(&["b"], 1, Some(1.0), Some(1.0)),
        ]);
        assert_eq!(result.vs, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(result.barycenter, Some(1.0));
        assert_eq!(result.weight, Some(2.0));
    }

    #[test]
    fn biases_right_with_bias_right() {
        let result = super::sort_biased(
            vec![
                entry(&["a"], 0, Some(1.0), Some(1.0)),
                entry(&["b"], 1, Some(1.0), Some(1.0)),
            ],
            true,
        );
        assert_eq!(result.vs, vec!["b".to_string(), "a".to_string()]);
    }

    #[test]
    fn can_sort_nodes_without_a_barycenter() {
        let result = super::sort(vec![
            entry(&["a"], 0, Some(2.0), Some(1.0)),
            entry(&["b"], 1, Some(6.0), Some(1.0)),
            entry(&["c"], 2, None, None),
            entry(&["d"], 3, Some(3.0), Some(1.0)),
        ]);
        assert_eq!(
            result.vs,
            vec![
                "a".to_string(),
                "d".to_string(),
                "c".to_string(),
                "b".to_string()
            ]
        );
        assert_eq!(result.barycenter, Some(11.0 / 3.0));
        assert_eq!(result.weight, Some(3.0));
    }

    #[test]
    fn handles_no_barycenters() {
        let result = super::sort(vec![
            entry(&["a"], 0, None, None),
            entry(&["b"], 3, None, None),
            entry(&["c"], 2, None, None),
            entry(&["d"], 1, None, None),
        ]);
        assert_eq!(
            result.vs,
            vec![
                "a".to_string(),
                "d".to_string(),
                "c".to_string(),
                "b".to_string()
            ]
        );
        assert_eq!(result.barycenter, None);
        assert_eq!(result.weight, None);
    }

    #[test]
    fn handles_a_barycenter_of_zero() {
        let result = super::sort(vec![
            entry(&["a"], 0, Some(0.0), Some(1.0)),
            entry(&["b"], 3, None, None),
            entry(&["c"], 2, None, None),
            entry(&["d"], 1, None, None),
        ]);
        assert_eq!(
            result.vs,
            vec![
                "a".to_string(),
                "d".to_string(),
                "c".to_string(),
                "b".to_string()
            ]
        );
        assert_eq!(result.barycenter, Some(0.0));
        assert_eq!(result.weight, Some(1.0));
    }
}
