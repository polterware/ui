//! Port of `adjustLayout.ts`, the renderer's after-paint hook: line hops on every routed edge.
//!
//! Upstream reads the geometry back from the painted edge paths, then rewrites each `d` that
//! carries a hop. This crate paints from layout data, so the hook returns the same rewritten
//! paths and the paint step uses them in place of the plain route.

use crate::render::pipeline::types::LayoutData;
use crate::scene::Path;

use super::SwimlaneError;
use super::stubs;

/// `applySwimlaneLineJumps`: the hop geometry of every crossing edge, keyed by edge id.
pub fn apply_swimlane_line_jumps(data: &LayoutData) -> Result<Vec<(String, Path)>, SwimlaneError> {
    let style = match &data.config.swimlane.line_hops {
        crate::config::LineHops::Enabled(false) => return Ok(Vec::new()),
        crate::config::LineHops::Kind(crate::config::LineHopsKind::Gap) => "gap",
        _ => "arc",
    };
    stubs::apply_line_jumps_to_svg(data, style)
}
