//! Port of `layout-base/src/fd/FDLayout.js`.
//!
//! The force-directed layout base: the constants of the spring embedder, the ideal edge
//! lengths, the spring, repulsion, and gravitation forces, the FR grid variant of
//! repulsion, convergence, and the movement of the nodes.
//!
//! Upstream `FDLayout` extends [`Layout`]; this port composes it (`fd.layout`), so the
//! graph manager, the parameters, and the l-level object model are the ones of
//! [`super::super::layout`]. The extra node and edge fields live in [`FDLayout::node`] and
//! [`FDLayout::edge`], parallel to the manager's arenas; [`FDLayout::sync_arenas`] keeps
//! them in step and every mutating method here calls it.
//!
//! Upstream's `FDLayoutNode` has no `move` (only cose-base's `CoSENode` does), but
//! `moveNodes` is here and must call it. Since layout-base cannot see cose-base, the
//! movement lives in [`FDLayout::node_move`], which is exactly `CoSENode.prototype.move`;
//! `cose_base::co_se_node` names that.
//!
//! `calcRepulsionRange` returns `0.0` here, as upstream; a subclass that overrides it
//! (cose-base does) sets `repulsion_range` again after `init_spring_embedder`.
//!
//! The list of nodes gravity applies to is `undefined` until
//! `LGraphManager.setAllNodesToApplyGravitation` runs; upstream would throw on
//! `calcGravitationalForces` before that, and the port treats `None` as an empty list.

use super::super::LayoutError;
use super::super::l_edge::EdgeId;
use super::super::l_graph::GraphId;
use super::super::l_node::NodeId;
use super::super::layout::{Layout, Updatable};
use super::super::util::i_geometry;
use super::super::util::i_math;
use super::fd_layout_constants as constants;
use super::fd_layout_edge::FDLayoutEdge;
use super::fd_layout_node::FDLayoutNode;

/// Which layout-base version's computations a layout runs.
///
/// The two versions differ in how the spring, repulsion, and ideal edge length are sized:
/// 1.0.2 (cose-bilkent 4.1.0) uses the layout-wide `springConstant` and
/// `repulsionConstant` and resets every edge's ideal length from `idealEdgeLength`; 2.0.1
/// (cose-base 2.x, cytoscape-fcose) uses each edge's `edgeElasticity` and each node's
/// `nodeRepulsion`, and keeps the edge's own ideal length as the base. [`LayoutVersion::V1`]
/// keeps cose-bilkent byte-identical.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutVersion {
    /// layout-base 1.0.2, the base of cose-base 1.0.3.
    #[default]
    V1,
    /// layout-base 2.0.1, the base of cose-base 2.2.0.
    V2,
}

/// The force-directed layout base, upstream's `FDLayout`.
pub struct FDLayout<V = ()> {
    /// The layout this one extends, upstream's `Layout` prototype chain.
    pub layout: Layout<V>,
    /// The force-directed half of each node, indexed by [`NodeId`].
    fd_nodes: Vec<FDLayoutNode>,
    /// The force-directed half of each edge, indexed by [`EdgeId`].
    fd_edges: Vec<FDLayoutEdge>,
    /// Whether ideal edge lengths account for nesting.
    pub use_smart_ideal_edge_length_calculation: bool,
    /// Ideal length of an edge.
    pub ideal_edge_length: f64,
    /// Spring strength.
    pub spring_constant: f64,
    /// Repulsion strength.
    pub repulsion_constant: f64,
    /// Gravity strength.
    pub gravity_constant: f64,
    /// Gravity strength inside a compound.
    pub compound_gravity_constant: f64,
    /// Multiplier of a graph's estimated size that bounds gravity.
    pub gravity_range_factor: f64,
    /// Multiplier of a compound's estimated size that bounds gravity.
    pub compound_gravity_range_factor: f64,
    /// Total displacement allowed for each node before convergence.
    pub displacement_threshold_per_node: f64,
    /// Current temperature.
    pub cooling_factor: f64,
    /// Temperature the run starts from.
    pub initial_cooling_factor: f64,
    /// Total displacement of the current iteration.
    pub total_displacement: f64,
    /// Total displacement of the previous iteration, upstream's `oldTotalDisplacement`.
    pub old_total_displacement: f64,
    /// Maximum number of iterations.
    pub max_iterations: i64,
    /// Iterations run so far.
    pub total_iterations: i64,
    /// Iterations since the last animation, upstream's `notAnimatedIterations`.
    pub not_animated_iterations: i64,
    /// Whether repulsion uses the FR grid variant.
    pub use_fr_grid_variant: bool,
    /// The grid of the FR variant, `grid[x][y]` listing the nodes of that cell.
    pub grid: Vec<Vec<Vec<NodeId>>>,
    /// Maximum displacement of a node per iteration.
    pub max_node_displacement: f64,
    /// Total displacement below which the run converged.
    pub total_displacement_threshold: f64,
    /// Distance up to which nodes repulse each other in the grid variant.
    pub repulsion_range: f64,
    /// How much an extra level of nesting adds to the ideal edge length.
    pub per_level_ideal_edge_length_factor: f64,
    /// Which layout-base version's force computations to run.
    pub version: LayoutVersion,
}

impl<V> Default for FDLayout<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> FDLayout<V> {
    /// Upstream's `FDLayout` constructor.
    pub fn new() -> Self {
        Self {
            layout: Layout::new(None),
            fd_nodes: Vec::new(),
            fd_edges: Vec::new(),
            use_smart_ideal_edge_length_calculation:
                constants::DEFAULT_USE_SMART_IDEAL_EDGE_LENGTH_CALCULATION,
            ideal_edge_length: constants::DEFAULT_EDGE_LENGTH,
            spring_constant: constants::DEFAULT_SPRING_STRENGTH,
            repulsion_constant: constants::DEFAULT_REPULSION_STRENGTH,
            gravity_constant: constants::DEFAULT_GRAVITY_STRENGTH,
            compound_gravity_constant: constants::DEFAULT_COMPOUND_GRAVITY_STRENGTH,
            gravity_range_factor: constants::DEFAULT_GRAVITY_RANGE_FACTOR,
            compound_gravity_range_factor: constants::DEFAULT_COMPOUND_GRAVITY_RANGE_FACTOR,
            displacement_threshold_per_node: (3.0 * constants::DEFAULT_EDGE_LENGTH) / 100.0,
            cooling_factor: constants::DEFAULT_COOLING_FACTOR_INCREMENTAL,
            initial_cooling_factor: constants::DEFAULT_COOLING_FACTOR_INCREMENTAL,
            total_displacement: 0.0,
            old_total_displacement: 0.0,
            max_iterations: constants::MAX_ITERATIONS,
            total_iterations: 0,
            not_animated_iterations: 0,
            // Upstream leaves these five undefined until `initParameters` and
            // `initSpringEmbedder`; the port starts them at the values those set, except
            // the two that only the embedder computes (0 until then).
            use_fr_grid_variant: constants::DEFAULT_USE_SMART_REPULSION_RANGE_CALCULATION,
            grid: Vec::new(),
            max_node_displacement: constants::MAX_NODE_DISPLACEMENT,
            total_displacement_threshold: 0.0,
            repulsion_range: 0.0,
            per_level_ideal_edge_length_factor: constants::PER_LEVEL_IDEAL_EDGE_LENGTH_FACTOR,
            version: LayoutVersion::V1,
        }
    }

    /// Makes the force-directed arenas as long as the manager's. Nodes and edges are only
    /// ever appended to the arenas, so this pads after nodes and edges created through
    /// [`Layout`] or the manager directly, and empties the arenas after
    /// [`FDLayout::new_graph_manager`].
    pub fn sync_arenas(&mut self) {
        let node_count = self.layout.graph_manager.node_count();
        if self.fd_nodes.len() != node_count {
            self.fd_nodes.truncate(node_count);
            self.fd_nodes.resize_with(node_count, FDLayoutNode::default);
        }

        let edge_count = self.layout.graph_manager.edge_count();
        if self.fd_edges.len() != edge_count {
            self.fd_edges.truncate(edge_count);
            self.fd_edges.resize_with(edge_count, FDLayoutEdge::default);
        }
    }

    /// The force-directed fields of a node. The arenas are in step after any mutating
    /// call of this type; when the graph was built through the manager directly, call
    /// [`FDLayout::sync_arenas`] first.
    pub fn node(&self, id: NodeId) -> &FDLayoutNode {
        &self.fd_nodes[id.0]
    }

    /// The force-directed fields of a node, mutably.
    pub fn node_mut(&mut self, id: NodeId) -> &mut FDLayoutNode {
        self.sync_arenas();
        &mut self.fd_nodes[id.0]
    }

    /// The force-directed fields of an edge.
    pub fn edge(&self, id: EdgeId) -> &FDLayoutEdge {
        &self.fd_edges[id.0]
    }

    /// The force-directed fields of an edge, mutably.
    pub fn edge_mut(&mut self, id: EdgeId) -> &mut FDLayoutEdge {
        self.sync_arenas();
        &mut self.fd_edges[id.0]
    }

    /// Upstream `Layout.prototype.newGraphManager`, then an empty force-directed arena.
    pub fn new_graph_manager(&mut self) {
        self.layout.new_graph_manager();
        self.fd_nodes.clear();
        self.fd_edges.clear();
    }

    /// Upstream `Layout.prototype.addRoot`.
    pub fn add_root(&mut self) -> GraphId {
        let root = self.layout.graph_manager.add_root();
        self.sync_arenas();
        root
    }

    /// Upstream `Layout.prototype.newGraph`.
    pub fn new_graph(&mut self, v_graph: Option<V>) -> GraphId {
        let graph = self.layout.new_graph(v_graph);
        self.sync_arenas();
        graph
    }

    /// Upstream `Layout.prototype.newNode`.
    pub fn new_node(&mut self, v_node: Option<V>) -> NodeId {
        let node = self.layout.new_node(v_node);
        self.sync_arenas();
        node
    }

    /// Upstream `Layout.prototype.newEdge`.
    pub fn new_edge(&mut self, v_edge: Option<V>) -> EdgeId {
        let edge = self.layout.new_edge(v_edge);
        self.sync_arenas();
        edge
    }

    /// Upstream `FDLayout.prototype.initParameters`.
    pub fn init_parameters(&mut self) {
        self.layout.init_parameters();

        self.total_iterations = 0;
        self.not_animated_iterations = 0;

        self.use_fr_grid_variant = constants::DEFAULT_USE_SMART_REPULSION_RANGE_CALCULATION;

        self.grid = Vec::new();
    }

    /// Upstream `FDLayout.prototype.calcIdealEdgeLengths`.
    pub fn calc_ideal_edge_lengths(&mut self) -> Result<(), LayoutError> {
        let all_edges = self.layout.get_all_edges();

        for edge in all_edges {
            // 1.0.2 resets every edge's ideal length from the layout's `idealEdgeLength`;
            // 2.0.1 starts from the edge's own value (`originalIdealLength`).
            let original_ideal_length = self.fd_edges[edge.0].ideal_length;
            let mut ideal_length = match self.version {
                LayoutVersion::V1 => self.ideal_edge_length,
                LayoutVersion::V2 => original_ideal_length,
            };
            let is_inter_graph = self.layout.graph_manager.edge(edge).is_inter_graph;

            if is_inter_graph {
                let (source, target) = {
                    let edge = self.layout.graph_manager.edge(edge);
                    (
                        edge.source
                            .ok_or(LayoutError("Source and/or target is null!"))?,
                        edge.target
                            .ok_or(LayoutError("Source and/or target is null!"))?,
                    )
                };
                let (source_in_lca, target_in_lca, lca) = {
                    let edge = self.layout.graph_manager.edge(edge);
                    (
                        edge.source_in_lca.ok_or(LayoutError("assert failed"))?,
                        edge.target_in_lca.ok_or(LayoutError("assert failed"))?,
                        edge.lca.ok_or(LayoutError("assert failed"))?,
                    )
                };

                let size_of_source_in_lca = self
                    .layout
                    .graph_manager
                    .node(source_in_lca)
                    .get_estimated_size()?;
                let size_of_target_in_lca = self
                    .layout
                    .graph_manager
                    .node(target_in_lca)
                    .get_estimated_size()?;

                if self.use_smart_ideal_edge_length_calculation {
                    ideal_length += size_of_source_in_lca + size_of_target_in_lca
                        - 2.0 * constants::SIMPLE_NODE_SIZE;
                }

                let lca_depth = self.layout.graph_manager.graph_inclusion_tree_depth(lca)?;

                let level_base = match self.version {
                    LayoutVersion::V1 => constants::DEFAULT_EDGE_LENGTH,
                    LayoutVersion::V2 => original_ideal_length,
                };

                ideal_length += level_base
                    * self.per_level_ideal_edge_length_factor
                    * (self
                        .layout
                        .graph_manager
                        .node(source)
                        .get_inclusion_tree_depth()?
                        + self
                            .layout
                            .graph_manager
                            .node(target)
                            .get_inclusion_tree_depth()?
                        - 2 * lca_depth) as f64;
            }

            self.edge_mut(edge).ideal_length = ideal_length;
        }

        Ok(())
    }

    /// Upstream `FDLayout.prototype.initSpringEmbedder`.
    pub fn init_spring_embedder(&mut self) {
        let s = self.layout.get_all_nodes().len() as f64;

        if self.layout.incremental {
            if s > constants::ADAPTATION_LOWER_NODE_LIMIT {
                self.cooling_factor = (self.cooling_factor * constants::COOLING_ADAPTATION_FACTOR)
                    .max(
                        self.cooling_factor
                            - (s - constants::ADAPTATION_LOWER_NODE_LIMIT)
                                / (constants::ADAPTATION_UPPER_NODE_LIMIT
                                    - constants::ADAPTATION_LOWER_NODE_LIMIT)
                                * self.cooling_factor
                                * (1.0 - constants::COOLING_ADAPTATION_FACTOR),
                    );
            }
            self.max_node_displacement = constants::MAX_NODE_DISPLACEMENT_INCREMENTAL;
        } else {
            if s > constants::ADAPTATION_LOWER_NODE_LIMIT {
                self.cooling_factor = constants::COOLING_ADAPTATION_FACTOR.max(
                    1.0 - (s - constants::ADAPTATION_LOWER_NODE_LIMIT)
                        / (constants::ADAPTATION_UPPER_NODE_LIMIT
                            - constants::ADAPTATION_LOWER_NODE_LIMIT)
                        * (1.0 - constants::COOLING_ADAPTATION_FACTOR),
                );
            } else {
                self.cooling_factor = 1.0;
            }
            self.initial_cooling_factor = self.cooling_factor;
            self.max_node_displacement = constants::MAX_NODE_DISPLACEMENT;
        }

        self.max_iterations = (s * 5.0).max(self.max_iterations as f64) as i64;

        self.total_displacement_threshold =
            self.displacement_threshold_per_node * self.layout.get_all_nodes().len() as f64;

        self.repulsion_range = self.calc_repulsion_range();
    }

    /// Upstream `FDLayout.prototype.calcSpringForces`.
    pub fn calc_spring_forces(&mut self) {
        self.sync_arenas();

        let l_edges = self.layout.get_all_edges();

        for edge in l_edges {
            let ideal_length = self.fd_edges[edge.0].ideal_length;

            self.calc_spring_force(edge, ideal_length);
        }
    }

    /// Upstream `FDLayout.prototype.calcRepulsionForces`.
    pub fn calc_repulsion_forces(
        &mut self,
        grid_update_allowed: bool,
        force_to_node_surrounding_update: bool,
    ) {
        self.sync_arenas();

        let l_nodes = self.layout.get_all_nodes();

        if self.use_fr_grid_variant {
            if self.total_iterations % constants::GRID_CALCULATION_CHECK_PERIOD == 1
                && grid_update_allowed
            {
                self.update_grid();
            }

            let mut processed_node_set: Vec<NodeId> = Vec::new();

            for node_a in l_nodes {
                self.calculate_repulsion_force_of_a_node(
                    node_a,
                    &processed_node_set,
                    grid_update_allowed,
                    force_to_node_surrounding_update,
                );
                processed_node_set.push(node_a);
            }
        } else {
            for i in 0..l_nodes.len() {
                let node_a = l_nodes[i];

                for &node_b in &l_nodes[i + 1..] {
                    // If both nodes are not members of the same graph, skip.
                    if self.layout.graph_manager.node(node_a).owner
                        != self.layout.graph_manager.node(node_b).owner
                    {
                        continue;
                    }

                    self.calc_repulsion_force(node_a, node_b);
                }
            }
        }
    }

    /// Upstream `FDLayout.prototype.calcGravitationalForces`.
    pub fn calc_gravitational_forces(&mut self) -> Result<(), LayoutError> {
        for node in self
            .layout
            .get_all_nodes_to_apply_gravitation()
            .unwrap_or_default()
        {
            self.calc_gravitational_force(node)?;
        }

        Ok(())
    }

    /// Upstream `FDLayout.prototype.moveNodes`.
    pub fn move_nodes(&mut self) {
        for node in self.layout.get_all_nodes() {
            self.node_move(node);
        }
    }

    /// Upstream `CoSENode.prototype.move`: the movement of one node by the forces of this
    /// iteration. Upstream's `FDLayoutNode` has no `move`; see the module doc for why it
    /// is here.
    pub fn node_move(&mut self, node: NodeId) {
        self.sync_arenas();

        let cooling_factor = self.cooling_factor;
        let max_node_displacement = self.max_node_displacement;
        let (force_x, force_y, no_of_children) = {
            let node = &self.fd_nodes[node.0];
            (
                node.spring_force_x + node.repulsion_force_x + node.gravitation_force_x,
                node.spring_force_y + node.repulsion_force_y + node.gravitation_force_y,
                node.no_of_children as f64,
            )
        };

        let mut displacement_x = cooling_factor * force_x / no_of_children;
        let mut displacement_y = cooling_factor * force_y / no_of_children;

        if displacement_x.abs() > cooling_factor * max_node_displacement {
            displacement_x = cooling_factor * max_node_displacement * i_math::sign(displacement_x);
        }

        if displacement_y.abs() > cooling_factor * max_node_displacement {
            displacement_y = cooling_factor * max_node_displacement * i_math::sign(displacement_y);
        }

        match self.layout.graph_manager.node(node).child {
            // A simple node, just move it.
            None => self
                .layout
                .graph_manager
                .node_mut(node)
                .move_by(displacement_x, displacement_y),
            // An empty compound node, again just move it.
            Some(child) if self.layout.graph_manager.graph(child).nodes.is_empty() => self
                .layout
                .graph_manager
                .node_mut(node)
                .move_by(displacement_x, displacement_y),
            // A non-empty compound node, propagate the movement to its children.
            Some(child) => {
                self.propagate_displacement_to_children(child, displacement_x, displacement_y);
            }
        }

        self.total_displacement += displacement_x.abs() + displacement_y.abs();

        let node = &mut self.fd_nodes[node.0];
        node.spring_force_x = 0.0;
        node.spring_force_y = 0.0;
        node.repulsion_force_x = 0.0;
        node.repulsion_force_y = 0.0;
        node.gravitation_force_x = 0.0;
        node.gravitation_force_y = 0.0;
        node.displacement_x = 0.0;
        node.displacement_y = 0.0;
    }

    /// cose-base 2.x's `CoSENode.prototype.calculateDisplacement`: the movement of one
    /// node by the forces of this iteration, accumulated on the node instead of applied,
    /// so constraints can adjust it before [`FDLayout::node_move_v2`] runs.
    ///
    /// A compound that holds fixed nodes divides the forces by its
    /// [`FDLayoutNode::fixed_node_weight`] instead of its children count, and the movement
    /// is passed on to its leaves' displacements.
    pub fn calculate_displacement(&mut self, node: NodeId) {
        self.sync_arenas();

        let cooling_factor = self.cooling_factor;
        let max_node_displacement = self.max_node_displacement;
        let child = self.layout.graph_manager.node(node).child;
        let (force_x, force_y, fixed_node_weight, no_of_children) = {
            let node = &self.fd_nodes[node.0];
            (
                node.spring_force_x + node.repulsion_force_x + node.gravitation_force_x,
                node.spring_force_y + node.repulsion_force_y + node.gravitation_force_y,
                node.fixed_node_weight,
                node.no_of_children as f64,
            )
        };

        {
            let node = &mut self.fd_nodes[node.0];
            match fixed_node_weight {
                Some(weight) if child.is_some() => {
                    node.displacement_x += cooling_factor * force_x / weight;
                    node.displacement_y += cooling_factor * force_y / weight;
                }
                _ => {
                    node.displacement_x += cooling_factor * force_x / no_of_children;
                    node.displacement_y += cooling_factor * force_y / no_of_children;
                }
            }

            if node.displacement_x.abs() > cooling_factor * max_node_displacement {
                node.displacement_x =
                    cooling_factor * max_node_displacement * i_math::sign(node.displacement_x);
            }

            if node.displacement_y.abs() > cooling_factor * max_node_displacement {
                node.displacement_y =
                    cooling_factor * max_node_displacement * i_math::sign(node.displacement_y);
            }
        }

        // A non-empty compound passes the movement on to its children as well.
        if let Some(child) = child
            && !self.layout.graph_manager.graph(child).nodes.is_empty()
        {
            let (dx, dy) = {
                let node = &self.fd_nodes[node.0];
                (node.displacement_x, node.displacement_y)
            };
            self.propagate_displacement_to_children_v2(child, dx, dy);
        }
    }

    /// cose-base 2.x's `CoSENode.prototype.propogateDisplacementToChildren`: adds the
    /// movement of a compound to the displacement of its leaves, without moving them.
    pub fn propagate_displacement_to_children_v2(&mut self, graph: GraphId, dx: f64, dy: f64) {
        self.sync_arenas();

        let nodes = self.layout.graph_manager.graph(graph).nodes.clone();

        for node in nodes {
            match self.layout.graph_manager.node(node).child {
                None => {
                    let node = &mut self.fd_nodes[node.0];
                    node.displacement_x += dx;
                    node.displacement_y += dy;
                }
                Some(child) => {
                    self.propagate_displacement_to_children_v2(child, dx, dy);
                }
            }
        }
    }

    /// cose-base 2.x's `CoSENode.prototype.move`: moves a simple or empty compound node by
    /// its accumulated displacement and clears the forces and displacement of every node.
    pub fn node_move_v2(&mut self, node: NodeId) {
        self.sync_arenas();

        let child = self.layout.graph_manager.node(node).child;
        let (displacement_x, displacement_y) = {
            let node = &self.fd_nodes[node.0];
            (node.displacement_x, node.displacement_y)
        };

        let moves = match child {
            None => true,
            Some(child) => self.layout.graph_manager.graph(child).nodes.is_empty(),
        };

        if moves {
            self.layout
                .graph_manager
                .node_mut(node)
                .move_by(displacement_x, displacement_y);

            self.total_displacement += displacement_x.abs() + displacement_y.abs();
        }

        let node = &mut self.fd_nodes[node.0];
        node.spring_force_x = 0.0;
        node.spring_force_y = 0.0;
        node.repulsion_force_x = 0.0;
        node.repulsion_force_y = 0.0;
        node.gravitation_force_x = 0.0;
        node.gravitation_force_y = 0.0;
        node.displacement_x = 0.0;
        node.displacement_y = 0.0;
    }

    /// cose-base 2.x's `CoSELayout.prototype.moveNodes`: calculate every displacement,
    /// then move every node with [`FDLayout::node_move_v2`]. The caller that handles
    /// constraints adjusts the displacements between the two loops.
    pub fn move_nodes_v2(&mut self) {
        let all_nodes = self.layout.get_all_nodes();

        for node in &all_nodes {
            self.calculate_displacement(*node);
        }

        for node in all_nodes {
            self.node_move_v2(node);
        }
    }

    /// Upstream `CoSENode.prototype.propogateDisplacementToChildren`: moves the leaf
    /// descendants of `graph` and records the movement on them.
    pub fn propagate_displacement_to_children(&mut self, graph: GraphId, dx: f64, dy: f64) {
        self.sync_arenas();

        let nodes = self.layout.graph_manager.graph(graph).nodes.clone();

        for node in nodes {
            match self.layout.graph_manager.node(node).child {
                None => {
                    self.layout.graph_manager.node_mut(node).move_by(dx, dy);
                    let node = &mut self.fd_nodes[node.0];
                    node.displacement_x += dx;
                    node.displacement_y += dy;
                }
                Some(child) => {
                    self.propagate_displacement_to_children(child, dx, dy);
                }
            }
        }
    }

    /// Upstream `FDLayout.prototype.calcSpringForce`.
    pub fn calc_spring_force(&mut self, edge: EdgeId, ideal_length: f64) {
        self.sync_arenas();

        let (source, target) = {
            let edge = self.layout.graph_manager.edge(edge);
            (edge.source, edge.target)
        };
        let (Some(source), Some(target)) = (source, target) else {
            return;
        };

        let uniform_leaf_node_sizes = self.layout.uniform_leaf_node_sizes
            && self.layout.graph_manager.node(source).child.is_none()
            && self.layout.graph_manager.node(target).child.is_none();

        if uniform_leaf_node_sizes {
            self.layout.graph_manager.edge_update_length_simple(edge);
        } else {
            self.layout.graph_manager.edge_update_length(edge);

            if self
                .layout
                .graph_manager
                .edge(edge)
                .is_overlaping_source_and_target
            {
                return;
            }
        }

        let (length, length_x, length_y) = {
            let edge = self.layout.graph_manager.edge(edge);
            (edge.length, edge.length_x, edge.length_y)
        };

        if length == 0.0 {
            return;
        }

        let spring_force = match self.version {
            LayoutVersion::V1 => self.spring_constant * (length - ideal_length),
            LayoutVersion::V2 => self.edge(edge).edge_elasticity * (length - ideal_length),
        };

        let spring_force_x = spring_force * (length_x / length);
        let spring_force_y = spring_force * (length_y / length);

        let source_node = &mut self.fd_nodes[source.0];
        source_node.spring_force_x += spring_force_x;
        source_node.spring_force_y += spring_force_y;

        let target_node = &mut self.fd_nodes[target.0];
        target_node.spring_force_x -= spring_force_x;
        target_node.spring_force_y -= spring_force_y;
    }

    /// Upstream `FDLayout.prototype.calcRepulsionForce`.
    pub fn calc_repulsion_force(&mut self, node_a: NodeId, node_b: NodeId) {
        self.sync_arenas();

        let rect_a = *self.layout.graph_manager.node(node_a).get_rect();
        let rect_b = *self.layout.graph_manager.node(node_b).get_rect();

        if rect_a.intersects(&rect_b) {
            // The two nodes overlap: calculate the separation amount in x and y.
            let mut overlap_amount = [0.0; 2];
            // Upstream throws here when the rectangles do not intersect, which the check
            // above has already excluded.
            if i_geometry::calc_separation_amount(
                &rect_a,
                &rect_b,
                &mut overlap_amount,
                constants::DEFAULT_EDGE_LENGTH / 2.0,
            )
            .is_err()
            {
                return;
            }

            let repulsion_force_x = 2.0 * overlap_amount[0];
            let repulsion_force_y = 2.0 * overlap_amount[1];

            let children_a = self.fd_nodes[node_a.0].no_of_children as f64;
            let children_b = self.fd_nodes[node_b.0].no_of_children as f64;
            let children_constant = children_a * children_b / (children_a + children_b);

            let node_a = &mut self.fd_nodes[node_a.0];
            node_a.repulsion_force_x -= children_constant * repulsion_force_x;
            node_a.repulsion_force_y -= children_constant * repulsion_force_y;

            let node_b = &mut self.fd_nodes[node_b.0];
            node_b.repulsion_force_x += children_constant * repulsion_force_x;
            node_b.repulsion_force_y += children_constant * repulsion_force_y;
        } else {
            let uniform_leaf_node_sizes = self.layout.uniform_leaf_node_sizes
                && self.layout.graph_manager.node(node_a).child.is_none()
                && self.layout.graph_manager.node(node_b).child.is_none();

            let (distance_x, distance_y) = if uniform_leaf_node_sizes {
                // Simply base repulsion on the distance of the node centers.
                (
                    rect_b.get_center_x() - rect_a.get_center_x(),
                    rect_b.get_center_y() - rect_a.get_center_y(),
                )
            } else {
                // Use clipping points.
                let mut clip_points = [0.0; 4];
                i_geometry::get_intersection2(&rect_a, &rect_b, &mut clip_points);

                (
                    clip_points[2] - clip_points[0],
                    clip_points[3] - clip_points[1],
                )
            };

            let mut distance_x = distance_x;
            let mut distance_y = distance_y;

            if distance_x.abs() < constants::MIN_REPULSION_DIST {
                distance_x = i_math::sign(distance_x) * constants::MIN_REPULSION_DIST;
            }

            if distance_y.abs() < constants::MIN_REPULSION_DIST {
                distance_y = i_math::sign(distance_y) * constants::MIN_REPULSION_DIST;
            }

            let distance_squared = distance_x * distance_x + distance_y * distance_y;
            let distance = distance_squared.sqrt();

            let children_a = self.fd_nodes[node_a.0].no_of_children as f64;
            let children_b = self.fd_nodes[node_b.0].no_of_children as f64;

            let repulsion_force = match self.version {
                LayoutVersion::V1 => {
                    self.repulsion_constant * children_a * children_b / distance_squared
                }
                LayoutVersion::V2 => {
                    let node_a = &self.fd_nodes[node_a.0];
                    let node_b = &self.fd_nodes[node_b.0];
                    (node_a.node_repulsion / 2.0 + node_b.node_repulsion / 2.0)
                        * children_a
                        * children_b
                        / distance_squared
                }
            };

            // Project the force onto the x and y axes.
            let repulsion_force_x = repulsion_force * distance_x / distance;
            let repulsion_force_y = repulsion_force * distance_y / distance;

            let node_a = &mut self.fd_nodes[node_a.0];
            node_a.repulsion_force_x -= repulsion_force_x;
            node_a.repulsion_force_y -= repulsion_force_y;

            let node_b = &mut self.fd_nodes[node_b.0];
            node_b.repulsion_force_x += repulsion_force_x;
            node_b.repulsion_force_y += repulsion_force_y;
        }
    }

    /// Upstream `FDLayout.prototype.calcGravitationalForce`.
    pub fn calc_gravitational_force(&mut self, node: NodeId) -> Result<(), LayoutError> {
        self.sync_arenas();

        let Some(owner_graph) = self.layout.graph_manager.node(node).owner else {
            return Ok(());
        };

        let (left, right, top, bottom) = {
            let graph = self.layout.graph_manager.graph(owner_graph);
            (graph.left, graph.right, graph.top, graph.bottom)
        };
        let owner_center_x = (right + left) / 2.0;
        let owner_center_y = (top + bottom) / 2.0;

        let (center_x, center_y, width, height) = {
            let node = self.layout.graph_manager.node(node);
            (
                node.get_center_x(),
                node.get_center_y(),
                node.get_width(),
                node.get_height(),
            )
        };
        let distance_x = center_x - owner_center_x;
        let distance_y = center_y - owner_center_y;
        let abs_distance_x = distance_x.abs() + width / 2.0;
        let abs_distance_y = distance_y.abs() + height / 2.0;

        let estimated_size = self
            .layout
            .graph_manager
            .graph(owner_graph)
            .get_estimated_size()?;

        if self.layout.graph_manager.get_root() == Some(owner_graph) {
            // In the root graph.
            let estimated_size = estimated_size * self.gravity_range_factor;

            if abs_distance_x > estimated_size || abs_distance_y > estimated_size {
                let node = &mut self.fd_nodes[node.0];
                node.gravitation_force_x = -self.gravity_constant * distance_x;
                node.gravitation_force_y = -self.gravity_constant * distance_y;
            }
        } else {
            // Inside a compound.
            let estimated_size = estimated_size * self.compound_gravity_range_factor;

            if abs_distance_x > estimated_size || abs_distance_y > estimated_size {
                let node = &mut self.fd_nodes[node.0];
                node.gravitation_force_x =
                    -self.gravity_constant * distance_x * self.compound_gravity_constant;
                node.gravitation_force_y =
                    -self.gravity_constant * distance_y * self.compound_gravity_constant;
            }
        }

        Ok(())
    }

    /// Upstream `FDLayout.prototype.isConverged`.
    pub fn is_converged(&mut self) -> bool {
        let mut oscilating = false;

        if self.total_iterations as f64 > self.max_iterations as f64 / 3.0 {
            oscilating = (self.total_displacement - self.old_total_displacement).abs() < 2.0;
        }

        let converged = self.total_displacement < self.total_displacement_threshold;

        self.old_total_displacement = self.total_displacement;

        converged || oscilating
    }

    /// Upstream `FDLayout.prototype.animate`: hands the geometry to the v-level objects
    /// every `animation_period` iterations. `animationDuringLayout` is `false` by default,
    /// and the port does not animate unless a caller turns it on.
    pub fn animate(&mut self)
    where
        V: Updatable,
    {
        if self.layout.animation_during_layout && !self.layout.is_sub_layout {
            if self.not_animated_iterations as f64 == self.layout.animation_period {
                self.layout.update(None);
                self.not_animated_iterations = 0;
            } else {
                self.not_animated_iterations += 1;
            }
        }
    }

    /// Upstream `FDLayout.prototype.calcNoOfChildrenForAllNodes`.
    pub fn calc_no_of_children_for_all_nodes(&mut self) {
        self.sync_arenas();

        let all_nodes = self.layout.get_all_nodes();

        for node in all_nodes {
            let no_of_children = self.layout.graph_manager.node_no_of_children(node);

            self.fd_nodes[node.0].no_of_children = no_of_children;
        }
    }

    /// Upstream `FDLayout.prototype.calcGrid`.
    pub fn calc_grid(&self, graph: GraphId) -> Vec<Vec<Vec<NodeId>>> {
        let graph = self.layout.graph_manager.graph(graph);

        let size_x = ((graph.right - graph.left) / self.repulsion_range).ceil() as i32;
        let size_y = ((graph.bottom - graph.top) / self.repulsion_range).ceil() as i32;

        vec![vec![Vec::new(); size_y.max(0) as usize]; size_x.max(0) as usize]
    }

    /// Upstream `FDLayout.prototype.addNodeToGrid`.
    pub fn add_node_to_grid(&mut self, node: NodeId, left: f64, top: f64) {
        self.sync_arenas();

        let rect = *self.layout.graph_manager.node(node).get_rect();

        let start_x = ((rect.x - left) / self.repulsion_range).floor() as i32;
        let finish_x = ((rect.width + rect.x - left) / self.repulsion_range).floor() as i32;
        let start_y = ((rect.y - top) / self.repulsion_range).floor() as i32;
        let finish_y = ((rect.height + rect.y - top) / self.repulsion_range).floor() as i32;

        for i in start_x..=finish_x {
            for j in start_y..=finish_y {
                if i < 0 || j < 0 {
                    continue;
                }

                if let Some(cell) = self
                    .grid
                    .get_mut(i as usize)
                    .and_then(|column| column.get_mut(j as usize))
                {
                    cell.push(node);
                }
            }
        }

        // Upstream sets the same coordinates inside the loop; they do not depend on i or j.
        self.fd_nodes[node.0].set_grid_coordinates(start_x, finish_x, start_y, finish_y);
    }

    /// Upstream `FDLayout.prototype.updateGrid`.
    pub fn update_grid(&mut self) {
        let Some(root) = self.layout.graph_manager.get_root() else {
            return;
        };

        self.grid = self.calc_grid(root);

        let (left, top) = {
            let graph = self.layout.graph_manager.graph(root);
            (graph.left, graph.top)
        };

        for node in self.layout.get_all_nodes() {
            self.add_node_to_grid(node, left, top);
        }
    }

    /// Upstream `FDLayout.prototype.calculateRepulsionForceOfANode`: the repulsion
    /// between `node_a` and the nodes around it in the grid.
    pub fn calculate_repulsion_force_of_a_node(
        &mut self,
        node_a: NodeId,
        processed_node_set: &[NodeId],
        grid_update_allowed: bool,
        force_to_node_surrounding_update: bool,
    ) {
        self.sync_arenas();

        if (self.total_iterations % constants::GRID_CALCULATION_CHECK_PERIOD == 1
            && grid_update_allowed)
            || force_to_node_surrounding_update
        {
            let (start_x, finish_x, start_y, finish_y) = {
                let node = &self.fd_nodes[node_a.0];
                (node.start_x, node.finish_x, node.start_y, node.finish_y)
            };

            let mut surrounding: Vec<NodeId> = Vec::new();
            let grid_width = self.grid.len() as i32;
            let grid_height = self.grid.first().map_or(0, |column| column.len()) as i32;

            for i in (start_x - 1)..(finish_x + 2) {
                for j in (start_y - 1)..(finish_y + 2) {
                    if i < 0 || j < 0 || i >= grid_width || j >= grid_height {
                        continue;
                    }

                    let cell = self.grid[i as usize][j as usize].clone();

                    for node_b in cell {
                        // If both nodes are not members of the same graph, or both nodes
                        // are the same, skip.
                        if self.layout.graph_manager.node(node_a).owner
                            != self.layout.graph_manager.node(node_b).owner
                            || node_a == node_b
                        {
                            continue;
                        }

                        // Check whether the repulsion force between node_a and node_b has
                        // already been calculated.
                        if !processed_node_set.contains(&node_b) && !surrounding.contains(&node_b) {
                            let node_a_rect = self.layout.graph_manager.node(node_a);
                            let node_b_rect = self.layout.graph_manager.node(node_b);

                            let distance_x =
                                (node_a_rect.get_center_x() - node_b_rect.get_center_x()).abs()
                                    - ((node_a_rect.get_width() / 2.0)
                                        + (node_b_rect.get_width() / 2.0));
                            let distance_y =
                                (node_a_rect.get_center_y() - node_b_rect.get_center_y()).abs()
                                    - ((node_a_rect.get_height() / 2.0)
                                        + (node_b_rect.get_height() / 2.0));

                            // If the distance between node_a and node_b is less than the
                            // calculation range, add node_b to the surrounding of node_a.
                            if distance_x <= self.repulsion_range
                                && distance_y <= self.repulsion_range
                            {
                                surrounding.push(node_b);
                            }
                        }
                    }
                }
            }

            self.fd_nodes[node_a.0].surrounding = surrounding;
        }

        for node_b in self.fd_nodes[node_a.0].surrounding.clone() {
            self.calc_repulsion_force(node_a, node_b);
        }
    }

    /// Upstream `FDLayout.prototype.calcRepulsionRange`: no range in the base class; the
    /// grid variant of cose-base overrides it.
    pub fn calc_repulsion_range(&self) -> f64 {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::layout_base::l_graph::GraphId;
    use crate::layout::layout_base::util::dimension_d::DimensionD;
    use crate::layout::layout_base::util::point::Point;

    // The expected values below were printed by upstream's JavaScript with
    // `node -e 'require(".../ref.js")'` over the same graphs (see the workspace's
    // `research/mermaid-port/upstream`), with the same seed and no iteration.

    fn add_node(
        layout: &mut FDLayout<()>,
        graph: GraphId,
        id: &str,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> NodeId {
        let node = layout.new_node(None);
        layout
            .layout
            .graph_manager
            .graph_add_node(graph, node)
            .unwrap();
        layout
            .layout
            .graph_manager
            .node_mut(node)
            .set_rect(&Point::new(x, y), &DimensionD::new(width, height));
        layout.node_mut(node).id = Some(id.to_string());
        node
    }

    fn add_edge(layout: &mut FDLayout<()>, source: NodeId, target: NodeId) -> EdgeId {
        let edge = layout.new_edge(None);
        layout
            .layout
            .graph_manager
            .add_edge(edge, source, target)
            .unwrap();
        edge
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }

    /// Layout-base 2.0.1's spring: the edge's own `edgeElasticity` replaces the layout's
    /// `springConstant`. Expected: `2.0 * (60 - 50) = 20` (1.0.2 would give
    /// `0.45 * (60 - 50) = 4.5`).
    #[test]
    fn spring_force_of_layout_base_v2_uses_edge_elasticity() {
        let mut layout: FDLayout<()> = FDLayout::new();
        layout.version = LayoutVersion::V2;
        layout.new_graph_manager();
        let root = layout.add_root();

        let a = add_node(&mut layout, root, "a", 0.0, 0.0, 40.0, 40.0);
        let b = add_node(&mut layout, root, "b", 100.0, 0.0, 40.0, 40.0);
        let edge = add_edge(&mut layout, a, b);

        layout.init_parameters();
        layout.edge_mut(edge).edge_elasticity = 2.0;
        layout.calc_ideal_edge_lengths().unwrap();
        layout.calc_spring_forces();

        assert_close(layout.node(a).spring_force_x, 20.0);
        assert_close(layout.node(a).spring_force_y, 0.0);
        assert_close(layout.node(b).spring_force_x, -20.0);
    }

    /// Layout-base 2.0.1's repulsion: halve each node's `nodeRepulsion` and add them.
    /// Expected from cose-base 2.2.0:
    /// `-0.061023591056393434, -0.01830707731691803` on `a` and the opposite on `b`.
    #[test]
    fn repulsion_of_layout_base_v2_uses_node_repulsion() {
        let mut layout: FDLayout<()> = FDLayout::new();
        layout.version = LayoutVersion::V2;
        layout.new_graph_manager();
        let root = layout.add_root();

        let a = add_node(&mut layout, root, "a", -20.0, -20.0, 40.0, 40.0);
        let b = add_node(&mut layout, root, "b", 80.0, 10.0, 40.0, 40.0);

        layout.init_parameters();
        layout.calc_no_of_children_for_all_nodes();
        layout.layout.uniform_leaf_node_sizes = false;
        layout.use_fr_grid_variant = false;
        layout.node_mut(a).node_repulsion = 100.0;
        layout.node_mut(b).node_repulsion = 400.0;

        layout.calc_repulsion_force(a, b);

        assert_close(layout.node(a).repulsion_force_x, -0.061023591056393434);
        assert_close(layout.node(a).repulsion_force_y, -0.01830707731691803);
        assert_close(layout.node(b).repulsion_force_x, 0.061023591056393434);
        assert_close(layout.node(b).repulsion_force_y, 0.01830707731691803);
    }

    /// Layout-base 2.0.1's ideal edge length: the extra nesting length multiplies the
    /// edge's own ideal length, not `DEFAULT_EDGE_LENGTH`. Expected `132` (1.0.2 would
    /// give `125`).
    #[test]
    fn ideal_length_of_layout_base_v2_scales_the_edges_own_length() {
        let mut layout: FDLayout<()> = FDLayout::new();
        layout.version = LayoutVersion::V2;
        layout.new_graph_manager();
        let root = layout.add_root();

        let a = add_node(&mut layout, root, "a", 0.0, 0.0, 40.0, 40.0);

        let parent = layout.new_node(None);
        layout
            .layout
            .graph_manager
            .graph_add_node(root, parent)
            .unwrap();
        layout
            .layout
            .graph_manager
            .node_mut(parent)
            .set_rect(&Point::new(-100.0, -100.0), &DimensionD::new(200.0, 200.0));
        layout.node_mut(parent).id = Some("p".to_string());

        let child_graph = layout.new_graph(None);
        layout
            .layout
            .graph_manager
            .add_graph(child_graph, parent)
            .unwrap();
        let b = add_node(&mut layout, child_graph, "b", -20.0, -20.0, 40.0, 40.0);

        let edge = add_edge(&mut layout, a, b);

        layout.init_parameters();
        layout.calc_no_of_children_for_all_nodes();
        layout
            .layout
            .graph_manager
            .calc_lowest_common_ancestors()
            .unwrap();
        layout.layout.graph_manager.calc_inclusion_tree_depths();
        layout.layout.graph_manager.graph_calc_estimated_size(root);
        layout.edge_mut(edge).ideal_length = 120.0;
        layout.calc_ideal_edge_lengths().unwrap();

        assert_close(layout.edge(edge).ideal_length, 132.0);
    }

    #[test]
    fn spring_force_of_two_nodes_is_along_the_edge() {
        let mut layout: FDLayout<()> = FDLayout::new();
        let root = layout.add_root();
        let a = add_node(&mut layout, root, "a", 0.0, 0.0, 40.0, 40.0);
        let b = add_node(&mut layout, root, "b", 120.0, 0.0, 40.0, 40.0);
        let edge = add_edge(&mut layout, a, b);

        layout.calc_no_of_children_for_all_nodes();
        layout.calc_spring_force(edge, 50.0);

        // The edge clips at (40, 20) and (120, 20): length 80, force
        // 0.45 * (80 - 50) = 13.5.
        assert_eq!(layout.edge(edge).ideal_length, 50.0);
        assert_eq!(layout.node(a).spring_force_x, 13.5);
        assert_eq!(layout.node(a).spring_force_y, 0.0);
        assert_eq!(layout.node(b).spring_force_x, -13.5);
        assert_eq!(layout.node(b).spring_force_y, 0.0);
    }

    #[test]
    fn spring_force_of_a_diagonal_edge() {
        let mut layout: FDLayout<()> = FDLayout::new();
        let root = layout.add_root();
        let a = add_node(&mut layout, root, "a", 0.0, 0.0, 40.0, 40.0);
        let b = add_node(&mut layout, root, "b", 130.0, 90.0, 60.0, 20.0);
        let edge = add_edge(&mut layout, a, b);

        layout.calc_no_of_children_for_all_nodes();
        layout.calc_spring_force(edge, 80.0);

        assert_close(layout.node(a).spring_force_x, 14.868246883519468);
        assert_close(layout.node(a).spring_force_y, 8.496141076296839);
        assert_close(layout.node(b).spring_force_x, -14.868246883519468);
        assert_close(layout.node(b).spring_force_y, -8.496141076296839);
    }

    #[test]
    fn repulsion_force_of_separated_nodes() {
        let mut layout: FDLayout<()> = FDLayout::new();
        let root = layout.add_root();
        let a = add_node(&mut layout, root, "a", 0.0, 0.0, 40.0, 40.0);
        let b = add_node(&mut layout, root, "b", 200.0, 0.0, 40.0, 40.0);

        layout.calc_no_of_children_for_all_nodes();
        layout.calc_repulsion_force(a, b);

        // Clipping points (40, 20) and (200, 20): 4500 / 160^2.
        assert_eq!(layout.node(a).repulsion_force_x, -0.17578125);
        assert_eq!(layout.node(a).repulsion_force_y, 0.0);
        assert_eq!(layout.node(b).repulsion_force_x, 0.17578125);
        assert_eq!(layout.node(b).repulsion_force_y, 0.0);
    }

    #[test]
    fn repulsion_force_of_diagonal_nodes() {
        let mut layout: FDLayout<()> = FDLayout::new();
        let root = layout.add_root();
        let a = add_node(&mut layout, root, "a", 0.0, 0.0, 40.0, 40.0);
        let c = add_node(&mut layout, root, "c", 0.0, 150.0, 100.0, 60.0);

        layout.calc_no_of_children_for_all_nodes();
        layout.calc_repulsion_force(a, c);

        assert_close(layout.node(a).repulsion_force_x, -0.0662093848051812);
        assert_close(layout.node(a).repulsion_force_y, -0.3531167189609664);
        assert_close(layout.node(c).repulsion_force_x, 0.0662093848051812);
        assert_close(layout.node(c).repulsion_force_y, 0.3531167189609664);
    }

    #[test]
    fn repulsion_force_of_overlapping_nodes() {
        let mut layout: FDLayout<()> = FDLayout::new();
        let root = layout.add_root();
        let a = add_node(&mut layout, root, "a", 0.0, 0.0, 100.0, 100.0);
        let b = add_node(&mut layout, root, "b", 40.0, 20.0, 100.0, 50.0);

        layout.calc_no_of_children_for_all_nodes();
        layout.calc_repulsion_force(a, b);

        // The separation amount with a buffer of 25 is (-55, 28.75) for `a`.
        assert_eq!(layout.node(a).repulsion_force_x, -55.0);
        assert_eq!(layout.node(a).repulsion_force_y, 28.75);
        assert_eq!(layout.node(b).repulsion_force_x, 55.0);
        assert_eq!(layout.node(b).repulsion_force_y, -28.75);
    }

    #[test]
    fn gravitation_force_pulls_a_far_node_towards_the_center() {
        let mut layout: FDLayout<()> = FDLayout::new();
        let root = layout.add_root();
        let a = add_node(&mut layout, root, "a", 0.0, 0.0, 40.0, 40.0);
        let b = add_node(&mut layout, root, "b", 1200.0, 0.0, 40.0, 40.0);
        let c = add_node(&mut layout, root, "c", 0.0, 900.0, 80.0, 20.0);

        layout.calc_no_of_children_for_all_nodes();
        layout
            .layout
            .graph_manager
            .graph_update_bounds(root, true)
            .unwrap();
        layout.layout.graph_manager.graph_calc_estimated_size(root);

        let (left, top, right, bottom) = {
            let graph = layout.layout.graph_manager.graph(root);
            (graph.left, graph.top, graph.right, graph.bottom)
        };
        assert_eq!((left, top, right, bottom), (-15.0, -15.0, 1255.0, 935.0));
        assert_close(
            layout
                .layout
                .graph_manager
                .graph(root)
                .get_estimated_size()
                .unwrap(),
            75.05553499465135,
        );

        for node in [a, b, c] {
            layout.calc_gravitational_force(node).unwrap();
        }

        // Owner center (620, 460), gravity 0.4.
        assert_eq!(layout.node(a).gravitation_force_x, 240.0);
        assert_eq!(layout.node(a).gravitation_force_y, 176.0);
        assert_eq!(layout.node(b).gravitation_force_x, -240.0);
        assert_eq!(layout.node(b).gravitation_force_y, 176.0);
        assert_eq!(layout.node(c).gravitation_force_x, 232.0);
        assert_eq!(layout.node(c).gravitation_force_y, -180.0);
    }

    #[test]
    fn gravitation_leaves_a_node_near_the_center_alone() {
        let mut layout: FDLayout<()> = FDLayout::new();
        let root = layout.add_root();
        let a = add_node(&mut layout, root, "a", 0.0, 0.0, 40.0, 40.0);
        let b = add_node(&mut layout, root, "b", 40.0, 0.0, 40.0, 40.0);

        layout
            .layout
            .graph_manager
            .graph_update_bounds(root, true)
            .unwrap();
        layout.layout.graph_manager.graph_calc_estimated_size(root);

        for node in [a, b] {
            layout.calc_gravitational_force(node).unwrap();
        }

        assert_eq!(layout.node(a).gravitation_force_x, 0.0);
        assert_eq!(layout.node(a).gravitation_force_y, 0.0);
        assert_eq!(layout.node(b).gravitation_force_x, 0.0);
        assert_eq!(layout.node(b).gravitation_force_y, 0.0);
    }
}
