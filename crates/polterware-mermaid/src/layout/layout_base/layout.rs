//! Port of `layout-base/src/Layout.js`.
//!
//! The layout base class: its parameters, the graph manager, and the steps that run
//! before and after a concrete layout. Upstream extends `Emitter` and calls
//! `this.layout()` — an abstract method a subclass overrides. Nothing in layout-base or
//! cose-base adds a listener, so the emitter is not ported; the concrete layout arrives
//! as the closure [`Layout::run_layout`] calls, and the `tilingPreLayout`/
//! `tilingPostLayout` overrides are the optional hooks
//! [`Layout::tiling_pre_layout`] and [`Layout::tiling_post_layout`].
//!
//! Upstream's `update` calls the v-level object's `update` method (`Updatable`);
//! [`Updatable`] is that interface, and a node, edge, or graph without a v-level object
//! is simply skipped. `LayoutConstants.ANIMATE` is undefined upstream, so the `'during'`
//! branch of `runLayout` never runs and is not ported. Upstream's `update2` walks all
//! edges and root nodes with the `update` calls commented out; only the root-graph update
//! is live, and the port does only that.

use std::collections::HashMap;

use super::LayoutError;
use super::l_edge::{EdgeId, LEdge};
use super::l_graph::{GraphId, LGraph};
use super::l_graph_manager::LGraphManager;
use super::l_node::{LNode, NodeId};
use super::layout_constants as constants;
use super::util::dimension_d::DimensionD;
use super::util::point::Point;
use super::util::point_d::PointD;
use super::util::transform::Transform;

/// The l-level object [`Layout::update`] visits, upstream's `instanceof` dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutObject {
    /// A node.
    Node(NodeId),
    /// An edge.
    Edge(EdgeId),
    /// A graph.
    Graph(GraphId),
}

/// A borrowed l-level object, handed to a v-level object's [`Updatable::update`].
#[derive(Clone, Copy, Debug)]
pub enum LayoutObjectRef<'a, V> {
    /// A node.
    Node(&'a LNode<V>),
    /// An edge.
    Edge(&'a LEdge<V>),
    /// A graph.
    Graph(&'a LGraph<V>),
}

/// Upstream's `Updatable` interface: what a v-level object does with its l-level
/// counterpart after layout. [`()`] ignores it, which is what a layout without v-level
/// objects wants.
pub trait Updatable: Sized {
    /// Receives the l-level object.
    fn update(&mut self, object: LayoutObjectRef<'_, Self>);
}

impl Updatable for () {
    fn update(&mut self, _object: LayoutObjectRef<'_, ()>) {}
}

/// A hook a subclass installs, upstream's `tilingPreLayout`/`tilingPostLayout`
/// overrides.
pub type LayoutHook<V> = Box<dyn FnMut(&mut Layout<V>)>;

/// A layout, upstream's `Layout`.
pub struct Layout<V = ()> {
    /// Layout quality: 0 draft, 1 default, 2 proof.
    pub layout_quality: i32,
    /// Whether layout creates bendpoints as needed.
    pub create_bends_as_needed: bool,
    /// Whether layout is incremental.
    pub incremental: bool,
    /// Whether the caller animates from the before to the after positions.
    pub animation_on_layout: bool,
    /// Whether the layout itself is animated.
    pub animation_during_layout: bool,
    /// Iterations between two successive animations.
    pub animation_period: f64,
    /// Whether leaf nodes have uniform sizes.
    pub uniform_leaf_node_sizes: bool,
    /// An edge's dummy bendpoint path, upstream's `edgeToDummyNodes`. Upstream's `Map`
    /// keeps insertion order, so this is an ordered list.
    pub edge_to_dummy_nodes: Vec<(EdgeId, Vec<NodeId>)>,
    /// The graph manager that owns every graph, node, and edge.
    pub graph_manager: LGraphManager<V>,
    /// Whether a layout run has finished.
    pub is_layout_finished: bool,
    /// Whether this layout lays out a sub-graph.
    pub is_sub_layout: bool,
    /// Whether the layout is run remotely, upstream's `isRemoteUse`.
    pub is_remote_use: bool,
    /// Upstream's `tilingPreLayout` override.
    pub tiling_pre_layout: Option<LayoutHook<V>>,
    /// Upstream's `tilingPostLayout` override.
    pub tiling_post_layout: Option<LayoutHook<V>>,
}

impl<V> Layout<V> {
    /// Upstream's `Layout.RANDOM_SEED`.
    pub const RANDOM_SEED: i32 = 1;

    /// A layout with upstream's default parameters. `is_remote_use` is upstream's
    /// constructor argument; `None` keeps it `false`.
    pub fn new(is_remote_use: Option<bool>) -> Self {
        Self {
            layout_quality: constants::QUALITY,
            create_bends_as_needed: constants::DEFAULT_CREATE_BENDS_AS_NEEDED,
            incremental: constants::DEFAULT_INCREMENTAL,
            animation_on_layout: constants::DEFAULT_ANIMATION_ON_LAYOUT,
            animation_during_layout: constants::DEFAULT_ANIMATION_DURING_LAYOUT,
            animation_period: constants::DEFAULT_ANIMATION_PERIOD,
            uniform_leaf_node_sizes: constants::DEFAULT_UNIFORM_LEAF_NODE_SIZES,
            edge_to_dummy_nodes: Vec::new(),
            graph_manager: LGraphManager::new(),
            is_layout_finished: false,
            is_sub_layout: false,
            is_remote_use: is_remote_use.unwrap_or(false),
            tiling_pre_layout: None,
            tiling_post_layout: None,
        }
    }

    /// Upstream `getGraphManager`.
    pub fn get_graph_manager(&self) -> &LGraphManager<V> {
        &self.graph_manager
    }

    /// Upstream `getGraphManager`, mutably.
    pub fn get_graph_manager_mut(&mut self) -> &mut LGraphManager<V> {
        &mut self.graph_manager
    }

    /// Upstream `getAllNodes`.
    pub fn get_all_nodes(&mut self) -> Vec<NodeId> {
        self.graph_manager.get_all_nodes()
    }

    /// Upstream `getAllEdges`.
    pub fn get_all_edges(&mut self) -> Vec<EdgeId> {
        self.graph_manager.get_all_edges()
    }

    /// Upstream `getAllNodesToApplyGravitation`.
    pub fn get_all_nodes_to_apply_gravitation(&self) -> Option<Vec<NodeId>> {
        self.graph_manager.get_all_nodes_to_apply_gravitation()
    }

    /// Upstream `newGraphManager`: replaces the manager, and so every graph, node, and
    /// edge, with an empty one.
    pub fn new_graph_manager(&mut self) {
        self.graph_manager = LGraphManager::new();
    }

    /// Upstream `newGraph`.
    pub fn new_graph(&mut self, v_graph: Option<V>) -> GraphId {
        self.graph_manager.new_graph(v_graph)
    }

    /// Upstream `newNode`.
    pub fn new_node(&mut self, v_node: Option<V>) -> NodeId {
        self.graph_manager.new_node(v_node)
    }

    /// Upstream `newEdge`.
    pub fn new_edge(&mut self, v_edge: Option<V>) -> EdgeId {
        self.graph_manager.new_edge(v_edge)
    }

    /// Upstream `setGraphManager`.
    pub fn set_graph_manager(&mut self, graph_manager: LGraphManager<V>) {
        self.graph_manager = graph_manager;
    }

    /// Upstream `checkLayoutSuccess`: a missing or empty root graph, or an inter-graph
    /// edge that joins an ancestor, makes the run fail.
    pub fn check_layout_success(&mut self) -> bool {
        let Some(root) = self.graph_manager.get_root() else {
            return true;
        };

        self.graph_manager.graph(root).nodes.is_empty()
            || self.graph_manager.includes_invalid_edge()
    }

    /// Upstream `runLayout`. `layout` is the subclass's `layout()` override and returns
    /// whether it succeeded.
    pub fn run_layout(&mut self, layout: &mut dyn FnMut(&mut Self) -> bool) -> bool
    where
        V: Updatable,
    {
        self.is_layout_finished = false;

        if let Some(mut hook) = self.tiling_pre_layout.take() {
            hook(self);
            self.tiling_pre_layout = Some(hook);
        }

        self.init_parameters();

        let is_layout_successful = if self.check_layout_success() {
            false
        } else {
            layout(self)
        };

        if is_layout_successful && !self.is_sub_layout {
            self.do_post_layout();
        }

        if let Some(mut hook) = self.tiling_post_layout.take() {
            hook(self);
            self.tiling_post_layout = Some(hook);
        }

        self.is_layout_finished = true;

        is_layout_successful
    }

    /// Upstream `doPostLayout`: propagate the geometry, then update the v-level objects.
    pub fn do_post_layout(&mut self)
    where
        V: Updatable,
    {
        if !self.incremental {
            self.transform(None);
        }
        self.update(None);
    }

    /// Upstream `update2`.
    pub fn update2(&mut self)
    where
        V: Updatable,
    {
        // Update the bend points.
        if self.create_bends_as_needed {
            debug_assert!(self.create_bendpoints_from_dummy_nodes().is_ok());

            // Reset all edges, since the topology has changed.
            self.graph_manager.reset_all_edges();
        }

        // Upstream walks all edges and root nodes here with the update calls commented
        // out, so only the root graph update is live.
        if !self.is_remote_use
            && let Some(root) = self.graph_manager.get_root()
        {
            self.update(Some(LayoutObject::Graph(root)));
        }
    }

    /// Upstream `update`: with no object, updates everything; otherwise hands the object
    /// (and, for a node, its child nodes first) to its v-level counterpart.
    pub fn update(&mut self, object: Option<LayoutObject>)
    where
        V: Updatable,
    {
        match object {
            None => self.update2(),
            Some(LayoutObject::Node(node)) => {
                if let Some(child) = self.graph_manager.node(node).child {
                    // Since the node is compound, update its child nodes recursively.
                    let nodes = self.graph_manager.graph(child).nodes.clone();
                    for child_node in nodes {
                        self.update(Some(LayoutObject::Node(child_node)));
                    }
                }

                if let Some(mut v_node) =
                    self.graph_manager.node_mut(node).base.v_graph_object.take()
                {
                    v_node.update(LayoutObjectRef::Node(self.graph_manager.node(node)));
                    self.graph_manager.node_mut(node).base.v_graph_object = Some(v_node);
                }
            }
            Some(LayoutObject::Edge(edge)) => {
                if let Some(mut v_edge) =
                    self.graph_manager.edge_mut(edge).base.v_graph_object.take()
                {
                    v_edge.update(LayoutObjectRef::Edge(self.graph_manager.edge(edge)));
                    self.graph_manager.edge_mut(edge).base.v_graph_object = Some(v_edge);
                }
            }
            Some(LayoutObject::Graph(graph)) => {
                if let Some(mut v_graph) = self
                    .graph_manager
                    .graph_mut(graph)
                    .base
                    .v_graph_object
                    .take()
                {
                    v_graph.update(LayoutObjectRef::Graph(self.graph_manager.graph(graph)));
                    self.graph_manager.graph_mut(graph).base.v_graph_object = Some(v_graph);
                }
            }
        }
    }

    /// Upstream `initParameters`: resets every parameter unless this is a sub-layout.
    pub fn init_parameters(&mut self) {
        if !self.is_sub_layout {
            self.layout_quality = constants::QUALITY;
            self.animation_during_layout = constants::DEFAULT_ANIMATION_DURING_LAYOUT;
            self.animation_period = constants::DEFAULT_ANIMATION_PERIOD;
            self.animation_on_layout = constants::DEFAULT_ANIMATION_ON_LAYOUT;
            self.incremental = constants::DEFAULT_INCREMENTAL;
            self.create_bends_as_needed = constants::DEFAULT_CREATE_BENDS_AS_NEEDED;
            self.uniform_leaf_node_sizes = constants::DEFAULT_UNIFORM_LEAF_NODE_SIZES;
        }

        if self.animation_during_layout {
            self.animation_on_layout = false;
        }
    }

    /// Upstream `transform`: maps every node from world to device coordinates. `None` is
    /// upstream's `undefined`, which recurses with the origin.
    pub fn transform(&mut self, new_left_top: Option<PointD>) {
        let new_left_top = new_left_top.unwrap_or_else(|| PointD::new(0.0, 0.0));

        // When an inverse transform is applied, the result is the upper-left coordinate
        // of the root graph at the given input coordinate (margins included).
        let mut transform = Transform::new();
        let left_top = match self.graph_manager.get_root() {
            Some(root) => self.graph_manager.graph_update_left_top(root),
            None => None,
        };

        if let Some(left_top) = left_top {
            transform.set_world_org_x(new_left_top.x);
            transform.set_world_org_y(new_left_top.y);

            transform.set_device_org_x(left_top.x);
            transform.set_device_org_y(left_top.y);

            let nodes = self.get_all_nodes();

            for node in nodes {
                self.graph_manager.node_mut(node).transform(&transform);
            }
        }
    }

    /// Upstream `positionNodesRandomly`.
    pub fn position_nodes_randomly(&mut self, graph: Option<GraphId>) {
        match graph {
            None => {
                if let Some(root) = self.graph_manager.get_root() {
                    self.position_nodes_randomly(Some(root));
                    debug_assert!(self.graph_manager.graph_update_bounds(root, true).is_ok());
                }
            }
            Some(graph) => {
                let nodes = self.graph_manager.graph(graph).nodes.clone();
                for node in nodes {
                    match self.graph_manager.node(node).child {
                        None => self.graph_manager.node_mut(node).scatter(),
                        Some(child) => {
                            if self.graph_manager.graph(child).nodes.is_empty() {
                                self.graph_manager.node_mut(node).scatter();
                            } else {
                                self.position_nodes_randomly(Some(child));
                                debug_assert!(self.graph_manager.node_update_bounds(node).is_ok());
                            }
                        }
                    }
                }
            }
        }
    }

    /// Upstream `getFlatForest`: the trees of a flat graph, each as the nodes a
    /// breadth-first walk visited, or nothing when the graph is not flat or not a forest.
    pub fn get_flat_forest(&self) -> Vec<Vec<NodeId>> {
        let mut flat_forest = Vec::new();
        let mut is_forest = true;

        let all_nodes = match self.graph_manager.get_root() {
            Some(root) => self.graph_manager.graph(root).nodes.clone(),
            None => Vec::new(),
        };

        // First be sure that the graph is flat.
        let is_flat = all_nodes
            .iter()
            .all(|&node| self.graph_manager.node(node).child.is_none());

        if !is_flat {
            return flat_forest;
        }

        // Run a breadth-first walk for each component.
        let mut visited: Vec<NodeId> = Vec::new();
        let mut to_be_visited: Vec<NodeId> = Vec::new();
        let mut parents: HashMap<NodeId, NodeId> = HashMap::new();
        let mut unprocessed_nodes = all_nodes;

        while !unprocessed_nodes.is_empty() && is_forest {
            to_be_visited.push(unprocessed_nodes[0]);

            while !to_be_visited.is_empty() && is_forest {
                let current_node = to_be_visited.remove(0);
                if !visited.contains(&current_node) {
                    visited.push(current_node);
                }

                // Traverse all neighbors of this node.
                let neighbor_edges = self.graph_manager.node(current_node).edges.clone();

                for neighbor_edge in neighbor_edges {
                    let current_neighbor = match self
                        .graph_manager
                        .edge_other_end(neighbor_edge, current_node)
                    {
                        Ok(neighbor) => neighbor,
                        Err(_) => continue,
                    };

                    // If the walk is not growing from this neighbor.
                    if parents.get(&current_node).copied() != Some(current_neighbor) {
                        if !visited.contains(&current_neighbor) {
                            to_be_visited.push(current_neighbor);
                            parents.insert(current_neighbor, current_node);
                        } else {
                            // A previously visited neighbor that is not the parent
                            // means this component is not a tree.
                            is_forest = false;
                            break;
                        }
                    }
                }
            }

            if !is_forest {
                flat_forest = Vec::new();
            } else {
                let temp = visited.clone();
                flat_forest.push(temp.clone());
                for value in temp {
                    if let Some(index) = unprocessed_nodes.iter().position(|&node| node == value) {
                        unprocessed_nodes.remove(index);
                    }
                }
                visited = Vec::new();
                parents = HashMap::new();
            }
        }

        flat_forest
    }

    /// Upstream `createDummyNodesForBendpoints`: replaces the edge with a chain of dummy
    /// nodes through its bendpoints. Upstream calls `dummyNodes.add` (an array has no
    /// `add`) and `new Dimension` (undefined), so it throws on the first bendpoint; the
    /// port pushes into the list and uses [`DimensionD`].
    pub fn create_dummy_nodes_for_bendpoints(
        &mut self,
        edge: EdgeId,
    ) -> Result<Vec<NodeId>, LayoutError> {
        let mut dummy_nodes = Vec::new();
        let (source, target) = {
            let edge = self.graph_manager.edge(edge);
            (
                edge.source
                    .ok_or(LayoutError("Source and/or target is null!"))?,
                edge.target
                    .ok_or(LayoutError("Source and/or target is null!"))?,
            )
        };
        let mut prev = source;

        let graph = self
            .graph_manager
            .calc_lowest_common_ancestor(source, target)
            .ok_or(LayoutError("assert failed"))?;

        let bendpoints = self.graph_manager.edge(edge).bendpoints.len();
        for _ in 0..bendpoints {
            // Create a new dummy node.
            let dummy_node = self.new_node(None);
            self.graph_manager
                .node_mut(dummy_node)
                .set_rect(&Point::new(0.0, 0.0), &DimensionD::new(1.0, 1.0));

            self.graph_manager.graph_add_node(graph, dummy_node)?;

            // Create a new dummy edge between the previous node and the dummy node.
            let dummy_edge = self.new_edge(None);
            self.graph_manager.add_edge(dummy_edge, prev, dummy_node)?;

            dummy_nodes.push(dummy_node);
            prev = dummy_node;
        }

        let dummy_edge = self.new_edge(None);
        self.graph_manager.add_edge(dummy_edge, prev, target)?;

        self.edge_to_dummy_nodes.push((edge, dummy_nodes.clone()));

        // Remove the real edge from the graph manager if it is inter-graph, or from its
        // graph otherwise.
        if self.graph_manager.edge(edge).is_inter_graph() {
            self.graph_manager.remove_edge(edge)?;
        } else {
            self.graph_manager.graph_remove_edge(graph, edge)?;
        }

        Ok(dummy_nodes)
    }

    /// Upstream `createBendpointsFromDummyNodes`: moves every edge's bendpoints onto
    /// their dummy nodes, removes the dummy nodes, and adds the real edges back.
    pub fn create_bendpoints_from_dummy_nodes(&mut self) -> Result<(), LayoutError> {
        let mut edges = self.graph_manager.get_all_edges();
        let mut ordered: Vec<EdgeId> = self
            .edge_to_dummy_nodes
            .iter()
            .map(|(edge, _)| *edge)
            .collect();
        ordered.append(&mut edges);

        for l_edge in ordered {
            if self.graph_manager.edge(l_edge).bendpoints.is_empty() {
                continue;
            }

            let path = self
                .edge_to_dummy_nodes
                .iter()
                .find(|(edge, _)| *edge == l_edge)
                .map(|(_, path)| path.clone());

            if let Some(path) = path {
                for (index, dummy_node) in path.iter().enumerate() {
                    let (center_x, center_y) = {
                        let node = self.graph_manager.node(*dummy_node);
                        (node.get_center_x(), node.get_center_y())
                    };

                    // Update the bendpoint's location according to the dummy node.
                    if let Some(bendpoint) = self
                        .graph_manager
                        .edge_mut(l_edge)
                        .bendpoints
                        .get_mut(index)
                    {
                        bendpoint.x = center_x;
                        bendpoint.y = center_y;
                    }

                    // Removing the dummy node also removes its incident dummy edges.
                    if let Some(owner) = self.graph_manager.node(*dummy_node).owner {
                        self.graph_manager.graph_remove_node(owner, *dummy_node)?;
                    }
                }
            }

            // Add the real edge back to the graph.
            let (source, target) = {
                let edge = self.graph_manager.edge(l_edge);
                (edge.source, edge.target)
            };
            if let (Some(source), Some(target)) = (source, target) {
                self.graph_manager.add_edge(l_edge, source, target)?;
            }
        }

        Ok(())
    }

    /// Upstream `Layout.transform(sliderValue, defaultValue, minDiv, maxMul)`, the
    /// static helper. `min_div` and `max_mul` are upstream's optional arguments.
    pub fn transform_value(
        slider_value: f64,
        default_value: f64,
        min_div: Option<f64>,
        max_mul: Option<f64>,
    ) -> f64 {
        if let (Some(min_div), Some(max_mul)) = (min_div, max_mul) {
            let mut value = default_value;

            if slider_value <= 50.0 {
                let min_value = default_value / min_div;
                value -= ((default_value - min_value) / 50.0) * (50.0 - slider_value);
            } else {
                let max_value = default_value * max_mul;
                value += ((max_value - default_value) / 50.0) * (slider_value - 50.0);
            }

            value
        } else {
            let (a, b) = if slider_value <= 50.0 {
                (9.0 * default_value / 500.0, default_value / 10.0)
            } else {
                (9.0 * default_value / 50.0, -8.0 * default_value)
            };

            a * slider_value + b
        }
    }

    /// Upstream `Layout.findCenterOfTree`: the center of a tree, found by repeatedly
    /// removing the leaves.
    ///
    /// Upstream splices `list` while indexing it by position, which skips every other
    /// node; the port keeps that walk. Upstream would loop forever on an empty list, so
    /// the port stops there.
    pub fn find_center_of_tree(&self, nodes: &[NodeId]) -> Option<NodeId> {
        let mut list: Vec<NodeId> = nodes.to_vec();

        let mut removed_nodes: Vec<NodeId> = Vec::new();
        let mut remaining_degrees: HashMap<NodeId, i64> = HashMap::new();
        let mut found_center = false;
        let mut center_node = None;

        if list.len() == 1 || list.len() == 2 {
            found_center = true;
            center_node = list.first().copied();
        }

        for &node in &list {
            let degree = self.graph_manager.node_neighbors(node).len() as i64;
            remaining_degrees.insert(node, degree);

            if degree == 1 {
                removed_nodes.push(node);
            }
        }

        while !found_center {
            // Upstream copies `removedNodes` into `tempList` and overwrites it on the
            // first line of the loop, so the copy is dead.
            let mut temp_list: Vec<NodeId> = Vec::new();

            let mut i = 0;
            while i < list.len() {
                let node = list[i];

                if let Some(index) = list.iter().position(|&candidate| candidate == node) {
                    list.remove(index);
                }

                for neighbour in self.graph_manager.node_neighbors(node) {
                    if !removed_nodes.contains(&neighbour) {
                        let other_degree = remaining_degrees.get(&neighbour).copied().unwrap_or(0);
                        let new_degree = other_degree - 1;

                        if new_degree == 1 {
                            temp_list.push(neighbour);
                        }

                        remaining_degrees.insert(neighbour, new_degree);
                    }
                }

                i += 1;
            }

            removed_nodes.extend_from_slice(&temp_list);

            if list.len() == 1 || list.len() == 2 {
                found_center = true;
                center_node = list.first().copied();
            }

            // Upstream would spin on an empty list; stop instead.
            if list.is_empty() {
                break;
            }
        }

        center_node
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::layout_base::util::point::Point as IntPoint;

    fn layout_with_path() -> (Layout<()>, GraphId, Vec<NodeId>) {
        let mut layout = Layout::new(None);
        let root = layout.graph_manager.add_root();
        let mut nodes = Vec::new();
        for index in 0..3 {
            let node = layout.new_node(None);
            layout.graph_manager.graph_add_node(root, node).unwrap();
            layout.graph_manager.node_mut(node).set_rect(
                &IntPoint::new(index as f64 * 100.0, 0.0),
                &DimensionD::new(10.0, 10.0),
            );
            nodes.push(node);
        }

        for pair in nodes.windows(2) {
            let edge = layout.new_edge(None);
            layout
                .graph_manager
                .add_edge(edge, pair[0], pair[1])
                .unwrap();
        }

        (layout, root, nodes)
    }

    #[test]
    fn defaults_match_upstream() {
        let layout: Layout<()> = Layout::new(None);
        assert_eq!(layout.layout_quality, 1);
        assert!(!layout.create_bends_as_needed);
        assert!(!layout.incremental);
        assert!(layout.animation_on_layout);
        assert!(!layout.animation_during_layout);
        assert_eq!(layout.animation_period, 50.0);
        assert!(!layout.uniform_leaf_node_sizes);
        assert!(!layout.is_layout_finished);
        assert!(!layout.is_sub_layout);
        assert!(!layout.is_remote_use);
        assert!(layout.graph_manager.get_root().is_none());

        let remote: Layout<()> = Layout::new(Some(true));
        assert!(remote.is_remote_use);
    }

    #[test]
    fn check_layout_success_needs_nodes() {
        let mut layout: Layout<()> = Layout::new(None);
        assert!(layout.check_layout_success());

        let root = layout.graph_manager.add_root();
        assert!(layout.check_layout_success());

        let node = layout.new_node(None);
        layout.graph_manager.graph_add_node(root, node).unwrap();
        assert!(!layout.check_layout_success());
    }

    #[test]
    fn run_layout_runs_the_algorithm_and_finishes() {
        let (mut layout, _root, _nodes) = layout_with_path();
        let mut called = false;

        let success = layout.run_layout(&mut |_layout| {
            called = true;
            true
        });

        assert!(success);
        assert!(called);
        assert!(layout.is_layout_finished);
    }

    #[test]
    fn run_layout_skips_the_algorithm_when_there_is_nothing_to_lay_out() {
        let mut layout: Layout<()> = Layout::new(None);
        let mut called = false;

        let success = layout.run_layout(&mut |_layout| {
            called = true;
            true
        });

        assert!(!success);
        assert!(!called);
        assert!(layout.is_layout_finished);
    }

    #[test]
    fn transform_value_matches_upstream() {
        // node -e 'const L = require(".../Layout.js");
        //          console.log(L.transform(30, 100), L.transform(80, 100),
        //                      L.transform(30, 100, 4, 2), L.transform(80, 100, 4, 2))'
        assert_eq!(Layout::<()>::transform_value(0.0, 100.0, None, None), 10.0);
        assert_eq!(Layout::<()>::transform_value(30.0, 100.0, None, None), 64.0);
        assert_eq!(
            Layout::<()>::transform_value(50.0, 100.0, None, None),
            100.0
        );
        assert_eq!(
            Layout::<()>::transform_value(80.0, 100.0, None, None),
            640.0
        );
        assert_eq!(
            Layout::<()>::transform_value(0.0, 100.0, Some(4.0), Some(2.0)),
            25.0
        );
        assert_eq!(
            Layout::<()>::transform_value(30.0, 100.0, Some(4.0), Some(2.0)),
            70.0
        );
        assert_eq!(
            Layout::<()>::transform_value(80.0, 100.0, Some(4.0), Some(2.0)),
            160.0
        );
    }

    #[test]
    fn flat_forest_of_a_tree() {
        let (layout, _root, nodes) = layout_with_path();
        assert_eq!(layout.get_flat_forest(), vec![nodes]);
    }

    #[test]
    fn flat_forest_of_a_cycle_is_empty() {
        let (mut layout, _root, nodes) = layout_with_path();
        let edge = layout.new_edge(None);
        layout
            .graph_manager
            .add_edge(edge, nodes[2], nodes[0])
            .unwrap();

        assert!(layout.get_flat_forest().is_empty());
    }

    #[test]
    fn flat_forest_of_a_compound_graph_is_empty() {
        let (mut layout, root, _nodes) = layout_with_path();
        let compound = layout.new_node(None);
        layout.graph_manager.graph_add_node(root, compound).unwrap();
        let child = layout.new_graph(None);
        layout.graph_manager.add_graph(child, compound).unwrap();

        assert!(layout.get_flat_forest().is_empty());
    }

    #[test]
    fn find_center_of_a_path() {
        let (layout, _root, nodes) = layout_with_path();
        assert_eq!(layout.find_center_of_tree(&nodes), Some(nodes[1]));
        assert_eq!(layout.find_center_of_tree(&nodes[..2]), Some(nodes[0]));
    }

    #[test]
    fn transform_moves_the_nodes_by_the_left_top() {
        let (mut layout, root, nodes) = layout_with_path();
        // Root bounds are (-15, -15) around the nodes at x = 0, 100, 200, y = 0.
        layout
            .graph_manager
            .graph_update_bounds(root, true)
            .unwrap();

        layout.transform(Some(PointD::new(0.0, 0.0)));

        // The left-top of the root graph becomes (0, 0), so the first node moves by
        // (15, 15).
        assert_eq!(
            layout.graph_manager.node(nodes[0]).get_location(),
            PointD::new(15.0, 15.0)
        );
        assert_eq!(
            layout.graph_manager.node(nodes[2]).get_location(),
            PointD::new(215.0, 15.0)
        );
    }
}
