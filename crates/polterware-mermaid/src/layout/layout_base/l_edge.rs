//! Port of `layout-base/src/LEdge.js`.
//!
//! An edge: its endpoints, whether it crosses graphs, its bendpoints, and its length.
//! Upstream has both a field and a method named `isInterGraph` and
//! `isOverlapingSourceAndTarget`; the port keeps both, the method reading the field.
//! `length`, `lengthX`, and `lengthY` are `undefined` until [`LGraphManager::edge_update_length`]
//! runs; they start at zero here and no caller reads them before that.

use super::LayoutError;
use super::l_graph_manager::LGraphManager;
use super::l_graph_object::LGraphObject;
use super::l_node::NodeId;
use super::util::i_geometry;
use super::util::i_math;
use super::util::point_d::PointD;

/// Index of an [`LEdge`] in [`LGraphManager`]'s edge arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EdgeId(pub usize);

/// An edge, upstream's `LEdge`.
#[derive(Clone, Debug, PartialEq)]
pub struct LEdge<V> {
    /// Upstream's `LGraphObject` base.
    pub base: LGraphObject<V>,
    /// The source node, upstream's `source`.
    pub source: Option<NodeId>,
    /// The target node, upstream's `target`.
    pub target: Option<NodeId>,
    /// Whether the edge crosses graphs, upstream's `isInterGraph`.
    pub is_inter_graph: bool,
    /// Whether the endpoints overlap, upstream's `isOverlapingSourceAndTarget`.
    pub is_overlaping_source_and_target: bool,
    /// Bendpoints in order, upstream's `bendpoints`.
    pub bendpoints: Vec<PointD>,
    /// Length, upstream's `length`.
    pub length: f64,
    /// Length in x, upstream's `lengthX`.
    pub length_x: f64,
    /// Length in y, upstream's `lengthY`.
    pub length_y: f64,
    /// The lowest common ancestor graph of the endpoints, upstream's `lca`.
    pub lca: Option<super::l_graph::GraphId>,
    /// The source's ancestor inside the lca, upstream's `sourceInLca`.
    pub source_in_lca: Option<NodeId>,
    /// The target's ancestor inside the lca, upstream's `targetInLca`.
    pub target_in_lca: Option<NodeId>,
}

impl<V> Default for LEdge<V> {
    fn default() -> Self {
        Self {
            base: LGraphObject::default(),
            source: None,
            target: None,
            is_inter_graph: false,
            is_overlaping_source_and_target: false,
            bendpoints: Vec::new(),
            length: 0.0,
            length_x: 0.0,
            length_y: 0.0,
            lca: None,
            source_in_lca: None,
            target_in_lca: None,
        }
    }
}

impl<V> LEdge<V> {
    /// An edge between `source` and `target`, upstream's constructor.
    pub fn new(source: Option<NodeId>, target: Option<NodeId>, v_edge: Option<V>) -> Self {
        Self {
            base: LGraphObject::new(v_edge),
            source,
            target,
            ..Self::default()
        }
    }

    /// Upstream `getSource`.
    pub fn get_source(&self) -> Option<NodeId> {
        self.source
    }

    /// Upstream `getTarget`.
    pub fn get_target(&self) -> Option<NodeId> {
        self.target
    }

    /// Upstream `isInterGraph`.
    pub fn is_inter_graph(&self) -> bool {
        self.is_inter_graph
    }

    /// Upstream `getLength`.
    pub fn get_length(&self) -> f64 {
        self.length
    }

    /// Upstream `isOverlapingSourceAndTarget`.
    pub fn is_overlaping_source_and_target(&self) -> bool {
        self.is_overlaping_source_and_target
    }

    /// Upstream `getBendpoints`.
    pub fn get_bendpoints(&self) -> &[PointD] {
        &self.bendpoints
    }

    /// Upstream `getLca`.
    pub fn get_lca(&self) -> Option<super::l_graph::GraphId> {
        self.lca
    }

    /// Upstream `getSourceInLca`.
    pub fn get_source_in_lca(&self) -> Option<NodeId> {
        self.source_in_lca
    }

    /// Upstream `getTargetInLca`.
    pub fn get_target_in_lca(&self) -> Option<NodeId> {
        self.target_in_lca
    }
}

impl<V> LGraphManager<V> {
    /// Upstream `LEdge.prototype.getOtherEnd`.
    pub fn edge_other_end(&self, edge: EdgeId, node: NodeId) -> Result<NodeId, LayoutError> {
        let edge = self.edge(edge);
        if edge.source == Some(node) {
            edge.target
                .ok_or(LayoutError("Node is not incident with this edge"))
        } else if edge.target == Some(node) {
            edge.source
                .ok_or(LayoutError("Node is not incident with this edge"))
        } else {
            Err(LayoutError("Node is not incident with this edge"))
        }
    }

    /// Upstream `LEdge.prototype.getOtherEndInGraph`: the other end of `edge` as seen
    /// from `graph`, walking up the inclusion tree.
    pub fn edge_other_end_in_graph(
        &self,
        edge: EdgeId,
        node: NodeId,
        graph: super::l_graph::GraphId,
    ) -> Option<NodeId> {
        let mut other_end = self.edge_other_end(edge, node).ok()?;
        let root = self.get_root();

        loop {
            let owner = self.node(other_end).owner?;

            if owner == graph {
                return Some(other_end);
            }

            if Some(owner) == root {
                break;
            }

            other_end = self.graph(owner).parent?;
        }

        None
    }

    /// Upstream `LEdge.prototype.updateLength`: the clipping points of the segment
    /// between the endpoints' centers, and its length. Overlapping endpoints leave the
    /// lengths untouched.
    pub fn edge_update_length(&mut self, edge: EdgeId) {
        let (source, target) = {
            let edge = self.edge(edge);
            (edge.source, edge.target)
        };
        let (Some(source), Some(target)) = (source, target) else {
            return;
        };

        let mut clip_point_coordinates = [0.0; 4];

        let overlapping = i_geometry::get_intersection2(
            self.node(target).get_rect(),
            self.node(source).get_rect(),
            &mut clip_point_coordinates,
        );

        let edge = self.edge_mut(edge);
        edge.is_overlaping_source_and_target = overlapping;

        if !edge.is_overlaping_source_and_target {
            edge.length_x = clip_point_coordinates[0] - clip_point_coordinates[2];
            edge.length_y = clip_point_coordinates[1] - clip_point_coordinates[3];

            if edge.length_x.abs() < 1.0 {
                edge.length_x = i_math::sign(edge.length_x);
            }

            if edge.length_y.abs() < 1.0 {
                edge.length_y = i_math::sign(edge.length_y);
            }

            edge.length = (edge.length_x * edge.length_x + edge.length_y * edge.length_y).sqrt();
        }
    }

    /// Upstream `LEdge.prototype.updateLengthSimple`: the length between the centers.
    pub fn edge_update_length_simple(&mut self, edge: EdgeId) {
        let (source, target) = {
            let edge = self.edge(edge);
            (edge.source, edge.target)
        };
        let (Some(source), Some(target)) = (source, target) else {
            return;
        };

        let length_x = self.node(target).get_center_x() - self.node(source).get_center_x();
        let length_y = self.node(target).get_center_y() - self.node(source).get_center_y();

        let edge = self.edge_mut(edge);
        edge.length_x = if length_x.abs() < 1.0 {
            i_math::sign(length_x)
        } else {
            length_x
        };
        edge.length_y = if length_y.abs() < 1.0 {
            i_math::sign(length_y)
        } else {
            length_y
        };

        edge.length = (edge.length_x * edge.length_x + edge.length_y * edge.length_y).sqrt();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::layout_base::l_graph::GraphId;
    use crate::layout::layout_base::util::dimension_d::DimensionD;
    use crate::layout::layout_base::util::point::Point;

    fn manager() -> (LGraphManager<()>, GraphId) {
        let mut manager = LGraphManager::new();
        let root = manager.add_root();
        (manager, root)
    }

    #[test]
    fn other_end() {
        let (mut manager, root) = manager();
        let a = manager.new_node(None);
        let b = manager.new_node(None);
        manager.graph_add_node(root, a).unwrap();
        manager.graph_add_node(root, b).unwrap();
        let edge = manager.new_edge(None);
        manager.add_edge(edge, a, b).unwrap();

        assert_eq!(manager.edge_other_end(edge, a), Ok(b));
        assert_eq!(manager.edge_other_end(edge, b), Ok(a));

        let c = manager.new_node(None);
        manager.graph_add_node(root, c).unwrap();
        assert_eq!(
            manager.edge_other_end(edge, c).unwrap_err(),
            LayoutError("Node is not incident with this edge")
        );
    }

    #[test]
    fn update_length_clips_at_the_rectangles() {
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
        let edge = manager.new_edge(None);
        manager.add_edge(edge, a, b).unwrap();

        manager.edge_update_length(edge);

        // Upstream: target rect (100, 50, 30, 40) against source rect (0, 0, 10, 20)
        // clips at (100, 61.81818181818182) and (10, 12.727272727272727).
        let edge = manager.edge(edge);
        assert!(!edge.is_overlaping_source_and_target());
        assert!((edge.length_x - 90.0).abs() < 1e-9);
        assert!((edge.length_y - 49.09090909090909).abs() < 1e-9);
        assert!((edge.get_length() - 102.51788797752273).abs() < 1e-9);
    }

    #[test]
    fn update_length_simple_uses_the_centers() {
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
        let edge = manager.new_edge(None);
        manager.add_edge(edge, a, b).unwrap();

        manager.edge_update_length_simple(edge);

        let edge = manager.edge(edge);
        assert_eq!(edge.length_x, 110.0);
        assert_eq!(edge.length_y, 60.0);
        assert_eq!(edge.get_length(), (110.0f64 * 110.0 + 60.0 * 60.0).sqrt());
    }

    #[test]
    fn overlapping_endpoints_keep_their_lengths() {
        let (mut manager, root) = manager();
        let a = manager.new_node(None);
        let b = manager.new_node(None);
        manager.graph_add_node(root, a).unwrap();
        manager.graph_add_node(root, b).unwrap();
        manager
            .node_mut(a)
            .set_rect(&Point::new(0.0, 0.0), &DimensionD::new(100.0, 100.0));
        manager
            .node_mut(b)
            .set_rect(&Point::new(10.0, 10.0), &DimensionD::new(100.0, 100.0));
        let edge = manager.new_edge(None);
        manager.add_edge(edge, a, b).unwrap();

        manager.edge_update_length(edge);

        let edge = manager.edge(edge);
        assert!(edge.is_overlaping_source_and_target());
        assert_eq!(edge.length, 0.0);
    }
}
