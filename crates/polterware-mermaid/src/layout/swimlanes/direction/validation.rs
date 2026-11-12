//! Port of `direction/validation.ts`: the final read-only scan of a completed layout for edge
//! segments that still pass through a node or cross another edge. Upstream logs the issues;
//! this crate has no logger, so [`collect_validation_issues`] returns them and
//! [`validate_swimlanes_layout`] accepts the layout, as the upstream pass does.

use std::collections::HashSet;

use crate::render::pipeline::types::LayoutData;
use crate::scene::Point;

use super::super::SwimlaneError;
use super::geometry::{collect_layout_node_rects, segment_bounds_overlap_rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationIssueType {
    EdgeNodeOverlap,
    EdgeEdgeCrossing,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidationIssue {
    pub issue_type: ValidationIssueType,
    pub edge_id: String,
    /// The second edge id for a crossing, or the node id for an overlap.
    pub target_id: String,
    pub detail: String,
}

/// Whether two line segments properly intersect: touching endpoints and collinear segments do
/// not count (counter-clockwise orientation test).
fn segments_intersect(p1: Point, p2: Point, p3: Point, p4: Point) -> bool {
    let d1x = p2.x - p1.x;
    let d1y = p2.y - p1.y;
    let d2x = p4.x - p3.x;
    let d2y = p4.y - p3.y;

    let cross = d1x * d2y - d1y * d2x;
    if cross.abs() < 1e-10 {
        return false;
    }

    let dx = p3.x - p1.x;
    let dy = p3.y - p1.y;
    let t = (dx * d2y - dy * d2x) / cross;
    let u = (dx * d1y - dy * d1x) / cross;

    // Strict interior intersection, so shared node endpoints do not read as crossings.
    let eps = 0.01;
    t > eps && t < 1.0 - eps && u > eps && u < 1.0 - eps
}

struct EdgeSegment {
    edge_id: String,
    start: String,
    end: String,
    p1: Point,
    p2: Point,
}

/// Scans the layout for remaining quality issues without fixing any.
pub fn collect_validation_issues(layout: &LayoutData) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    if layout.edges.is_empty() || layout.nodes.is_empty() {
        return issues;
    }

    let node_rects = collect_layout_node_rects(&layout.nodes, true);
    let epsilon = 1.0;
    let mut edge_segments: Vec<EdgeSegment> = Vec::new();

    for edge in &layout.edges {
        if edge.is_layout_only.unwrap_or(false) {
            continue;
        }
        let Some(points) = edge.points.as_deref() else {
            continue;
        };
        if points.len() < 2 {
            continue;
        }
        let edge_start = edge.start.clone().unwrap_or_default();
        let edge_end = edge.end.clone().unwrap_or_default();
        let own_label_id = edge.label_node_id.clone();
        let edge_id = if edge.id.is_empty() {
            format!("{edge_start}->{edge_end}")
        } else {
            edge.id.clone()
        };

        for rect in &node_rects {
            if rect.node_id == edge_start || rect.node_id == edge_end {
                continue;
            }
            if own_label_id.as_deref() == Some(rect.node_id.as_str()) {
                continue;
            }
            for index in 0..points.len() - 1 {
                if segment_bounds_overlap_rect(
                    points[index],
                    points[index + 1],
                    rect.rect,
                    -epsilon,
                ) {
                    issues.push(ValidationIssue {
                        issue_type: ValidationIssueType::EdgeNodeOverlap,
                        edge_id: edge_id.clone(),
                        target_id: rect.node_id.clone(),
                        detail: format!("segment {index} passes through node \"{}\"", rect.node_id),
                    });
                    break;
                }
            }
        }

        for index in 0..points.len() - 1 {
            edge_segments.push(EdgeSegment {
                edge_id: edge_id.clone(),
                start: edge_start.clone(),
                end: edge_end.clone(),
                p1: points[index],
                p2: points[index + 1],
            });
        }
    }

    let mut crossing_pairs: HashSet<String> = HashSet::new();
    for i in 0..edge_segments.len() {
        for j in (i + 1)..edge_segments.len() {
            let a = &edge_segments[i];
            let b = &edge_segments[j];
            if a.edge_id == b.edge_id {
                continue;
            }
            if a.start == b.start || a.start == b.end || a.end == b.start || a.end == b.end {
                continue;
            }
            if segments_intersect(a.p1, a.p2, b.p1, b.p2) {
                let pair_key = if a.edge_id < b.edge_id {
                    format!("{}|{}", a.edge_id, b.edge_id)
                } else {
                    format!("{}|{}", b.edge_id, a.edge_id)
                };
                if crossing_pairs.insert(pair_key) {
                    issues.push(ValidationIssue {
                        issue_type: ValidationIssueType::EdgeEdgeCrossing,
                        edge_id: a.edge_id.clone(),
                        target_id: b.edge_id.clone(),
                        detail: format!("edges \"{}\" and \"{}\" cross", a.edge_id, b.edge_id),
                    });
                }
            }
        }
    }

    issues
}

/// Accepts the layout after scanning it. Upstream only logs what the scan finds, so a drawing
/// with warnings still renders; the issues are available through [`collect_validation_issues`].
pub fn validate_swimlanes_layout(layout: &LayoutData) -> Result<(), SwimlaneError> {
    let _issues = collect_validation_issues(layout);
    Ok(())
}
