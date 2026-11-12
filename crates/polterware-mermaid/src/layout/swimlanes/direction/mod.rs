//! Port of upstream `rendering-util/layout-algorithms/swimlanes/direction`: the geometry
//! helpers and the terminal, port, label, routing, and direction passes the post-processing
//! pipeline runs after routing. The modules follow the TypeScript files one for one, and the
//! public entry points keep upstream's names; [`super::stubs`] is the call site part A uses.
//!
//! Parts A through C are filled: the routing cleanup passes (`materialized_geometry`,
//! `detour_simplification`, `shared_track_nudging`, `sibling_shared_face_routing`,
//! `aligned_edge_straightening`) port the modules of those names.

pub mod aligned_edge_straightening;
pub mod detour_simplification;
pub mod endpoint_clip;
pub mod geometry;
pub mod label_anchoring;
pub mod lr_transform;
pub mod materialized_geometry;
pub mod port_swap;
pub mod shared_track_nudging;
pub mod sibling_shared_face_routing;
pub mod terminal_geometry;
pub mod terminal_leg_alignment;
pub mod terminal_stub;
pub mod validation;
