//! Port of `index.ts`: the two preparation steps the swimlanes renderer runs before
//! [`super::layout_core::run_swimlane_layout_core`].

use crate::render::pipeline::types::LayoutData;

use super::SwimlaneError;
use super::edge_label_nodes::create_edge_label_nodes;
use super::helpers::{SharedMember, prepare_layout_for_swimlanes};

pub fn prepare_swimlane_layout(data: &mut LayoutData, dropped_members: &[SharedMember]) {
    prepare_layout_for_swimlanes(data, dropped_members);

    let transformed = create_edge_label_nodes(data);
    data.nodes = transformed.nodes;
    data.edges = transformed.edges;
}

/// `createCommonLayoutRenderer`'s swimlanes arm. Part D registers it under the `swimlanes`
/// layout name and draws the scene; until then the entry point reports that it is missing.
pub fn render(_data: &mut LayoutData) -> Result<(), SwimlaneError> {
    Err(SwimlaneError::unimplemented(
        "D",
        "createCommonLayoutRenderer(swimlanes)",
    ))
}
