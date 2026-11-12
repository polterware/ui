//! Port of `cose-base/src/CoSELayout.js`.
//!
//! The compound spring embedder: the classic layout steps (gravity list, children
//! counts, lowest common ancestors, inclusion depths, estimated sizes, ideal edge
//! lengths), the radial placement of a flat forest, tree reduction and growth, tiling of
//! zero-degree members and compound members, and the iteration loop.
//!
//! Upstream `CoSELayout` extends `FDLayout`, which extends `Layout`. This port composes
//! them: [`CoSELayout::fd`] is the [`FDLayout`], and `fd.layout` the
//! [`Layout`](super::super::layout_base::layout::Layout) whose
//! graph manager owns every graph, node, and edge.
//!
//! `CoSELayout` overrides `tilingPreLayout`, `tilingPostLayout`, and `calcRepulsionRange`.
//! Because [`Layout::run_layout`](super::super::layout_base::layout::Layout::run_layout)
//! hands a subclass only the base layout, [`CoSELayout`]
//! runs the same sequence itself ([`CoSELayout::run_layout`]) so the two tiling overrides
//! can see the CoSE state; the repulsion-range override is applied right after
//! [`FDLayout::init_spring_embedder`] ([`CoSELayout::init_spring_embedder`]).
//!
//! `Math.random()` in `findPlaceforPrunedNode` goes through layout-base's
//! [`random_seed`], so a run repeats; upstream's has no seed.
//!
//! `shiftToLastRow` reads `node.width`, which `LNode` does not define, so its guard is
//! always false and the body never runs; the port keeps the method and says so instead of
//! porting dead code.
//!
//! The id cose-bilkent attaches to an l-node (`theNode.id = theChild.data("id")`) is
//! [`super::super::layout_base::fd::FDLayoutNode::id`]. Tiling keys its maps by it, with
//! JavaScript's object-key coercion: `None` behaves as the string `"undefined"`.

use std::collections::HashMap;

use super::super::layout_base::LayoutError;
use super::super::layout_base::fd::fd_layout::FDLayout;
use super::super::layout_base::l_edge::EdgeId;
use super::super::layout_base::l_graph::GraphId;
use super::super::layout_base::l_graph_manager::LGraphManager;
use super::super::layout_base::l_node::NodeId;
use super::super::layout_base::layout::Updatable;
use super::super::layout_base::layout_constants;
use super::super::layout_base::util::i_geometry;
use super::super::layout_base::util::point::Point;
use super::super::layout_base::util::point_d::PointD;
use super::super::layout_base::util::random_seed;
use super::super::layout_base::util::transform::Transform;
use super::co_se_constants as constants;

/// The position of one node, upstream's `getPositionsData` entry.
#[derive(Clone, Debug, PartialEq)]
pub struct PositionData {
    /// The node's id, upstream's `id`.
    pub id: Option<String>,
    /// Center x.
    pub x: f64,
    /// Center y.
    pub y: f64,
    /// Width.
    pub w: f64,
    /// Height.
    pub h: f64,
}

/// A node removed by tree reduction, upstream's `[node, edge, owner]` triple.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrunedNode {
    /// The pruned node.
    pub node: NodeId,
    /// The edge that reattaches it.
    pub edge: EdgeId,
    /// The graph it belonged to.
    pub owner: GraphId,
}

/// Tiled rows of nodes, upstream's `organization` object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Organization {
    /// The rows, each a list of nodes in insertion order.
    pub rows: Vec<Vec<NodeId>>,
    /// Width of each row, upstream's `rowWidth`.
    pub row_width: Vec<f64>,
    /// Height of each row, upstream's `rowHeight`.
    pub row_height: Vec<f64>,
    /// Width of the widest row.
    pub width: f64,
    /// Height of all rows.
    pub height: f64,
    /// Vertical space between two rows.
    pub vertical_padding: f64,
    /// Horizontal space between two nodes of a row.
    pub horizontal_padding: f64,
}

/// The layout, upstream's `CoSELayout`.
pub struct CoSELayout<V = ()> {
    /// The force-directed layout this one extends.
    pub fd: FDLayout<V>,
    /// Whether each node is to be tiled, upstream's `toBeTiled` map.
    pub to_be_tiled: HashMap<String, bool>,
    /// The nodes gravity applies to, upstream's `nodesWithGravity`.
    pub nodes_with_gravity: Vec<NodeId>,
    /// Nesting level of the current layout, upstream's `level`.
    pub level: f64,
    /// The nodes tree reduction removed, step by step, upstream's `prunedNodesAll`.
    pub pruned_nodes_all: Vec<Vec<PrunedNode>>,
    /// Iterations since the tree started growing again, upstream's `growTreeIterations`.
    pub grow_tree_iterations: i64,
    /// Iterations since growth finished, upstream's `afterGrowthIterations`.
    pub after_growth_iterations: i64,
    /// Whether the pruned trees are growing back, upstream's `isTreeGrowing`.
    pub is_tree_growing: bool,
    /// Whether growth finished, upstream's `isGrowthFinished`.
    pub is_growth_finished: bool,
    /// Cooling cycle, upstream's `coolingCycle`.
    pub cooling_cycle: f64,
    /// Cooling cycle the run cools down to, upstream's `maxCoolingCycle`.
    pub max_cooling_cycle: f64,
    /// Temperature the run ends at, upstream's `finalTemperature`.
    pub final_temperature: f64,
    /// Spacing of the cooling schedule, upstream's `coolingAdjuster`.
    pub cooling_adjuster: f64,
    /// Iterations between two animations at the start, upstream's
    /// `initialAnimationPeriod`.
    pub initial_animation_period: f64,
    /// The dummy compounds of the zero-degree members, in creation order, upstream's
    /// `memberGroups` map.
    pub member_groups: Vec<(String, Vec<NodeId>)>,
    /// The dummy compound of each `memberGroups` key, upstream's `idToDummyNode` map.
    pub id_to_dummy_node: HashMap<String, NodeId>,
    /// The compounds to tile, inner ones first, upstream's `compoundOrder`.
    pub compound_order: Vec<NodeId>,
    /// The tiled members of each compound, upstream's `tiledMemberPack` map.
    pub tiled_member_pack: HashMap<String, Organization>,
    /// The tiled zero-degree members of each dummy compound, upstream's
    /// `tiledZeroDegreePack` map.
    pub tiled_zero_degree_pack: Vec<(String, Organization)>,
    /// Vertical space between two rows of tiled nodes.
    pub tiling_padding_vertical: f64,
    /// Horizontal space between two nodes of a tiled row.
    pub tiling_padding_horizontal: f64,
    /// Layout quality: 0 draft, 1 default, 2 proof.
    pub quality: Option<i32>,
    /// Node repulsion strength.
    pub node_repulsion: Option<f64>,
    /// Ideal edge length.
    pub ideal_edge_length: Option<f64>,
    /// Spring elasticity.
    pub edge_elasticity: Option<f64>,
    /// Nesting factor for compound edges.
    pub nesting_factor: Option<f64>,
    /// Gravity strength.
    pub gravity: Option<f64>,
    /// Max iterations.
    pub num_iter: Option<i64>,
    /// Gravity range factor.
    pub gravity_range: Option<f64>,
    /// Compound gravity strength.
    pub gravity_compound: Option<f64>,
    /// Compound gravity range factor.
    pub gravity_range_compound: Option<f64>,
    /// Initial energy on incremental layout.
    pub initial_energy_on_incremental: Option<f64>,
}

impl<V> Default for CoSELayout<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> CoSELayout<V> {
    /// Upstream's `CoSELayout` constructor.
    pub fn new() -> Self {
        Self {
            fd: FDLayout::new(),
            to_be_tiled: HashMap::new(),
            nodes_with_gravity: Vec::new(),
            level: 0.0,
            pruned_nodes_all: Vec::new(),
            grow_tree_iterations: 0,
            after_growth_iterations: 0,
            is_tree_growing: false,
            is_growth_finished: false,
            cooling_cycle: 0.0,
            max_cooling_cycle: 0.0,
            final_temperature: 0.0,
            cooling_adjuster: 1.0,
            initial_animation_period: 0.0,
            member_groups: Vec::new(),
            id_to_dummy_node: HashMap::new(),
            compound_order: Vec::new(),
            tiled_member_pack: HashMap::new(),
            tiled_zero_degree_pack: Vec::new(),
            tiling_padding_vertical: constants::TILING_PADDING_VERTICAL,
            tiling_padding_horizontal: constants::TILING_PADDING_HORIZONTAL,
            quality: None,
            node_repulsion: None,
            ideal_edge_length: None,
            edge_elasticity: None,
            nesting_factor: None,
            gravity: None,
            num_iter: None,
            gravity_range: None,
            gravity_compound: None,
            gravity_range_compound: None,
            initial_energy_on_incremental: None,
        }
    }

    /// Upstream `CoSELayout.prototype.newGraphManager`.
    pub fn new_graph_manager(&mut self) {
        self.fd.new_graph_manager();
    }

    /// Upstream `CoSELayout.prototype.newGraph`.
    pub fn new_graph(&mut self, v_graph: Option<V>) -> GraphId {
        self.fd.new_graph(v_graph)
    }

    /// Upstream `CoSELayout.prototype.newNode`.
    pub fn new_node(&mut self, v_node: Option<V>) -> NodeId {
        self.fd.new_node(v_node)
    }

    /// Upstream `CoSELayout.prototype.newEdge`.
    pub fn new_edge(&mut self, v_edge: Option<V>) -> EdgeId {
        self.fd.new_edge(v_edge)
    }

    /// Upstream `CoSELayout.prototype.initParameters`.
    pub fn init_parameters(&mut self) {
        self.fd.init_parameters();

        if !self.fd.layout.is_sub_layout {
            if constants::DEFAULT_EDGE_LENGTH < 10.0 {
                self.fd.ideal_edge_length = 10.0;
            } else {
                self.fd.ideal_edge_length = constants::DEFAULT_EDGE_LENGTH;
            }

            self.fd.use_smart_ideal_edge_length_calculation =
                constants::DEFAULT_USE_SMART_IDEAL_EDGE_LENGTH_CALCULATION;
            self.fd.spring_constant = constants::DEFAULT_SPRING_STRENGTH;
            self.fd.repulsion_constant = constants::DEFAULT_REPULSION_STRENGTH;
            self.fd.gravity_constant = constants::DEFAULT_GRAVITY_STRENGTH;
            self.fd.compound_gravity_constant = constants::DEFAULT_COMPOUND_GRAVITY_STRENGTH;
            self.fd.gravity_range_factor = constants::DEFAULT_GRAVITY_RANGE_FACTOR;
            self.fd.compound_gravity_range_factor =
                constants::DEFAULT_COMPOUND_GRAVITY_RANGE_FACTOR;

            if let Some(repulsion) = self.node_repulsion {
                self.fd.repulsion_constant = repulsion;
            }
            if let Some(ideal) = self.ideal_edge_length {
                self.fd.ideal_edge_length = ideal.max(10.0);
            }
            if let Some(spring) = self.edge_elasticity {
                self.fd.spring_constant = spring;
            }
            if let Some(nesting) = self.nesting_factor {
                self.fd.per_level_ideal_edge_length_factor = nesting;
            }
            if let Some(gravity) = self.gravity {
                self.fd.gravity_constant = gravity;
            }
            if let Some(num_iter) = self.num_iter {
                self.fd.max_iterations = num_iter;
            }
            if let Some(gravity_range) = self.gravity_range {
                self.fd.gravity_range_factor = gravity_range;
            }
            if let Some(gravity_comp) = self.gravity_compound {
                self.fd.compound_gravity_constant = gravity_comp;
            }
            if let Some(gravity_range_comp) = self.gravity_range_compound {
                self.fd.compound_gravity_range_factor = gravity_range_comp;
            }
            if let Some(cooling_inc) = self.initial_energy_on_incremental {
                self.fd.initial_cooling_factor = cooling_inc;
            }
            if let Some(quality) = self.quality {
                self.fd.layout.layout_quality = quality;
            }

            // Variables for tree reduction support.
            self.pruned_nodes_all = Vec::new();
            self.grow_tree_iterations = 0;
            self.after_growth_iterations = 0;
            self.is_tree_growing = false;
            self.is_growth_finished = false;

            // Variables for cooling.
            self.cooling_cycle = 0.0;
            self.max_cooling_cycle =
                self.fd.max_iterations as f64 / constants::CONVERGENCE_CHECK_PERIOD as f64;
            self.final_temperature =
                constants::CONVERGENCE_CHECK_PERIOD as f64 / self.fd.max_iterations as f64;
            self.cooling_adjuster = 1.0;
        }
    }

    /// Upstream `CoSELayout.prototype.layout`.
    pub fn layout(&mut self) -> Result<bool, LayoutError>
    where
        V: Updatable,
    {
        // Upstream reads `LayoutConstants.DEFAULT_CREATE_BENDS_AS_NEEDED`, not the
        // instance parameter; the constant is `false`.
        let create_bends_as_needed = layout_constants::DEFAULT_CREATE_BENDS_AS_NEEDED;

        if create_bends_as_needed {
            self.create_bendpoints()?;
            self.fd.layout.graph_manager.reset_all_edges();
        }

        self.level = 0.0;
        self.classic_layout()
    }

    /// Upstream `CoSELayout.prototype.classicLayout`.
    pub fn classic_layout(&mut self) -> Result<bool, LayoutError>
    where
        V: Updatable,
    {
        self.nodes_with_gravity = self.calculate_nodes_to_apply_gravitation_to();
        self.fd
            .layout
            .graph_manager
            .set_all_nodes_to_apply_gravitation(self.nodes_with_gravity.clone())?;
        self.fd.calc_no_of_children_for_all_nodes();
        self.fd
            .layout
            .graph_manager
            .calc_lowest_common_ancestors()?;
        self.fd.layout.graph_manager.calc_inclusion_tree_depths();

        let root = self
            .fd
            .layout
            .graph_manager
            .get_root()
            .ok_or(LayoutError("assert failed"))?;
        self.fd.layout.graph_manager.graph_calc_estimated_size(root);
        self.fd.calc_ideal_edge_lengths()?;

        if !self.fd.layout.incremental {
            let forest = self.fd.layout.get_flat_forest();

            if !forest.is_empty() {
                // The graph is flat and a forest.
                self.position_nodes_radially(forest)?;
            } else {
                // Reduce the trees, then update the nodes gravity applies to.
                self.reduce_trees()?;
                self.update_nodes_with_gravity()?;
                self.fd.layout.position_nodes_randomly(None);
            }
        } else if constants::TREE_REDUCTION_ON_INCREMENTAL {
            // Reduce the trees in incremental mode as well.
            self.reduce_trees()?;
            self.update_nodes_with_gravity()?;
        }

        self.init_spring_embedder();
        self.run_spring_embedder()?;

        Ok(true)
    }

    /// The second half of `classicLayout`'s tree-reduction branch: reset the gravity list
    /// to the nodes that are still in the manager.
    fn update_nodes_with_gravity(&mut self) -> Result<(), LayoutError> {
        self.fd
            .layout
            .graph_manager
            .reset_all_nodes_to_apply_gravitation();

        let all_nodes = self.fd.layout.get_all_nodes();
        let intersection: Vec<NodeId> = self
            .nodes_with_gravity
            .iter()
            .copied()
            .filter(|&node| all_nodes.contains(&node))
            .collect();

        self.fd
            .layout
            .graph_manager
            .set_all_nodes_to_apply_gravitation(intersection)
    }

    /// Upstream `CoSELayout.prototype.initSpringEmbedder`, inherited from `FDLayout` with
    /// `calcRepulsionRange` overridden; the override is applied after the base call.
    pub fn init_spring_embedder(&mut self) {
        self.fd.init_spring_embedder();

        self.fd.repulsion_range = self.calc_repulsion_range();
    }

    /// Upstream `CoSELayout.prototype.calcRepulsionRange`.
    pub fn calc_repulsion_range(&self) -> f64 {
        // The formula is 2 x (level + 1) x idealEdgeLength.
        2.0 * (self.level + 1.0) * self.fd.ideal_edge_length
    }

    /// Upstream `CoSELayout.prototype.tick`: one iteration of the spring embedder. Returns
    /// whether the layout ended.
    pub fn tick(&mut self) -> Result<bool, LayoutError>
    where
        V: Updatable,
    {
        self.fd.total_iterations += 1;

        if self.fd.total_iterations == self.fd.max_iterations
            && !self.is_tree_growing
            && !self.is_growth_finished
        {
            if !self.pruned_nodes_all.is_empty() {
                self.is_tree_growing = true;
            } else {
                return Ok(true);
            }
        }

        if self.fd.total_iterations % constants::CONVERGENCE_CHECK_PERIOD == 0
            && !self.is_tree_growing
            && !self.is_growth_finished
        {
            if self.fd.is_converged() {
                if !self.pruned_nodes_all.is_empty() {
                    self.is_tree_growing = true;
                } else {
                    return Ok(true);
                }
            }

            self.cooling_cycle += 1.0;

            if self.fd.layout.layout_quality == 0 {
                // Quality: "draft".
                self.cooling_adjuster = self.cooling_cycle;
            } else if self.fd.layout.layout_quality == 1 {
                // Quality: "default".
                self.cooling_adjuster = self.cooling_cycle / 3.0;
            }

            // The cooling schedule is based on
            // http://www.btluke.com/simanf1.html -> cooling schedule 3.
            let cooling_exponent =
                (100.0 * (self.fd.initial_cooling_factor - self.final_temperature)).ln()
                    / self.max_cooling_cycle.ln();
            self.fd.cooling_factor = (self.fd.initial_cooling_factor
                - self.cooling_cycle.powf(cooling_exponent) / 100.0 * self.cooling_adjuster)
                .max(self.final_temperature);
            self.fd.layout.animation_period =
                (self.initial_animation_period * self.fd.cooling_factor.sqrt()).ceil();
        }

        // Operations while the tree is growing again.
        if self.is_tree_growing {
            if self.grow_tree_iterations % 10 == 0 {
                if !self.pruned_nodes_all.is_empty() {
                    self.fd.layout.graph_manager.update_bounds()?;
                    self.fd.update_grid();
                    self.grow_tree()?;
                    self.update_nodes_with_gravity()?;

                    self.fd.layout.graph_manager.update_bounds()?;
                    self.fd.update_grid();
                    self.fd.cooling_factor = constants::DEFAULT_COOLING_FACTOR_INCREMENTAL;
                } else {
                    self.is_tree_growing = false;
                    self.is_growth_finished = true;
                }
            }
            self.grow_tree_iterations += 1;
        }

        // Operations after growth is finished.
        if self.is_growth_finished {
            if self.fd.is_converged() {
                return Ok(true);
            }

            if self.after_growth_iterations % 10 == 0 {
                self.fd.layout.graph_manager.update_bounds()?;
                self.fd.update_grid();
            }

            self.fd.cooling_factor = constants::DEFAULT_COOLING_FACTOR_INCREMENTAL
                * ((100.0 - self.after_growth_iterations as f64) / 100.0);
            self.after_growth_iterations += 1;
        }

        let grid_update_allowed = !self.is_tree_growing && !self.is_growth_finished;
        let force_to_node_surrounding_update = (self.grow_tree_iterations % 10 == 1
            && self.is_tree_growing)
            || (self.after_growth_iterations % 10 == 1 && self.is_growth_finished);

        self.fd.total_displacement = 0.0;
        self.fd.layout.graph_manager.update_bounds()?;
        self.fd.calc_spring_forces();
        self.fd
            .calc_repulsion_forces(grid_update_allowed, force_to_node_surrounding_update);
        self.fd.calc_gravitational_forces()?;
        self.fd.move_nodes();
        self.fd.animate();

        // The layout is not ended yet.
        Ok(false)
    }

    /// Upstream `CoSELayout.prototype.getPositionsData`.
    pub fn get_positions_data(&mut self) -> Vec<PositionData> {
        let all_nodes = self.fd.layout.graph_manager.get_all_nodes();

        all_nodes
            .into_iter()
            .map(|node| {
                let rect = *self.fd.layout.graph_manager.node(node).get_rect();

                PositionData {
                    id: self.fd.node(node).id.clone(),
                    x: rect.get_center_x(),
                    y: rect.get_center_y(),
                    w: rect.width,
                    h: rect.height,
                }
            })
            .collect()
    }

    /// Upstream `CoSELayout.prototype.runSpringEmbedder`: iterate until the layout ends,
    /// then refresh the bounds.
    pub fn run_spring_embedder(&mut self) -> Result<(), LayoutError>
    where
        V: Updatable,
    {
        self.initial_animation_period = 25.0;
        self.fd.layout.animation_period = self.initial_animation_period;

        // Upstream starts iterating from an animation listener when `ANIMATE` is
        // 'during'; `LayoutConstants` never defines it, so the synchronous loop is the
        // only path.
        loop {
            if self.tick()? {
                break;
            }
        }

        self.fd.layout.graph_manager.update_bounds()
    }

    /// Upstream `CoSELayout.prototype.calculateNodesToApplyGravitationTo`: the nodes of
    /// every graph that is not connected.
    pub fn calculate_nodes_to_apply_gravitation_to(&mut self) -> Vec<NodeId> {
        let mut node_list = Vec::new();

        let graphs = self.fd.layout.graph_manager.get_graphs().to_vec();

        for graph in graphs {
            self.fd.layout.graph_manager.graph_update_connected(graph);

            if !self.fd.layout.graph_manager.graph(graph).is_connected {
                node_list.extend_from_slice(&self.fd.layout.graph_manager.graph(graph).nodes);
            }
        }

        node_list
    }

    /// Upstream `CoSELayout.prototype.createBendpoints`: replace self-loops and parallel
    /// edges with dummy bendpoint nodes.
    pub fn create_bendpoints(&mut self) -> Result<(), LayoutError> {
        let edges = self.fd.layout.get_all_edges();
        let edge_count = edges.len();
        let mut visited: Vec<EdgeId> = Vec::new();

        for edge in edges {
            if !visited.contains(&edge) {
                let (source, target) = {
                    let edge = self.fd.layout.graph_manager.edge(edge);
                    (
                        edge.source
                            .ok_or(LayoutError("Source and/or target is null!"))?,
                        edge.target
                            .ok_or(LayoutError("Source and/or target is null!"))?,
                    )
                };

                if source == target {
                    {
                        let edge = self.fd.layout.graph_manager.edge_mut(edge);
                        edge.bendpoints.push(PointD::new(0.0, 0.0));
                        edge.bendpoints.push(PointD::new(0.0, 0.0));
                    }
                    self.fd.layout.create_dummy_nodes_for_bendpoints(edge)?;
                    self.fd.sync_arenas();
                    visited.push(edge);
                } else {
                    let mut edge_list = self
                        .fd
                        .layout
                        .graph_manager
                        .node_edge_list_to(source, target);
                    edge_list.extend(
                        self.fd
                            .layout
                            .graph_manager
                            .node_edge_list_to(target, source),
                    );

                    let first_visited = edge_list
                        .first()
                        .map(|&first| visited.contains(&first))
                        .unwrap_or(false);

                    if !first_visited {
                        if edge_list.len() > 1 {
                            for multi_edge in edge_list.clone() {
                                self.fd
                                    .layout
                                    .graph_manager
                                    .edge_mut(multi_edge)
                                    .bendpoints
                                    .push(PointD::new(0.0, 0.0));
                                self.fd
                                    .layout
                                    .create_dummy_nodes_for_bendpoints(multi_edge)?;
                                self.fd.sync_arenas();
                            }
                        }

                        for edge in edge_list {
                            if !visited.contains(&edge) {
                                visited.push(edge);
                            }
                        }
                    }
                }
            }

            if visited.len() == edge_count {
                break;
            }
        }

        Ok(())
    }

    /// Upstream `CoSELayout.prototype.positionNodesRadially`: place the trees of a flat
    /// forest in rows, then center the whole arrangement in the world.
    pub fn position_nodes_radially(&mut self, forest: Vec<Vec<NodeId>>) -> Result<(), LayoutError> {
        let mut current_starting_point = Point::new(0.0, 0.0);
        let number_of_columns = (forest.len() as f64).sqrt().ceil();
        let mut height = 0.0;
        let mut current_y = 0.0;
        let mut current_x = 0.0;
        let mut point = PointD::new(0.0, 0.0);

        for (i, tree) in forest.into_iter().enumerate() {
            if i as f64 % number_of_columns == 0.0 {
                // Start a new row: x back to zero, y past the tallest tree of the
                // previous row.
                current_x = 0.0;
                current_y = height;

                if i != 0 {
                    current_y += constants::DEFAULT_COMPONENT_SEPERATION;
                }

                height = 0.0;
            }

            let center_node = self
                .fd
                .layout
                .find_center_of_tree(&tree)
                .ok_or(LayoutError("assert failed"))?;

            current_starting_point.x = current_x;
            current_starting_point.y = current_y;

            point = Self::radial_layout(
                &mut self.fd.layout.graph_manager,
                &tree,
                center_node,
                &current_starting_point,
            );

            if point.y > height {
                height = point.y.floor();
            }

            current_x = (point.x + constants::DEFAULT_COMPONENT_SEPERATION).floor();
        }

        self.fd.layout.transform(Some(PointD::new(
            layout_constants::WORLD_CENTER_X - point.x / 2.0,
            layout_constants::WORLD_CENTER_Y - point.y / 2.0,
        )));

        Ok(())
    }

    /// Upstream `CoSELayout.radialLayout`: place one tree around its center, then move it
    /// to `starting_point`.
    fn radial_layout(
        manager: &mut LGraphManager<V>,
        tree: &[NodeId],
        center_node: NodeId,
        starting_point: &Point,
    ) -> PointD {
        let radial_sep =
            Self::max_diagonal_in_tree(manager, tree).max(constants::DEFAULT_RADIAL_SEPARATION);
        Self::branch_radial_layout(manager, center_node, None, 0.0, 359.0, 0.0, radial_sep);

        let bounds = manager.calc_bounds(tree);

        let mut transform = Transform::new();
        transform.set_device_org_x(bounds.get_min_x());
        transform.set_device_org_y(bounds.get_min_y());
        transform.set_world_org_x(starting_point.x);
        transform.set_world_org_y(starting_point.y);

        for &node in tree {
            manager.node_mut(node).transform(&transform);
        }

        let bottom_right = PointD::new(bounds.get_max_x(), bounds.get_max_y());

        transform.inverse_transform_point(&bottom_right)
    }

    /// Upstream `CoSELayout.branchRadialLayout`: place one node at the angle its parents
    /// gave it, then recurse into its children.
    fn branch_radial_layout(
        manager: &mut LGraphManager<V>,
        node: NodeId,
        parent_of_node: Option<NodeId>,
        start_angle: f64,
        end_angle: f64,
        distance: f64,
        radial_separation: f64,
    ) {
        let mut half_interval = ((end_angle - start_angle) + 1.0) / 2.0;

        if half_interval < 0.0 {
            half_interval += 180.0;
        }

        let node_angle = (half_interval + start_angle) % 360.0;
        let teta = (node_angle * i_geometry::TWO_PI) / 360.0;

        // The polar to Java coordinate conversion (upstream also computes an unused
        // `cos_teta`).
        let x = distance * teta.cos();
        let y = distance * teta.sin();

        manager.node_mut(node).set_center(x, y);

        let mut neighbor_edges = manager.node(node).edges.clone();
        let mut child_count = neighbor_edges.len() as f64;

        if parent_of_node.is_some() {
            child_count -= 1.0;
        }

        let mut inc_edges_count = neighbor_edges.len();

        let mut edges = match parent_of_node {
            Some(parent) => manager.node_edges_between(node, parent),
            None => Vec::new(),
        };

        // If there are multiple edges, prune them until only one remains.
        while edges.len() > 1 {
            let temp = edges.remove(0);
            if let Some(index) = neighbor_edges.iter().position(|&edge| edge == temp) {
                neighbor_edges.remove(index);
            }
            inc_edges_count -= 1;
            child_count -= 1.0;
        }

        if inc_edges_count == 0 {
            // Upstream divides by zero here; with no neighbor there is nothing to place.
            return;
        }

        let start_index = match (parent_of_node, edges.first()) {
            (Some(_), Some(&edge)) => {
                (neighbor_edges
                    .iter()
                    .position(|&candidate| candidate == edge)
                    .map(|index| index + 1)
                    .unwrap_or(0))
                    % inc_edges_count
            }
            _ => 0,
        };

        let step_angle = (end_angle - start_angle).abs() / child_count;

        let mut i = start_index;
        let mut branch_count = 0.0;
        let step_limit = neighbor_edges.len() * 4 + 8;

        for steps in 0.. {
            if branch_count == child_count {
                break;
            }

            // A neighbor list that never reaches the child count spins upstream; stop
            // after a few cycles instead.
            if steps > step_limit {
                break;
            }

            let edge = neighbor_edges[i];

            if let Ok(current_neighbor) = manager.edge_other_end(edge, node) {
                // Do not back traverse to the root node of the current tree.
                if Some(current_neighbor) != parent_of_node {
                    let child_start_angle = (start_angle + branch_count * step_angle) % 360.0;
                    let child_end_angle = (child_start_angle + step_angle) % 360.0;

                    Self::branch_radial_layout(
                        manager,
                        current_neighbor,
                        Some(node),
                        child_start_angle,
                        child_end_angle,
                        distance + radial_separation,
                        radial_separation,
                    );

                    branch_count += 1.0;
                }
            }

            i = (i + 1) % inc_edges_count;
        }
    }

    /// Upstream `CoSELayout.maxDiagonalInTree`.
    fn max_diagonal_in_tree(manager: &LGraphManager<V>, tree: &[NodeId]) -> f64 {
        let mut max_diagonal = f64::from(i32::MIN);

        for &node in tree {
            let diagonal = manager.node(node).get_diagonal();

            if diagonal > max_diagonal {
                max_diagonal = diagonal;
            }
        }

        max_diagonal
    }

    // -------------------------------------------------------------------------
    // Tiling
    // -------------------------------------------------------------------------

    /// Upstream `CoSELayout.prototype.tilingPreLayout`.
    pub fn tiling_pre_layout(&mut self) -> Result<(), LayoutError> {
        if constants::TILE {
            // Find zero-degree nodes and create a compound for each level.
            self.group_zero_degree_members()?;
            // Tile and clear the children of each compound.
            self.clear_compounds()?;
            // Separately tile and clear the zero-degree nodes of each level.
            self.clear_zero_degree_members();
        }

        Ok(())
    }

    /// Upstream `CoSELayout.prototype.tilingPostLayout`.
    pub fn tiling_post_layout(&mut self) -> Result<(), LayoutError> {
        if constants::TILE {
            self.repopulate_zero_degree_members();
            self.repopulate_compounds();
        }

        Ok(())
    }

    /// The key tiling uses for a node, upstream's `node.id` read as an object key.
    pub fn node_id_key(&self, node: NodeId) -> String {
        self.fd
            .node(node)
            .id
            .clone()
            .unwrap_or_else(|| "undefined".to_string())
    }

    /// The node that owns `node`'s graph, upstream's `node.getParent()`.
    fn node_parent(&self, node: NodeId) -> Option<NodeId> {
        self.fd.layout.graph_manager.node_parent(node)
    }

    /// Upstream `CoSELayout.prototype.groupZeroDegreeMembers`: gather the zero-degree
    /// members of each level whose parent is not to be tiled, and move them under a dummy
    /// compound when a level has at least two.
    pub fn group_zero_degree_members(&mut self) -> Result<(), LayoutError> {
        self.fd.sync_arenas();

        self.member_groups.clear();
        self.id_to_dummy_node.clear();

        let mut temp_member_groups: Vec<(String, Vec<NodeId>)> = Vec::new();
        let mut zero_degree: Vec<NodeId> = Vec::new();

        let all_nodes = self.fd.layout.get_all_nodes();

        // Fill the zero-degree list.
        for node in all_nodes {
            let parent = self.node_parent(node);
            let parent_not_to_be_tiled = match parent {
                // A node without a parent has an undefined id upstream, which passes the
                // check.
                None => true,
                Some(parent) => self.fd.node(parent).id.is_none() || !self.get_to_be_tiled(parent),
            };

            if self.get_node_degree_with_children(node) == 0 && parent_not_to_be_tiled {
                zero_degree.push(node);
            }
        }

        // Map each parent to its zero-degree members.
        for node in zero_degree {
            let parent = self.node_parent(node).ok_or(LayoutError("assert failed"))?;
            let parent_id = self.node_id_key(parent);

            match temp_member_groups
                .iter_mut()
                .find(|(id, _)| *id == parent_id)
            {
                Some((_, members)) => members.push(node),
                None => temp_member_groups.push((parent_id, vec![node])),
            }
        }

        // If there are at least two nodes at a level, create a dummy compound for them.
        for (parent_id, members) in temp_member_groups {
            if members.len() > 1 {
                let dummy_compound_id = format!("DummyCompound_{parent_id}");
                self.member_groups
                    .push((dummy_compound_id.clone(), members.clone()));

                let parent = self
                    .node_parent(members[0])
                    .ok_or(LayoutError("assert failed"))?;

                // Create a dummy compound with the calculated id.
                let dummy_compound = self.new_node(None);
                self.fd.node_mut(dummy_compound).id = Some(dummy_compound_id.clone());

                let (padding_left, padding_right, padding_bottom, padding_top) = {
                    let parent = self.fd.layout.graph_manager.node(parent);
                    (
                        parent.padding_left.unwrap_or(0.0),
                        parent.padding_right.unwrap_or(0.0),
                        parent.padding_bottom.unwrap_or(0.0),
                        parent.padding_top.unwrap_or(0.0),
                    )
                };
                {
                    let node = self.fd.layout.graph_manager.node_mut(dummy_compound);
                    node.padding_left = Some(padding_left);
                    node.padding_right = Some(padding_right);
                    node.padding_bottom = Some(padding_bottom);
                    node.padding_top = Some(padding_top);
                }

                self.id_to_dummy_node
                    .insert(dummy_compound_id, dummy_compound);

                let dummy_parent_graph = self.new_graph(None);
                let dummy_parent_graph = self
                    .fd
                    .layout
                    .graph_manager
                    .add_graph(dummy_parent_graph, dummy_compound)?;
                let parent_graph = self
                    .fd
                    .layout
                    .graph_manager
                    .node(parent)
                    .child
                    .ok_or(LayoutError("assert failed"))?;

                // Add the dummy compound to the parent graph.
                self.fd
                    .layout
                    .graph_manager
                    .graph_add_node(parent_graph, dummy_compound)?;

                // Move each zero-degree node from its parent graph to the graph of the
                // dummy parent.
                for node in members {
                    self.fd
                        .layout
                        .graph_manager
                        .graph_remove_node(parent_graph, node)?;
                    self.fd
                        .layout
                        .graph_manager
                        .graph_add_node(dummy_parent_graph, node)?;
                }
            }
        }

        Ok(())
    }

    /// Upstream `CoSELayout.prototype.clearCompounds`: remove the child graph of each
    /// compound to tile, then tile its members.
    pub fn clear_compounds(&mut self) -> Result<(), LayoutError> {
        let mut child_graph_map: Vec<(String, Vec<NodeId>)> = Vec::new();
        let mut id_to_node: HashMap<String, NodeId> = HashMap::new();

        // Compounds inner first.
        self.perform_dfs_on_compounds();

        let compound_order = self.compound_order.clone();
        for compound in compound_order {
            let id = self.node_id_key(compound);
            id_to_node.insert(id.clone(), compound);

            let child = self
                .fd
                .layout
                .graph_manager
                .node(compound)
                .child
                .ok_or(LayoutError("assert failed"))?;
            let nodes = self.fd.layout.graph_manager.graph(child).nodes.clone();
            child_graph_map.push((id, nodes));

            // Remove the children of the compound.
            self.fd.layout.graph_manager.remove_graph(child)?;
            self.fd.layout.graph_manager.node_mut(compound).child = None;
        }

        self.fd.layout.graph_manager.reset_all_nodes();

        // Tile the removed children.
        self.tile_compound_members(&child_graph_map, &id_to_node);

        Ok(())
    }

    /// Upstream `CoSELayout.prototype.clearZeroDegreeMembers`.
    pub fn clear_zero_degree_members(&mut self) {
        self.tiled_zero_degree_pack.clear();

        let member_groups = self.member_groups.clone();
        for (id, members) in member_groups {
            let Some(&compound_node) = self.id_to_dummy_node.get(&id) else {
                continue;
            };

            let (padding_left, padding_right) = {
                let node = self.fd.layout.graph_manager.node(compound_node);
                (
                    node.padding_left.unwrap_or(0.0),
                    node.padding_right.unwrap_or(0.0),
                )
            };

            let pack = self.tile_nodes(&members, padding_left + padding_right);

            // Set the width and height of the dummy compound as calculated.
            {
                let node = self.fd.layout.graph_manager.node_mut(compound_node);
                node.rect.width = pack.width;
                node.rect.height = pack.height;
            }

            self.tiled_zero_degree_pack.push((id, pack));
        }
    }

    /// Upstream `CoSELayout.prototype.repopulateCompounds`: move the tiled members of each
    /// compound back under it.
    pub fn repopulate_compounds(&mut self) {
        let packs = std::mem::take(&mut self.tiled_member_pack);

        for i in (0..self.compound_order.len()).rev() {
            let compound = self.compound_order[i];
            let id = self.node_id_key(compound);

            let (x, y, padding_left, padding_top) = {
                let node = self.fd.layout.graph_manager.node(compound);
                (
                    node.rect.x,
                    node.rect.y,
                    node.padding_left.unwrap_or(0.0),
                    node.padding_top.unwrap_or(0.0),
                )
            };

            if let Some(pack) = packs.get(&id) {
                self.adjust_locations(pack, x, y, padding_left, padding_top);
            }
        }

        self.tiled_member_pack = packs;
    }

    /// Upstream `CoSELayout.prototype.repopulateZeroDegreeMembers`.
    pub fn repopulate_zero_degree_members(&mut self) {
        let packs = std::mem::take(&mut self.tiled_zero_degree_pack);

        for (id, pack) in &packs {
            let Some(&compound_node) = self.id_to_dummy_node.get(id) else {
                continue;
            };

            let (x, y, padding_left, padding_top) = {
                let node = self.fd.layout.graph_manager.node(compound_node);
                (
                    node.rect.x,
                    node.rect.y,
                    node.padding_left.unwrap_or(0.0),
                    node.padding_top.unwrap_or(0.0),
                )
            };

            // Adjust the positions of the nodes with respect to their compound.
            self.adjust_locations(pack, x, y, padding_left, padding_top);
        }

        self.tiled_zero_degree_pack = packs;
    }

    /// Upstream `CoSELayout.prototype.getToBeTiled`.
    pub fn get_to_be_tiled(&mut self, node: NodeId) -> bool {
        let id = self.node_id_key(node);

        // First check the previous results.
        if let Some(&to_be_tiled) = self.to_be_tiled.get(&id) {
            return to_be_tiled;
        }

        // Only compound nodes are to be tiled.
        let Some(child_graph) = self.fd.layout.graph_manager.node(node).child else {
            self.to_be_tiled.insert(id, false);
            return false;
        };

        let children = self
            .fd
            .layout
            .graph_manager
            .graph(child_graph)
            .nodes
            .clone();

        // A compound node is not to be tiled if all of its compound children are not to
        // be tiled.
        for the_child in children {
            if self.get_node_degree(the_child) > 0 {
                self.to_be_tiled.insert(id, false);
                return false;
            }

            // Pass the children that have no compound structure.
            if self.fd.layout.graph_manager.node(the_child).child.is_none() {
                let child_id = self.node_id_key(the_child);
                self.to_be_tiled.insert(child_id, false);
                continue;
            }

            if !self.get_to_be_tiled(the_child) {
                self.to_be_tiled.insert(id, false);
                return false;
            }
        }

        self.to_be_tiled.insert(id, true);
        true
    }

    /// Upstream `CoSELayout.prototype.getNodeDegree`: the degree of a node, ignoring its
    /// children and its self-loops. Upstream compares the v-level ids, not the nodes.
    pub fn get_node_degree(&self, node: NodeId) -> i64 {
        let edges = self.fd.layout.graph_manager.node(node).edges.clone();
        let mut degree = 0;

        for edge in edges {
            let (source, target) = {
                let edge = self.fd.layout.graph_manager.edge(edge);
                (edge.source, edge.target)
            };

            if let (Some(source), Some(target)) = (source, target)
                && self.node_id_key(source) != self.node_id_key(target)
            {
                degree += 1;
            }
        }

        degree
    }

    /// Upstream `CoSELayout.prototype.getNodeDegreeWithChildren`: the degree of a node
    /// plus the degrees of its descendants.
    pub fn get_node_degree_with_children(&self, node: NodeId) -> i64 {
        let mut degree = self.get_node_degree(node);

        let Some(child_graph) = self.fd.layout.graph_manager.node(node).child else {
            return degree;
        };

        let children = self
            .fd
            .layout
            .graph_manager
            .graph(child_graph)
            .nodes
            .clone();
        for child in children {
            degree += self.get_node_degree_with_children(child);
        }

        degree
    }

    /// Upstream `CoSELayout.prototype.performDFSOnCompounds`.
    pub fn perform_dfs_on_compounds(&mut self) {
        self.compound_order = Vec::new();

        let root = self.fd.layout.graph_manager.get_root();
        if let Some(root) = root {
            let nodes = self.fd.layout.graph_manager.graph(root).nodes.clone();
            self.fill_compound_order_by_dfs(&nodes);
        }
    }

    /// Upstream `CoSELayout.prototype.fillCompexOrderByDFS`.
    fn fill_compound_order_by_dfs(&mut self, children: &[NodeId]) {
        for &child in children {
            let child_graph = self.fd.layout.graph_manager.node(child).child;
            if let Some(child_graph) = child_graph {
                let nodes = self
                    .fd
                    .layout
                    .graph_manager
                    .graph(child_graph)
                    .nodes
                    .clone();
                self.fill_compound_order_by_dfs(&nodes);
            }

            if self.get_to_be_tiled(child) {
                self.compound_order.push(child);
            }
        }
    }

    /// Upstream `CoSELayout.prototype.adjustLocations`: place the rows of `organization`
    /// starting at `(x, y)` plus the compound's padding.
    pub fn adjust_locations(
        &mut self,
        organization: &Organization,
        x: f64,
        y: f64,
        compound_horizontal_margin: f64,
        compound_vertical_margin: f64,
    ) {
        let mut x = x + compound_horizontal_margin;
        let mut y = y + compound_vertical_margin;

        let left = x;

        for row in &organization.rows {
            x = left;
            let mut max_height = 0.0;

            for &lnode in row {
                let node = self.fd.layout.graph_manager.node_mut(lnode);
                node.rect.x = x;
                node.rect.y = y;

                x += node.rect.width + organization.horizontal_padding;

                if node.rect.height > max_height {
                    max_height = node.rect.height;
                }
            }

            y += max_height + organization.vertical_padding;
        }
    }

    /// Upstream `CoSELayout.prototype.tileCompoundMembers`.
    pub fn tile_compound_members(
        &mut self,
        child_graph_map: &[(String, Vec<NodeId>)],
        id_to_node: &HashMap<String, NodeId>,
    ) {
        self.tiled_member_pack.clear();

        for (id, nodes) in child_graph_map {
            let Some(&compound_node) = id_to_node.get(id) else {
                continue;
            };

            let (padding_left, padding_right) = {
                let node = self.fd.layout.graph_manager.node(compound_node);
                (
                    node.padding_left.unwrap_or(0.0),
                    node.padding_right.unwrap_or(0.0),
                )
            };

            let pack = self.tile_nodes(nodes, padding_left + padding_right);

            {
                let node = self.fd.layout.graph_manager.node_mut(compound_node);
                node.rect.width = pack.width;
                node.rect.height = pack.height;
            }

            self.tiled_member_pack.insert(id.clone(), pack);
        }
    }

    /// Upstream `CoSELayout.prototype.tileNodes`: lay the nodes out in rows that keep the
    /// aspect ratio near one.
    pub fn tile_nodes(&mut self, nodes: &[NodeId], min_width: f64) -> Organization {
        let vertical_padding = self.tiling_padding_vertical;
        let horizontal_padding = self.tiling_padding_horizontal;

        let mut organization = Organization {
            rows: Vec::new(),
            row_width: Vec::new(),
            row_height: Vec::new(),
            width: 0.0,
            // Assume minHeight equals minWidth.
            height: min_width,
            vertical_padding,
            horizontal_padding,
        };

        // Sort the nodes in ascending order of their areas.
        let mut sorted_nodes = nodes.to_vec();
        sorted_nodes.sort_by(|&n1, &n2| {
            let area_1 = {
                let rect = self.fd.layout.graph_manager.node(n1).get_rect();
                rect.width * rect.height
            };
            let area_2 = {
                let rect = self.fd.layout.graph_manager.node(n2).get_rect();
                rect.width * rect.height
            };

            if area_1 > area_2 {
                std::cmp::Ordering::Less
            } else if area_1 < area_2 {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });

        // Create the organization and tile the members.
        for l_node in sorted_nodes {
            if organization.rows.is_empty() {
                self.insert_node_to_row(&mut organization, l_node, 0, min_width);
            } else {
                let rect = *self.fd.layout.graph_manager.node(l_node).get_rect();

                if self.can_add_horizontal(&organization, rect.width, rect.height) {
                    let shortest = self.get_shortest_row_index(&organization).unwrap_or(0);
                    self.insert_node_to_row(&mut organization, l_node, shortest, min_width);
                } else {
                    let last = organization.rows.len();
                    self.insert_node_to_row(&mut organization, l_node, last, min_width);
                }
            }

            self.shift_to_last_row(&organization);
        }

        organization
    }

    /// Upstream `CoSELayout.prototype.insertNodeToRow`.
    pub fn insert_node_to_row(
        &mut self,
        organization: &mut Organization,
        node: NodeId,
        row_index: usize,
        min_width: f64,
    ) {
        let min_compound_size = min_width;

        // Add a new row if needed.
        if row_index == organization.rows.len() {
            organization.rows.push(Vec::new());
            organization.row_width.push(min_compound_size);
            organization.row_height.push(0.0);
        }

        let rect = *self.fd.layout.graph_manager.node(node).get_rect();

        // Update the row width.
        let mut w = organization.row_width[row_index] + rect.width;

        if !organization.rows[row_index].is_empty() {
            w += organization.horizontal_padding;
        }

        organization.row_width[row_index] = w;

        // Update the compound width.
        if organization.width < w {
            organization.width = w;
        }

        // Update the height.
        let mut h = rect.height;
        if row_index > 0 {
            h += organization.vertical_padding;
        }

        let mut extra_height = 0.0;
        if h > organization.row_height[row_index] {
            extra_height = organization.row_height[row_index];
            organization.row_height[row_index] = h;
            extra_height = organization.row_height[row_index] - extra_height;
        }

        organization.height += extra_height;

        // Insert the node.
        organization.rows[row_index].push(node);
    }

    /// Upstream `CoSELayout.prototype.getShortestRowIndex`.
    pub fn get_shortest_row_index(&self, organization: &Organization) -> Option<usize> {
        let mut shortest = None;
        let mut min = f64::MAX;

        for (i, &row_width) in organization.row_width.iter().enumerate() {
            if row_width < min {
                shortest = Some(i);
                min = row_width;
            }
        }

        shortest
    }

    /// Upstream `CoSELayout.prototype.getLongestRowIndex`.
    pub fn get_longest_row_index(&self, organization: &Organization) -> Option<usize> {
        let mut longest = None;
        let mut max = f64::MIN_POSITIVE;

        for (i, &row_width) in organization.row_width.iter().enumerate() {
            if row_width > max {
                longest = Some(i);
                max = row_width;
            }
        }

        longest
    }

    /// Upstream `CoSELayout.prototype.canAddHorizontal`: whether adding a node of
    /// `extra_width` by `extra_height` keeps the aspect ratio closer to one than adding a
    /// new row does.
    pub fn can_add_horizontal(
        &self,
        organization: &Organization,
        extra_width: f64,
        extra_height: f64,
    ) -> bool {
        let Some(shortest) = self.get_shortest_row_index(organization) else {
            return true;
        };

        let min = organization.row_width[shortest];

        if min + organization.horizontal_padding + extra_width <= organization.width {
            return true;
        }

        let mut height_diff = 0.0;

        // Adding to an existing row.
        if organization.row_height[shortest] < extra_height && shortest > 0 {
            height_diff =
                extra_height + organization.vertical_padding - organization.row_height[shortest];
        }

        let add_to_row_ratio =
            if organization.width - min >= extra_width + organization.horizontal_padding {
                (organization.height + height_diff)
                    / (min + extra_width + organization.horizontal_padding)
            } else {
                (organization.height + height_diff) / organization.width
            };

        // Adding a new row for this node.
        height_diff = extra_height + organization.vertical_padding;
        let mut add_new_row_ratio = if organization.width < extra_width {
            (organization.height + height_diff) / extra_width
        } else {
            (organization.height + height_diff) / organization.width
        };

        if add_new_row_ratio < 1.0 {
            add_new_row_ratio = 1.0 / add_new_row_ratio;
        }

        let mut add_to_row_ratio = add_to_row_ratio;

        if add_to_row_ratio < 1.0 {
            add_to_row_ratio = 1.0 / add_to_row_ratio;
        }

        add_to_row_ratio < add_new_row_ratio
    }

    /// Upstream `CoSELayout.prototype.shiftToLastRow`. Its guard reads `node.width`,
    /// which `LNode` does not define, so `diff` is `NaN` and the body never runs; the
    /// port keeps the method without its dead body.
    pub fn shift_to_last_row(&mut self, _organization: &Organization) {}

    // -------------------------------------------------------------------------
    // Tree reduction
    // -------------------------------------------------------------------------

    /// Upstream `CoSELayout.prototype.reduceTrees`: repeatedly remove the leaves, one step
    /// at a time, remembering the node, its edge, and its owner.
    pub fn reduce_trees(&mut self) -> Result<(), LayoutError> {
        let mut pruned_nodes_all: Vec<Vec<PrunedNode>> = Vec::new();
        let mut contains_leaf = true;

        while contains_leaf {
            let all_nodes = self.fd.layout.get_all_nodes();
            let mut pruned_nodes_in_step_temp: Vec<PrunedNode> = Vec::new();
            contains_leaf = false;

            for node in all_nodes {
                let (edges, child) = {
                    let node = self.fd.layout.graph_manager.node(node);
                    (node.edges.clone(), node.child)
                };

                if edges.len() == 1
                    && !self.fd.layout.graph_manager.edge(edges[0]).is_inter_graph
                    && child.is_none()
                {
                    let owner = self
                        .fd
                        .layout
                        .graph_manager
                        .node(node)
                        .owner
                        .ok_or(LayoutError("assert failed"))?;

                    pruned_nodes_in_step_temp.push(PrunedNode {
                        node,
                        edge: edges[0],
                        owner,
                    });
                    contains_leaf = true;
                }
            }

            if contains_leaf {
                let mut pruned_nodes_in_step: Vec<PrunedNode> = Vec::new();

                for data in pruned_nodes_in_step_temp {
                    if self.fd.layout.graph_manager.node(data.node).edges.len() == 1 {
                        pruned_nodes_in_step.push(data);
                        self.fd
                            .layout
                            .graph_manager
                            .graph_remove_node(data.owner, data.node)?;
                    }
                }

                pruned_nodes_all.push(pruned_nodes_in_step);

                self.fd.layout.graph_manager.reset_all_nodes();
                self.fd.layout.graph_manager.reset_all_edges();
            }
        }

        self.pruned_nodes_all = pruned_nodes_all;

        Ok(())
    }

    /// Upstream `CoSELayout.prototype.growTree`: put the nodes of the last pruned step
    /// back, each beside the node it connects to.
    pub fn grow_tree(&mut self) -> Result<(), LayoutError> {
        let Some(pruned_nodes_in_step) = self.pruned_nodes_all.last().cloned() else {
            return Ok(());
        };

        for data in &pruned_nodes_in_step {
            self.find_place_for_pruned_node(data)?;

            let (source, target) = {
                let edge = self.fd.layout.graph_manager.edge(data.edge);
                (edge.source, edge.target)
            };

            self.fd
                .layout
                .graph_manager
                .graph_add_node(data.owner, data.node)?;

            if let (Some(source), Some(target)) = (source, target) {
                self.fd
                    .layout
                    .graph_manager
                    .graph_add_edge(data.owner, data.edge, source, target)?;
            }
        }

        self.pruned_nodes_all.pop();
        self.fd.layout.graph_manager.reset_all_nodes();
        self.fd.layout.graph_manager.reset_all_edges();

        Ok(())
    }

    /// Upstream `CoSELayout.prototype.findPlaceforPrunedNode`: place a pruned node in the
    /// least occupied direction around the node it connects to.
    pub fn find_place_for_pruned_node(
        &mut self,
        node_data: &PrunedNode,
    ) -> Result<(), LayoutError> {
        self.fd.sync_arenas();

        let pruned_node = node_data.node;

        let node_to_connect = {
            let edge = self.fd.layout.graph_manager.edge(node_data.edge);

            if edge.source == Some(pruned_node) {
                edge.target
            } else {
                edge.source
            }
            .ok_or(LayoutError("assert failed"))?
        };

        let (start_grid_x, finish_grid_x, start_grid_y, finish_grid_y) = {
            let node = self.fd.node(node_to_connect);
            (node.start_x, node.finish_x, node.start_y, node.finish_y)
        };

        let mut control_regions = [0.0f64; 4];

        if start_grid_y > 0 {
            for i in start_grid_x..=finish_grid_x {
                control_regions[0] +=
                    self.grid_len(i, start_grid_y - 1) + self.grid_len(i, start_grid_y) - 1.0;
            }
        }
        if finish_grid_x < self.fd.grid.len() as i32 - 1 {
            for i in start_grid_y..=finish_grid_y {
                control_regions[1] +=
                    self.grid_len(finish_grid_x + 1, i) + self.grid_len(finish_grid_x, i) - 1.0;
            }
        }
        if finish_grid_y < self.fd.grid.first().map_or(0, |column| column.len()) as i32 - 1 {
            for i in start_grid_x..=finish_grid_x {
                control_regions[2] +=
                    self.grid_len(i, finish_grid_y + 1) + self.grid_len(i, finish_grid_y) - 1.0;
            }
        }
        if start_grid_x > 0 {
            for i in start_grid_y..=finish_grid_y {
                control_regions[3] +=
                    self.grid_len(start_grid_x - 1, i) + self.grid_len(start_grid_x, i) - 1.0;
            }
        }

        let mut min = f64::from(i32::MAX);
        let mut min_count = 0;
        let mut min_index = 0;

        for (j, &region) in control_regions.iter().enumerate() {
            if region < min {
                min = region;
                min_count = 1;
                min_index = j;
            } else if region == min {
                min_count += 1;
            }
        }

        let grid_for_pruned_node: usize;

        if min_count == 3 && min == 0.0 {
            if control_regions[0] == 0.0 && control_regions[1] == 0.0 && control_regions[2] == 0.0 {
                grid_for_pruned_node = 1;
            } else if control_regions[0] == 0.0
                && control_regions[1] == 0.0
                && control_regions[3] == 0.0
            {
                grid_for_pruned_node = 0;
            } else if control_regions[0] == 0.0
                && control_regions[2] == 0.0
                && control_regions[3] == 0.0
            {
                grid_for_pruned_node = 3;
            } else if control_regions[1] == 0.0
                && control_regions[2] == 0.0
                && control_regions[3] == 0.0
            {
                grid_for_pruned_node = 2;
            } else {
                grid_for_pruned_node = min_index;
            }
        } else if min_count == 2 && min == 0.0 {
            // Upstream calls `Math.random()`; the port uses the shared seed so the run
            // repeats.
            let random = (random_seed::next_double() * 2.0).floor() as i32;

            if control_regions[0] == 0.0 && control_regions[1] == 0.0 {
                grid_for_pruned_node = if random == 0 { 0 } else { 1 };
            } else if control_regions[0] == 0.0 && control_regions[2] == 0.0 {
                grid_for_pruned_node = if random == 0 { 0 } else { 2 };
            } else if control_regions[0] == 0.0 && control_regions[3] == 0.0 {
                grid_for_pruned_node = if random == 0 { 0 } else { 3 };
            } else if control_regions[1] == 0.0 && control_regions[2] == 0.0 {
                grid_for_pruned_node = if random == 0 { 1 } else { 2 };
            } else if control_regions[1] == 0.0 && control_regions[3] == 0.0 {
                grid_for_pruned_node = if random == 0 { 1 } else { 3 };
            } else if random == 0 {
                grid_for_pruned_node = 2;
            } else {
                grid_for_pruned_node = 3;
            }
        } else if min_count == 4 && min == 0.0 {
            let random = (random_seed::next_double() * 4.0).floor() as i32;
            grid_for_pruned_node = random as usize;
        } else {
            grid_for_pruned_node = min_index;
        }

        let (center_x, center_y, width, height, pruned_width, pruned_height) = {
            let node_to_connect = self.fd.layout.graph_manager.node(node_to_connect);
            let pruned_node = self.fd.layout.graph_manager.node(pruned_node);
            (
                node_to_connect.get_center_x(),
                node_to_connect.get_center_y(),
                node_to_connect.get_width(),
                node_to_connect.get_height(),
                pruned_node.get_width(),
                pruned_node.get_height(),
            )
        };

        let (x, y) = match grid_for_pruned_node {
            0 => (
                center_x,
                center_y - height / 2.0 - constants::DEFAULT_EDGE_LENGTH - pruned_height / 2.0,
            ),
            1 => (
                center_x + width / 2.0 + constants::DEFAULT_EDGE_LENGTH + pruned_width / 2.0,
                center_y,
            ),
            2 => (
                center_x,
                center_y + height / 2.0 + constants::DEFAULT_EDGE_LENGTH + pruned_height / 2.0,
            ),
            _ => (
                center_x - width / 2.0 - constants::DEFAULT_EDGE_LENGTH - pruned_width / 2.0,
                center_y,
            ),
        };

        self.fd
            .layout
            .graph_manager
            .node_mut(pruned_node)
            .set_center(x, y);

        Ok(())
    }

    /// The number of nodes in one grid cell. Upstream indexes the array directly and
    /// throws outside it; the layout keeps the node inside the grid, and the port returns
    /// zero instead of panicking if it does not.
    fn grid_len(&self, x: i32, y: i32) -> f64 {
        if x < 0 || y < 0 {
            return 0.0;
        }

        self.fd
            .grid
            .get(x as usize)
            .and_then(|column| column.get(y as usize))
            .map_or(0.0, |cell| cell.len() as f64)
    }

    /// Upstream `Layout.prototype.runLayout`, with this class's tiling overrides. See the
    /// module doc for why the sequence is repeated here.
    pub fn run_layout(&mut self) -> Result<bool, LayoutError>
    where
        V: Updatable,
    {
        self.fd.sync_arenas();
        self.fd.layout.is_layout_finished = false;

        self.tiling_pre_layout()?;

        self.init_parameters();

        let is_layout_successful = if self.fd.layout.check_layout_success() {
            false
        } else {
            self.layout()?
        };

        if is_layout_successful && !self.fd.layout.is_sub_layout {
            self.fd.layout.do_post_layout();
        }

        self.tiling_post_layout()?;

        self.fd.layout.is_layout_finished = true;

        Ok(is_layout_successful)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::layout_base::util::dimension_d::DimensionD;
    use crate::layout::layout_base::util::point::Point;

    // The expected values below were printed by upstream's JavaScript:
    // node -e 'require(".../ref.js")' over the same graph, with RandomSeed.seed = 1.

    fn build(
        specs: &[(&str, f64, f64, f64, f64)],
        edges: &[(&str, &str)],
    ) -> (CoSELayout<()>, HashMap<String, NodeId>) {
        let mut layout: CoSELayout<()> = CoSELayout::new();
        layout.new_graph_manager();
        let root = layout.fd.add_root();

        let mut nodes = HashMap::new();
        for &(id, x, y, width, height) in specs {
            let node = layout.new_node(None);
            layout
                .fd
                .layout
                .graph_manager
                .graph_add_node(root, node)
                .unwrap();
            layout
                .fd
                .layout
                .graph_manager
                .node_mut(node)
                .set_rect(&Point::new(x, y), &DimensionD::new(width, height));
            layout.fd.node_mut(node).id = Some(id.to_string());
            nodes.insert(id.to_string(), node);
        }

        for &(source, target) in edges {
            let edge = layout.new_edge(None);
            layout
                .fd
                .layout
                .graph_manager
                .add_edge(edge, nodes[source], nodes[target])
                .unwrap();
        }

        (layout, nodes)
    }

    fn tree() -> (CoSELayout<()>, HashMap<String, NodeId>) {
        build(
            &[
                ("a", -20.0, -20.0, 40.0, 40.0),
                ("b", 80.0, -20.0, 40.0, 40.0),
                ("c", 180.0, -20.0, 40.0, 40.0),
                ("d", 280.0, -20.0, 40.0, 40.0),
                ("e", 130.0, -120.0, 40.0, 40.0),
                ("f", 130.0, 80.0, 40.0, 40.0),
            ],
            &[("a", "b"), ("b", "c"), ("c", "d"), ("b", "e"), ("b", "f")],
        )
    }

    #[test]
    fn a_six_node_tree_matches_upstream() {
        let (mut layout, nodes) = tree();

        assert!(layout.run_layout().unwrap());

        // Upstream reaches its convergence check at iteration 100.
        assert_eq!(layout.fd.total_iterations, 100);
        assert!(
            (layout.fd.total_displacement - 0.1587644779147852).abs() < 1e-6,
            "{}",
            layout.fd.total_displacement
        );

        let expected = [
            ("a", 15.0, 153.10550661926573),
            ("b", 109.38315062716492, 169.5893211323421),
            ("c", 123.59448798009294, 75.01220767498944),
            ("d", 207.00600867681646, 15.0),
            ("e", 204.07068739693045, 182.73690018946536),
            ("f", 95.02073644916891, 264.1750257244985),
        ];

        for (id, x, y) in expected {
            let rect = *layout.fd.layout.graph_manager.node(nodes[id]).get_rect();
            assert!(
                (rect.x - x).abs() < 1e-6 && (rect.y - y).abs() < 1e-6,
                "{id}: ({}, {}) != ({x}, {y})",
                rect.x,
                rect.y
            );
            assert_eq!((rect.width, rect.height), (40.0, 40.0));
        }
    }

    #[test]
    fn a_flat_forest_is_placed_radially() {
        let (mut layout, nodes) = tree();

        // The classic layout positions a forest radially before the embedder runs.
        let forest = layout.fd.layout.get_flat_forest();
        assert_eq!(forest.len(), 1);
        layout.position_nodes_radially(forest).unwrap();

        // The tree center is `d`: it is the node left after removing the leaves.
        assert_eq!(
            layout.fd.layout.find_center_of_tree(&forest_nodes(&layout)),
            Some(nodes["d"])
        );
    }

    fn forest_nodes(layout: &CoSELayout<()>) -> Vec<NodeId> {
        let root = layout.fd.layout.graph_manager.get_root().unwrap();
        layout.fd.layout.graph_manager.graph(root).nodes.clone()
    }

    #[test]
    fn tiling_places_three_isolated_nodes_like_upstream() {
        // The scatter of `positionNodesRandomly` starts from the shared seed, as upstream
        // does in layout-base's RandomSeed.
        random_seed::set_seed(1);

        let (mut layout, nodes) = build(
            &[
                ("a", -20.0, -20.0, 40.0, 40.0),
                ("b", 100.0, -20.0, 40.0, 40.0),
                ("c", 220.0, -20.0, 40.0, 40.0),
            ],
            &[],
        );

        assert!(layout.run_layout().unwrap());
        assert_eq!(layout.fd.total_iterations, 100);

        let expected = [("a", 15.0, 15.0), ("b", 65.0, 15.0), ("c", 15.0, 65.0)];

        for (id, x, y) in expected {
            let rect = *layout.fd.layout.graph_manager.node(nodes[id]).get_rect();
            assert!(
                (rect.x - x).abs() < 1e-6 && (rect.y - y).abs() < 1e-6,
                "{id}: ({}, {}) != ({x}, {y})",
                rect.x,
                rect.y
            );
        }
    }

    #[test]
    fn tiling_organizes_three_nodes_by_area() {
        let (mut layout, nodes) = build(
            &[
                ("a", 0.0, 0.0, 40.0, 20.0),
                ("b", 0.0, 0.0, 60.0, 40.0),
                ("c", 0.0, 0.0, 20.0, 60.0),
            ],
            &[],
        );

        // Largest first: b (2400), c (1200), a (800).
        let organization = layout.tile_nodes(&[nodes["a"], nodes["b"], nodes["c"]], 0.0);

        assert_eq!(
            organization.rows,
            vec![vec![nodes["b"], nodes["c"]], vec![nodes["a"]]]
        );
        assert_eq!(organization.row_width, vec![90.0, 40.0]);
        assert_eq!(organization.row_height, vec![60.0, 30.0]);
        assert_eq!(organization.width, 90.0);
        assert_eq!(organization.height, 90.0);
    }
}
