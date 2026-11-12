//! Port of Mermaid 12.3.0's
//! `rendering-util/layout-algorithms/grid/edgeLabels.ts`.
//!
//! Part D ports the label preparation and placement transaction. Part A leaves a no-op placement
//! pass so the layout core pipeline matches upstream's phase order without committing label
//! coordinates.

use crate::layout::grid::router_instrumentation::GridRoutingInstrumentation;
use crate::layout::grid::types::{GridError, LayoutData};

/// Edge-label work counters. Part D expands this type with the counters of `edgeLabels.ts`.
#[derive(Clone, Debug, Default)]
pub struct GridEdgeLabelInstrumentation;

#[derive(Clone, Debug, Default)]
pub struct GridEdgeLabelOptions;

/// Labels consume final routes and may transactionally reroute them, so they are the last layout
/// phase before the working copy is committed to the render model. Part D fills this stub.
pub fn position_grid_edge_labels(
    _data: &mut LayoutData,
    _instrumentation: Option<&mut GridEdgeLabelInstrumentation>,
    _routing_instrumentation: Option<&mut GridRoutingInstrumentation>,
    _options: &GridEdgeLabelOptions,
) -> Result<(), GridError> {
    Ok(())
}
