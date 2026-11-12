//! Port of upstream `rendering-util/cloneLayoutDataForMeasure.ts`.

use super::types::LayoutData;

/// Shallow-safe clone for a measurement pass.
pub fn clone_layout_data_for_dom_measure(layout: &LayoutData) -> LayoutData {
    layout.clone()
}

/// Copy the measured `nodes`/`edges` back onto `canonical`, leaving its `config` untouched.
pub fn copy_measured_graph_onto_canonical(canonical: &mut LayoutData, measured: LayoutData) {
    canonical.nodes = measured.nodes;
    canonical.edges = measured.edges;
}
