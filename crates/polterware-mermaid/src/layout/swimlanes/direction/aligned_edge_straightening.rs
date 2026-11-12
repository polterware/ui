//! Port of `direction/alignedEdgeStraightening.ts`: replaces the route of an edge between two
//! nodes in different groups whose centres are aligned by one straight segment between their
//! facing faces, where that line stays clear. A diamond's vertex and a rectangle's face centre
//! both count as a port.

use std::collections::HashMap;

use crate::render::pipeline::types::{Edge, Node};
use crate::scene::Point;

use super::geometry::{
    RectBounds, RectSide, collect_node_rect_entries, dedupe_consecutive_points, rect_of_node_bounds,
};
use super::terminal_geometry::{
    EPS, NodeFace, Obstacles, clear_of_nodes, clear_of_other_edges, face_is_own,
};

/// How far apart two node centres may sit across the line and still count as aligned. The line
/// is drawn at the source's centre, so that distance lands on the target end, which the
/// renderer clips to the target afterwards.
pub const ALIGNED_CENTRE_TOLERANCE: f64 = 1.0;

#[derive(Clone, Debug)]
struct Placed {
    node_index: usize,
    id: String,
    rect: RectBounds,
    cx: f64,
    cy: f64,
}

#[derive(Clone, Debug)]
struct StraightLine {
    from: NodeFace,
    to: NodeFace,
    points: [Point; 2],
}

fn placed(nodes: &[Node], node_index: usize) -> Option<Placed> {
    let node = &nodes[node_index];
    if node.is_group.unwrap_or(false) {
        return None;
    }
    let rect = rect_of_node_bounds(node)?;
    Some(Placed {
        node_index,
        id: node.id.clone(),
        rect,
        cx: node.x.unwrap_or(0.0),
        cy: node.y.unwrap_or(0.0),
    })
}

/// The facing faces of two nodes stacked one above the other, joined at the source's centre.
fn vertical_line(src: &Placed, dst: &Placed) -> Option<StraightLine> {
    let down = dst.rect.top > src.rect.bottom;
    if !down && dst.rect.bottom >= src.rect.top {
        return None;
    }
    let (src_side, dst_side) = if down {
        (RectSide::Bottom, RectSide::Top)
    } else {
        (RectSide::Top, RectSide::Bottom)
    };
    Some(StraightLine {
        from: NodeFace {
            node_id: src.id.clone(),
            rect: src.rect,
            side: src_side,
        },
        to: NodeFace {
            node_id: dst.id.clone(),
            rect: dst.rect,
            side: dst_side,
        },
        points: [
            Point::new(src.cx, if down { src.rect.bottom } else { src.rect.top }),
            Point::new(src.cx, if down { dst.rect.top } else { dst.rect.bottom }),
        ],
    })
}

/// The facing faces of two nodes side by side, joined at the source's centre.
fn horizontal_line(src: &Placed, dst: &Placed) -> Option<StraightLine> {
    let right = dst.rect.left > src.rect.right;
    if !right && dst.rect.right >= src.rect.left {
        return None;
    }
    let (src_side, dst_side) = if right {
        (RectSide::Right, RectSide::Left)
    } else {
        (RectSide::Left, RectSide::Right)
    };
    Some(StraightLine {
        from: NodeFace {
            node_id: src.id.clone(),
            rect: src.rect,
            side: src_side,
        },
        to: NodeFace {
            node_id: dst.id.clone(),
            rect: dst.rect,
            side: dst_side,
        },
        points: [
            Point::new(if right { src.rect.right } else { src.rect.left }, src.cy),
            Point::new(if right { dst.rect.left } else { dst.rect.right }, src.cy),
        ],
    })
}

/// The edge's two nodes, when they are distinct nodes in different groups.
fn cross_group_pair(
    edge: &Edge,
    nodes: &[Node],
    by_id: &HashMap<&str, usize>,
) -> Option<(Placed, Placed)> {
    let src = placed(
        nodes,
        *by_id.get(edge.start.as_deref().unwrap_or_default())?,
    )?;
    let dst = placed(nodes, *by_id.get(edge.end.as_deref().unwrap_or_default())?)?;
    if src.node_index == dst.node_index {
        return None;
    }
    if nodes[src.node_index].parent_id == nodes[dst.node_index].parent_id {
        return None;
    }
    Some((src, dst))
}

/// The straight line between the facing faces of two aligned nodes in different groups.
fn straight_line_for(
    edge: &Edge,
    nodes: &[Node],
    by_id: &HashMap<&str, usize>,
) -> Option<StraightLine> {
    let (src, dst) = cross_group_pair(edge, nodes, by_id)?;
    if (src.cx - dst.cx).abs() <= ALIGNED_CENTRE_TOLERANCE {
        return vertical_line(&src, &dst);
    }
    if (src.cy - dst.cy).abs() <= ALIGNED_CENTRE_TOLERANCE {
        return horizontal_line(&src, &dst);
    }
    None
}

fn is_already(current: &[Point], line: &[Point; 2]) -> bool {
    current.len() == line.len()
        && current.iter().zip(line.iter()).all(|(point, target)| {
            (point.x - target.x).abs() < EPS && (point.y - target.y).abs() < EPS
        })
}

fn line_is_clear(
    edge: &Edge,
    edge_index: usize,
    line: &StraightLine,
    current: &[Point],
    edges: &[Edge],
    obstacles: &Obstacles,
) -> bool {
    face_is_own(&line.from, edge_index, edges)
        && face_is_own(&line.to, edge_index, edges)
        && clear_of_nodes(edge, &line.points, obstacles)
        && clear_of_other_edges(edge_index, current, &line.points, edges)
}

fn straighten(
    nodes: &[Node],
    by_id: &HashMap<&str, usize>,
    edges: &mut [Edge],
    edge_index: usize,
    obstacles: &Obstacles,
) -> bool {
    let line = straight_line_for(&edges[edge_index], nodes, by_id);
    let current =
        dedupe_consecutive_points(edges[edge_index].points.as_deref().unwrap_or(&[]), EPS);
    let Some(line) = line else {
        return false;
    };
    if current.len() < 2 || is_already(&current, &line.points) {
        return false;
    }
    if !line_is_clear(
        &edges[edge_index],
        edge_index,
        &line,
        &current,
        edges,
        obstacles,
    ) {
        return false;
    }
    edges[edge_index].points = Some(line.points.to_vec());
    true
}

/// Replaces the route of an edge between two nodes in different groups (lanes, or subgraphs
/// inside them) whose centres are aligned by one straight segment between their facing faces.
/// An edge keeps its route when another edge meets either node through those faces, when the
/// segment would pass a node or a label, overlap an edge, or cross or touch an edge the current
/// route does not. Returns whether any route changed.
pub fn straighten_aligned_cross_lane_edges(edges: &mut [Edge], nodes: &[Node]) -> bool {
    let entries = collect_node_rect_entries(nodes);
    let obstacles = Obstacles {
        real_node_rects: entries.real_node_rects,
        label_node_rects: entries.label_node_rects,
    };
    let by_id: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.as_str(), index))
        .collect();
    let mut changed = false;
    for edge_index in 0..edges.len() {
        if edges[edge_index].is_layout_only.unwrap_or(false) {
            continue;
        }
        if straighten(nodes, &by_id, edges, edge_index, &obstacles) {
            changed = true;
        }
    }
    changed
}
