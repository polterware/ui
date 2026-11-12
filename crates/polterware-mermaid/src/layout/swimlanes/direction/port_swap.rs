//! Port of `direction/portSwap.ts`: rewriting a four-point H-V-H or V-H-V detour into a
//! three-point L-shape from the source face when that saves a bend. Port assignment is a free
//! variable in bend-minimization (Tamassia; Di Battista–Eades–Tamassia–Tollis §5) and Kandinsky
//! gives each face a port spacing, so every candidate is checked for crossings, node hits, and
//! face span before it is taken.

use crate::render::pipeline::types::LayoutData;
use crate::scene::Point;

use super::geometry::{
    ThreeSegmentKind, classify_three_segment_route, collect_real_node_bounds,
    dedupe_consecutive_points, get_node_pair_geometry, same_point, segment_conflicts_with_any_edge,
    segment_hits_any_rect,
};

const EPS: f64 = 1e-6;
/// δ_s, the Kandinsky port-spacing constant (Fößmeier–Kaufmann 1995; Siebenhaller §6.1.2.2).
/// This pass shifts only the edge being swapped, so it moves the full δ_s to keep the same
/// canonical spacing as a pair that shifts symmetrically by δ_s/2.
const MIN_PORT_SPACING: f64 = 8.0;
const PORT_SHIFT: f64 = MIN_PORT_SPACING;
const TRY_DELTAS: [f64; 5] = [
    0.0,
    PORT_SHIFT,
    -PORT_SHIFT,
    2.0 * PORT_SHIFT,
    -2.0 * PORT_SHIFT,
];

/// Swaps a source port for a perpendicular one when the current route is a straight-through
/// four-point detour and the swap reaches the existing destination port with one bend.
pub fn port_swap_to_l_shape(layout: &mut LayoutData) {
    let bounds = collect_real_node_bounds(&layout.nodes);

    for edge_index in 0..layout.edges.len() {
        if layout.edges[edge_index].is_layout_only.unwrap_or(false) {
            continue;
        }
        let Some(pts) = layout.edges[edge_index].points.clone() else {
            continue;
        };
        if pts.len() < 4 {
            continue;
        }

        let deduped = dedupe_consecutive_points(&pts, EPS);
        let Some(route) = classify_three_segment_route(Some(&deduped), EPS) else {
            continue;
        };

        let p3 = route.p3;
        let is_hvh = route.kind == ThreeSegmentKind::Hvh;

        let Some(node_pair) =
            get_node_pair_geometry(&layout.edges[edge_index], &bounds.node_info_by_id, EPS)
        else {
            continue;
        };

        // Collinear source and destination pairs belong to the sibling straightener.
        if node_pair.collinear_x || node_pair.collinear_y {
            continue;
        }

        let src_rect = node_pair.src_info.rect;
        let mut new_points: Option<Vec<Point>> = None;

        for delta in TRY_DELTAS {
            let (np0, np1, np2);

            if is_hvh {
                let dst_below = node_pair.dst_info.cy > node_pair.src_info.cy;
                let new_src_y = if dst_below {
                    src_rect.bottom
                } else {
                    src_rect.top
                };
                let new_src_x = node_pair.src_info.cx + delta;
                // Must lie strictly within the source face span (exclusive of corners).
                if new_src_x <= src_rect.left + EPS || new_src_x >= src_rect.right - EPS {
                    continue;
                }
                np0 = Point::new(new_src_x, new_src_y);
                np1 = Point::new(new_src_x, p3.y);
                np2 = Point::new(p3.x, p3.y);
            } else {
                let dst_east = node_pair.dst_info.cx > node_pair.src_info.cx;
                let new_src_x = if dst_east {
                    src_rect.right
                } else {
                    src_rect.left
                };
                let new_src_y = node_pair.src_info.cy + delta;
                if new_src_y <= src_rect.top + EPS || new_src_y >= src_rect.bottom - EPS {
                    continue;
                }
                np0 = Point::new(new_src_x, new_src_y);
                np1 = Point::new(p3.x, new_src_y);
                np2 = Point::new(p3.x, p3.y);
            }

            // A degenerate first or second segment collapses the L to a straight line, which is
            // even better than one bend; keep it as two points.
            let first_seg_degenerate = same_point(np0, np1, EPS);
            let second_seg_degenerate = same_point(np1, np2, EPS);
            if first_seg_degenerate && second_seg_degenerate {
                continue;
            }

            // Guard: no edge-node collisions. The first segment may touch the source and the
            // second the destination, so those ids are excluded.
            let src_ids: Vec<&str> = vec![node_pair.src_id.as_str()];
            let dst_ids: Vec<&str> = vec![node_pair.dst_id.as_str()];
            if !first_seg_degenerate
                && segment_hits_any_rect(np0, np1, &bounds.real_node_rects, &src_ids, 1.0)
            {
                continue;
            }
            if !second_seg_degenerate
                && segment_hits_any_rect(np1, np2, &bounds.real_node_rects, &dst_ids, 1.0)
            {
                continue;
            }

            // Label overlap is checked by `anchor_labels_to_polyline`, which runs after this
            // pass and re-anchors each label onto the owning edge's polyline.

            // Guards: no perpendicular crossing or collinear-axis overlap with another edge.
            let first_seg_conflicts = !first_seg_degenerate
                && segment_conflicts_with_any_edge(
                    np0,
                    np1,
                    &layout.edges,
                    Some(edge_index),
                    EPS,
                    true,
                );
            let second_seg_conflicts = !second_seg_degenerate
                && segment_conflicts_with_any_edge(
                    np1,
                    np2,
                    &layout.edges,
                    Some(edge_index),
                    EPS,
                    true,
                );
            if first_seg_conflicts || second_seg_conflicts {
                continue;
            }

            new_points = Some(if first_seg_degenerate {
                vec![np1, np2]
            } else if second_seg_degenerate {
                vec![np0, np1]
            } else {
                vec![np0, np1, np2]
            });
            break;
        }

        if let Some(new_points) = new_points {
            layout.edges[edge_index].points = Some(new_points);
        }
    }
}

/// The route the pass reads, for the tests of its shape filter.
pub fn is_l_shape_candidate(points: &[Point]) -> bool {
    classify_three_segment_route(Some(points), EPS).is_some()
}
