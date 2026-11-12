//! Port of `direction/sharedTrackNudging.ts`: separates interior rails that run within a
//! track gap of a parallel rail on another edge, in the Wybrow style, while keeping the
//! terminal stubs pinned to their nodes.

use crate::render::pipeline::types::{Edge, Node};
use crate::scene::Point;

use super::geometry::{
    OrthogonalSegment, RectBounds, collect_node_rect_entries, dedupe_consecutive_points,
    orthogonal_segments_for_points, orthogonal_segments_strictly_cross, overlap_length,
    rect_of_node_bounds, segment_hits_any_rect,
};

const EPS_LOCAL: f64 = 1e-3;
const MIN_SHARED: f64 = 8.0;
const TRACK_SHIFT: f64 = 7.0;
const MIN_TRACK_GAP: f64 = TRACK_SHIFT;
const SOURCE_DETOUR_STUB: f64 = 20.0;
const BUFFER: f64 = 2.0;

type Path = Vec<Point>;

#[derive(Clone, Debug)]
struct SegmentLite {
    segment: OrthogonalSegment,
    edge_index: usize,
    interior: bool,
}

fn segments_for(edge_index: usize, points: &[Point]) -> Vec<SegmentLite> {
    let length = points.len();
    orthogonal_segments_for_points(points, EPS_LOCAL)
        .into_iter()
        .map(|segment| SegmentLite {
            edge_index,
            interior: segment.index >= 1 && segment.index <= length.saturating_sub(3),
            segment,
        })
        .collect()
}

fn all_segments(edges: &[Edge]) -> Vec<SegmentLite> {
    let mut result = Vec::new();
    for (edge_index, edge) in edges.iter().enumerate() {
        if edge.is_layout_only.unwrap_or(false) {
            continue;
        }
        let Some(points) = edge.points.as_deref() else {
            continue;
        };
        if points.len() < 2 {
            continue;
        }
        result.extend(segments_for(
            edge_index,
            &dedupe_consecutive_points(points, EPS_LOCAL),
        ));
    }
    result
}

fn has_crowded_parallel_track(a: &SegmentLite, b: &SegmentLite) -> bool {
    let (a, b) = (&a.segment, &b.segment);
    if a.horizontal && b.horizontal {
        return overlap_length(a.a.x, a.b.x, b.a.x, b.b.x) >= MIN_SHARED
            && (a.a.y - b.a.y).abs() < MIN_TRACK_GAP;
    }
    if a.vertical && b.vertical {
        return overlap_length(a.a.y, a.b.y, b.a.y, b.b.y) >= MIN_SHARED
            && (a.a.x - b.a.x).abs() < MIN_TRACK_GAP;
    }
    false
}

fn candidate_is_safe(
    edges: &[Edge],
    edge_index: usize,
    candidate: &[Point],
    obstacles: &super::geometry::NodeRectEntries,
) -> bool {
    let edge = &edges[edge_index];
    let endpoint_ids: Vec<&str> = [edge.start.as_deref(), edge.end.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    let own_label_ids: Vec<&str> = edge.label_node_id.as_deref().into_iter().collect();
    let candidate_segments = segments_for(edge_index, candidate);
    if candidate_segments.len() != candidate.len() - 1 {
        return false;
    }

    for segment in &candidate_segments {
        let segment = &segment.segment;
        if segment_hits_any_rect(
            segment.a,
            segment.b,
            &obstacles.real_node_rects,
            &endpoint_ids,
            -BUFFER,
        ) {
            return false;
        }
        if segment_hits_any_rect(
            segment.a,
            segment.b,
            &obstacles.label_node_rects,
            &own_label_ids,
            -BUFFER,
        ) {
            return false;
        }
    }

    for (other_index, other) in edges.iter().enumerate() {
        if other_index == edge_index || other.is_layout_only.unwrap_or(false) {
            continue;
        }
        let Some(other_points) = other.points.as_deref() else {
            continue;
        };
        if other_points.len() < 2 {
            continue;
        }
        for candidate_segment in &candidate_segments {
            for other_segment in segments_for(
                other_index,
                &dedupe_consecutive_points(other_points, EPS_LOCAL),
            ) {
                if has_crowded_parallel_track(candidate_segment, &other_segment) {
                    return false;
                }
                if orthogonal_segments_strictly_cross(
                    candidate_segment.segment.a,
                    candidate_segment.segment.b,
                    other_segment.segment.a,
                    other_segment.segment.b,
                    EPS_LOCAL,
                ) {
                    return false;
                }
            }
        }
    }

    true
}

fn shifted_candidate(edges: &[Edge], segment: &SegmentLite, shift: f64) -> Option<Path> {
    let points = dedupe_consecutive_points(
        edges[segment.edge_index].points.as_deref().unwrap_or(&[]),
        EPS_LOCAL,
    );
    if points.len() < 4 || segment.segment.index >= points.len() - 1 {
        return None;
    }
    let mut candidate = points;
    if segment.segment.horizontal {
        candidate[segment.segment.index].y += shift;
        candidate[segment.segment.index + 1].y += shift;
    } else if segment.segment.vertical {
        candidate[segment.segment.index].x += shift;
        candidate[segment.segment.index + 1].x += shift;
    } else {
        return None;
    }
    if segments_for(segment.edge_index, &candidate).len() == candidate.len() - 1 {
        Some(candidate)
    } else {
        None
    }
}

struct SourceDetourContext {
    source_center: Point,
    target_center: Point,
    source_rect: RectBounds,
    tail: Path,
}

fn node_center(node: &Node, rect: RectBounds) -> Point {
    Point::new(
        node.x.unwrap_or((rect.left + rect.right) / 2.0),
        node.y.unwrap_or((rect.top + rect.bottom) / 2.0),
    )
}

fn source_detour_context_for(
    edges: &[Edge],
    nodes: &[Node],
    segment: &SegmentLite,
) -> Option<SourceDetourContext> {
    let edge = &edges[segment.edge_index];
    let points = dedupe_consecutive_points(edge.points.as_deref().unwrap_or(&[]), EPS_LOCAL);
    if points.len() != 4 || segment.segment.index != 1 {
        return None;
    }

    let source_node = edge
        .start
        .as_deref()
        .and_then(|id| nodes.iter().find(|node| node.id == id));
    let target_node = edge
        .end
        .as_deref()
        .and_then(|id| nodes.iter().find(|node| node.id == id));
    let source_rect = source_node.and_then(rect_of_node_bounds);
    let target_rect = target_node.and_then(rect_of_node_bounds);
    let tail = points[segment.segment.index + 2..].to_vec();
    let (Some(source_node), Some(target_node), Some(source_rect), Some(target_rect)) =
        (source_node, target_node, source_rect, target_rect)
    else {
        return None;
    };
    if tail.is_empty() {
        return None;
    }

    Some(SourceDetourContext {
        source_center: node_center(source_node, source_rect),
        target_center: node_center(target_node, target_rect),
        source_rect,
        tail,
    })
}

fn vertical_source_detour(
    segment: &SegmentLite,
    shift: f64,
    context: &SourceDetourContext,
) -> Option<Path> {
    let target_below = context.target_center.y >= context.source_center.y;
    let source_port_y = if target_below {
        context.source_rect.bottom
    } else {
        context.source_rect.top
    };
    let stub_y = source_port_y
        + if target_below {
            SOURCE_DETOUR_STUB
        } else {
            -SOURCE_DETOUR_STUB
        };
    if (target_below && segment.segment.b.y <= stub_y + EPS_LOCAL)
        || (!target_below && segment.segment.b.y >= stub_y - EPS_LOCAL)
    {
        return None;
    }

    let rail_x = segment.segment.a.x + shift;
    let mut points = vec![
        Point::new(context.source_center.x, source_port_y),
        Point::new(context.source_center.x, stub_y),
        Point::new(rail_x, stub_y),
        Point::new(rail_x, segment.segment.b.y),
    ];
    points.extend_from_slice(&context.tail);
    Some(dedupe_consecutive_points(&points, EPS_LOCAL))
}

fn horizontal_source_detour(
    segment: &SegmentLite,
    shift: f64,
    context: &SourceDetourContext,
) -> Option<Path> {
    let target_right = context.target_center.x >= context.source_center.x;
    let source_port_x = if target_right {
        context.source_rect.right
    } else {
        context.source_rect.left
    };
    let stub_x = source_port_x
        + if target_right {
            SOURCE_DETOUR_STUB
        } else {
            -SOURCE_DETOUR_STUB
        };
    if (target_right && segment.segment.b.x <= stub_x + EPS_LOCAL)
        || (!target_right && segment.segment.b.x >= stub_x - EPS_LOCAL)
    {
        return None;
    }

    let rail_y = segment.segment.a.y + shift;
    let mut points = vec![
        Point::new(source_port_x, context.source_center.y),
        Point::new(stub_x, context.source_center.y),
        Point::new(stub_x, rail_y),
        Point::new(segment.segment.b.x, rail_y),
    ];
    points.extend_from_slice(&context.tail);
    Some(dedupe_consecutive_points(&points, EPS_LOCAL))
}

fn source_detour_candidate(
    edges: &[Edge],
    nodes: &[Node],
    segment: &SegmentLite,
    shift: f64,
) -> Option<Path> {
    let context = source_detour_context_for(edges, nodes, segment)?;
    if segment.segment.vertical {
        return vertical_source_detour(segment, shift, &context);
    }
    if segment.segment.horizontal {
        return horizontal_source_detour(segment, shift, &context);
    }
    None
}

/// `nudgeSharedInteriorSubpaths`: shifts an interior rail off a parallel rail it crowds, or
/// moves its source stub outward, keeping every terminal fixed.
pub fn nudge_shared_interior_subpaths(edges: &mut [Edge], nodes: &[Node]) {
    let obstacles = collect_node_rect_entries(nodes);
    let shifts = [
        -TRACK_SHIFT,
        TRACK_SHIFT,
        -2.0 * TRACK_SHIFT,
        2.0 * TRACK_SHIFT,
        -3.0 * TRACK_SHIFT,
        3.0 * TRACK_SHIFT,
    ];

    for _ in 0..12 {
        let segments = all_segments(edges);
        let mut fixed = false;

        for i in 0..segments.len() {
            if fixed {
                break;
            }
            for j in (i + 1)..segments.len() {
                if fixed {
                    break;
                }
                let first = &segments[i];
                let second = &segments[j];
                if first.edge_index == second.edge_index
                    || !has_crowded_parallel_track(first, second)
                {
                    continue;
                }

                let candidates: Vec<&SegmentLite> = [first, second]
                    .into_iter()
                    .filter(|segment| segment.interior)
                    .collect();
                for segment in candidates {
                    for shift in shifts {
                        if let Some(direct) = shifted_candidate(edges, segment, shift)
                            && candidate_is_safe(edges, segment.edge_index, &direct, &obstacles)
                        {
                            edges[segment.edge_index].points = Some(direct);
                            fixed = true;
                            break;
                        }

                        if let Some(detoured) =
                            source_detour_candidate(edges, nodes, segment, shift)
                            && candidate_is_safe(edges, segment.edge_index, &detoured, &obstacles)
                        {
                            edges[segment.edge_index].points = Some(detoured);
                            fixed = true;
                            break;
                        }
                    }
                    if fixed {
                        break;
                    }
                }
            }
        }

        if !fixed {
            return;
        }
    }
}
