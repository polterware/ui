//! Port of `layout-base/src/LNode.js`.
//!
//! A node: a rectangle, its incident edges, its owner graph, and its child graph when it
//! is compound. Upstream's two constructor alternatives (`LNode(gm, loc, size, vNode)`
//! and `LNode(layout, vNode)`) become [`LNode::with_bounds`] and the empty node the
//! manager creates with [`LGraphManager::new_node`].
//!
//! `estimatedSize` and `inclusionTreeDepth` use `Integer.MIN_VALUE`/`MAX_VALUE` as "not
//! computed" and throw from their getters; the port stores `Option`s. `labelWidth`,
//! `labelHeight`, and `labelPos` are read only when `NODE_DIMENSIONS_INCLUDE_LABELS` is
//! set (it is `false`). `paddingLeft` is a property upstream reads off the v-level node
//! through `getParent()`, kept here as [`LNode::padding_left`].

use super::LayoutError;
use super::l_edge::EdgeId;
use super::l_graph::GraphId;
use super::l_graph_manager::LGraphManager;
use super::l_graph_object::LGraphObject;
use super::layout_constants as constants;
use super::util::dimension_d::DimensionD;
use super::util::point::Point;
use super::util::point_d::PointD;
use super::util::random_seed;
use super::util::rectangle_d::RectangleD;
use super::util::transform::Transform;

/// Index of an [`LNode`] in [`LGraphManager`]'s node arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub usize);

/// Where a compound node's label sits, upstream's `labelPos` values read by
/// `updateBounds`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelPos {
    /// The label is centered on the node.
    Center,
    /// The label sits above the node.
    Top,
}

/// A node, upstream's `LNode`.
#[derive(Clone, Debug, PartialEq)]
pub struct LNode<V> {
    /// Upstream's `LGraphObject` base.
    pub base: LGraphObject<V>,
    /// The graph that holds this node, upstream's `owner`.
    pub owner: Option<GraphId>,
    /// The graph of this node's children, upstream's `child`.
    pub child: Option<GraphId>,
    /// Incident edges, in insertion order, upstream's `edges`.
    pub edges: Vec<EdgeId>,
    /// The node's rectangle, upstream's `rect`.
    pub rect: RectangleD,
    /// The size the last estimate computed, upstream's `estimatedSize`.
    pub estimated_size: Option<f64>,
    /// The depth in the inclusion tree, upstream's `inclusionTreeDepth`.
    pub inclusion_tree_depth: Option<i32>,
    /// Label width, upstream's `labelWidth`.
    pub label_width: f64,
    /// Label height, upstream's `labelHeight`.
    pub label_height: f64,
    /// Label position, upstream's `labelPos`.
    pub label_pos: Option<LabelPos>,
    /// Padding the parent node applies to this node's graph, upstream's v-level
    /// `paddingLeft` read through `getParent()`.
    pub padding_left: Option<f64>,
}

impl<V> Default for LNode<V> {
    fn default() -> Self {
        Self {
            base: LGraphObject::default(),
            owner: None,
            child: None,
            edges: Vec::new(),
            rect: RectangleD::default(),
            estimated_size: None,
            inclusion_tree_depth: None,
            label_width: 0.0,
            label_height: 0.0,
            label_pos: None,
            padding_left: None,
        }
    }
}

impl<V> LNode<V> {
    /// An empty node, upstream's alternative constructor 2.
    pub fn new() -> Self {
        Self::default()
    }

    /// A node at `loc` with `size`, upstream's alternative constructor 1.
    pub fn with_bounds(loc: PointD, size: DimensionD) -> Self {
        Self {
            rect: RectangleD::new(loc.x, loc.y, size.width, size.height),
            ..Self::default()
        }
    }

    /// Upstream `getEdges`.
    pub fn get_edges(&self) -> &[EdgeId] {
        &self.edges
    }

    /// Upstream `getChild`.
    pub fn get_child(&self) -> Option<GraphId> {
        self.child
    }

    /// Upstream `getOwner`.
    pub fn get_owner(&self) -> Option<GraphId> {
        self.owner
    }

    /// Upstream `getWidth`.
    pub fn get_width(&self) -> f64 {
        self.rect.width
    }

    /// Upstream `setWidth`.
    pub fn set_width(&mut self, width: f64) {
        self.rect.width = width;
    }

    /// Upstream `getHeight`.
    pub fn get_height(&self) -> f64 {
        self.rect.height
    }

    /// Upstream `setHeight`.
    pub fn set_height(&mut self, height: f64) {
        self.rect.height = height;
    }

    /// Upstream `getCenterX`.
    pub fn get_center_x(&self) -> f64 {
        self.rect.x + self.rect.width / 2.0
    }

    /// Upstream `getCenterY`.
    pub fn get_center_y(&self) -> f64 {
        self.rect.y + self.rect.height / 2.0
    }

    /// Upstream `getCenter`.
    pub fn get_center(&self) -> PointD {
        PointD::new(
            self.rect.x + self.rect.width / 2.0,
            self.rect.y + self.rect.height / 2.0,
        )
    }

    /// Upstream `getLocation`.
    pub fn get_location(&self) -> PointD {
        PointD::new(self.rect.x, self.rect.y)
    }

    /// Upstream `getRect`.
    pub fn get_rect(&self) -> &RectangleD {
        &self.rect
    }

    /// Upstream `getDiagonal`.
    pub fn get_diagonal(&self) -> f64 {
        (self.rect.width * self.rect.width + self.rect.height * self.rect.height).sqrt()
    }

    /// Upstream `getHalfTheDiagonal`: note the order of the squares.
    pub fn get_half_the_diagonal(&self) -> f64 {
        (self.rect.height * self.rect.height + self.rect.width * self.rect.width).sqrt() / 2.0
    }

    /// Upstream `setRect`.
    pub fn set_rect(&mut self, upper_left: &Point, dimension: &DimensionD) {
        self.rect.x = upper_left.x;
        self.rect.y = upper_left.y;
        self.rect.width = dimension.width;
        self.rect.height = dimension.height;
    }

    /// Upstream `setCenter`.
    pub fn set_center(&mut self, center_x: f64, center_y: f64) {
        self.rect.x = center_x - self.rect.width / 2.0;
        self.rect.y = center_y - self.rect.height / 2.0;
    }

    /// Upstream `setLocation`.
    pub fn set_location(&mut self, x: f64, y: f64) {
        self.rect.x = x;
        self.rect.y = y;
    }

    /// Upstream `moveBy`.
    pub fn move_by(&mut self, dx: f64, dy: f64) {
        self.rect.x += dx;
        self.rect.y += dy;
    }

    /// Upstream `scatter`: places the node at a random point of the initial world.
    pub fn scatter(&mut self) {
        let min_x = -constants::INITIAL_WORLD_BOUNDARY;
        let max_x = constants::INITIAL_WORLD_BOUNDARY;
        let random_center_x =
            constants::WORLD_CENTER_X + (random_seed::next_double() * (max_x - min_x)) + min_x;

        let min_y = -constants::INITIAL_WORLD_BOUNDARY;
        let max_y = constants::INITIAL_WORLD_BOUNDARY;
        let random_center_y =
            constants::WORLD_CENTER_Y + (random_seed::next_double() * (max_y - min_y)) + min_y;

        self.rect.x = random_center_x;
        self.rect.y = random_center_y;
    }

    /// Upstream `getEstimatedSize`.
    pub fn get_estimated_size(&self) -> Result<f64, LayoutError> {
        self.estimated_size.ok_or(LayoutError("assert failed"))
    }

    /// Upstream `getInclusionTreeDepth`.
    pub fn get_inclusion_tree_depth(&self) -> Result<i32, LayoutError> {
        self.inclusion_tree_depth
            .ok_or(LayoutError("assert failed"))
    }

    /// Upstream `transform`: clamps the location to the world boundary and maps it back
    /// through `transform`. The two comparisons per axis are upstream's; `clamp` would
    /// be the same for these constants but not for NaN.
    #[allow(clippy::manual_clamp)]
    pub fn transform(&mut self, transform: &Transform) {
        let mut left = self.rect.x;

        if left > constants::WORLD_BOUNDARY {
            left = constants::WORLD_BOUNDARY;
        } else if left < -constants::WORLD_BOUNDARY {
            left = -constants::WORLD_BOUNDARY;
        }

        let mut top = self.rect.y;

        if top > constants::WORLD_BOUNDARY {
            top = constants::WORLD_BOUNDARY;
        } else if top < -constants::WORLD_BOUNDARY {
            top = -constants::WORLD_BOUNDARY;
        }

        let left_top = PointD::new(left, top);
        let v_left_top = transform.inverse_transform_point(&left_top);

        self.set_location(v_left_top.x, v_left_top.y);
    }

    /// Upstream `getLeft`.
    pub fn get_left(&self) -> f64 {
        self.rect.x
    }

    /// Upstream `getRight`.
    pub fn get_right(&self) -> f64 {
        self.rect.x + self.rect.width
    }

    /// Upstream `getTop`.
    pub fn get_top(&self) -> f64 {
        self.rect.y
    }

    /// Upstream `getBottom`.
    pub fn get_bottom(&self) -> f64 {
        self.rect.y + self.rect.height
    }
}

impl<V> LGraphManager<V> {
    /// Upstream `LNode.prototype.getParent`: the parent node of the node's owner graph.
    pub fn node_parent(&self, node: NodeId) -> Option<NodeId> {
        let owner = self.node(node).owner?;

        self.graph(owner).parent
    }

    /// Upstream `LNode.prototype.getEdgeListToNode`. Upstream throws when an incident
    /// edge does not have this node as its source; the port trusts the graph, since
    /// edges are only added by [`LGraphManager::graph_add_edge`].
    pub fn node_edge_list_to(&self, node: NodeId, to: NodeId) -> Vec<EdgeId> {
        let mut edge_list = Vec::new();

        for &edge in self.node(node).get_edges() {
            if self.edge(edge).target == Some(to) {
                edge_list.push(edge);
            }
        }

        edge_list
    }

    /// Upstream `LNode.prototype.getEdgesBetween`.
    pub fn node_edges_between(&self, node: NodeId, other: NodeId) -> Vec<EdgeId> {
        let mut edge_list = Vec::new();

        for &edge in self.node(node).get_edges() {
            let incident = self.edge(edge);
            if incident.target == Some(other) || incident.source == Some(other) {
                edge_list.push(edge);
            }
        }

        edge_list
    }

    /// Upstream `LNode.prototype.getNeighborsList`: the nodes across the incident
    /// edges, in edge order, each once (upstream's `Set`).
    pub fn node_neighbors(&self, node: NodeId) -> Vec<NodeId> {
        let mut neighbors = Vec::new();

        for &edge in self.node(node).get_edges() {
            let incident = self.edge(edge);
            let neighbor = if incident.source == Some(node) {
                incident.target
            } else {
                incident.source
            };

            if let Some(neighbor) = neighbor
                && !neighbors.contains(&neighbor)
            {
                neighbors.push(neighbor);
            }
        }

        neighbors
    }

    /// Upstream `LNode.prototype.withChildren`: the node and, recursively, every node
    /// of its child graphs, in visit order.
    pub fn node_with_children(&self, node: NodeId) -> Vec<NodeId> {
        let mut with_children = Vec::new();

        with_children.push(node);

        if let Some(child) = self.node(node).child {
            let nodes = self.graph(child).nodes.clone();
            for child_node in nodes {
                for descendant in self.node_with_children(child_node) {
                    if !with_children.contains(&descendant) {
                        with_children.push(descendant);
                    }
                }
            }
        }

        with_children
    }

    /// Upstream `LNode.prototype.getNoOfChildren`.
    pub fn node_no_of_children(&self, node: NodeId) -> i64 {
        let mut no_of_children = 0;

        match self.node(node).child {
            None => no_of_children = 1,
            Some(child) => {
                let nodes = self.graph(child).nodes.clone();
                for child_node in nodes {
                    no_of_children += self.node_no_of_children(child_node);
                }
            }
        }

        if no_of_children == 0 {
            no_of_children = 1;
        }

        no_of_children
    }

    /// Upstream `LNode.prototype.calcEstimatedSize`.
    pub fn node_calc_estimated_size(&mut self, node: NodeId) -> f64 {
        let Some(child) = self.node(node).child else {
            let node = self.node_mut(node);
            node.estimated_size = Some((node.rect.width + node.rect.height) / 2.0);
            return node.estimated_size.unwrap_or_default();
        };

        let size = self.graph_calc_estimated_size(child);
        let node = self.node_mut(node);
        node.estimated_size = Some(size);
        node.rect.width = size;
        node.rect.height = size;

        size
    }

    /// Upstream `LNode.prototype.updateBounds`: wraps the node around its child graph.
    pub fn node_update_bounds(&mut self, node: NodeId) -> Result<(), LayoutError> {
        let child = self.node(node).child.ok_or(LayoutError("assert failed"))?;
        if self.graph(child).nodes.is_empty() {
            return Ok(());
        }

        // Wrap the children by re-arranging the boundaries.
        self.graph_update_bounds(child, true)?;

        let (left, top, right, bottom) = {
            let child_graph = self.graph(child);
            (
                child_graph.get_left(),
                child_graph.get_top(),
                child_graph.get_right(),
                child_graph.get_bottom(),
            )
        };

        let node = self.node_mut(node);
        node.rect.x = left;
        node.rect.y = top;
        node.set_width(right - left);
        node.set_height(bottom - top);

        // Update the compound bounds considering its label properties.
        if constants::NODE_DIMENSIONS_INCLUDE_LABELS {
            let width = right - left;
            let height = bottom - top;

            if node.label_width > width {
                node.rect.x -= (node.label_width - width) / 2.0;
                node.set_width(node.label_width);
            }

            if node.label_height > height {
                match node.label_pos {
                    Some(LabelPos::Center) => {
                        node.rect.y -= (node.label_height - height) / 2.0;
                    }
                    Some(LabelPos::Top) => {
                        node.rect.y -= node.label_height - height;
                    }
                    None => {}
                }
                node.set_height(node.label_height);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node() -> LNode<()> {
        LNode::with_bounds(PointD::new(10.0, 20.0), DimensionD::new(40.0, 60.0))
    }

    #[test]
    fn geometry() {
        let mut node = node();
        assert_eq!(node.get_width(), 40.0);
        assert_eq!(node.get_height(), 60.0);
        assert_eq!(node.get_center_x(), 30.0);
        assert_eq!(node.get_center_y(), 50.0);
        assert_eq!(node.get_center(), PointD::new(30.0, 50.0));
        assert_eq!(node.get_location(), PointD::new(10.0, 20.0));
        assert_eq!(node.get_diagonal(), 5200.0f64.sqrt());
        assert_eq!(node.get_half_the_diagonal(), 5200.0f64.sqrt() / 2.0);
        assert_eq!(node.get_left(), 10.0);
        assert_eq!(node.get_right(), 50.0);
        assert_eq!(node.get_top(), 20.0);
        assert_eq!(node.get_bottom(), 80.0);

        node.set_center(100.0, 100.0);
        assert_eq!(node.get_location(), PointD::new(80.0, 70.0));

        node.move_by(-5.0, 5.0);
        assert_eq!(node.get_location(), PointD::new(75.0, 75.0));

        node.set_rect(&Point::new(0.0, 0.0), &DimensionD::new(10.0, 20.0));
        assert_eq!(node.rect, RectangleD::new(0.0, 0.0, 10.0, 20.0));
    }

    #[test]
    fn unset_sizes_throw_like_upstream() {
        let node: LNode<()> = LNode::new();
        assert_eq!(
            node.get_estimated_size().unwrap_err(),
            LayoutError("assert failed")
        );
        assert_eq!(
            node.get_inclusion_tree_depth().unwrap_err(),
            LayoutError("assert failed")
        );
    }

    #[test]
    fn scatter_uses_the_shared_seed() {
        random_seed::set_seed(1);
        let mut node: LNode<()> = LNode::new();
        node.scatter();
        // 1200 + 0.7098480789645691 * 2000 - 1000 and the same for y with the next
        // double, upstream's `scatter`.
        assert!((node.rect.x - 1619.6961579291382).abs() < 1e-9);
        assert!((node.rect.y - 1848.5365136351903).abs() < 1e-9);
    }

    #[test]
    fn transform_clamps_to_the_world_boundary() {
        let mut node: LNode<()> = LNode::with_bounds(
            PointD::new(constants::WORLD_BOUNDARY + 10.0, 0.0),
            DimensionD::new(10.0, 10.0),
        );
        let mut transform = Transform::new();
        transform.set_device_org_x(0.0);
        transform.set_device_ext_x(2.0);
        node.transform(&transform);
        assert_eq!(
            node.get_location(),
            PointD::new(constants::WORLD_BOUNDARY / 2.0, 0.0)
        );
    }
}
