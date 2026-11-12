//! Port of `layout-base/src/LGraph.js`.
//!
//! A graph: its nodes, its intra-graph edges, its parent node, and the bounds it wraps
//! its contents in. Upstream's `add`, `remove`, and `updateBounds` reach nodes, edges,
//! and the manager, so they are methods on [`LGraphManager`] here; the plain getters stay
//! on [`LGraph`].
//!
//! `LGraph.prototype.updateBounds` reads `nodes[0].getParent().paddingLeft` for the
//! margin and, for an empty graph, copies the parent node's bounds before reading
//! `nodes[0]`, which throws. The port reads [`LNode::padding_left`] and, for an empty
//! graph, keeps the parent-node copy and stops.

use super::LayoutError;
use super::l_edge::EdgeId;
use super::l_graph_manager::LGraphManager;
use super::l_graph_object::LGraphObject;
use super::l_node::NodeId;
use super::layout_constants as constants;
use super::util::integer;
use super::util::linked_list::LinkedList;
use super::util::point::Point;
use super::util::rectangle_d::RectangleD;

/// Index of an [`LGraph`] in [`LGraphManager`]'s graph arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GraphId(pub usize);

/// A graph, upstream's `LGraph`.
#[derive(Clone, Debug, PartialEq)]
pub struct LGraph<V> {
    /// Upstream's `LGraphObject` base.
    pub base: LGraphObject<V>,
    /// The node that owns this graph, upstream's `parent`.
    pub parent: Option<NodeId>,
    /// Nodes in insertion order, upstream's `nodes`.
    pub nodes: Vec<NodeId>,
    /// Intra-graph edges in insertion order, upstream's `edges`.
    pub edges: Vec<EdgeId>,
    /// Margin around the contents, upstream's `margin`.
    pub margin: f64,
    /// Whether the graph is connected, upstream's `isConnected`.
    pub is_connected: bool,
    /// The size the last estimate computed, upstream's `estimatedSize`.
    pub estimated_size: Option<f64>,
    /// Left bound, upstream's `left`.
    pub left: f64,
    /// Right bound, upstream's `right`.
    pub right: f64,
    /// Top bound, upstream's `top`.
    pub top: f64,
    /// Bottom bound, upstream's `bottom`.
    pub bottom: f64,
}

impl<V> Default for LGraph<V> {
    fn default() -> Self {
        Self {
            base: LGraphObject::default(),
            parent: None,
            nodes: Vec::new(),
            edges: Vec::new(),
            margin: constants::DEFAULT_GRAPH_MARGIN,
            is_connected: false,
            estimated_size: None,
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
        }
    }
}

impl<V> LGraph<V> {
    /// A graph with a v-level object, upstream's constructor. Upstream also takes the
    /// graph manager or the layout here; the manager owns the arena, so it is not a
    /// field.
    pub fn new(v_graph: Option<V>) -> Self {
        Self {
            base: LGraphObject::new(v_graph),
            ..Self::default()
        }
    }

    /// Upstream `getNodes`.
    pub fn get_nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    /// Upstream `getEdges`.
    pub fn get_edges(&self) -> &[EdgeId] {
        &self.edges
    }

    /// Upstream `getParent`.
    pub fn get_parent(&self) -> Option<NodeId> {
        self.parent
    }

    /// Upstream `getLeft`.
    pub fn get_left(&self) -> f64 {
        self.left
    }

    /// Upstream `getRight`.
    pub fn get_right(&self) -> f64 {
        self.right
    }

    /// Upstream `getTop`.
    pub fn get_top(&self) -> f64 {
        self.top
    }

    /// Upstream `getBottom`.
    pub fn get_bottom(&self) -> f64 {
        self.bottom
    }

    /// Upstream `isConnected`.
    pub fn is_connected(&self) -> bool {
        self.is_connected
    }

    /// Upstream `getEstimatedSize`.
    pub fn get_estimated_size(&self) -> Result<f64, LayoutError> {
        self.estimated_size.ok_or(LayoutError("assert failed"))
    }
}

impl<V> LGraphManager<V> {
    /// Upstream `LGraph.prototype.add(node)`.
    pub fn graph_add_node(&mut self, graph: GraphId, node: NodeId) -> Result<NodeId, LayoutError> {
        if self.graph(graph).nodes.contains(&node) {
            return Err(LayoutError("Node already in graph!"));
        }

        self.node_mut(node).owner = Some(graph);
        self.graph_mut(graph).nodes.push(node);

        Ok(node)
    }

    /// Upstream `LGraph.prototype.add(edge, sourceNode, targetNode)`.
    pub fn graph_add_edge(
        &mut self,
        graph: GraphId,
        edge: EdgeId,
        source: NodeId,
        target: NodeId,
    ) -> Result<EdgeId, LayoutError> {
        if !(self.graph(graph).nodes.contains(&source) && self.graph(graph).nodes.contains(&target))
        {
            return Err(LayoutError("Source or target not in graph!"));
        }

        let source_owner = self.node(source).owner;
        let target_owner = self.node(target).owner;
        if !(source_owner == target_owner && source_owner == Some(graph)) {
            return Err(LayoutError("Both owners must be this graph!"));
        }
        // Upstream returns null when the owners differ; the check above has already
        // thrown in that case.

        {
            let edge = self.edge_mut(edge);
            edge.source = Some(source);
            edge.target = Some(target);
            edge.is_inter_graph = false;
        }

        self.graph_mut(graph).edges.push(edge);
        self.node_mut(source).edges.push(edge);

        if target != source {
            self.node_mut(target).edges.push(edge);
        }

        Ok(edge)
    }

    /// Upstream `LGraph.prototype.remove(node)`: removes the incident edges first.
    pub fn graph_remove_node(&mut self, graph: GraphId, node: NodeId) -> Result<(), LayoutError> {
        if self.node(node).owner != Some(graph) {
            return Err(LayoutError("Owner graph is invalid!"));
        }

        // Remove the incident edges first, from a copy.
        let edges_to_be_removed = self.node(node).edges.clone();
        for edge in edges_to_be_removed {
            if self.edge(edge).is_inter_graph {
                self.remove_edge(edge)?;
            } else {
                let owner = self
                    .edge(edge)
                    .source
                    .and_then(|source| self.node(source).owner)
                    .ok_or(LayoutError("Owner graph is invalid!"))?;
                self.graph_remove_edge(owner, edge)?;
            }
        }

        // Now the node itself.
        let index = self
            .graph(graph)
            .nodes
            .iter()
            .position(|&candidate| candidate == node)
            .ok_or(LayoutError("Node not in owner node list!"))?;
        self.graph_mut(graph).nodes.remove(index);

        Ok(())
    }

    /// Upstream `LGraph.prototype.remove(edge)`.
    pub fn graph_remove_edge(&mut self, graph: GraphId, edge: EdgeId) -> Result<(), LayoutError> {
        let (source, target) = {
            let edge = self.edge(edge);
            (
                edge.source
                    .ok_or(LayoutError("Source and/or target is null!"))?,
                edge.target
                    .ok_or(LayoutError("Source and/or target is null!"))?,
            )
        };

        let source_owner = self.node(source).owner;
        let target_owner = self.node(target).owner;
        if !(source_owner.is_some()
            && target_owner.is_some()
            && source_owner == Some(graph)
            && target_owner == Some(graph))
        {
            return Err(LayoutError("Source and/or target owner is invalid!"));
        }

        let source_index = self
            .node(source)
            .edges
            .iter()
            .position(|&candidate| candidate == edge)
            .ok_or(LayoutError("Source and/or target doesn't know this edge!"))?;
        let target_index = self
            .node(target)
            .edges
            .iter()
            .position(|&candidate| candidate == edge)
            .ok_or(LayoutError("Source and/or target doesn't know this edge!"))?;

        self.node_mut(source).edges.remove(source_index);

        if target != source {
            self.node_mut(target).edges.remove(target_index);
        }

        let owner = self
            .node(source)
            .owner
            .ok_or(LayoutError("Source and/or target owner is invalid!"))?;
        let index = self
            .graph(owner)
            .edges
            .iter()
            .position(|&candidate| candidate == edge)
            .ok_or(LayoutError("Not in owner's edge list!"))?;
        self.graph_mut(owner).edges.remove(index);

        Ok(())
    }

    /// The margin of `graph`, upstream's `nodes[0].getParent().paddingLeft` falling back
    /// to the graph's own margin.
    fn graph_margin(&self, graph: GraphId) -> f64 {
        let graph = self.graph(graph);
        let parent = graph.nodes.first().and_then(|&node| self.node_parent(node));

        match parent {
            Some(parent) => self.node(parent).padding_left.unwrap_or(graph.margin),
            None => graph.margin,
        }
    }

    /// Upstream `LGraph.prototype.updateLeftTop`: the top-left of the node bounds with
    /// the margin, or `None` when the graph has no nodes.
    pub fn graph_update_left_top(&mut self, graph: GraphId) -> Option<Point> {
        let mut top = integer::MAX_VALUE;
        let mut left = integer::MAX_VALUE;

        let nodes = self.graph(graph).nodes.clone();
        for node in nodes {
            let node_top = self.node(node).get_top();
            let node_left = self.node(node).get_left();

            if top > node_top {
                top = node_top;
            }

            if left > node_left {
                left = node_left;
            }
        }

        // Is there any node in this graph?
        if top == integer::MAX_VALUE {
            return None;
        }

        let margin = self.graph_margin(graph);
        let graph = self.graph_mut(graph);
        graph.left = left - margin;
        graph.top = top - margin;

        // Apply the margins and return the result.
        Some(Point::new(graph.left, graph.top))
    }

    /// Upstream `LGraph.prototype.updateBounds`.
    pub fn graph_update_bounds(
        &mut self,
        graph: GraphId,
        recursive: bool,
    ) -> Result<(), LayoutError> {
        let nodes = self.graph(graph).nodes.clone();

        // Upstream writes the parent node's bounds for an empty graph and then reads
        // `nodes[0]`, which throws; the port keeps the parent-node copy and stops.
        if nodes.is_empty() {
            if let Some(parent) = self.graph(graph).parent {
                let (left, right, top, bottom) = {
                    let parent = self.node(parent);
                    (
                        parent.get_left(),
                        parent.get_right(),
                        parent.get_top(),
                        parent.get_bottom(),
                    )
                };
                let graph = self.graph_mut(graph);
                graph.left = left;
                graph.right = right;
                graph.top = top;
                graph.bottom = bottom;
            }
            return Ok(());
        }

        let mut left = integer::MAX_VALUE;
        let mut right = -integer::MAX_VALUE;
        let mut top = integer::MAX_VALUE;
        let mut bottom = -integer::MAX_VALUE;

        for node in nodes {
            if recursive && self.node(node).child.is_some() {
                self.node_update_bounds(node)?;
            }

            let node = self.node(node);
            let node_left = node.get_left();
            let node_right = node.get_right();
            let node_top = node.get_top();
            let node_bottom = node.get_bottom();

            if left > node_left {
                left = node_left;
            }

            if right < node_right {
                right = node_right;
            }

            if top > node_top {
                top = node_top;
            }

            if bottom < node_bottom {
                bottom = node_bottom;
            }
        }

        let bounding_rect = RectangleD::new(left, top, right - left, bottom - top);

        let margin = self.graph_margin(graph);
        let graph = self.graph_mut(graph);
        graph.left = bounding_rect.x - margin;
        graph.right = bounding_rect.x + bounding_rect.width + margin;
        graph.top = bounding_rect.y - margin;
        graph.bottom = bounding_rect.y + bounding_rect.height + margin;

        Ok(())
    }

    /// Upstream `LGraph.calculateBounds`: the bounds of `nodes` without the margin.
    pub fn calc_bounds(&self, nodes: &[NodeId]) -> RectangleD {
        let mut left = integer::MAX_VALUE;
        let mut right = -integer::MAX_VALUE;
        let mut top = integer::MAX_VALUE;
        let mut bottom = -integer::MAX_VALUE;

        for &node in nodes {
            let node = self.node(node);
            let node_left = node.get_left();
            let node_right = node.get_right();
            let node_top = node.get_top();
            let node_bottom = node.get_bottom();

            if left > node_left {
                left = node_left;
            }

            if right < node_right {
                right = node_right;
            }

            if top > node_top {
                top = node_top;
            }

            if bottom < node_bottom {
                bottom = node_bottom;
            }
        }

        RectangleD::new(left, top, right - left, bottom - top)
    }

    /// Upstream `LGraph.prototype.getInclusionTreeDepth`.
    pub fn graph_inclusion_tree_depth(&self, graph: GraphId) -> Result<i32, LayoutError> {
        if self.get_root() == Some(graph) {
            Ok(1)
        } else {
            let parent = self
                .graph(graph)
                .parent
                .ok_or(LayoutError("assert failed"))?;

            self.node(parent).get_inclusion_tree_depth()
        }
    }

    /// Upstream `LGraph.prototype.calcEstimatedSize`.
    pub fn graph_calc_estimated_size(&mut self, graph: GraphId) -> f64 {
        let mut size = 0.0;

        let nodes = self.graph(graph).nodes.clone();
        for node in nodes.iter() {
            size += self.node_calc_estimated_size(*node);
        }

        let estimated_size = if size == 0.0 {
            constants::EMPTY_COMPOUND_NODE_SIZE
        } else {
            size / (nodes.len() as f64).sqrt()
        };

        self.graph_mut(graph).estimated_size = Some(estimated_size);

        estimated_size
    }

    /// Upstream `LGraph.prototype.updateConnected`: breadth-first walk from the first
    /// node, across child graphs, marking the graph connected when every node was seen.
    pub fn graph_update_connected(&mut self, graph: GraphId) {
        let nodes = self.graph(graph).nodes.clone();
        if nodes.is_empty() {
            self.graph_mut(graph).is_connected = true;
            return;
        }

        let mut queue = LinkedList::new();
        let mut visited: Vec<NodeId> = Vec::new();
        let current_node = nodes[0];
        for node in self.node_with_children(current_node) {
            queue.push(node);
            if !visited.contains(&node) {
                visited.push(node);
            }
        }

        while let Some(current_node) = queue.shift() {
            // Traverse all neighbors of this node.
            let neighbor_edges = self.node(current_node).get_edges().to_vec();
            for neighbor_edge in neighbor_edges {
                let current_neighbor =
                    self.edge_other_end_in_graph(neighbor_edge, current_node, graph);

                // Add unvisited neighbors to the list to visit.
                if let Some(current_neighbor) = current_neighbor
                    && !visited.contains(&current_neighbor)
                {
                    for node in self.node_with_children(current_neighbor) {
                        queue.push(node);
                        if !visited.contains(&node) {
                            visited.push(node);
                        }
                    }
                }
            }
        }

        self.graph_mut(graph).is_connected = false;

        if visited.len() >= nodes.len() {
            let mut no_of_visited_in_this_graph = 0;

            for visited_node in &visited {
                if self.node(*visited_node).owner == Some(graph) {
                    no_of_visited_in_this_graph += 1;
                }
            }

            if no_of_visited_in_this_graph == nodes.len() {
                self.graph_mut(graph).is_connected = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::layout_base::util::dimension_d::DimensionD;

    fn manager() -> (LGraphManager<()>, GraphId) {
        let mut manager = LGraphManager::new();
        let root = manager.add_root();
        (manager, root)
    }

    #[test]
    fn adds_nodes_and_edges() {
        let (mut manager, root) = manager();
        let a = manager.new_node(None);
        let b = manager.new_node(None);

        assert_eq!(manager.graph_add_node(root, a), Ok(a));
        assert_eq!(manager.graph_add_node(root, b), Ok(b));
        assert_eq!(
            manager.graph_add_node(root, a).unwrap_err(),
            LayoutError("Node already in graph!")
        );
        assert_eq!(manager.graph(root).nodes, vec![a, b]);
        assert_eq!(manager.node(a).owner, Some(root));

        let edge = manager.new_edge(None);
        assert_eq!(manager.add_edge(edge, a, b), Ok(edge));
        assert!(!manager.edge(edge).is_inter_graph());
        assert_eq!(manager.graph(root).edges, vec![edge]);
        assert_eq!(manager.node(a).edges, vec![edge]);
        assert_eq!(manager.node(b).edges, vec![edge]);
    }

    #[test]
    fn edge_needs_both_endpoints_in_a_graph() {
        let (mut manager, root) = manager();
        let a = manager.new_node(None);
        let b = manager.new_node(None);
        manager.graph_add_node(root, a).unwrap();

        // The manager needs both endpoints to have an owner.
        let edge = manager.new_edge(None);
        assert_eq!(
            manager.add_edge(edge, a, b).unwrap_err(),
            LayoutError("Target not in this graph mgr!")
        );

        // The graph needs both endpoints among its nodes.
        let other = manager.new_graph(None);
        let edge = manager.new_edge(None);
        assert_eq!(
            manager.graph_add_edge(other, edge, a, b).unwrap_err(),
            LayoutError("Source or target not in graph!")
        );
    }

    #[test]
    fn removing_a_node_removes_its_edges() {
        let (mut manager, root) = manager();
        let a = manager.new_node(None);
        let b = manager.new_node(None);
        let c = manager.new_node(None);
        for node in [a, b, c] {
            manager.graph_add_node(root, node).unwrap();
        }
        let ab = manager.new_edge(None);
        let bc = manager.new_edge(None);
        manager.add_edge(ab, a, b).unwrap();
        manager.add_edge(bc, b, c).unwrap();

        manager.graph_remove_node(root, b).unwrap();

        assert_eq!(manager.graph(root).nodes, vec![a, c]);
        assert!(manager.graph(root).edges.is_empty());
        assert!(manager.node(a).edges.is_empty());
        assert!(manager.node(b).edges.is_empty());
        assert!(manager.node(c).edges.is_empty());
        assert!(manager.node(b).owner.is_some());
    }

    #[test]
    fn removing_an_edge_updates_both_ends() {
        let (mut manager, root) = manager();
        let a = manager.new_node(None);
        let b = manager.new_node(None);
        manager.graph_add_node(root, a).unwrap();
        manager.graph_add_node(root, b).unwrap();
        let edge = manager.new_edge(None);
        manager.add_edge(edge, a, b).unwrap();

        manager.graph_remove_edge(root, edge).unwrap();

        assert!(manager.graph(root).edges.is_empty());
        assert!(manager.node(a).edges.is_empty());
        assert!(manager.node(b).edges.is_empty());
        assert_eq!(
            manager.graph_remove_edge(root, edge).unwrap_err(),
            LayoutError("Source and/or target doesn't know this edge!")
        );
    }

    #[test]
    fn update_left_top_applies_the_margin() {
        let (mut manager, root) = manager();
        let a = manager.new_node(None);
        manager.graph_add_node(root, a).unwrap();
        manager
            .node_mut(a)
            .set_rect(&Point::new(10.0, 20.0), &DimensionD::new(30.0, 40.0));

        assert_eq!(
            manager.graph_update_left_top(root),
            Some(Point::new(-5.0, 5.0))
        );
        assert_eq!(manager.graph(root).left, -5.0);
        assert_eq!(manager.graph(root).top, 5.0);

        let empty = manager.new_graph(None);
        assert_eq!(manager.graph_update_left_top(empty), None);
    }

    #[test]
    fn update_bounds_wraps_the_nodes() {
        let (mut manager, root) = manager();
        let a = manager.new_node(None);
        let b = manager.new_node(None);
        manager.graph_add_node(root, a).unwrap();
        manager.graph_add_node(root, b).unwrap();
        manager
            .node_mut(a)
            .set_rect(&Point::new(0.0, 0.0), &DimensionD::new(10.0, 20.0));
        manager
            .node_mut(b)
            .set_rect(&Point::new(100.0, 50.0), &DimensionD::new(30.0, 40.0));

        manager.graph_update_bounds(root, true).unwrap();

        let graph = manager.graph(root);
        assert_eq!((graph.left, graph.right), (-15.0, 145.0));
        assert_eq!((graph.top, graph.bottom), (-15.0, 105.0));
    }

    #[test]
    fn calc_bounds_does_not_add_the_margin() {
        let (mut manager, root) = manager();
        let a = manager.new_node(None);
        let b = manager.new_node(None);
        manager.graph_add_node(root, a).unwrap();
        manager.graph_add_node(root, b).unwrap();
        manager
            .node_mut(a)
            .set_rect(&Point::new(0.0, 0.0), &DimensionD::new(10.0, 20.0));
        manager
            .node_mut(b)
            .set_rect(&Point::new(100.0, 50.0), &DimensionD::new(30.0, 40.0));

        assert_eq!(
            manager.calc_bounds(&[a, b]),
            RectangleD::new(0.0, 0.0, 130.0, 90.0)
        );
    }

    #[test]
    fn estimated_size_of_a_leaf_graph() {
        let (mut manager, root) = manager();
        assert_eq!(
            manager.graph_calc_estimated_size(root),
            constants::EMPTY_COMPOUND_NODE_SIZE
        );

        let a = manager.new_node(None);
        let b = manager.new_node(None);
        manager.graph_add_node(root, a).unwrap();
        manager.graph_add_node(root, b).unwrap();
        manager
            .node_mut(a)
            .set_rect(&Point::new(0.0, 0.0), &DimensionD::new(10.0, 20.0));
        manager
            .node_mut(b)
            .set_rect(&Point::new(0.0, 0.0), &DimensionD::new(30.0, 50.0));

        // (10 + 20) / 2 + (30 + 50) / 2 = 55, over sqrt(2) nodes.
        assert_eq!(
            manager.graph_calc_estimated_size(root),
            55.0 / 2.0f64.sqrt()
        );
        assert_eq!(
            manager.graph(root).get_estimated_size(),
            Ok(55.0 / 2.0f64.sqrt())
        );
    }

    #[test]
    fn update_connected_walks_the_edges() {
        let (mut manager, root) = manager();
        let a = manager.new_node(None);
        let b = manager.new_node(None);
        let c = manager.new_node(None);
        for node in [a, b, c] {
            manager.graph_add_node(root, node).unwrap();
        }
        let edge = manager.new_edge(None);
        manager.add_edge(edge, a, b).unwrap();

        manager.graph_update_connected(root);
        assert!(!manager.graph(root).is_connected());

        let second = manager.new_edge(None);
        manager.add_edge(second, b, c).unwrap();

        manager.graph_update_connected(root);
        assert!(manager.graph(root).is_connected());
    }
}
