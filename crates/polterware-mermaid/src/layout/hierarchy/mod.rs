//! Port of `d3-hierarchy` 3.1.2's `src/hierarchy/` and `src/treemap/` (ISC, Copyright
//! 2010-2023 Mike Bostock), the layout Mermaid 12.3.0's treemap diagram runs
//! (`packages/mermaid/src/diagrams/treemap/renderer.ts`, which calls
//! `hierarchy(root).sum(...).sort(...)`, then `treemap().size(...).padding*(...).round(true)`
//! with the default `squarify` tiling).
//!
//! Sources: `src/hierarchy/{index,sum,sort,each,eachBefore,eachAfter,descendants,leaves}.js`
//! and `src/treemap/{index,squarify,binary,dice,slice,sliceDice,resquarify,round}.js` in
//! `research/mermaid-port/upstream/mermaid/node_modules/.pnpm/d3-hierarchy@3.1.2/node_modules/d3-hierarchy`.
//!
//! Upstream's nodes are objects that point at each other, and `treemap` writes `x0`, `y0`,
//! `x1`, and `y1` onto them. Here the nodes live in an arena owned by [`Hierarchy`] and are
//! referred to by [`NodeId`]; a node keeps its parent, children, depth, height, value, and
//! that rectangle. `value` is `Option<f64>` because upstream's is `undefined` until `sum`
//! (or `count`) sets it, and `undefined` behaves differently from `0` in the tiling code —
//! the port keeps the distinction and turns `undefined` into `NaN` only where JavaScript
//! arithmetic would.
//!
//! Arithmetic and iteration order are upstream's line by line. Children are added through
//! `sum` in reverse index order, `sort` is stable (as JavaScript's is), `eachAfter` keeps
//! upstream's two-stack order, and `each` walks the tree level by level in insertion order.
//! Children returned as `None` or as an empty list from the `children` accessor make a leaf,
//! as upstream's `if (children && (n = ...length))` does. A node with `Some(vec![])` is not a
//! leaf in either implementation.
//!
//! Not ported, because Mermaid's treemap does not use them: `count`, `find`, `path`,
//! `ancestors`, `links`, `copy`, and the `Map` input form of `hierarchy`. `round` uses
//! [JavaScript's `Math.round`](js_round), not Rust's `f64::round`, which rounds halves away
//! from zero where JavaScript rounds them toward positive infinity.
//!
//! [`Treemap`] is in [`treemap`], with the tiling methods in [`treemap::tile`].

pub mod treemap;

#[cfg(test)]
mod differential;

pub use treemap::{Padding, PaddingFn, Tile, Treemap};

/// The index of a node in its [`Hierarchy`]; upstream's node object identity.
pub type NodeId = usize;

/// One node of a [`Hierarchy`], upstream's `Node`.
#[derive(Clone, Debug, PartialEq)]
pub struct Node<T> {
    /// Upstream's `data`.
    pub data: T,
    /// Upstream's `parent` (`null` for the root).
    pub parent: Option<NodeId>,
    /// Upstream's `children`: `None` on a leaf, `Some` (possibly empty) on a parent.
    pub children: Option<Vec<NodeId>>,
    /// Upstream's `depth`, counting from the root's `0`.
    pub depth: usize,
    /// Upstream's `height`, the longest distance to a descendant leaf.
    pub height: usize,
    /// Upstream's `value`, `undefined` (here `None`) until `sum` or `count` sets it.
    pub value: Option<f64>,
    /// The left edge [`Treemap`] writes (`x0`).
    pub x0: f64,
    /// The top edge [`Treemap`] writes (`y0`).
    pub y0: f64,
    /// The right edge [`Treemap`] writes (`x1`).
    pub x1: f64,
    /// The bottom edge [`Treemap`] writes (`y1`).
    pub y1: f64,
    /// Upstream's `_squarify`, the rows [`Tile::Resquarify`] reuses on the next layout.
    pub(crate) squarify: Option<treemap::tile::SquarifyRows>,
}

impl<T> Node<T> {
    fn new(data: T) -> Self {
        Self {
            data,
            parent: None,
            children: None,
            depth: 0,
            height: 0,
            value: None,
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
            squarify: None,
        }
    }
}

/// A tree of [`Node`]s, upstream's `hierarchy()` root.
#[derive(Clone, Debug)]
pub struct Hierarchy<T> {
    nodes: Vec<Node<T>>,
    root: NodeId,
}

impl<T> Hierarchy<T> {
    /// `hierarchy(data)` without a children accessor: a single node.
    pub fn new(data: T) -> Self {
        Self {
            nodes: vec![Node::new(data)],
            root: 0,
        }
    }

    /// `hierarchy(data, children)`: `children` returns `None` (or an empty list) for a
    /// leaf, and the child data in order otherwise, as upstream's accessor returns
    /// `undefined` or an array-like.
    pub fn build(data: T, children: impl Fn(&T) -> Option<Vec<T>>) -> Self {
        let mut nodes = vec![Node::new(data)];
        let mut stack: Vec<NodeId> = vec![0];

        while let Some(id) = stack.pop() {
            let Some(childs) = children(&nodes[id].data) else {
                continue;
            };
            if childs.is_empty() {
                // Upstream leaves `children` undefined when the accessor yields nothing.
                continue;
            }
            let depth = nodes[id].depth + 1;
            let mut ids = Vec::with_capacity(childs.len());
            for child in childs {
                let child_id = nodes.len();
                let mut node = Node::new(child);
                node.parent = Some(id);
                node.depth = depth;
                nodes.push(node);
                ids.push(child_id);
            }
            nodes[id].children = Some(ids.clone());
            for child_id in ids.into_iter().rev() {
                stack.push(child_id);
            }
        }

        let mut hierarchy = Self { nodes, root: 0 };
        for id in hierarchy.order_each_before() {
            compute_height(&mut hierarchy.nodes, id);
        }
        hierarchy
    }

    /// Upstream's `root`.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// The node at `id`.
    pub fn node(&self, id: NodeId) -> &Node<T> {
        &self.nodes[id]
    }

    /// The node at `id`, mutably.
    pub fn node_mut(&mut self, id: NodeId) -> &mut Node<T> {
        &mut self.nodes[id]
    }

    /// The number of nodes in the tree.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Always `false`: a hierarchy has a root.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Upstream's `sum`: sets every node's `value` to `value(node.data)`, coerced with
    /// JavaScript's unary `+` and `|| 0`, plus its children's values.
    pub fn sum(&mut self, value: impl Fn(&T) -> Option<f64>) {
        for id in self.order_each_after() {
            let mut sum = match value(&self.nodes[id].data) {
                Some(sum) if !sum.is_nan() && sum != 0.0 => sum,
                Some(_) | None => 0.0,
            };
            if let Some(children) = &self.nodes[id].children {
                // Upstream adds in reverse index order; float addition is order dependent.
                for &child in children.iter().rev() {
                    sum += self.nodes[child].value.unwrap_or(f64::NAN);
                }
            }
            self.nodes[id].value = Some(sum);
        }
    }

    /// Upstream's `sort`: sorts every node's children in place with `compare`, in stable
    /// order (JavaScript's `Array.prototype.sort` is stable since ES2019), before
    /// descending into them.
    pub fn sort_by(&mut self, compare: impl Fn(&Node<T>, &Node<T>) -> std::cmp::Ordering) {
        for id in self.order_each_before() {
            let Some(mut children) = self.nodes[id].children.take() else {
                continue;
            };
            children.sort_by(|a, b| compare(&self.nodes[*a], &self.nodes[*b]));
            self.nodes[id].children = Some(children);
        }
    }

    /// Upstream's `each`: visits every node level by level, in insertion order.
    pub fn each(&self, mut callback: impl FnMut(&Node<T>, usize)) {
        for (index, id) in self.order_each().into_iter().enumerate() {
            callback(&self.nodes[id], index);
        }
    }

    /// Upstream's `eachBefore`: visits every node before its children, depth first, in
    /// child order.
    pub fn each_before(&self, mut callback: impl FnMut(&Node<T>, usize)) {
        for (index, id) in self.order_each_before().into_iter().enumerate() {
            callback(&self.nodes[id], index);
        }
    }

    /// Upstream's `eachAfter`: visits every node after its children, depth first.
    pub fn each_after(&self, mut callback: impl FnMut(&Node<T>, usize)) {
        for (index, id) in self.order_each_after().into_iter().enumerate() {
            callback(&self.nodes[id], index);
        }
    }

    /// Upstream's `descendants`: every node in `each` order.
    pub fn descendants(&self) -> Vec<NodeId> {
        self.order_each()
    }

    /// Upstream's `leaves`: every node without children, in `eachBefore` order.
    pub fn leaves(&self) -> Vec<NodeId> {
        self.order_each_before()
            .into_iter()
            .filter(|&id| self.nodes[id].children.is_none())
            .collect()
    }

    /// `eachBefore` order, usable while the hierarchy is borrowed mutably elsewhere.
    pub(crate) fn order_each_before(&self) -> Vec<NodeId> {
        let mut nodes = vec![self.root];
        let mut order = Vec::with_capacity(self.nodes.len());
        while let Some(id) = nodes.pop() {
            order.push(id);
            if let Some(children) = &self.nodes[id].children {
                for &child in children.iter().rev() {
                    nodes.push(child);
                }
            }
        }
        order
    }

    /// `eachAfter` order, usable while the hierarchy is borrowed mutably elsewhere.
    pub(crate) fn order_each_after(&self) -> Vec<NodeId> {
        let mut nodes = vec![self.root];
        let mut order = Vec::with_capacity(self.nodes.len());
        while let Some(id) = nodes.pop() {
            order.push(id);
            if let Some(children) = &self.nodes[id].children {
                for &child in children {
                    nodes.push(child);
                }
            }
        }
        order.reverse();
        order
    }

    /// `each` order, usable while the hierarchy is borrowed mutably elsewhere.
    fn order_each(&self) -> Vec<NodeId> {
        let mut current = vec![self.root];
        let mut order = Vec::with_capacity(self.nodes.len());
        loop {
            let mut next = Vec::new();
            for id in current.drain(..) {
                order.push(id);
                if let Some(children) = &self.nodes[id].children {
                    next.extend_from_slice(children);
                }
            }
            if next.is_empty() {
                return order;
            }
            current = next;
        }
    }
}

/// Upstream's `computeHeight`: sets `height` on `id` and raises its ancestors' until one is
/// already at least as tall.
fn compute_height<T>(nodes: &mut [Node<T>], id: NodeId) {
    let mut height = 0;
    let mut current = Some(id);
    while let Some(id) = current {
        nodes[id].height = height;
        current = match nodes[id].parent {
            Some(parent) if nodes[parent].height < height + 1 => {
                height += 1;
                Some(parent)
            }
            _ => None,
        };
    }
}

/// JavaScript's `Math.round`: halves round toward positive infinity, and `-0` is kept for
/// `-0.5 <= x < 0`. Rust's `f64::round` rounds halves away from zero instead.
pub(crate) fn js_round(value: f64) -> f64 {
    if !value.is_finite() || value == 0.0 {
        return value;
    }
    let rounded = (value + 0.5).floor();
    if rounded == 0.0 && value < 0.0 {
        -0.0
    } else {
        rounded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Item {
        name: &'static str,
        children: Vec<Item>,
    }

    fn item(name: &'static str, children: Vec<Item>) -> Item {
        Item { name, children }
    }

    /// root(a(b, c), d)
    fn tree() -> Hierarchy<Item> {
        let data = item(
            "root",
            vec![
                item("a", vec![item("b", vec![]), item("c", vec![])]),
                item("d", vec![]),
            ],
        );
        Hierarchy::build(data, |item| {
            (!item.children.is_empty()).then(|| item.children.clone())
        })
    }

    fn names(hierarchy: &Hierarchy<Item>, ids: &[NodeId]) -> Vec<&'static str> {
        ids.iter().map(|&id| hierarchy.node(id).data.name).collect()
    }

    #[test]
    fn traversals_keep_upstream_order() {
        let hierarchy = tree();
        let mut order = Vec::new();
        hierarchy.each(|node, _| order.push(node.data.name));
        assert_eq!(order, ["root", "a", "d", "b", "c"]);

        let mut before = Vec::new();
        hierarchy.each_before(|node, _| before.push(node.data.name));
        assert_eq!(before, ["root", "a", "b", "c", "d"]);

        let mut after = Vec::new();
        hierarchy.each_after(|node, _| after.push(node.data.name));
        assert_eq!(after, ["b", "c", "a", "d", "root"]);

        assert_eq!(
            names(&hierarchy, &hierarchy.descendants()),
            ["root", "a", "d", "b", "c"]
        );
        assert_eq!(names(&hierarchy, &hierarchy.leaves()), ["b", "c", "d"]);
    }

    #[test]
    fn sum_sets_every_value_and_height() {
        let mut hierarchy = tree();
        hierarchy.sum(|_| Some(1.0));
        let values: Vec<f64> = hierarchy
            .descendants()
            .into_iter()
            .map(|id| hierarchy.node(id).value.unwrap_or(f64::NAN))
            .collect();
        assert_eq!(values, [5.0, 3.0, 1.0, 1.0, 1.0]);
        assert_eq!(hierarchy.node(hierarchy.root()).height, 2);
        assert_eq!(hierarchy.node(hierarchy.root()).value, Some(5.0));
    }

    #[test]
    fn sort_is_stable() {
        let data = item(
            "root",
            ["bb", "a", "ccc", "dd"]
                .into_iter()
                .map(|name| item(name, vec![]))
                .collect(),
        );
        let mut hierarchy = Hierarchy::build(data, |item| {
            (!item.children.is_empty()).then(|| item.children.clone())
        });
        hierarchy.sum(|item| Some(item.name.len() as f64));
        hierarchy.sort_by(|a, b| {
            b.value
                .unwrap_or(0.0)
                .partial_cmp(&a.value.unwrap_or(0.0))
                .unwrap()
        });
        let children = hierarchy.node(hierarchy.root()).children.clone().unwrap();
        assert_eq!(names(&hierarchy, &children), ["ccc", "bb", "dd", "a"]);
    }

    #[test]
    fn build_treats_empty_children_as_a_leaf() {
        let data = item("root", vec![item("empty", vec![])]);
        let hierarchy = Hierarchy::build(data, |item| Some(item.children.clone()));
        assert_eq!(hierarchy.len(), 2);
        assert_eq!(names(&hierarchy, &hierarchy.leaves()), ["empty"]);
        assert!(hierarchy.node(1).children.is_none());
    }
}
