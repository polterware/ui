//! The parts of the upstream `*.spec.ts` suites that exercise the phase modules ported here.

mod anchored_nodes;
mod direction_endpoint_clip;
mod direction_geometry;
mod direction_label_anchoring;
mod direction_lr_transform;
mod direction_port_swap;
mod direction_terminal_leg_alignment;
mod direction_terminal_stub;
mod edge_label_nodes;
mod helpers;
mod lane_ordering;
mod lanes;
mod phase0;
mod phase1;
mod phase2;
mod phase3;
mod phase4;
mod pipeline;

use std::collections::HashMap;

use crate::layout::swimlanes::helpers::{EdgeRef, Graph};
use crate::render::pipeline::types::{Edge, LayoutData, Node};

/// The minimal graph `mkGraph` builds in the upstream specs: layout nodes with only ids, and
/// graph edges whose `layout.edges` stay empty.
pub fn mk_graph(nodes: &[&str], pairs: &[(&str, &str)]) -> Graph {
    let layout = LayoutData {
        nodes: nodes
            .iter()
            .map(|id| Node {
                id: (*id).to_string(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let node_by_id: HashMap<String, Node> = layout
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect();
    let edges = pairs
        .iter()
        .enumerate()
        .map(|(index, (src, dst))| EdgeRef {
            id: format!("e{index}"),
            src: (*src).to_string(),
            dst: (*dst).to_string(),
            weight: None,
            reference: Edge {
                id: format!("e{index}"),
                start: Some((*src).to_string()),
                end: Some((*dst).to_string()),
                ..Default::default()
            },
        })
        .collect();
    Graph {
        nodes: nodes.iter().map(|id| (*id).to_string()).collect(),
        edges,
        layout,
        node_by_id,
    }
}
