//! Port of `layout-base/src/LGraphManager.js`.
//!
//! The graph manager owns every graph, node, and edge of one layout and the operations
//! that span them. Upstream keeps a back-reference to its layout, which creates graphs
//! and nodes for it; here the manager owns the arenas and creates them itself
//! ([`LGraphManager::new_graph`], [`LGraphManager::new_node`],
//! [`LGraphManager::new_edge`], upstream's `Layout.prototype.newGraph` and friends).
//!
//! Upstream's `add` and `remove` are overloaded (a graph with two arguments, an edge
//! with five); the port names the halves [`LGraphManager::add_graph`] and
//! [`LGraphManager::add_edge`], and [`LGraphManager::remove_graph`] and
//! [`LGraphManager::remove_edge`].
//!
//! `getAllNodes`, `getAllEdges`, and `getAllNodesToApplyGravitation` cache their result
//! upstream; the port caches too and returns a copy, so callers can keep mutating the
//! manager. `setRootGraph(null)` (removing the root graph) throws upstream because it
//! dereferences `null`; the port clears the root instead.

use super::LayoutError;
use super::l_edge::{EdgeId, LEdge};
use super::l_graph::{GraphId, LGraph};
use super::l_graph_object::LGraphObject;
use super::l_node::{LNode, NodeId};

/// The graphs, nodes, and edges of one layout, upstream's `LGraphManager`.
pub struct LGraphManager<V> {
    graphs: Vec<GraphId>,
    edges: Vec<EdgeId>,
    nodes: Vec<LNode<V>>,
    edge_store: Vec<LEdge<V>>,
    graph_store: Vec<LGraph<V>>,
    root_graph: Option<GraphId>,
    all_nodes: Option<Vec<NodeId>>,
    all_edges: Option<Vec<EdgeId>>,
    all_nodes_to_apply_gravitation: Option<Vec<NodeId>>,
}

impl<V> Default for LGraphManager<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> LGraphManager<V> {
    /// An empty manager.
    pub fn new() -> Self {
        Self {
            graphs: Vec::new(),
            edges: Vec::new(),
            nodes: Vec::new(),
            edge_store: Vec::new(),
            graph_store: Vec::new(),
            root_graph: None,
            all_nodes: None,
            all_edges: None,
            all_nodes_to_apply_gravitation: None,
        }
    }

    /// The node behind a [`NodeId`].
    pub fn node(&self, id: NodeId) -> &LNode<V> {
        &self.nodes[id.0]
    }

    /// The node behind a [`NodeId`], mutably.
    pub fn node_mut(&mut self, id: NodeId) -> &mut LNode<V> {
        &mut self.nodes[id.0]
    }

    /// The edge behind an [`EdgeId`].
    pub fn edge(&self, id: EdgeId) -> &LEdge<V> {
        &self.edge_store[id.0]
    }

    /// The edge behind an [`EdgeId`], mutably.
    pub fn edge_mut(&mut self, id: EdgeId) -> &mut LEdge<V> {
        &mut self.edge_store[id.0]
    }

    /// The graph behind a [`GraphId`].
    pub fn graph(&self, id: GraphId) -> &LGraph<V> {
        &self.graph_store[id.0]
    }

    /// The graph behind a [`GraphId`], mutably.
    pub fn graph_mut(&mut self, id: GraphId) -> &mut LGraph<V> {
        &mut self.graph_store[id.0]
    }

    /// How many nodes the manager owns.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// How many edges the manager owns.
    pub fn edge_count(&self) -> usize {
        self.edge_store.len()
    }

    /// How many graphs the manager owns.
    pub fn graph_count(&self) -> usize {
        self.graph_store.len()
    }

    /// Upstream `Layout.prototype.newGraph`: a graph that belongs to no manager list
    /// until [`LGraphManager::add_graph`] adds it.
    pub fn new_graph(&mut self, v_graph: Option<V>) -> GraphId {
        let id = GraphId(self.graph_store.len());
        self.graph_store.push(LGraph::new(v_graph));
        id
    }

    /// Upstream `Layout.prototype.newNode`.
    pub fn new_node(&mut self, v_node: Option<V>) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(LNode {
            base: LGraphObject::new(v_node),
            ..LNode::default()
        });
        id
    }

    /// Upstream `Layout.prototype.newEdge`.
    pub fn new_edge(&mut self, v_edge: Option<V>) -> EdgeId {
        let id = EdgeId(self.edge_store.len());
        self.edge_store.push(LEdge::new(None, None, v_edge));
        id
    }

    /// Upstream `LGraphManager.prototype.addRoot`.
    pub fn add_root(&mut self) -> GraphId {
        let ngraph = self.new_graph(None);
        let nnode = self.new_node(None);
        let root = self
            .add_graph(ngraph, nnode)
            .expect("a fresh graph and node are valid");
        self.set_root_graph(root);
        root
    }

    /// Upstream `LGraphManager.prototype.add(graph, parentNode)`. Upstream pushes the
    /// graph into its list before validating the parent and the child, so a failing call
    /// still leaves the graph in the list; the port does the same.
    pub fn add_graph(
        &mut self,
        new_graph: GraphId,
        parent_node: NodeId,
    ) -> Result<GraphId, LayoutError> {
        if self.graphs.contains(&new_graph) {
            return Err(LayoutError("Graph already in this graph mgr!"));
        }

        self.graphs.push(new_graph);

        if self.graph(new_graph).parent.is_some() {
            return Err(LayoutError("Already has a parent!"));
        }
        if self.node(parent_node).child.is_some() {
            return Err(LayoutError("Already has a child!"));
        }

        self.graph_mut(new_graph).parent = Some(parent_node);
        self.node_mut(parent_node).child = Some(new_graph);

        Ok(new_graph)
    }

    /// Upstream `LGraphManager.prototype.add(edge, sourceNode, targetNode)`: an edge
    /// inside one graph is added there, an edge across graphs becomes inter-graph.
    pub fn add_edge(
        &mut self,
        edge: EdgeId,
        source: NodeId,
        target: NodeId,
    ) -> Result<EdgeId, LayoutError> {
        let source_graph = self
            .node(source)
            .owner
            .ok_or(LayoutError("Source not in this graph mgr!"))?;
        let target_graph = self
            .node(target)
            .owner
            .ok_or(LayoutError("Target not in this graph mgr!"))?;
        // Upstream also checks that each owner graph's manager is this one, which the
        // arena guarantees.

        if source_graph == target_graph {
            self.edge_mut(edge).is_inter_graph = false;
            return self.graph_add_edge(source_graph, edge, source, target);
        }

        {
            let edge = self.edge_mut(edge);
            edge.is_inter_graph = true;
            edge.source = Some(source);
            edge.target = Some(target);
        }

        if self.edges.contains(&edge) {
            return Err(LayoutError("Edge already in inter-graph edge list!"));
        }

        self.edges.push(edge);

        if self.edge(edge).source.is_none() || self.edge(edge).target.is_none() {
            return Err(LayoutError("Edge source and/or target is null!"));
        }

        if self.node(source).edges.contains(&edge) || self.node(target).edges.contains(&edge) {
            return Err(LayoutError(
                "Edge already in source and/or target incidency list!",
            ));
        }

        self.node_mut(source).edges.push(edge);
        self.node_mut(target).edges.push(edge);

        Ok(edge)
    }

    /// Upstream `LGraphManager.prototype.remove(graph)`.
    pub fn remove_graph(&mut self, graph: GraphId) -> Result<(), LayoutError> {
        let is_root = self.get_root() == Some(graph);
        if !(is_root || self.graph(graph).parent.is_some()) {
            return Err(LayoutError("Invalid parent node!"));
        }

        // First the edges, then the nodes, from copies.
        let edges_to_be_removed = self.graph(graph).edges.clone();
        for edge in edges_to_be_removed {
            self.graph_remove_edge(graph, edge)?;
        }

        let nodes_to_be_removed = self.graph(graph).nodes.clone();
        for node in nodes_to_be_removed {
            self.graph_remove_node(graph, node)?;
        }

        // Upstream calls `setRootGraph(null)` here, which throws; the port clears it.
        if is_root {
            self.root_graph = None;
        }

        if let Some(index) = self.graphs.iter().position(|&candidate| candidate == graph) {
            self.graphs.remove(index);
        }

        self.graph_mut(graph).parent = None;

        Ok(())
    }

    /// Upstream `LGraphManager.prototype.remove(edge)` for an inter-graph edge.
    pub fn remove_edge(&mut self, edge: EdgeId) -> Result<(), LayoutError> {
        if !self.edge(edge).is_inter_graph {
            return Err(LayoutError("Not an inter-graph edge!"));
        }

        let (source, target) = {
            let edge = self.edge(edge);
            (
                edge.source
                    .ok_or(LayoutError("Source and/or target is null!"))?,
                edge.target
                    .ok_or(LayoutError("Source and/or target is null!"))?,
            )
        };

        if !(self.node(source).edges.contains(&edge) && self.node(target).edges.contains(&edge)) {
            return Err(LayoutError("Source and/or target doesn't know this edge!"));
        }

        if let Some(index) = self.node(source).edges.iter().position(|&e| e == edge) {
            self.node_mut(source).edges.remove(index);
        }
        if let Some(index) = self.node(target).edges.iter().position(|&e| e == edge) {
            self.node_mut(target).edges.remove(index);
        }

        if self.node(source).owner.is_none() {
            return Err(LayoutError(
                "Edge owner graph or owner graph manager is null!",
            ));
        }
        // Upstream reads the manager's edge list through the owner graph; the manager is
        // this one.
        if !self.edges.contains(&edge) {
            return Err(LayoutError("Not in owner graph manager's edge list!"));
        }

        if let Some(index) = self.edges.iter().position(|&e| e == edge) {
            self.edges.remove(index);
        }

        Ok(())
    }

    /// Upstream `LGraphManager.prototype.updateBounds`.
    pub fn update_bounds(&mut self) -> Result<(), LayoutError> {
        match self.get_root() {
            Some(root) => self.graph_update_bounds(root, true),
            None => Ok(()),
        }
    }

    /// Upstream `LGraphManager.prototype.getGraphs`.
    pub fn get_graphs(&self) -> &[GraphId] {
        &self.graphs
    }

    /// Upstream `LGraphManager.prototype.getAllNodes`: every graph's nodes in graph
    /// order. The result is cached until [`LGraphManager::reset_all_nodes`].
    pub fn get_all_nodes(&mut self) -> Vec<NodeId> {
        if self.all_nodes.is_none() {
            let mut node_list = Vec::new();
            let graphs = self.graphs.clone();
            for graph in graphs {
                node_list.extend_from_slice(&self.graph(graph).nodes);
            }
            self.all_nodes = Some(node_list);
        }

        self.all_nodes.clone().unwrap_or_default()
    }

    /// Upstream `LGraphManager.prototype.resetAllNodes`.
    pub fn reset_all_nodes(&mut self) {
        self.all_nodes = None;
    }

    /// Upstream `LGraphManager.prototype.resetAllEdges`.
    pub fn reset_all_edges(&mut self) {
        self.all_edges = None;
    }

    /// Upstream `LGraphManager.prototype.resetAllNodesToApplyGravitation`.
    pub fn reset_all_nodes_to_apply_gravitation(&mut self) {
        self.all_nodes_to_apply_gravitation = None;
    }

    /// Upstream `LGraphManager.prototype.getAllEdges`: every graph's edges in graph
    /// order, then the inter-graph edges. Cached until
    /// [`LGraphManager::reset_all_edges`].
    pub fn get_all_edges(&mut self) -> Vec<EdgeId> {
        if self.all_edges.is_none() {
            let mut edge_list = Vec::new();
            let graphs = self.graphs.clone();
            for graph in graphs {
                edge_list.extend_from_slice(&self.graph(graph).edges);
            }

            edge_list.extend_from_slice(&self.edges);

            self.all_edges = Some(edge_list);
        }

        self.all_edges.clone().unwrap_or_default()
    }

    /// Upstream `LGraphManager.prototype.getAllNodesToApplyGravitation`.
    pub fn get_all_nodes_to_apply_gravitation(&self) -> Option<Vec<NodeId>> {
        self.all_nodes_to_apply_gravitation.clone()
    }

    /// Upstream `LGraphManager.prototype.setAllNodesToApplyGravitation`.
    pub fn set_all_nodes_to_apply_gravitation(
        &mut self,
        node_list: Vec<NodeId>,
    ) -> Result<(), LayoutError> {
        if self.all_nodes_to_apply_gravitation.is_some() {
            return Err(LayoutError("assert failed"));
        }

        self.all_nodes_to_apply_gravitation = Some(node_list);

        Ok(())
    }

    /// Upstream `LGraphManager.prototype.getRoot`.
    pub fn get_root(&self) -> Option<GraphId> {
        self.root_graph
    }

    /// Upstream `LGraphManager.prototype.setRootGraph`. Upstream throws when the graph
    /// belongs to another manager, which the arena makes impossible, and tags a graph
    /// without a parent with a node whose v-level object is the string `"Root node"`;
    /// the port creates that node without a v-level object, since `V` is the caller's.
    pub fn set_root_graph(&mut self, graph: GraphId) {
        self.root_graph = Some(graph);

        // The root graph needs a root node for convenience.
        if self.graph(graph).parent.is_none() {
            let root_node = self.new_node(None);
            self.graph_mut(graph).parent = Some(root_node);
        }
    }

    /// Upstream `LGraphManager.prototype.isOneAncestorOfOther`.
    pub fn is_one_ancestor_of_other(&self, first_node: NodeId, second_node: NodeId) -> bool {
        if first_node == second_node {
            return true;
        }

        // Is the second node an ancestor of the first one?
        let mut owner_graph = self.node(first_node).owner;
        loop {
            let Some(graph) = owner_graph else {
                break;
            };
            let Some(parent_node) = self.graph(graph).parent else {
                break;
            };

            if parent_node == second_node {
                return true;
            }

            owner_graph = self.node(parent_node).owner;
        }

        // Is the first node an ancestor of the second one?
        owner_graph = self.node(second_node).owner;
        loop {
            let Some(graph) = owner_graph else {
                break;
            };
            let Some(parent_node) = self.graph(graph).parent else {
                break;
            };

            if parent_node == first_node {
                return true;
            }

            owner_graph = self.node(parent_node).owner;
        }

        false
    }

    /// Upstream `LGraphManager.prototype.calcLowestCommonAncestors`.
    pub fn calc_lowest_common_ancestors(&mut self) -> Result<(), LayoutError> {
        let edges = self.get_all_edges();
        for edge in edges {
            let (source_node, target_node) = {
                let edge = self.edge(edge);
                (
                    edge.source
                        .ok_or(LayoutError("Source and/or target is null!"))?,
                    edge.target
                        .ok_or(LayoutError("Source and/or target is null!"))?,
                )
            };

            let mut lca = None;
            let mut source_in_lca = Some(source_node);
            let mut target_in_lca = Some(target_node);

            if source_node == target_node {
                lca = self.node(source_node).owner;
            } else {
                let mut source_ancestor_graph = self.node(source_node).owner;

                while lca.is_none() {
                    target_in_lca = Some(target_node);
                    let mut target_ancestor_graph = self.node(target_node).owner;

                    loop {
                        if target_ancestor_graph == source_ancestor_graph {
                            lca = target_ancestor_graph;
                            break;
                        }

                        if target_ancestor_graph == self.get_root() {
                            break;
                        }

                        let Some(target_graph) = target_ancestor_graph else {
                            break;
                        };

                        target_in_lca = self.graph(target_graph).parent;
                        let Some(target_node_in_lca) = target_in_lca else {
                            break;
                        };
                        target_ancestor_graph = self.node(target_node_in_lca).owner;
                    }

                    if source_ancestor_graph == self.get_root() {
                        break;
                    }

                    if lca.is_none() {
                        let Some(source_graph) = source_ancestor_graph else {
                            break;
                        };

                        source_in_lca = self.graph(source_graph).parent;
                        let Some(source_node_in_lca) = source_in_lca else {
                            break;
                        };
                        source_ancestor_graph = self.node(source_node_in_lca).owner;
                    }
                }
            }

            if lca.is_none() {
                return Err(LayoutError("assert failed"));
            }

            let edge = self.edge_mut(edge);
            edge.lca = lca;
            edge.source_in_lca = source_in_lca;
            edge.target_in_lca = target_in_lca;
        }

        Ok(())
    }

    /// Upstream `LGraphManager.prototype.calcLowestCommonAncestor`.
    pub fn calc_lowest_common_ancestor(
        &self,
        first_node: NodeId,
        second_node: NodeId,
    ) -> Option<GraphId> {
        if first_node == second_node {
            return self.node(first_node).owner;
        }

        let mut first_owner_graph = self.node(first_node).owner;

        loop {
            let Some(first_graph) = first_owner_graph else {
                break;
            };

            let mut second_owner_graph = self.node(second_node).owner;

            loop {
                let Some(second_graph) = second_owner_graph else {
                    break;
                };

                if second_graph == first_graph {
                    return Some(second_graph);
                }

                second_owner_graph = self
                    .graph(second_graph)
                    .parent
                    .and_then(|parent| self.node(parent).owner);
            }

            first_owner_graph = self
                .graph(first_graph)
                .parent
                .and_then(|parent| self.node(parent).owner);
        }

        first_owner_graph
    }

    /// Upstream `LGraphManager.prototype.calcInclusionTreeDepths`, which takes `graph`
    /// and `depth` and defaults both when they are `null`.
    pub fn calc_inclusion_tree_depths(&mut self) {
        if let Some(root) = self.get_root() {
            self.calc_inclusion_tree_depths_from(root, 1);
        }
    }

    fn calc_inclusion_tree_depths_from(&mut self, graph: GraphId, depth: i32) {
        let nodes = self.graph(graph).nodes.clone();
        for node in nodes {
            self.node_mut(node).inclusion_tree_depth = Some(depth);

            if let Some(child) = self.node(node).child {
                self.calc_inclusion_tree_depths_from(child, depth + 1);
            }
        }
    }

    /// Upstream `LGraphManager.prototype.includesInvalidEdge`: whether an inter-graph
    /// edge joins a node with one of its ancestors.
    pub fn includes_invalid_edge(&self) -> bool {
        for &edge in &self.edges {
            let edge = self.edge(edge);
            if let (Some(source), Some(target)) = (edge.source, edge.target)
                && self.is_one_ancestor_of_other(source, target)
            {
                return true;
            }
        }

        false
    }

    /// Layout-base 2.0.1's `LGraphManager.prototype.includesInvalidEdge`: the same test as
    /// [`LGraphManager::includes_invalid_edge`], but every invalid edge is removed from
    /// the manager instead of only reported, and the method always reports that none are
    /// left.
    ///
    /// Upstream `LGraphManager.remove` removes an inter-graph edge through
    /// [`LGraphManager::remove_edge`] and an intra-graph edge through its owner graph;
    /// both paths are kept.
    pub fn remove_invalid_edges(&mut self) -> Result<(), LayoutError> {
        let mut edges_to_remove = Vec::new();

        for &edge in &self.edges {
            let (source, target) = {
                let edge = self.edge(edge);
                (edge.source, edge.target)
            };
            if let (Some(source), Some(target)) = (source, target)
                && self.is_one_ancestor_of_other(source, target)
            {
                edges_to_remove.push(edge);
            }
        }

        for edge in edges_to_remove {
            if self.edge(edge).is_inter_graph {
                self.remove_edge(edge)?;
            } else {
                let owner = self
                    .edge(edge)
                    .source
                    .and_then(|source| self.node(source).owner)
                    .ok_or(LayoutError(
                        "Edge owner graph or owner graph manager is null!",
                    ))?;
                self.graph_remove_edge(owner, edge)?;
                if let Some(index) = self.edges.iter().position(|&candidate| candidate == edge) {
                    self.edges.remove(index);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::layout_base::util::dimension_d::DimensionD;
    use crate::layout::layout_base::util::point::Point;

    fn manager() -> (LGraphManager<()>, GraphId) {
        let mut manager = LGraphManager::new();
        let root = manager.add_root();
        (manager, root)
    }

    fn add_node(manager: &mut LGraphManager<()>, graph: GraphId, x: f64, y: f64) -> NodeId {
        let node = manager.new_node(None);
        manager.graph_add_node(graph, node).unwrap();
        manager
            .node_mut(node)
            .set_rect(&Point::new(x, y), &DimensionD::new(10.0, 10.0));
        node
    }

    #[test]
    fn add_root_creates_a_root_node() {
        let (manager, root) = manager();
        assert_eq!(manager.get_root(), Some(root));
        assert_eq!(manager.get_graphs(), [root].as_slice());
        let parent = manager.graph(root).parent.unwrap();
        assert_eq!(manager.node(parent).child, Some(root));
        assert!(manager.node(parent).owner.is_none());
    }

    #[test]
    fn all_nodes_keep_graph_order_and_are_cached() {
        let (mut manager, root) = manager();
        let a = add_node(&mut manager, root, 0.0, 0.0);
        let b = add_node(&mut manager, root, 100.0, 0.0);

        assert_eq!(manager.get_all_nodes(), vec![a, b]);

        let c = add_node(&mut manager, root, 200.0, 0.0);
        assert_eq!(manager.get_all_nodes(), vec![a, b]);
        manager.reset_all_nodes();
        assert_eq!(manager.get_all_nodes(), vec![a, b, c]);
    }

    #[test]
    fn all_nodes_of_nested_graphs_follow_graph_insertion_order() {
        let (mut manager, root) = manager();
        let compound = add_node(&mut manager, root, 0.0, 0.0);
        let sibling = add_node(&mut manager, root, 100.0, 0.0);

        let child_graph = manager.new_graph(None);
        manager.add_graph(child_graph, compound).unwrap();
        let inside = add_node(&mut manager, child_graph, 0.0, 0.0);

        manager.reset_all_nodes();
        assert_eq!(
            manager.get_all_nodes(),
            vec![compound, sibling, inside],
            "root graph nodes first, then the child graph's"
        );
    }

    #[test]
    fn inclusion_tree_depths() {
        let (mut manager, root) = manager();
        let compound = add_node(&mut manager, root, 0.0, 0.0);
        let child_graph = manager.new_graph(None);
        manager.add_graph(child_graph, compound).unwrap();
        let inside = add_node(&mut manager, child_graph, 0.0, 0.0);

        manager.calc_inclusion_tree_depths();

        assert_eq!(manager.node(compound).get_inclusion_tree_depth(), Ok(1));
        assert_eq!(manager.node(inside).get_inclusion_tree_depth(), Ok(2));
        assert_eq!(manager.graph_inclusion_tree_depth(root), Ok(1));
        assert_eq!(manager.graph_inclusion_tree_depth(child_graph), Ok(1));
    }

    #[test]
    fn add_graph_validates_parent_and_child() {
        let (mut manager, root) = manager();
        let compound = add_node(&mut manager, root, 0.0, 0.0);
        let child_graph = manager.new_graph(None);
        manager.add_graph(child_graph, compound).unwrap();

        let second_graph = manager.new_graph(None);
        assert_eq!(
            manager.add_graph(second_graph, compound).unwrap_err(),
            LayoutError("Already has a child!")
        );

        assert_eq!(
            manager.add_graph(child_graph, compound).unwrap_err(),
            LayoutError("Graph already in this graph mgr!")
        );
    }

    #[test]
    fn inter_graph_edges_and_lowest_common_ancestors() {
        let (mut manager, root) = manager();
        let compound = add_node(&mut manager, root, 0.0, 0.0);
        let sibling = add_node(&mut manager, root, 100.0, 0.0);
        let child_graph = manager.new_graph(None);
        manager.add_graph(child_graph, compound).unwrap();
        let inside = add_node(&mut manager, child_graph, 0.0, 0.0);

        let edge = manager.new_edge(None);
        manager.add_edge(edge, inside, sibling).unwrap();

        assert!(manager.edge(edge).is_inter_graph());
        assert_eq!(manager.edge(edge).source, Some(inside));
        assert_eq!(manager.edge(edge).target, Some(sibling));
        assert_eq!(manager.node(inside).edges, vec![edge]);
        assert_eq!(manager.node(sibling).edges, vec![edge]);

        manager.calc_lowest_common_ancestors().unwrap();
        {
            let edge = manager.edge(edge);
            assert_eq!(edge.get_lca(), Some(root));
            assert_eq!(edge.get_source_in_lca(), Some(compound));
            assert_eq!(edge.get_target_in_lca(), Some(sibling));
        }

        assert_eq!(
            manager.calc_lowest_common_ancestor(inside, sibling),
            Some(root)
        );

        manager.remove_edge(edge).unwrap();
        assert!(manager.node(inside).edges.is_empty());
        assert!(manager.node(sibling).edges.is_empty());
    }

    #[test]
    fn intra_graph_edge_gets_its_owner_as_lca() {
        let (mut manager, root) = manager();
        let a = add_node(&mut manager, root, 0.0, 0.0);
        let b = add_node(&mut manager, root, 100.0, 0.0);
        let edge = manager.new_edge(None);
        manager.add_edge(edge, a, b).unwrap();

        manager.calc_lowest_common_ancestors().unwrap();

        assert_eq!(manager.edge(edge).get_lca(), Some(root));
        assert_eq!(manager.edge(edge).get_source_in_lca(), Some(a));
        assert_eq!(manager.edge(edge).get_target_in_lca(), Some(b));
    }

    #[test]
    fn self_loop_lca_is_the_owner() {
        let (mut manager, root) = manager();
        let a = add_node(&mut manager, root, 0.0, 0.0);
        let edge = manager.new_edge(None);
        manager.add_edge(edge, a, a).unwrap();

        manager.calc_lowest_common_ancestors().unwrap();

        assert_eq!(manager.edge(edge).get_lca(), Some(root));
    }

    #[test]
    fn ancestors() {
        let (mut manager, root) = manager();
        let compound = add_node(&mut manager, root, 0.0, 0.0);
        let sibling = add_node(&mut manager, root, 100.0, 0.0);
        let child_graph = manager.new_graph(None);
        manager.add_graph(child_graph, compound).unwrap();
        let inside = add_node(&mut manager, child_graph, 0.0, 0.0);

        assert!(manager.is_one_ancestor_of_other(compound, inside));
        assert!(manager.is_one_ancestor_of_other(inside, compound));
        assert!(manager.is_one_ancestor_of_other(compound, compound));
        assert!(!manager.is_one_ancestor_of_other(compound, sibling));
    }

    #[test]
    fn invalid_inter_graph_edges_join_ancestors() {
        let (mut manager, root) = manager();
        let compound = add_node(&mut manager, root, 0.0, 0.0);
        let child_graph = manager.new_graph(None);
        manager.add_graph(child_graph, compound).unwrap();
        let inside = add_node(&mut manager, child_graph, 0.0, 0.0);

        assert!(!manager.includes_invalid_edge());

        let edge = manager.new_edge(None);
        manager.add_edge(edge, compound, inside).unwrap();
        assert!(manager.includes_invalid_edge());
    }

    #[test]
    fn removing_a_graph_removes_its_contents() {
        let (mut manager, root) = manager();
        let compound = add_node(&mut manager, root, 0.0, 0.0);
        let child_graph = manager.new_graph(None);
        manager.add_graph(child_graph, compound).unwrap();
        let inside = add_node(&mut manager, child_graph, 0.0, 0.0);
        let outside = add_node(&mut manager, root, 100.0, 0.0);
        let edge = manager.new_edge(None);
        manager.add_edge(edge, inside, outside).unwrap();

        manager.remove_graph(child_graph).unwrap();

        assert_eq!(manager.get_graphs(), [root].as_slice());
        assert!(manager.graph(child_graph).nodes.is_empty());
        // Upstream clears the removed graph's parent but leaves the parent node's
        // `child` pointing at it.
        assert_eq!(manager.node(compound).child, Some(child_graph));
        assert_eq!(manager.graph(child_graph).parent, None);
        assert!(manager.node(inside).edges.is_empty());
        assert!(manager.node(outside).edges.is_empty());
        assert!(manager.get_all_edges().is_empty());
    }

    #[test]
    fn update_bounds_wraps_compound_nodes() {
        let (mut manager, root) = manager();
        let compound = add_node(&mut manager, root, 0.0, 0.0);
        let child_graph = manager.new_graph(None);
        manager.add_graph(child_graph, compound).unwrap();
        let inside = add_node(&mut manager, child_graph, 0.0, 0.0);

        manager.update_bounds().unwrap();

        // The child graph wraps its node with the 15pt margin, and the compound node
        // takes the child graph's bounds.
        let node = manager.node(compound);
        assert_eq!((node.get_left(), node.get_top()), (-15.0, -15.0));
        assert_eq!((node.get_width(), node.get_height()), (40.0, 40.0));

        let graph = manager.graph(root);
        assert_eq!((graph.left, graph.right), (-30.0, 40.0));
        assert_eq!((graph.top, graph.bottom), (-30.0, 40.0));

        assert_eq!(manager.node(inside).get_left(), 0.0);
    }

    #[test]
    fn padding_left_of_the_parent_replaces_the_margin() {
        let (mut manager, root) = manager();
        let a = add_node(&mut manager, root, 0.0, 0.0);
        let parent = manager.graph(root).parent.unwrap();
        manager.node_mut(parent).padding_left = Some(5.0);

        manager.graph_update_bounds(root, true).unwrap();

        let graph = manager.graph(root);
        assert_eq!((graph.left, graph.right), (-5.0, 15.0));
        assert_eq!((graph.top, graph.bottom), (-5.0, 15.0));
        assert_eq!(manager.node(a).get_left(), 0.0);
    }

    #[test]
    fn gravitation_list_is_set_once() {
        let (mut manager, _root) = manager();
        assert_eq!(manager.get_all_nodes_to_apply_gravitation(), None);
        manager
            .set_all_nodes_to_apply_gravitation(Vec::new())
            .unwrap();
        assert_eq!(
            manager
                .set_all_nodes_to_apply_gravitation(Vec::new())
                .unwrap_err(),
            LayoutError("assert failed")
        );
        manager.reset_all_nodes_to_apply_gravitation();
        assert_eq!(manager.get_all_nodes_to_apply_gravitation(), None);
    }
}
