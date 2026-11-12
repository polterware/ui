//! Port of `direction/detourSimplification.ts`: rewrites a heavily dog-legged edge onto a
//! shorter orthogonal port path when that keeps it clear of the nodes and does not raise its
//! crossing count. A route with fewer than four bends keeps its shape.

use std::collections::HashMap;

use crate::render::pipeline::types::{Edge, Node};
use crate::scene::Point;

use super::geometry::{
    NodeBoundsInfo, RectSide, build_orthogonal_port_path, build_same_side_track_path,
    collect_real_node_bounds, count_orthogonal_bends, orthogonal_segments_cross,
    orthogonal_segments_for_points, port_for_rect_side, same_axis_segment_overlap_length,
    segment_hits_any_rect,
};

const EPS: f64 = 1e-3;
const MIN_SHARED: f64 = 8.0;
/// The four faces, in upstream's evaluation order.
const SIDES: [RectSide; 4] = [
    RectSide::Top,
    RectSide::Bottom,
    RectSide::Left,
    RectSide::Right,
];

/// The outermost tracks, one `ANCHOR` outside every real node.
#[derive(Clone, Copy, Debug, Default)]
struct OutsideTracks {
    top: f64,
    bottom: f64,
    left: f64,
    right: f64,
}

fn outside_tracks_of(rects: &[super::geometry::RectEntry]) -> OutsideTracks {
    let mut top = f64::INFINITY;
    let mut bottom = f64::NEG_INFINITY;
    let mut left = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    for entry in rects {
        top = top.min(entry.rect.top);
        bottom = bottom.max(entry.rect.bottom);
        left = left.min(entry.rect.left);
        right = right.max(entry.rect.right);
    }
    OutsideTracks {
        top: top - 20.0,
        bottom: bottom + 20.0,
        left: left - 20.0,
        right: right + 20.0,
    }
}

fn track_for_side(tracks: &OutsideTracks, side: RectSide) -> f64 {
    match side {
        RectSide::Top => tracks.top,
        RectSide::Bottom => tracks.bottom,
        RectSide::Left => tracks.left,
        RectSide::Right => tracks.right,
    }
}

fn build_orthogonal_path_candidates(
    src: Point,
    src_side: RectSide,
    dst: Point,
    dst_side: RectSide,
    outside: &OutsideTracks,
) -> Vec<Vec<Point>> {
    let mut paths = Vec::new();
    if let Some(base) = build_orthogonal_port_path(src, src_side, dst, dst_side, 20.0, EPS) {
        paths.push(base);
    }

    // When the local "just outside these two ports" track still crosses an existing connector,
    // also try the corresponding global outer channel: keep the port pair and topology class,
    // move the maximal middle segment into an uncongested alley if safety checks accept it.
    if src_side == dst_side {
        paths.push(build_same_side_track_path(
            src,
            src_side,
            dst,
            track_for_side(outside, src_side),
        ));
    }

    paths
}

fn path_hits_node(
    pts: &[Point],
    exclude_ids: &[&str],
    real_node_rects: &[super::geometry::RectEntry],
) -> bool {
    for pair in pts.windows(2) {
        if segment_hits_any_rect(pair[0], pair[1], real_node_rects, exclude_ids, 1.0) {
            return true;
        }
    }
    false
}

fn path_conflict_count(
    path: &[Point],
    current_edge_index: usize,
    include_incident_edges: bool,
    edges: &[Edge],
) -> usize {
    let mut conflicts = 0;
    let path_segments = orthogonal_segments_for_points(path, EPS);
    let current_start = edges[current_edge_index].start.as_deref();
    let current_end = edges[current_edge_index].end.as_deref();
    for (index, other) in edges.iter().enumerate() {
        if index == current_edge_index || other.is_layout_only.unwrap_or(false) {
            continue;
        }
        let other_start = other.start.as_deref();
        let other_end = other.end.as_deref();
        if !include_incident_edges
            && current_start.is_some()
            && current_end.is_some()
            && (other_start == current_start
                || other_start == current_end
                || other_end == current_start
                || other_end == current_end)
        {
            continue;
        }
        let Some(other_pts) = other.points.as_deref() else {
            continue;
        };
        if other_pts.len() < 2 {
            continue;
        }
        for path_segment in &path_segments {
            for other_segment in orthogonal_segments_for_points(other_pts, EPS) {
                if orthogonal_segments_cross(
                    path_segment.a,
                    path_segment.b,
                    other_segment.a,
                    other_segment.b,
                    EPS,
                    EPS,
                ) {
                    conflicts += 1;
                    continue;
                }
                if same_axis_segment_overlap_length(path_segment, &other_segment, EPS) >= MIN_SHARED
                {
                    conflicts += 1;
                }
            }
        }
    }
    conflicts
}

fn nearest_side_of_rect(pt: Point, info: &NodeBoundsInfo) -> RectSide {
    let d_top = (pt.y - info.rect.top).abs();
    let d_bottom = (pt.y - info.rect.bottom).abs();
    let d_left = (pt.x - info.rect.left).abs();
    let d_right = (pt.x - info.rect.right).abs();
    let mut best = RectSide::Top;
    let mut best_dist = d_top;
    if d_bottom < best_dist {
        best = RectSide::Bottom;
        best_dist = d_bottom;
    }
    if d_left < best_dist {
        best = RectSide::Left;
        best_dist = d_left;
    }
    if d_right < best_dist {
        best = RectSide::Right;
    }
    best
}

#[derive(Clone, Debug)]
struct FaceClaim {
    side: RectSide,
    edge_id: String,
}

fn add_face_claim(
    face_claims: &mut HashMap<String, Vec<FaceClaim>>,
    node_id: &str,
    side: RectSide,
    edge_id: &str,
) {
    face_claims
        .entry(node_id.to_string())
        .or_default()
        .push(FaceClaim {
            side,
            edge_id: edge_id.to_string(),
        });
}

fn face_is_claimed(
    face_claims: &HashMap<String, Vec<FaceClaim>>,
    node_id: &str,
    side: RectSide,
    ignore_edge_id: &str,
) -> bool {
    face_claims.get(node_id).is_some_and(|claims| {
        claims
            .iter()
            .any(|claim| claim.edge_id != ignore_edge_id && claim.side == side)
    })
}

/// `simplifyDetouredEdges`: rewrites each edge with four or more bends onto the shortest
/// obstacle-free port pair, breaking ties by source face, destination face, and candidate
/// order.
pub fn simplify_detoured_edges(edges: &mut [Edge], nodes: &[Node]) {
    let bounds = collect_real_node_bounds(nodes);
    let outside = outside_tracks_of(&bounds.real_node_rects);

    let node_info = &bounds.node_info_by_id;

    let mut face_claims: HashMap<String, Vec<FaceClaim>> = HashMap::new();
    for edge in edges.iter() {
        if edge.is_layout_only.unwrap_or(false) {
            continue;
        }
        let Some(pts) = edge.points.as_deref() else {
            continue;
        };
        if pts.is_empty() {
            continue;
        }
        if let Some(start_id) = edge.start.as_deref()
            && let Some(info) = node_info.get(start_id)
        {
            add_face_claim(
                &mut face_claims,
                start_id,
                nearest_side_of_rect(pts[0], info),
                &edge.id,
            );
        }
        if let Some(end_id) = edge.end.as_deref()
            && let Some(info) = node_info.get(end_id)
        {
            add_face_claim(
                &mut face_claims,
                end_id,
                nearest_side_of_rect(pts[pts.len() - 1], info),
                &edge.id,
            );
        }
    }

    for edge_index in 0..edges.len() {
        if edges[edge_index].is_layout_only.unwrap_or(false) {
            continue;
        }
        let current_points = match edges[edge_index].points.clone() {
            Some(points) if points.len() >= 2 => points,
            _ => continue,
        };
        let current_bends = count_orthogonal_bends(&current_points, EPS);
        if current_bends < 4 {
            continue;
        }
        let (Some(src_id), Some(dst_id)) = (
            edges[edge_index].start.clone(),
            edges[edge_index].end.clone(),
        ) else {
            continue;
        };
        let (Some(src_info), Some(dst_info)) = (node_info.get(&src_id), node_info.get(&dst_id))
        else {
            continue;
        };
        let edge_id = edges[edge_index].id.clone();
        let current_crossing_conflicts =
            path_conflict_count(&current_points, edge_index, true, edges);
        let current_non_incident_conflicts =
            path_conflict_count(&current_points, edge_index, false, edges);

        let mut best_path: Option<Vec<Point>> = None;
        let mut best_crossing_conflicts = current_crossing_conflicts;
        let mut best_bends = current_bends;

        for src_side in SIDES {
            if face_is_claimed(&face_claims, &src_id, src_side, &edge_id) {
                continue;
            }
            let src_port = port_for_rect_side(src_info, src_side);
            for dst_side in SIDES {
                if face_is_claimed(&face_claims, &dst_id, dst_side, &edge_id) {
                    continue;
                }
                let dst_port = port_for_rect_side(dst_info, dst_side);
                for path in build_orthogonal_path_candidates(
                    src_port, src_side, dst_port, dst_side, &outside,
                ) {
                    if path_hits_node(&path, &[&src_id, &dst_id], &bounds.real_node_rects) {
                        continue;
                    }

                    let path_bends = count_orthogonal_bends(&path, EPS);
                    if current_crossing_conflicts > 0 {
                        let path_crossing_conflicts =
                            path_conflict_count(&path, edge_index, true, edges);
                        if path_crossing_conflicts > best_crossing_conflicts
                            || (path_crossing_conflicts == best_crossing_conflicts
                                && path_bends >= best_bends)
                        {
                            continue;
                        }
                        best_crossing_conflicts = path_crossing_conflicts;
                        best_bends = path_bends;
                        best_path = Some(path);
                        continue;
                    }

                    if path_conflict_count(&path, edge_index, false, edges)
                        > current_non_incident_conflicts
                    {
                        continue;
                    }
                    if path_bends < best_bends {
                        best_bends = path_bends;
                        best_path = Some(path);
                    }
                }
            }
        }

        if let Some(best_path) = best_path {
            let refresh_src = face_claims.get_mut(&src_id);
            if let Some(claims) = refresh_src {
                claims.retain(|claim| claim.edge_id != edge_id);
            }
            let refresh_dst = face_claims.get_mut(&dst_id);
            if let Some(claims) = refresh_dst {
                claims.retain(|claim| claim.edge_id != edge_id);
            }
            add_face_claim(
                &mut face_claims,
                &src_id,
                nearest_side_of_rect(best_path[0], src_info),
                &edge_id,
            );
            add_face_claim(
                &mut face_claims,
                &dst_id,
                nearest_side_of_rect(best_path[best_path.len() - 1], dst_info),
                &edge_id,
            );
            edges[edge_index].points = Some(best_path);
        }
    }
}
