//! Port of `direction/terminalLegAlignment.ts`: moving an edge's terminal leg along the rail it
//! turns onto so it meets a node where the node's shape has a port — a diamond at the vertex of
//! the face, a rectangle at the face centre when endpoint clipping left the leg against a
//! corner.

use std::collections::HashMap;

use crate::render::pipeline::types::{Edge, LayoutData, Node};
use crate::scene::Point;

use super::endpoint_clip::CORNER_CLEARANCE;
use super::geometry::{
    RectBounds, RectSide, dedupe_consecutive_points, rect_of_node_bounds, same_x, same_y,
    simplify_polyline,
};
use super::terminal_geometry::{
    Axis, EPS, NodeFace, Obstacles, clear_of_nodes, clear_of_other_edges, face_is_own,
    runs_along_face,
};

struct Terminal {
    node: Node,
    rect: RectBounds,
    side: RectSide,
    route: Vec<Point>,
}

/// Every name the shape registry accepts for a diamond: a node keeps the one its author wrote.
const DIAMOND_SHAPES: [&str; 4] = ["question", "diamond", "diam", "decision"];

pub fn is_diamond(node: &Node) -> bool {
    DIAMOND_SHAPES.contains(&node.shape.as_deref().unwrap_or(""))
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < EPS
}

fn within(value: f64, lo: f64, hi: f64) -> bool {
    value >= lo - EPS && value <= hi + EPS
}

fn vertical_leg_face(endpoint: Point, next: Point, rect: RectBounds) -> Option<RectSide> {
    if !same_x(endpoint, next, EPS) || !within(endpoint.x, rect.left, rect.right) {
        return None;
    }
    if near(endpoint.y, rect.bottom) && next.y > endpoint.y {
        return Some(RectSide::Bottom);
    }
    if near(endpoint.y, rect.top) && next.y < endpoint.y {
        return Some(RectSide::Top);
    }
    None
}

fn horizontal_leg_face(endpoint: Point, next: Point, rect: RectBounds) -> Option<RectSide> {
    if !same_y(endpoint, next, EPS) || !within(endpoint.y, rect.top, rect.bottom) {
        return None;
    }
    if near(endpoint.x, rect.right) && next.x > endpoint.x {
        return Some(RectSide::Right);
    }
    if near(endpoint.x, rect.left) && next.x < endpoint.x {
        return Some(RectSide::Left);
    }
    None
}

/// The face a terminal leg leaves outward from, or `None` when the leg is not on a face.
fn face_of_leg(endpoint: Point, next: Point, rect: RectBounds) -> Option<RectSide> {
    vertical_leg_face(endpoint, next, rect).or_else(|| horizontal_leg_face(endpoint, next, rect))
}

/// The route ordered from the terminal outward, when its first leg leaves a face and turns.
fn terminal_from(node: &Node, rect: RectBounds, outward: Vec<Point>) -> Option<Terminal> {
    let side = face_of_leg(outward[0], outward[1], rect)?;
    let axis = runs_along_face(side);
    if near(axis.coordinate(outward[1]), axis.coordinate(outward[2])) {
        return None;
    }
    Some(Terminal {
        node: node.clone(),
        rect,
        side,
        route: outward,
    })
}

fn endpoint_id(edge: &Edge, at_start: bool) -> &str {
    let id = if at_start { &edge.start } else { &edge.end };
    id.as_deref().unwrap_or("")
}

fn terminal_at(
    edge: &Edge,
    at_start: bool,
    node_by_id: &HashMap<String, Node>,
) -> Option<Terminal> {
    let route = dedupe_consecutive_points(edge.points.as_deref().unwrap_or(&[]), EPS);
    let node = node_by_id.get(endpoint_id(edge, at_start))?;
    let rect = rect_of_node_bounds(node)?;
    if route.len() < 3 {
        return None;
    }
    let outward = if at_start {
        route
    } else {
        let mut reversed = route;
        reversed.reverse();
        reversed
    };
    terminal_from(node, rect, outward)
}

/// Where the terminal leg belongs on its face: a diamond's face is its vertex, and a leg pushed
/// against a rectangle's corner belongs at the face centre. `None` when the leg stays put.
fn aligned_coord(terminal: &Terminal) -> Option<f64> {
    let axis = runs_along_face(terminal.side);
    let centre = match axis {
        Axis::X => terminal.node.x,
        Axis::Y => terminal.node.y,
    }
    .unwrap_or(0.0);
    let along = axis.coordinate(terminal.route[0]);
    if (along - centre).abs() < EPS {
        return None;
    }
    if is_diamond(&terminal.node) {
        return Some(centre);
    }
    let (lo, hi) = match axis {
        Axis::X => (terminal.rect.left, terminal.rect.right),
        Axis::Y => (terminal.rect.top, terminal.rect.bottom),
    };
    if (along - lo).min(hi - along) <= CORNER_CLEARANCE + EPS {
        Some(centre)
    } else {
        None
    }
}

/// The route a terminal takes once its first leg moves onto the port, in outward order.
fn moved_outward(terminal: &Terminal, target: f64) -> Vec<Point> {
    let axis = runs_along_face(terminal.side);
    let mut endpoint = terminal.route[0];
    axis.set(&mut endpoint, target);
    let mut leg_end = terminal.route[1];
    axis.set(&mut leg_end, target);
    let mut points = vec![endpoint, leg_end];
    points.extend_from_slice(&terminal.route[2..]);
    simplify_polyline(&dedupe_consecutive_points(&points, EPS))
}

fn align_terminal(
    terminal: &Terminal,
    edge_index: usize,
    edges: &[Edge],
    obstacles: &Obstacles,
    at_start: bool,
) -> Option<Vec<Point>> {
    let target = aligned_coord(terminal)?;
    let face = NodeFace {
        node_id: terminal.node.id.clone(),
        rect: terminal.rect,
        side: terminal.side,
    };
    if !face_is_own(&face, edge_index, edges) {
        return None;
    }
    let outward = moved_outward(terminal, target);
    let current = if at_start {
        terminal.route.clone()
    } else {
        let mut reversed = terminal.route.clone();
        reversed.reverse();
        reversed
    };
    let moved = if at_start {
        outward
    } else {
        let mut reversed = outward;
        reversed.reverse();
        reversed
    };
    if !clear_of_nodes(&edges[edge_index], &moved, obstacles)
        || !clear_of_other_edges(edge_index, &current, &moved, edges)
    {
        return None;
    }
    Some(moved)
}

/// Moves an edge's terminal leg along the rail it turns onto, so it meets its node where the
/// node's shape has a port. A leg moves only when no other edge ends on that face and the moved
/// route hits no node or label, overlaps no edge, and crosses or touches no edge it did not
/// already. Returns whether any route changed; endpoints are left deduplicated, so the caller
/// re-prepares them for the renderer.
pub fn align_terminal_legs_to_ports(layout: &mut LayoutData) -> bool {
    let node_by_id: HashMap<String, Node> = layout
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect();
    let entries = super::geometry::collect_node_rect_entries(&layout.nodes);
    let obstacles = Obstacles {
        real_node_rects: entries.real_node_rects,
        label_node_rects: entries.label_node_rects,
    };
    let mut changed = false;
    for edge_index in 0..layout.edges.len() {
        if layout.edges[edge_index].is_layout_only.unwrap_or(false) {
            continue;
        }
        for at_start in [true, false] {
            let terminal = match terminal_at(&layout.edges[edge_index], at_start, &node_by_id) {
                Some(terminal) => terminal,
                None => continue,
            };
            if let Some(moved) =
                align_terminal(&terminal, edge_index, &layout.edges, &obstacles, at_start)
            {
                layout.edges[edge_index].points = Some(moved);
                changed = true;
            }
        }
    }
    changed
}

/// The node a terminal face belongs to, for the readers of the pass.
pub fn terminal_face_node<'a>(face: &NodeFace, nodes: &'a [Node]) -> Option<&'a Node> {
    nodes.iter().find(|node| node.id == face.node_id)
}
