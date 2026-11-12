//! Port of Mermaid 12.3.0's
//! `rendering-util/layout-algorithms/grid/routerOccupancy.ts`.
//!
//! Container-scoped index of committed route segments. Routes from different edges must not share
//! a corridor, so every routed edge registers its nonterminal segments here and later candidates
//! query it with the clearance rule. Routes are append-only and can be truncated back to an earlier
//! size, which is how bundle retries roll back state.

use std::collections::BTreeMap;

use crate::layout::grid::router_constraint::{OrthogonalSegment, nonterminal_segments};
use crate::layout::grid::router_topology::EDGE_CLEARANCE_PX;
use crate::layout::grid::types::{GridOrientation, RouterPoint};

fn segments_too_close(candidate: &OrthogonalSegment, committed: &OrthogonalSegment) -> bool {
    if candidate.orientation != committed.orientation {
        return false;
    }
    let horizontal = candidate.orientation == GridOrientation::H;
    let low_of = |segment: &OrthogonalSegment| {
        if horizontal {
            segment.a.x.min(segment.b.x)
        } else {
            segment.a.y.min(segment.b.y)
        }
    };
    let high_of = |segment: &OrthogonalSegment| {
        if horizontal {
            segment.a.x.max(segment.b.x)
        } else {
            segment.a.y.max(segment.b.y)
        }
    };
    let overlap =
        high_of(candidate).min(high_of(committed)) - low_of(candidate).max(low_of(committed));
    let separation = if horizontal {
        (candidate.a.y - committed.a.y).abs()
    } else {
        (candidate.a.x - committed.a.x).abs()
    };
    overlap > 0.0 && separation < EDGE_CLEARANCE_PX
}

#[derive(Clone, Debug)]
struct OccupiedSegment {
    segment: OrthogonalSegment,
    pair_key: String,
}

#[derive(Clone, Copy, Debug)]
struct OccupancyLogEntry {
    orientation: GridOrientation,
    coordinate: f64,
}

/// Container-scoped index of committed route segments, keyed by their fixed coordinate. A query
/// only inspects lines within the clearance instead of every committed segment.
#[derive(Clone, Debug, Default)]
pub struct RouteOccupancyIndex {
    // `Coordinate` gives f64 a total order, so a `BTreeMap` supplies the sorted line keys the
    // binary search reads.
    horizontal_lines: BTreeMap<crate::layout::grid::types::Coordinate, Vec<OccupiedSegment>>,
    vertical_lines: BTreeMap<crate::layout::grid::types::Coordinate, Vec<OccupiedSegment>>,
    log: Vec<OccupancyLogEntry>,
}

impl RouteOccupancyIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn size(&self) -> usize {
        self.log.len()
    }

    fn lines_mut(
        &mut self,
        orientation: GridOrientation,
    ) -> &mut BTreeMap<crate::layout::grid::types::Coordinate, Vec<OccupiedSegment>> {
        match orientation {
            GridOrientation::H => &mut self.horizontal_lines,
            GridOrientation::V => &mut self.vertical_lines,
        }
    }

    fn lines(
        &self,
        orientation: GridOrientation,
    ) -> &BTreeMap<crate::layout::grid::types::Coordinate, Vec<OccupiedSegment>> {
        match orientation {
            GridOrientation::H => &self.horizontal_lines,
            GridOrientation::V => &self.vertical_lines,
        }
    }

    pub fn add(&mut self, route: &[RouterPoint], pair_key: &str) {
        for segment in nonterminal_segments(route) {
            let orientation = segment.orientation;
            let coordinate = if orientation == GridOrientation::H {
                segment.a.y
            } else {
                segment.a.x
            };
            self.lines_mut(orientation)
                .entry(crate::layout::grid::types::Coordinate::new(coordinate))
                .or_default()
                .push(OccupiedSegment {
                    segment,
                    pair_key: pair_key.to_string(),
                });
            self.log.push(OccupancyLogEntry {
                orientation,
                coordinate,
            });
        }
    }

    pub fn truncate(&mut self, size: usize) {
        while self.log.len() > size {
            let entry = self.log.pop().expect("log is not empty");
            if let Some(bucket) = self.lines_mut(entry.orientation).get_mut(
                &crate::layout::grid::types::Coordinate::new(entry.coordinate),
            ) {
                bucket.pop();
            }
        }
    }

    /// Reports whether a route shares a corridor with a committed route of another edge pair.
    pub fn conflicts_with_route(&self, route: &[RouterPoint], pair_key: &str) -> bool {
        nonterminal_segments(route)
            .iter()
            .any(|segment| self.conflicts(segment, pair_key))
    }

    pub fn conflicts_with_arc(&self, from: RouterPoint, to: RouterPoint, pair_key: &str) -> bool {
        let orientation = if from.y == to.y {
            GridOrientation::H
        } else {
            GridOrientation::V
        };
        self.conflicts(
            &OrthogonalSegment {
                a: from,
                b: to,
                orientation,
            },
            pair_key,
        )
    }

    fn conflicts(&self, candidate: &OrthogonalSegment, pair_key: &str) -> bool {
        let orientation = candidate.orientation;
        let coordinate = if orientation == GridOrientation::H {
            candidate.a.y
        } else {
            candidate.a.x
        };
        let lines = self.lines(orientation);
        for (key, occupied) in lines.range(..) {
            let key = key.value();
            // Upstream starts after `coordinate - clearance` and stops at `coordinate + clearance`.
            if key <= coordinate - EDGE_CLEARANCE_PX {
                continue;
            }
            if key >= coordinate + EDGE_CLEARANCE_PX {
                break;
            }
            for entry in occupied {
                if entry.pair_key != pair_key && segments_too_close(candidate, &entry.segment) {
                    return true;
                }
            }
        }
        false
    }
}
