//! Port of `adjustLayout.ts`, the renderer's after-paint hook: line hops on every routed edge.
//! The hop geometry and the SVG groups it paints into belong to part D.

use crate::render::pipeline::types::LayoutData;

use super::SwimlaneError;
use super::stubs;

pub fn apply_swimlane_line_jumps(data: &LayoutData) -> Result<(), SwimlaneError> {
    let style = match &data.config.swimlane.line_hops {
        crate::config::LineHops::Enabled(false) => return Ok(()),
        crate::config::LineHops::Kind(crate::config::LineHopsKind::Gap) => "gap",
        _ => "arc",
    };
    stubs::apply_line_jumps_to_svg(data, style)
}
