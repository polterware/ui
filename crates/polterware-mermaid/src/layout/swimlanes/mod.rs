//! Port of upstream mermaid 12.3.0 `rendering-util/layout-algorithms/swimlanes`
//! (`research/mermaid-port/upstream/mermaid/packages/mermaid/src/rendering-util/layout-algorithms/swimlanes`).
//!
//! The swimlanes diagram reuses the flowchart parser, database, and renderer and swaps in this
//! Sugiyama layout: the top-level [`index`] hooks, the [`pipeline`] phases, and every phase's
//! state model live here. The modules follow the TypeScript files one for one; where a phase
//! calls into direction/geometry, the routing, or the rendering registration, the call goes
//! through [`stubs`], which keeps the call site in one place. The orthogonal edge router is
//! ported in [`orthogonal_router`], and [`index::render`] is the swimlanes arm of the common
//! renderer.
//!
//! The arithmetic keeps upstream's `f64` order: helper [`helpers::js_round`] is `Math.round`
//! and [`helpers::locale_compare`] is `String.prototype.localeCompare` for the identifiers a
//! diagram uses.

pub mod adjust_layout;
pub mod anchored_nodes;
pub mod config;
pub mod direction;
pub mod driving_tree;
pub mod edge_label_nodes;
pub mod helpers;
pub mod index;
pub mod lane_ordering;
pub mod lanes;
pub mod layout_core;
pub mod orthogonal_router;
pub mod phase0_helpers;
pub mod phase1_cycles;
pub mod phase2_cross_counts;
pub mod phase2_cross_lane_adjust;
pub mod phase2_cross_optimization;
pub mod phase2_dummies;
pub mod phase2_gravity;
pub mod phase2_lane_aware_compact;
pub mod phase2_longest_path;
pub mod phase2_multitree_core;
pub mod phase2_multitree_order;
pub mod phase2_options;
pub mod phase3_ordering;
pub mod phase4_coordinates;
pub mod pipeline;
pub mod post_processing;
pub mod stubs;

#[cfg(test)]
mod tests;

use std::fmt;

/// What can stop the swimlane layout while a later part is still a stub.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwimlaneError {
    /// A phase this port has not filled yet: the part that fills it and the upstream function.
    Unimplemented {
        part: &'static str,
        function: &'static str,
    },
    /// The layout could not be written back or validated; carries the upstream message.
    Layout(String),
}

impl SwimlaneError {
    pub fn unimplemented(part: &'static str, function: &'static str) -> Self {
        Self::Unimplemented { part, function }
    }

    pub fn layout(message: impl Into<String>) -> Self {
        Self::Layout(message.into())
    }
}

impl fmt::Display for SwimlaneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unimplemented { part, function } => {
                write!(f, "swimlane {function} is filled by part {part}")
            }
            Self::Layout(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for SwimlaneError {}
