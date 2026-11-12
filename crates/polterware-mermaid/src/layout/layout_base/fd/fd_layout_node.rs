//! Port of `layout-base/src/fd/FDLayoutNode.js`.
//!
//! Upstream's `FDLayoutNode` extends `LNode` with the forces of the spring embedder, the
//! displacement of the current iteration, the grid cells the node falls into, and its
//! geometric neighbors. The port keeps the `LNode` half in the manager's arena
//! ([`super::super::l_node::LNode`]) and these extra fields in a parallel arena indexed by
//! the same [`NodeId`].
//!
//! `id` is the property cose-bilkent attaches to its l-level nodes; cose-base itself never
//! sets it, so it is `None` there. Tiling reads it (through the object-key coercion of
//! upstream's plain objects), which is why it is modeled as a string.

use super::super::l_node::NodeId;
use super::fd_layout_constants as constants;

/// The force-directed half of a node, upstream's `FDLayoutNode`.
#[derive(Clone, Debug, PartialEq)]
pub struct FDLayoutNode {
    /// The v-level id cose-bilkent attaches, upstream's dynamic `id`. `None` behaves as
    /// JavaScript's `undefined`.
    pub id: Option<String>,
    /// Spring force on x, upstream's `springForceX`.
    pub spring_force_x: f64,
    /// Spring force on y, upstream's `springForceY`.
    pub spring_force_y: f64,
    /// Repulsion force on x, upstream's `repulsionForceX`.
    pub repulsion_force_x: f64,
    /// Repulsion force on y, upstream's `repulsionForceY`.
    pub repulsion_force_y: f64,
    /// Gravitation force on x, upstream's `gravitationForceX`.
    pub gravitation_force_x: f64,
    /// Gravitation force on y, upstream's `gravitationForceY`.
    pub gravitation_force_y: f64,
    /// Movement of this iteration on x, upstream's `displacementX`.
    pub displacement_x: f64,
    /// Movement of this iteration on y, upstream's `displacementY`.
    pub displacement_y: f64,
    /// First grid column the node falls into, upstream's `startX`.
    pub start_x: i32,
    /// Last grid column the node falls into, upstream's `finishX`.
    pub finish_x: i32,
    /// First grid row the node falls into, upstream's `startY`.
    pub start_y: i32,
    /// Last grid row the node falls into, upstream's `finishY`.
    pub finish_y: i32,
    /// Nodes close enough to repulse this one, upstream's `surrounding`.
    pub surrounding: Vec<NodeId>,
    /// Number of nodes this one stands for, upstream's `noOfChildren`. The constructor
    /// leaves it `undefined`; `calcNoOfChildrenForAllNodes` fills it before the first
    /// force calculation, so the port starts it at zero.
    pub no_of_children: i64,
    /// Repulsion this node contributes, layout-base 2.0.1's `nodeRepulsion`.
    pub node_repulsion: f64,
    /// Weight of a compound that holds fixed nodes, cose-base 2.x's `fixedNodeWeight`.
    /// `None` behaves as JavaScript's `undefined`.
    pub fixed_node_weight: Option<f64>,
}

impl Default for FDLayoutNode {
    fn default() -> Self {
        Self {
            id: None,
            spring_force_x: 0.0,
            spring_force_y: 0.0,
            repulsion_force_x: 0.0,
            repulsion_force_y: 0.0,
            gravitation_force_x: 0.0,
            gravitation_force_y: 0.0,
            displacement_x: 0.0,
            displacement_y: 0.0,
            start_x: 0,
            finish_x: 0,
            start_y: 0,
            finish_y: 0,
            surrounding: Vec::new(),
            no_of_children: 0,
            node_repulsion: constants::DEFAULT_REPULSION_STRENGTH,
            fixed_node_weight: None,
        }
    }
}

impl FDLayoutNode {
    /// An empty node.
    pub fn new() -> Self {
        Self::default()
    }

    /// Upstream `setGridCoordinates`.
    pub fn set_grid_coordinates(
        &mut self,
        start_x: i32,
        finish_x: i32,
        start_y: i32,
        finish_y: i32,
    ) {
        self.start_x = start_x;
        self.finish_x = finish_x;
        self.start_y = start_y;
        self.finish_y = finish_y;
    }
}
