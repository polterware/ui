//! Port of `cose-base/src/ConstraintHandler.js` (cose-base 2.2.0), the constraint
//! handling cytoscape-fcose 2.2.0 runs on top of cose-base's layout.
//!
//! The handler takes the draft layout and, in two passes:
//!
//! 1. **Transform** (when [`TRANSFORM_ON_CONSTRAINT_HANDLING`](super::co_se_constants::TRANSFORM_ON_CONSTRAINT_HANDLING)
//!    is set): solves the orthogonal Procrustean problem of Borg and Groenen (2005),
//!    ch. 20.2, to rotate and/or reflect the draft layout so it better matches the fixed
//!    node, alignment, and relative placement constraints. The solution uses the singular
//!    value decomposition of [`matrix`](super::super::layout_base::util::matrix) and
//!    [`svd`](super::super::layout_base::util::svd).
//! 2. **Enforce** (when [`ENFORCE_CONSTRAINTS`](super::super::cose_base::co_se_constants::ENFORCE_CONSTRAINTS)
//!    is set): translates the layout so the fixed nodes' average error is zero, aligns the
//!    alignment groups, and lays out the relative placement constraints as directed acyclic
//!    graphs, one for each axis, with one dummy vertex per alignment group.
//!
//! The port follows upstream's data shapes: `Map`s of ids become insertion-ordered
//! `Vec<(String, T)>`, `Set`s of ids become `Vec<String>`, and coordinates stay in two
//! parallel arrays indexed by [`NodeIndexes`], like upstream's `xCoords`/`yCoords`.
//! Iteration order is upstream's, which matters for `findAppropriatePosition...` (both
//! its queue and its readjustment walk the maps) and for the average positions.
//!
//! Where upstream reads a property off a JavaScript object that it assumes exists (the
//! node of a constraint id, the neighbor list of a dag vertex), the port skips the entry;
//! upstream would throw. The one intentionally dead branch of upstream is the
//! non-relaxed relative placement movement in `CoSELayout` (the constant is `true`), which
//! lives in `co_se_layout` rather than here.

use super::super::layout_base::LayoutError;
use super::super::layout_base::l_node::NodeId;
use super::super::layout_base::util::matrix;
use super::super::layout_base::util::point_d::PointD;
use super::super::layout_base::util::svd;
use super::co_se_constants as constants;
use super::co_se_layout::CoSELayout;

/// A fixed node, upstream's entry of `fixedNodeConstraint`: the node's id and the
/// position its center must have.
#[derive(Clone, Debug, PartialEq)]
pub struct FixedNodeConstraint {
    /// The id of the fixed node, upstream's `nodeId`.
    pub node_id: String,
    /// The center the node must have, upstream's `position`.
    pub position: PointD,
}

/// Alignment constraints, upstream's `alignmentConstraint`: groups of node ids that must
/// share an x coordinate (`vertical`, a vertical line) or a y coordinate (`horizontal`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlignmentConstraint {
    /// Groups that must share an x coordinate.
    pub vertical: Option<Vec<Vec<String>>>,
    /// Groups that must share a y coordinate.
    pub horizontal: Option<Vec<Vec<String>>>,
}

/// A relative placement constraint, upstream's entry of `relativePlacementConstraint`.
/// A horizontal constraint fills `left` and `right`; a vertical one fills `top` and
/// `bottom`.
#[derive(Clone, Debug, PartialEq)]
pub struct RelativePlacementConstraint {
    /// The node left of `right`.
    pub left: Option<String>,
    /// The node right of `left`.
    pub right: Option<String>,
    /// The node above `bottom`.
    pub top: Option<String>,
    /// The node below `top`.
    pub bottom: Option<String>,
    /// The minimum distance between the two nodes' centers. `None` behaves as
    /// JavaScript's `undefined`, which the handler replaces with an edge length plus half
    /// of each node's size.
    pub gap: Option<f64>,
}

impl RelativePlacementConstraint {
    /// A horizontal constraint: `left` must be left of `right`.
    pub fn horizontal(left: impl Into<String>, right: impl Into<String>, gap: Option<f64>) -> Self {
        Self {
            left: Some(left.into()),
            right: Some(right.into()),
            top: None,
            bottom: None,
            gap,
        }
    }

    /// A vertical constraint: `top` must be above `bottom`.
    pub fn vertical(top: impl Into<String>, bottom: impl Into<String>, gap: Option<f64>) -> Self {
        Self {
            left: None,
            right: None,
            top: Some(top.into()),
            bottom: Some(bottom.into()),
            gap,
        }
    }
}

/// The constraints of a layout, upstream's `layout.constraints` object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Constraints {
    /// Nodes that must keep a fixed position.
    pub fixed_node_constraint: Option<Vec<FixedNodeConstraint>>,
    /// Groups that must be aligned.
    pub alignment_constraint: Option<AlignmentConstraint>,
    /// Required relative placements.
    pub relative_placement_constraint: Option<Vec<RelativePlacementConstraint>>,
}

impl Constraints {
    /// Whether any constraint is set, upstream's `Object.keys(this.constraints).length > 0`.
    pub fn is_empty(&self) -> bool {
        self.fixed_node_constraint.is_none()
            && self.alignment_constraint.is_none()
            && self.relative_placement_constraint.is_none()
    }
}

/// The axis a relative placement constraint works on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Horizontal,
    Vertical,
}

/// One edge of the constraint dag, upstream's `{id, gap, direction}` object.
#[derive(Clone, Debug, PartialEq)]
struct Adjacent {
    id: String,
    gap: f64,
    direction: Direction,
}

/// A directed graph of ids, upstream's `Map` of adjacency lists.
type Graph = Vec<(String, Vec<Adjacent>)>;

/// An order-preserving lookup, upstream's `Map`.
fn map_get<'a, T>(map: &'a [(String, T)], key: &str) -> Option<&'a T> {
    map.iter().find(|(k, _)| k == key).map(|(_, value)| value)
}

fn map_get_mut<'a, T>(map: &'a mut [(String, T)], key: &str) -> Option<&'a mut T> {
    map.iter_mut()
        .find(|(k, _)| k == key)
        .map(|(_, value)| value)
}

fn map_set<T>(map: &mut Vec<(String, T)>, key: &str, value: T) {
    if let Some(entry) = map.iter_mut().find(|(k, _)| k == key) {
        entry.1 = value;
    } else {
        map.push((key.to_string(), value));
    }
}

fn set_add(set: &mut Vec<String>, key: &str) {
    if !set.iter().any(|k| k == key) {
        set.push(key.to_string());
    }
}

fn graph_get<'a>(graph: &'a Graph, key: &str) -> Option<&'a [Adjacent]> {
    map_get(graph, key).map(|adjacents| adjacents.as_slice())
}

fn graph_ensure(graph: &mut Graph, key: &str) {
    if !graph.iter().any(|(k, _)| k == key) {
        graph.push((key.to_string(), Vec::new()));
    }
}

fn graph_push(graph: &mut Graph, key: &str, adjacent: Adjacent) {
    graph_ensure(graph, key);
    if let Some(entry) = map_get_mut(graph, key) {
        entry.push(adjacent);
    }
}

/// The leaves of the layout and their draft coordinates, upstream's
/// `idToNodeMap`, `nodeIndexes`, `xCoords`, and `yCoords`.
struct Handler {
    constraints: Constraints,
    /// Ids of the leaf nodes, in layout order.
    node_ids: Vec<String>,
    /// Id to index in `x_coords` and `y_coords`, upstream's `nodeIndexes`.
    node_indexes: Vec<(String, usize)>,
    x_coords: Vec<f64>,
    y_coords: Vec<f64>,
    widths: Vec<(String, f64)>,
    heights: Vec<(String, f64)>,
}

impl Handler {
    /// Reads the layout's leaves and their draft positions.
    fn new<V>(layout: &mut CoSELayout<V>) -> Self {
        let all_nodes = layout.fd.layout.get_all_nodes();
        let mut handler = Self {
            constraints: layout.constraints.clone(),
            node_ids: Vec::new(),
            node_indexes: Vec::new(),
            x_coords: Vec::new(),
            y_coords: Vec::new(),
            widths: Vec::new(),
            heights: Vec::new(),
        };

        for node in all_nodes {
            let node_ref = layout.fd.layout.graph_manager.node(node);
            if node_ref.get_child().is_none() {
                let id = layout.node_id(node);
                let index = handler.node_ids.len();
                handler.node_indexes.push((id.clone(), index));
                handler.x_coords.push(node_ref.get_center_x());
                handler.y_coords.push(node_ref.get_center_y());
                handler.widths.push((id.clone(), node_ref.get_width()));
                handler.heights.push((id.clone(), node_ref.get_height()));
                handler.node_ids.push(id);
            }
        }

        handler
    }

    fn index(&self, id: &str) -> Option<usize> {
        map_get(&self.node_indexes, id).copied()
    }

    /// The draft coordinate of a constraint id, which may be an alignment dummy: the
    /// node's coordinate when it is a node, else the dummy's position. `None` is
    /// JavaScript's `undefined`.
    fn position(
        &self,
        id: &str,
        dummy_positions: &[(String, f64)],
        direction: Direction,
    ) -> Option<f64> {
        if let Some(index) = self.index(id) {
            Some(match direction {
                Direction::Horizontal => self.x_coords[index],
                Direction::Vertical => self.y_coords[index],
            })
        } else {
            map_get(dummy_positions, id).copied()
        }
    }

    /// Upstream's `calculateAvgPosition`: the average draft position of a set of nodes.
    fn average_position(&self, node_id_set: &[String]) -> (f64, f64) {
        let mut x_sum = 0.0;
        let mut y_sum = 0.0;

        for id in node_id_set {
            if let Some(index) = self.index(id) {
                x_sum += self.x_coords[index];
                y_sum += self.y_coords[index];
            }
        }

        let size = node_id_set.len() as f64;
        (x_sum / size, y_sum / size)
    }
}

/// Upstream `ConstraintHandler.handleConstraints`: transform the draft layout for the
/// constraints, then enforce them.
pub fn handle_constraints<V>(layout: &mut CoSELayout<V>) -> Result<(), LayoutError> {
    let mut handler = Handler::new(layout);

    // If there exists a relative placement constraint without a gap value, set it to the
    // default. `!gap && gap != 0` is true for `undefined`, false for `0`.
    if let Some(relative) = handler.constraints.relative_placement_constraint.clone() {
        let mut relative = relative;
        for constraint in &mut relative {
            if constraint.gap.is_none() {
                if let Some(left) = &constraint.left {
                    let right = constraint
                        .right
                        .clone()
                        .ok_or(LayoutError("assert failed"))?;
                    let left_width = map_get(&handler.widths, left)
                        .copied()
                        .ok_or(LayoutError("assert failed"))?;
                    let right_width = map_get(&handler.widths, &right)
                        .copied()
                        .ok_or(LayoutError("assert failed"))?;
                    constraint.gap =
                        Some(constants::DEFAULT_EDGE_LENGTH + left_width / 2.0 + right_width / 2.0);
                } else {
                    let top = constraint.top.clone().ok_or(LayoutError("assert failed"))?;
                    let bottom = constraint
                        .bottom
                        .clone()
                        .ok_or(LayoutError("assert failed"))?;
                    let top_height = map_get(&handler.heights, &top)
                        .copied()
                        .ok_or(LayoutError("assert failed"))?;
                    let bottom_height = map_get(&handler.heights, &bottom)
                        .copied()
                        .ok_or(LayoutError("assert failed"))?;
                    constraint.gap = Some(
                        constants::DEFAULT_EDGE_LENGTH + top_height / 2.0 + bottom_height / 2.0,
                    );
                }
            }
        }
        handler.constraints.relative_placement_constraint = Some(relative);
    }

    // Fill the fixed node collection.
    let mut fixed_nodes: Vec<String> = Vec::new();
    if let Some(fixed) = &handler.constraints.fixed_node_constraint {
        for node_data in fixed {
            set_add(&mut fixed_nodes, &node_data.node_id);
        }
    }

    // Construct the dag from the relative placement constraints.
    let mut dag: Graph = Vec::new();
    let mut dag_undirected: Graph = Vec::new();
    let mut components: Vec<Vec<String>> = Vec::new();

    if let Some(relative) = &handler.constraints.relative_placement_constraint {
        for constraint in relative {
            if let Some(left) = &constraint.left {
                let right = constraint
                    .right
                    .clone()
                    .ok_or(LayoutError("assert failed"))?;
                graph_push(
                    &mut dag,
                    left,
                    Adjacent {
                        id: right.clone(),
                        gap: constraint.gap.unwrap_or(f64::NAN),
                        direction: Direction::Horizontal,
                    },
                );
                graph_ensure(&mut dag, &right);
            } else {
                let top = constraint.top.clone().ok_or(LayoutError("assert failed"))?;
                let bottom = constraint
                    .bottom
                    .clone()
                    .ok_or(LayoutError("assert failed"))?;
                graph_push(
                    &mut dag,
                    &top,
                    Adjacent {
                        id: bottom.clone(),
                        gap: constraint.gap.unwrap_or(f64::NAN),
                        direction: Direction::Vertical,
                    },
                );
                graph_ensure(&mut dag, &bottom);
            }
        }

        dag_undirected = dag_to_undirected(&dag);
        components = find_components(&dag_undirected);
    }

    // Solve the Procrustean problem for the transformed draft layout.
    let mut target_matrix: Vec<[f64; 2]> = Vec::new();
    let mut source_matrix: Vec<[f64; 2]> = Vec::new();
    let mut standard_transformation = false;
    let mut reflection_type = false;

    if constants::TRANSFORM_ON_CONSTRAINT_HANDLING {
        if let Some(fixed) = &handler.constraints.fixed_node_constraint
            && fixed.len() > 1
        {
            for node_data in fixed {
                target_matrix.push([node_data.position.x, node_data.position.y]);
                let index = handler.index(&node_data.node_id);
                let (x, y) = index
                    .map(|index| (handler.x_coords[index], handler.y_coords[index]))
                    .unwrap_or((f64::NAN, f64::NAN));
                source_matrix.push([x, y]);
            }
            standard_transformation = true;
        } else if let Some(alignment) = &handler.constraints.alignment_constraint {
            let mut count = 0;

            if let Some(vertical_align) = &alignment.vertical {
                for alignment_group in vertical_align {
                    let mut alignment_set: Vec<String> = Vec::new();
                    for node_id in alignment_group {
                        set_add(&mut alignment_set, node_id);
                    }

                    let first_fixed = alignment_set
                        .iter()
                        .find(|node_id| fixed_nodes.contains(node_id));

                    let x_pos = match first_fixed {
                        Some(node_id) => {
                            handler.index(node_id).map(|index| handler.x_coords[index])
                        }
                        None => Some(handler.average_position(&alignment_set).0),
                    };

                    if let Some(x_pos) = x_pos {
                        for node_id in alignment_group {
                            let index = handler.index(node_id);
                            let (x, y) = index
                                .map(|index| (handler.x_coords[index], handler.y_coords[index]))
                                .unwrap_or((f64::NAN, f64::NAN));
                            target_matrix.push([x_pos, y]);
                            source_matrix.push([x, y]);
                            count += 1;
                        }
                    }
                }
                standard_transformation = true;
            }

            if let Some(horizontal_align) = &alignment.horizontal {
                for alignment_group in horizontal_align {
                    let mut alignment_set: Vec<String> = Vec::new();
                    for node_id in alignment_group {
                        set_add(&mut alignment_set, node_id);
                    }

                    let first_fixed = alignment_set
                        .iter()
                        .find(|node_id| fixed_nodes.contains(node_id));

                    // Upstream reads `xCoords` for the y position of a fixed node here.
                    let y_pos = match first_fixed {
                        Some(node_id) => {
                            handler.index(node_id).map(|index| handler.x_coords[index])
                        }
                        None => Some(handler.average_position(&alignment_set).1),
                    };

                    if let Some(y_pos) = y_pos {
                        for node_id in alignment_group {
                            let index = handler.index(node_id);
                            let (x, y) = index
                                .map(|index| (handler.x_coords[index], handler.y_coords[index]))
                                .unwrap_or((f64::NAN, f64::NAN));
                            target_matrix.push([x, y_pos]);
                            source_matrix.push([x, y]);
                            count += 1;
                        }
                    }
                }
                standard_transformation = true;
            }

            let _ = count;
            if handler.constraints.relative_placement_constraint.is_some() {
                reflection_type = true;
            }
        } else if let Some(relative) = handler.constraints.relative_placement_constraint.clone() {
            // Find the largest component of the dag.
            let mut largest_component_size = 0;
            let mut largest_component_index = 0;

            for (i, component) in components.iter().enumerate() {
                if component.len() > largest_component_size {
                    largest_component_size = component.len();
                    largest_component_index = i;
                }
            }

            // If the largest component is not dominant, take the reflection votes.
            if (largest_component_size as f64) < (dag_undirected.len() as f64 / 2.0) {
                apply_reflection_for_relative_placement(&mut handler, &relative);
                standard_transformation = false;
                reflection_type = false;
            } else {
                // Use the largest component for the transformation.
                let mut sub_graph_on_horizontal: Graph = Vec::new();
                let mut sub_graph_on_vertical: Graph = Vec::new();
                let mut constraints_in_largest_component: Vec<RelativePlacementConstraint> =
                    Vec::new();

                if let Some(component) = components.get(largest_component_index) {
                    for node_id in component {
                        if let Some(adjacents) = graph_get(&dag, node_id) {
                            for adjacent in adjacents {
                                match adjacent.direction {
                                    Direction::Horizontal => {
                                        graph_push(
                                            &mut sub_graph_on_horizontal,
                                            node_id,
                                            adjacent.clone(),
                                        );
                                        graph_ensure(&mut sub_graph_on_horizontal, &adjacent.id);
                                        constraints_in_largest_component.push(
                                            RelativePlacementConstraint::horizontal(
                                                node_id.clone(),
                                                adjacent.id.clone(),
                                                None,
                                            ),
                                        );
                                    }
                                    Direction::Vertical => {
                                        graph_push(
                                            &mut sub_graph_on_vertical,
                                            node_id,
                                            adjacent.clone(),
                                        );
                                        graph_ensure(&mut sub_graph_on_vertical, &adjacent.id);
                                        constraints_in_largest_component.push(
                                            RelativePlacementConstraint::vertical(
                                                node_id.clone(),
                                                adjacent.id.clone(),
                                                None,
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                    }

                    apply_reflection_for_relative_placement(
                        &mut handler,
                        &constraints_in_largest_component,
                    );
                    reflection_type = false;

                    // Calculate appropriate positioning for the subgraphs.
                    let position_map_horizontal = find_appropriate_position_for_relative_placement(
                        &handler,
                        &sub_graph_on_horizontal,
                        Direction::Horizontal,
                        None,
                        &[],
                        &[],
                    );
                    let position_map_vertical = find_appropriate_position_for_relative_placement(
                        &handler,
                        &sub_graph_on_vertical,
                        Direction::Vertical,
                        None,
                        &[],
                        &[],
                    );

                    // Construct the source and target configurations.
                    for (i, node_id) in component.iter().enumerate() {
                        let index = handler.index(node_id);
                        let (x, y) = index
                            .map(|index| (handler.x_coords[index], handler.y_coords[index]))
                            .unwrap_or((f64::NAN, f64::NAN));
                        let target_x = map_get(&position_map_horizontal, node_id)
                            .copied()
                            .unwrap_or(x);
                        let target_y = map_get(&position_map_vertical, node_id)
                            .copied()
                            .unwrap_or(y);
                        if i < source_matrix.len() {
                            source_matrix[i] = [x, y];
                            target_matrix[i] = [target_x, target_y];
                        } else {
                            source_matrix.push([x, y]);
                            target_matrix.push([target_x, target_y]);
                        }
                    }

                    standard_transformation = true;
                }
            }
        }

        // If a transformation is required, calculate and apply its matrix.
        if standard_transformation && !target_matrix.is_empty() {
            let mut target_matrix_transpose = matrix::transpose(&to_matrix(&target_matrix));
            let mut source_matrix_transpose = matrix::transpose(&to_matrix(&source_matrix));

            for i in 0..target_matrix_transpose.len() {
                target_matrix_transpose[i] = matrix::mult_gamma(&target_matrix_transpose[i]);
                source_matrix_transpose[i] = matrix::mult_gamma(&source_matrix_transpose[i]);
            }

            let temp_matrix = matrix::mult_mat(
                &target_matrix_transpose,
                &matrix::transpose(&source_matrix_transpose),
            );
            let svd_result = svd::svd(temp_matrix);
            let transformation_matrix =
                matrix::mult_mat(&svd_result.v, &matrix::transpose(&svd_result.u));

            for i in 0..handler.node_indexes.len() {
                let temp1 = [handler.x_coords[i], handler.y_coords[i]];
                let temp2 = [transformation_matrix[0][0], transformation_matrix[1][0]];
                let temp3 = [transformation_matrix[0][1], transformation_matrix[1][1]];
                handler.x_coords[i] = matrix::dot_product(&temp1, &temp2);
                handler.y_coords[i] = matrix::dot_product(&temp1, &temp3);
            }

            // Applied only when both alignment and relative placement constraints exist.
            if reflection_type
                && let Some(relative) = handler.constraints.relative_placement_constraint.clone()
            {
                apply_reflection_for_relative_placement(&mut handler, &relative);
            }
        }
    }

    if constants::ENFORCE_CONSTRAINTS {
        enforce_constraints(&mut handler, &dag, &fixed_nodes);
    }

    // Assign the new coordinates to the nodes.
    let all_nodes = layout.fd.layout.get_all_nodes();
    for node in all_nodes {
        if layout
            .fd
            .layout
            .graph_manager
            .node(node)
            .get_child()
            .is_none()
        {
            let id = layout.node_id(node);
            if let Some(index) = handler.index(&id) {
                layout
                    .fd
                    .layout
                    .graph_manager
                    .node_mut(node)
                    .set_center(handler.x_coords[index], handler.y_coords[index]);
            }
        }
    }

    layout.constraints = handler.constraints;

    Ok(())
}

/// Turns `Vec<[f64; 2]>` into the 2d array [`matrix`] works on.
fn to_matrix(rows: &[[f64; 2]]) -> Vec<Vec<f64>> {
    rows.iter().map(|row| vec![row[0], row[1]]).collect()
}

/// Upstream's `setUnion` over id lists, keeping `set_a`'s order.
fn set_union(set_a: &[String], set_b: &[String]) -> Vec<String> {
    let mut union = set_a.to_vec();
    for element in set_b {
        set_add(&mut union, element);
    }
    union
}

/// Upstream `findAppropriatePositionForRelativePlacement`: positions of the dag's
/// vertices that satisfy the gaps, honouring fixed nodes and the positions of alignment
/// dummies.
fn find_appropriate_position_for_relative_placement(
    handler: &Handler,
    graph: &Graph,
    direction: Direction,
    fixed_nodes: Option<&Vec<String>>,
    dummy_positions: &[(String, f64)],
    component_sources: &[Vec<String>],
) -> Vec<(String, f64)> {
    // Find the indegree count for each node.
    let mut in_degrees: Vec<(String, i64)> = Vec::new();
    for (key, _) in graph {
        map_set(&mut in_degrees, key, 0);
    }
    for (_, adjacents) in graph {
        for adjacent in adjacents {
            if let Some(in_degree) = map_get_mut(&mut in_degrees, &adjacent.id) {
                *in_degree += 1;
            }
        }
    }

    let mut position_map: Vec<(String, f64)> = Vec::new();
    let mut past_map: Vec<(String, Vec<String>)> = Vec::new();
    let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();

    for (key, value) in in_degrees.clone() {
        if value == 0 {
            queue.push_back(key.clone());
            if fixed_nodes.is_none()
                && let Some(position) = handler.position(&key, dummy_positions, direction)
            {
                map_set(&mut position_map, &key, position);
            }
        } else {
            map_set(&mut position_map, &key, f64::NEG_INFINITY);
        }

        if fixed_nodes.is_some() {
            map_set(&mut past_map, &key, vec![key.clone()]);
        }
    }

    // Align the sources of each component in the enforcement phase.
    if let Some(fixed_nodes) = fixed_nodes {
        for component in component_sources {
            let fixed_ids: Vec<String> = component
                .iter()
                .filter(|node_id| fixed_nodes.contains(node_id))
                .cloned()
                .collect();

            if !fixed_ids.is_empty() {
                let mut position = 0.0;
                for fixed_id in &fixed_ids {
                    if let Some(value) = handler.position(fixed_id, dummy_positions, direction) {
                        map_set(&mut position_map, fixed_id, value);
                        position += value;
                    }
                }
                position /= fixed_ids.len() as f64;
                for node_id in component {
                    if !fixed_nodes.contains(node_id) {
                        map_set(&mut position_map, node_id, position);
                    }
                }
            } else {
                let mut position = 0.0;
                for node_id in component {
                    position += handler
                        .position(node_id, dummy_positions, direction)
                        .unwrap_or(f64::NAN);
                }
                position /= component.len() as f64;
                for node_id in component {
                    map_set(&mut position_map, node_id, position);
                }
            }
        }
    }

    // Calculate the positions of the nodes.
    while let Some(current_node) = queue.pop_front() {
        let neighbors: Vec<Adjacent> = graph_get(graph, &current_node)
            .map(|adjacents| adjacents.to_vec())
            .unwrap_or_default();

        for neighbor in neighbors {
            let current_position = map_get(&position_map, &current_node)
                .copied()
                .unwrap_or(f64::NAN);
            let neighbor_position = map_get(&position_map, &neighbor.id).copied();

            let needs_place = match neighbor_position {
                Some(neighbor_position) => neighbor_position < current_position + neighbor.gap,
                // Upstream reads `undefined` from the map, and `undefined < x` is false.
                None => false,
            };

            if needs_place {
                if let Some(fixed_nodes) = fixed_nodes
                    && fixed_nodes.contains(&neighbor.id)
                {
                    if let Some(fixed_position) =
                        handler.position(&neighbor.id, dummy_positions, direction)
                    {
                        map_set(&mut position_map, &neighbor.id, fixed_position);

                        if fixed_position < current_position + neighbor.gap {
                            let diff = (current_position + neighbor.gap) - fixed_position;
                            let past = map_get(&past_map, &current_node)
                                .cloned()
                                .unwrap_or_default();
                            for node_id in past {
                                if let Some(position) = map_get_mut(&mut position_map, &node_id) {
                                    *position -= diff;
                                }
                            }
                        }
                    }
                } else {
                    map_set(
                        &mut position_map,
                        &neighbor.id,
                        current_position + neighbor.gap,
                    );
                }
            }

            if let Some(in_degree) = map_get_mut(&mut in_degrees, &neighbor.id) {
                *in_degree -= 1;
                if *in_degree == 0 {
                    queue.push_back(neighbor.id.clone());
                }
            }

            if fixed_nodes.is_some() {
                let current_past = map_get(&past_map, &current_node)
                    .cloned()
                    .unwrap_or_default();
                let neighbor_past = map_get(&past_map, &neighbor.id)
                    .cloned()
                    .unwrap_or_default();
                map_set(
                    &mut past_map,
                    &neighbor.id,
                    set_union(&current_past, &neighbor_past),
                );
            }
        }
    }

    // Readjust the positions of the nodes after enforcement.
    if fixed_nodes.is_some() {
        let mut sink_nodes: Vec<String> = Vec::new();
        for (key, value) in graph {
            if value.is_empty() {
                set_add(&mut sink_nodes, key);
            }
        }

        let mut component_list: Vec<Vec<String>> = Vec::new();
        for (key, value) in past_map.clone() {
            if sink_nodes.contains(&key) {
                let is_fixed_component = value.iter().any(|node_id| {
                    fixed_nodes
                        .map(|fixed_nodes| fixed_nodes.contains(node_id))
                        .unwrap_or(false)
                });

                if !is_fixed_component {
                    let first = value.first().cloned();
                    let exist_at = component_list.iter().position(|component| {
                        first.as_ref().is_some_and(|f| component.contains(f))
                    });

                    match exist_at {
                        Some(exist_at) => {
                            for element in value {
                                set_add(&mut component_list[exist_at], &element);
                            }
                        }
                        None => component_list.push(value),
                    }
                }
            }
        }

        for component in &component_list {
            let mut min_before = f64::INFINITY;
            let mut min_after = f64::INFINITY;
            let mut max_before = f64::NEG_INFINITY;
            let mut max_after = f64::NEG_INFINITY;

            for node_id in component {
                let pos_before = handler
                    .position(node_id, dummy_positions, direction)
                    .unwrap_or(f64::NAN);
                let pos_after = map_get(&position_map, node_id).copied().unwrap_or(f64::NAN);

                if pos_before < min_before {
                    min_before = pos_before;
                }
                if pos_before > max_before {
                    max_before = pos_before;
                }
                if pos_after < min_after {
                    min_after = pos_after;
                }
                if pos_after > max_after {
                    max_after = pos_after;
                }
            }

            let diff = (min_before + max_before) / 2.0 - (min_after + max_after) / 2.0;

            for node_id in component {
                if let Some(position) = map_get_mut(&mut position_map, node_id) {
                    *position += diff;
                }
            }
        }
    }

    position_map
}

/// Upstream `applyReflectionForRelativePlacement`: reflect the draft layout on one or both
/// axes when most constraints point the other way.
fn apply_reflection_for_relative_placement(
    handler: &mut Handler,
    relative_placement_constraints: &[RelativePlacementConstraint],
) {
    let mut reflect_on_y = 0;
    let mut not_reflect_on_y = 0;
    let mut reflect_on_x = 0;
    let mut not_reflect_on_x = 0;

    for constraint in relative_placement_constraints {
        if let Some(left) = &constraint.left {
            let right = match &constraint.right {
                Some(right) => right,
                None => continue,
            };
            let (Some(left_index), Some(right_index)) = (handler.index(left), handler.index(right))
            else {
                continue;
            };

            if handler.x_coords[left_index] - handler.x_coords[right_index] >= 0.0 {
                reflect_on_y += 1;
            } else {
                not_reflect_on_y += 1;
            }
        } else {
            let (Some(top), Some(bottom)) = (&constraint.top, &constraint.bottom) else {
                continue;
            };
            let (Some(top_index), Some(bottom_index)) = (handler.index(top), handler.index(bottom))
            else {
                continue;
            };

            if handler.y_coords[top_index] - handler.y_coords[bottom_index] >= 0.0 {
                reflect_on_x += 1;
            } else {
                not_reflect_on_x += 1;
            }
        }
    }

    if reflect_on_y > not_reflect_on_y && reflect_on_x > not_reflect_on_x {
        for i in 0..handler.node_indexes.len() {
            handler.x_coords[i] = -handler.x_coords[i];
            handler.y_coords[i] = -handler.y_coords[i];
        }
    } else if reflect_on_y > not_reflect_on_y {
        for i in 0..handler.node_indexes.len() {
            handler.x_coords[i] = -handler.x_coords[i];
        }
    } else if reflect_on_x > not_reflect_on_x {
        for i in 0..handler.node_indexes.len() {
            handler.y_coords[i] = -handler.y_coords[i];
        }
    }
}

/// Upstream `findComponents`: the weakly connected components of a graph, in vertex
/// insertion order.
fn find_components(graph: &Graph) -> Vec<Vec<String>> {
    let mut components: Vec<Vec<String>> = Vec::new();
    let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    let mut visited: Vec<String> = Vec::new();

    for (key, _) in graph {
        if !visited.contains(key) {
            let mut component: Vec<String> = Vec::new();
            let mut current_node = key.clone();
            queue.push_back(current_node.clone());
            set_add(&mut visited, &current_node);
            component.push(current_node.clone());

            while let Some(next) = queue.pop_front() {
                current_node = next;
                let neighbors: Vec<Adjacent> = graph_get(graph, &current_node)
                    .map(|adjacents| adjacents.to_vec())
                    .unwrap_or_default();
                for neighbor in neighbors {
                    if !visited.contains(&neighbor.id) {
                        queue.push_back(neighbor.id.clone());
                        set_add(&mut visited, &neighbor.id);
                        component.push(neighbor.id.clone());
                    }
                }
            }

            components.push(component);
        }
    }

    components
}

/// Upstream `dagToUndirected`: the same vertices and both directions of every edge.
fn dag_to_undirected(dag: &Graph) -> Graph {
    let mut undirected: Graph = Vec::new();

    for (key, _) in dag {
        graph_ensure(&mut undirected, key);
    }

    for (key, value) in dag {
        for adjacent in value {
            if let Some(entry) = map_get_mut(&mut undirected, key) {
                entry.push(adjacent.clone());
            }
            if let Some(entry) = map_get_mut(&mut undirected, &adjacent.id) {
                entry.push(Adjacent {
                    id: key.clone(),
                    gap: adjacent.gap,
                    direction: adjacent.direction,
                });
            }
        }
    }

    undirected
}

/// Upstream `dagToReversed`: the same vertices with every edge reversed.
fn dag_to_reversed(dag: &Graph) -> Graph {
    let mut reversed: Graph = Vec::new();

    for (key, _) in dag {
        graph_ensure(&mut reversed, key);
    }

    for (key, value) in dag {
        for adjacent in value {
            if let Some(entry) = map_get_mut(&mut reversed, &adjacent.id) {
                entry.push(Adjacent {
                    id: key.clone(),
                    gap: adjacent.gap,
                    direction: adjacent.direction,
                });
            }
        }
    }

    reversed
}

/// Upstream's enforcement phase of `ConstraintHandler.handleConstraints`: fixed nodes,
/// then alignment, then relative placement, on the transformed draft layout.
fn enforce_constraints(handler: &mut Handler, dag: &Graph, fixed_nodes: &[String]) {
    let fixed_constraint = handler.constraints.fixed_node_constraint.clone();
    let alignment_constraint = handler.constraints.alignment_constraint.clone();
    let has_relative = handler.constraints.relative_placement_constraint.is_some();

    // First enforce the fixed node constraint.
    if let Some(fixed) = &fixed_constraint
        && !fixed.is_empty()
    {
        let mut translation_x = 0.0;
        let mut translation_y = 0.0;

        for node_data in fixed {
            if let Some(index) = handler.index(&node_data.node_id) {
                translation_x += node_data.position.x - handler.x_coords[index];
                translation_y += node_data.position.y - handler.y_coords[index];
            }
        }

        translation_x /= fixed.len() as f64;
        translation_y /= fixed.len() as f64;

        for i in 0..handler.x_coords.len() {
            handler.x_coords[i] += translation_x;
        }
        for i in 0..handler.y_coords.len() {
            handler.y_coords[i] += translation_y;
        }

        for node_data in fixed {
            if let Some(index) = handler.index(&node_data.node_id) {
                handler.x_coords[index] = node_data.position.x;
                handler.y_coords[index] = node_data.position.y;
            }
        }
    }

    // Then enforce the alignment constraint.
    if let Some(alignment) = &alignment_constraint {
        if let Some(vertical) = &alignment.vertical {
            for alignment_group in vertical {
                let mut alignment_set: Vec<String> = Vec::new();
                for node_id in alignment_group {
                    set_add(&mut alignment_set, node_id);
                }

                let first_fixed = alignment_set
                    .iter()
                    .find(|node_id| fixed_nodes.contains(node_id));

                let x_pos = match first_fixed {
                    Some(node_id) => handler.index(node_id).map(|index| handler.x_coords[index]),
                    None => Some(handler.average_position(&alignment_set).0),
                };

                if let Some(x_pos) = x_pos {
                    for node_id in &alignment_set {
                        if !fixed_nodes.contains(node_id)
                            && let Some(index) = handler.index(node_id)
                        {
                            handler.x_coords[index] = x_pos;
                        }
                    }
                }
            }
        }

        if let Some(horizontal) = &alignment.horizontal {
            for alignment_group in horizontal {
                let mut alignment_set: Vec<String> = Vec::new();
                for node_id in alignment_group {
                    set_add(&mut alignment_set, node_id);
                }

                let first_fixed = alignment_set
                    .iter()
                    .find(|node_id| fixed_nodes.contains(node_id));

                let y_pos = match first_fixed {
                    Some(node_id) => handler.index(node_id).map(|index| handler.y_coords[index]),
                    None => Some(handler.average_position(&alignment_set).1),
                };

                if let Some(y_pos) = y_pos {
                    for node_id in &alignment_set {
                        if !fixed_nodes.contains(node_id)
                            && let Some(index) = handler.index(node_id)
                        {
                            handler.y_coords[index] = y_pos;
                        }
                    }
                }
            }
        }
    }

    // Finally enforce the relative placement constraint.
    if !has_relative {
        return;
    }

    let mut node_to_dummy_for_vertical_alignment: Vec<(String, String)> = Vec::new();
    let mut node_to_dummy_for_horizontal_alignment: Vec<(String, String)> = Vec::new();
    let mut dummy_to_node_for_vertical_alignment: Vec<(String, Vec<String>)> = Vec::new();
    let mut dummy_to_node_for_horizontal_alignment: Vec<(String, Vec<String>)> = Vec::new();
    let mut dummy_positions_for_vertical_alignment: Vec<(String, f64)> = Vec::new();
    let mut dummy_positions_for_horizontal_alignment: Vec<(String, f64)> = Vec::new();
    let mut fixed_nodes_on_horizontal: Vec<String> = Vec::new();
    let mut fixed_nodes_on_vertical: Vec<String> = Vec::new();

    for node_id in fixed_nodes {
        set_add(&mut fixed_nodes_on_horizontal, node_id);
        set_add(&mut fixed_nodes_on_vertical, node_id);
    }

    if let Some(alignment) = &alignment_constraint {
        if let Some(vertical) = &alignment.vertical {
            for (i, alignment_group) in vertical.iter().enumerate() {
                let dummy = format!("dummy{i}");
                map_set(
                    &mut dummy_to_node_for_vertical_alignment,
                    &dummy,
                    Vec::new(),
                );
                for node_id in alignment_group {
                    map_set(
                        &mut node_to_dummy_for_vertical_alignment,
                        node_id,
                        dummy.clone(),
                    );
                    if let Some(nodes) =
                        map_get_mut(&mut dummy_to_node_for_vertical_alignment, &dummy)
                    {
                        nodes.push(node_id.clone());
                    }
                    if fixed_nodes.contains(node_id) {
                        set_add(&mut fixed_nodes_on_horizontal, &dummy);
                    }
                }
                if let Some(first) = alignment_group.first()
                    && let Some(index) = handler.index(first)
                {
                    map_set(
                        &mut dummy_positions_for_vertical_alignment,
                        &dummy,
                        handler.x_coords[index],
                    );
                }
            }
        }

        if let Some(horizontal) = &alignment.horizontal {
            for (i, alignment_group) in horizontal.iter().enumerate() {
                let dummy = format!("dummy{i}");
                map_set(
                    &mut dummy_to_node_for_horizontal_alignment,
                    &dummy,
                    Vec::new(),
                );
                for node_id in alignment_group {
                    map_set(
                        &mut node_to_dummy_for_horizontal_alignment,
                        node_id,
                        dummy.clone(),
                    );
                    if let Some(nodes) =
                        map_get_mut(&mut dummy_to_node_for_horizontal_alignment, &dummy)
                    {
                        nodes.push(node_id.clone());
                    }
                    if fixed_nodes.contains(node_id) {
                        set_add(&mut fixed_nodes_on_vertical, &dummy);
                    }
                }
                if let Some(first) = alignment_group.first()
                    && let Some(index) = handler.index(first)
                {
                    map_set(
                        &mut dummy_positions_for_horizontal_alignment,
                        &dummy,
                        handler.y_coords[index],
                    );
                }
            }
        }
    }

    // Construct the horizontal and vertical dags from the overall dag.
    let mut dag_on_horizontal: Graph = Vec::new();
    let mut dag_on_vertical: Graph = Vec::new();

    for (node_id, adjacents) in dag {
        for adjacent in adjacents {
            match adjacent.direction {
                Direction::Horizontal => {
                    let source_id = map_get(&node_to_dummy_for_vertical_alignment, node_id)
                        .cloned()
                        .unwrap_or_else(|| node_id.clone());
                    let target_node =
                        match map_get(&node_to_dummy_for_vertical_alignment, &adjacent.id) {
                            Some(dummy) => Adjacent {
                                id: dummy.clone(),
                                gap: adjacent.gap,
                                direction: adjacent.direction,
                            },
                            None => adjacent.clone(),
                        };

                    graph_push(&mut dag_on_horizontal, &source_id, target_node.clone());
                    graph_ensure(&mut dag_on_horizontal, &target_node.id);
                }
                Direction::Vertical => {
                    let source_id = map_get(&node_to_dummy_for_horizontal_alignment, node_id)
                        .cloned()
                        .unwrap_or_else(|| node_id.clone());
                    let target_node =
                        match map_get(&node_to_dummy_for_horizontal_alignment, &adjacent.id) {
                            Some(dummy) => Adjacent {
                                id: dummy.clone(),
                                gap: adjacent.gap,
                                direction: adjacent.direction,
                            },
                            None => adjacent.clone(),
                        };

                    graph_push(&mut dag_on_vertical, &source_id, target_node.clone());
                    graph_ensure(&mut dag_on_vertical, &target_node.id);
                }
            }
        }
    }

    // Find the source nodes of each component in the horizontal and vertical dags.
    let undirected_on_horizontal = dag_to_undirected(&dag_on_horizontal);
    let undirected_on_vertical = dag_to_undirected(&dag_on_vertical);
    let components_on_horizontal = find_components(&undirected_on_horizontal);
    let components_on_vertical = find_components(&undirected_on_vertical);
    let reversed_dag_on_horizontal = dag_to_reversed(&dag_on_horizontal);
    let reversed_dag_on_vertical = dag_to_reversed(&dag_on_vertical);

    let component_sources_on_horizontal: Vec<Vec<String>> = components_on_horizontal
        .iter()
        .map(|component| {
            component
                .iter()
                .filter(|node_id| {
                    graph_get(&reversed_dag_on_horizontal, node_id)
                        .map(|adjacents| adjacents.is_empty())
                        .unwrap_or(true)
                })
                .cloned()
                .collect()
        })
        .collect();

    let component_sources_on_vertical: Vec<Vec<String>> = components_on_vertical
        .iter()
        .map(|component| {
            component
                .iter()
                .filter(|node_id| {
                    graph_get(&reversed_dag_on_vertical, node_id)
                        .map(|adjacents| adjacents.is_empty())
                        .unwrap_or(true)
                })
                .cloned()
                .collect()
        })
        .collect();

    let position_map_horizontal = find_appropriate_position_for_relative_placement(
        handler,
        &dag_on_horizontal,
        Direction::Horizontal,
        Some(&fixed_nodes_on_horizontal),
        &dummy_positions_for_vertical_alignment,
        &component_sources_on_horizontal,
    );
    let position_map_vertical = find_appropriate_position_for_relative_placement(
        handler,
        &dag_on_vertical,
        Direction::Vertical,
        Some(&fixed_nodes_on_vertical),
        &dummy_positions_for_horizontal_alignment,
        &component_sources_on_vertical,
    );

    // Update the positions of the nodes based on the relative placement constraints.
    for (key, position) in &position_map_horizontal {
        if let Some(nodes) = map_get(&dummy_to_node_for_vertical_alignment, key) {
            for node_id in nodes {
                if let Some(index) = handler.index(node_id) {
                    handler.x_coords[index] = *position;
                }
            }
        } else if let Some(index) = handler.index(key) {
            handler.x_coords[index] = *position;
        }
    }

    for (key, position) in &position_map_vertical {
        if let Some(nodes) = map_get(&dummy_to_node_for_horizontal_alignment, key) {
            for node_id in nodes {
                if let Some(index) = handler.index(node_id) {
                    handler.y_coords[index] = *position;
                }
            }
        } else if let Some(index) = handler.index(key) {
            handler.y_coords[index] = *position;
        }
    }
}

/// The node id a layout's node answers to, upstream's `node.id`.
impl<V> CoSELayout<V> {
    /// The id of `node`'s [`FDLayoutNode`](super::super::layout_base::fd::fd_layout_node::FDLayoutNode),
    /// with JavaScript's `undefined` key spelled `"undefined"`.
    pub fn node_id(&self, node: NodeId) -> String {
        self.fd
            .node(node)
            .id
            .clone()
            .unwrap_or_else(|| "undefined".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::layout_base::l_node::NodeId;
    use crate::layout::layout_base::util::dimension_d::DimensionD;
    use crate::layout::layout_base::util::point::Point;
    use std::collections::HashMap;

    /// Four nodes of `40 x 40` at the draft centers
    /// `a (0, 0)`, `b (100, 30)`, `c (50, 80)`, `d (20, 60)`, the graph the expected
    /// values below were generated on with
    /// `node -e '... ConstraintHandler.handleConstraints(layout)'` on cose-base 2.2.0.
    fn build() -> (CoSELayout<()>, HashMap<String, NodeId>) {
        let mut layout: CoSELayout<()> = CoSELayout::new();
        layout.new_graph_manager();
        let root = layout.fd.add_root();

        let mut nodes = HashMap::new();
        for &(id, center_x, center_y) in &[
            ("a", 0.0, 0.0),
            ("b", 100.0, 30.0),
            ("c", 50.0, 80.0),
            ("d", 20.0, 60.0),
        ] {
            let node = layout.new_node(None);
            layout
                .fd
                .layout
                .graph_manager
                .graph_add_node(root, node)
                .unwrap();
            layout.fd.layout.graph_manager.node_mut(node).set_rect(
                &Point::new(center_x - 20.0, center_y - 20.0),
                &DimensionD::new(40.0, 40.0),
            );
            layout.fd.node_mut(node).id = Some(id.to_string());
            nodes.insert(id.to_string(), node);
        }

        (layout, nodes)
    }

    fn center(layout: &CoSELayout<()>, node: NodeId) -> (f64, f64) {
        let rect = layout.fd.layout.graph_manager.node(node).get_rect();
        (rect.get_center_x(), rect.get_center_y())
    }

    fn assert_close(actual: (f64, f64), expected: (f64, f64)) {
        assert!(
            (actual.0 - expected.0).abs() < 1e-6 && (actual.1 - expected.1).abs() < 1e-6,
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn fixed_nodes_are_transformed_and_enforced() {
        let (mut layout, nodes) = build();
        layout.constraints = Constraints {
            fixed_node_constraint: Some(vec![
                FixedNodeConstraint {
                    node_id: "a".into(),
                    position: PointD::new(10.0, 20.0),
                },
                FixedNodeConstraint {
                    node_id: "b".into(),
                    position: PointD::new(200.0, 100.0),
                },
            ]),
            ..Constraints::default()
        };

        handle_constraints(&mut layout).unwrap();

        assert_close(center(&layout, nodes["a"]), (10.0, 20.0));
        assert_close(center(&layout, nodes["b"]), (200.0, 100.0));
        assert_close(
            center(&layout, nodes["c"]),
            (98.05402079680601, 124.62780649928322),
        );
        assert_close(
            center(&layout, nodes["d"]),
            (70.36302678273498, 101.53649102110654),
        );
    }

    #[test]
    fn relative_placement_moves_the_draft_layout() {
        let (mut layout, nodes) = build();
        layout.constraints = Constraints {
            relative_placement_constraint: Some(vec![
                RelativePlacementConstraint::horizontal("a", "b", Some(60.0)),
                RelativePlacementConstraint::horizontal("b", "c", Some(50.0)),
                RelativePlacementConstraint::vertical("a", "d", Some(70.0)),
            ]),
            ..Constraints::default()
        };

        handle_constraints(&mut layout).unwrap();

        assert_close(
            center(&layout, nodes["a"]),
            (-2.9585442471952206, -10.642108249927386),
        );
        assert_close(
            center(&layout, nodes["b"]),
            (57.04145575280478, -8.169916310186458),
        );
        assert_close(
            center(&layout, nodes["c"]),
            (107.04145575280478, 56.521242577163505),
        );
        assert_close(
            center(&layout, nodes["d"]),
            (40.33326713727736, 59.35789175007261),
        );
    }

    #[test]
    fn alignment_and_relative_placement_with_default_gaps() {
        let (mut layout, nodes) = build();
        layout.constraints = Constraints {
            alignment_constraint: Some(AlignmentConstraint {
                vertical: Some(vec![vec!["a".into(), "b".into()]]),
                horizontal: Some(vec![vec!["a".into(), "c".into()]]),
            }),
            // No gap: the handler uses DEFAULT_EDGE_LENGTH plus half of each node.
            relative_placement_constraint: Some(vec![RelativePlacementConstraint::horizontal(
                "b", "c", None,
            )]),
            ..Constraints::default()
        };

        handle_constraints(&mut layout).unwrap();

        assert_close(
            center(&layout, nodes["a"]),
            (-102.23456881183118, 35.25983655269014),
        );
        assert_close(
            center(&layout, nodes["b"]),
            (-102.23456881183118, 12.867333718280264),
        );
        assert_close(
            center(&layout, nodes["c"]),
            (-12.234568811831181, 35.25983655269014),
        );
        assert_close(
            center(&layout, nodes["d"]),
            (-29.745264959141473, 55.81414885591709),
        );
    }

    #[test]
    fn fixed_and_alignment_constraints() {
        let (mut layout, nodes) = build();
        layout.constraints = Constraints {
            fixed_node_constraint: Some(vec![FixedNodeConstraint {
                node_id: "d".into(),
                position: PointD::new(300.0, 300.0),
            }]),
            alignment_constraint: Some(AlignmentConstraint {
                vertical: Some(vec![vec!["a".into(), "b".into()]]),
                horizontal: None,
            }),
            ..Constraints::default()
        };

        handle_constraints(&mut layout).unwrap();

        assert_close(
            center(&layout, nodes["a"]),
            (351.72261940194215, 263.6026011615962),
        );
        assert_close(
            center(&layout, nodes["b"]),
            (351.72261940194215, 368.0056662507017),
        );
        assert_close(
            center(&layout, nodes["c"]),
            (289.46391086256733, 334.4817462679614),
        );
        assert_close(center(&layout, nodes["d"]), (300.0, 300.0));
    }

    /// Three nodes at `a (0, 0)`, `b (100, 0)`, `c (200, 40)` for the relaxed relative
    /// placement walk of cose-base 2.2.0.
    fn build_three() -> (CoSELayout<()>, HashMap<String, NodeId>) {
        let mut layout: CoSELayout<()> = CoSELayout::new();
        layout.new_graph_manager();
        let root = layout.fd.add_root();

        let mut nodes = HashMap::new();
        for &(id, center_x, center_y) in &[("a", 0.0, 0.0), ("b", 100.0, 0.0), ("c", 200.0, 40.0)] {
            let node = layout.new_node(None);
            layout
                .fd
                .layout
                .graph_manager
                .graph_add_node(root, node)
                .unwrap();
            layout.fd.layout.graph_manager.node_mut(node).set_rect(
                &Point::new(center_x - 20.0, center_y - 20.0),
                &DimensionD::new(40.0, 40.0),
            );
            layout.fd.node_mut(node).id = Some(id.to_string());
            nodes.insert(id.to_string(), node);
        }

        (layout, nodes)
    }

    /// cose-base 2.2.0's `initConstraintVariables` and `updateDisplacements` with
    /// `RELAX_MOVEMENT_ON_CONSTRAINTS` on. The displacements below were printed by
    /// `node -e '... handleConstraints(layout); layout.initConstraintVariables();
    /// layout.updateDisplacements()'` with `totalIterations = 1` so the shuffle is
    /// skipped.
    #[test]
    fn relaxed_relative_placement_adjusts_displacements() {
        let (mut layout, nodes) = build_three();
        layout.constraints = Constraints {
            relative_placement_constraint: Some(vec![
                RelativePlacementConstraint::horizontal("a", "b", Some(60.0)),
                RelativePlacementConstraint::horizontal("b", "c", Some(60.0)),
            ]),
            ..Constraints::default()
        };

        handle_constraints(&mut layout).unwrap();
        layout.init_constraint_variables();

        assert_close(center(&layout, nodes["a"]), (36.82781506049068, 0.0));
        assert_close(
            center(&layout, nodes["c"]),
            (156.82781506049068, 64.01169363831602),
        );

        for (id, displacement_x) in [("a", 3.0), ("b", -2.0), ("c", 5.0)] {
            layout.fd.node_mut(nodes[id]).displacement_x = displacement_x;
        }
        for (id, displacement_y) in [("a", 1.0), ("b", 4.0), ("c", -3.0)] {
            layout.fd.node_mut(nodes[id]).displacement_y = displacement_y;
        }
        layout.fd.total_iterations = 1;
        layout.update_displacements();

        assert_close((layout.fd.node(nodes["a"]).displacement_x, 0.0), (0.0, 0.0));
        assert_close((layout.fd.node(nodes["b"]).displacement_x, 0.0), (0.0, 0.0));
        assert_close((layout.fd.node(nodes["c"]).displacement_x, 0.0), (5.0, 0.0));
        assert_close((layout.fd.node(nodes["a"]).displacement_y, 0.0), (1.0, 0.0));
        assert_close((layout.fd.node(nodes["b"]).displacement_y, 0.0), (4.0, 0.0));
        assert_close(
            (layout.fd.node(nodes["c"]).displacement_y, 0.0),
            (-3.0, 0.0),
        );
    }

    #[test]
    fn relaxed_vertical_relative_placement_adjusts_displacements() {
        let (mut layout, nodes) = build_three();
        layout.constraints = Constraints {
            relative_placement_constraint: Some(vec![RelativePlacementConstraint::vertical(
                "a",
                "b",
                Some(70.0),
            )]),
            ..Constraints::default()
        };

        handle_constraints(&mut layout).unwrap();
        layout.init_constraint_variables();

        assert_close(center(&layout, nodes["a"]), (0.0, -6.326882781833582));
        assert_close(
            center(&layout, nodes["b"]),
            (81.92319205190404, 63.67311721816642),
        );
        assert_close(
            center(&layout, nodes["c"]),
            (140.90789032927495, 147.4617456934273),
        );

        for (id, displacement_x) in [("a", 3.0), ("b", -2.0), ("c", 5.0)] {
            layout.fd.node_mut(nodes[id]).displacement_x = displacement_x;
        }
        for (id, displacement_y) in [("a", 1.0), ("b", 4.0), ("c", -3.0)] {
            layout.fd.node_mut(nodes[id]).displacement_y = displacement_y;
        }
        layout.fd.total_iterations = 1;
        layout.update_displacements();

        assert_close((layout.fd.node(nodes["a"]).displacement_x, 0.0), (3.0, 0.0));
        assert_close(
            (layout.fd.node(nodes["b"]).displacement_x, 0.0),
            (-2.0, 0.0),
        );
        assert_close((layout.fd.node(nodes["c"]).displacement_x, 0.0), (5.0, 0.0));
        assert_close((layout.fd.node(nodes["a"]).displacement_y, 0.0), (0.0, 0.0));
        assert_close((layout.fd.node(nodes["b"]).displacement_y, 0.0), (4.0, 0.0));
        assert_close(
            (layout.fd.node(nodes["c"]).displacement_y, 0.0),
            (-3.0, 0.0),
        );
    }

    #[test]
    fn fixed_and_relative_placement_constraints() {
        let (mut layout, nodes) = build();
        layout.constraints = Constraints {
            fixed_node_constraint: Some(vec![FixedNodeConstraint {
                node_id: "a".into(),
                position: PointD::new(0.0, 0.0),
            }]),
            relative_placement_constraint: Some(vec![
                RelativePlacementConstraint::horizontal("a", "b", Some(80.0)),
                RelativePlacementConstraint::horizontal("b", "c", Some(80.0)),
            ]),
            ..Constraints::default()
        };

        handle_constraints(&mut layout).unwrap();

        assert_close(center(&layout, nodes["a"]), (0.0, 0.0));
        assert_close(center(&layout, nodes["b"]), (80.0, -31.034693246734776));
        assert_close(center(&layout, nodes["c"]), (160.0, 38.37500672298452));
        assert_close(
            center(&layout, nodes["d"]),
            (50.127114827446604, 38.5651702845454),
        );
    }
}
