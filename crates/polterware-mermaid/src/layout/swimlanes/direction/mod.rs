//! Port of upstream `rendering-util/layout-algorithms/swimlanes/direction`: the geometry
//! helpers and the terminal, port, label, and direction passes the post-processing pipeline
//! runs after routing. The modules follow the TypeScript files one for one, and the public
//! entry points keep upstream's names; [`super::stubs`] is the call site part A uses.
//!
//! The routing helpers of parts C and D (`materializedGeometry`, `detourSimplification`,
//! `sharedTrackNudging`, `siblingSharedFaceRouting`, `alignedEdgeStraightening`) are not here
//! yet; they stay stubs in [`super::stubs`].

pub mod endpoint_clip;
pub mod geometry;
pub mod label_anchoring;
pub mod lr_transform;
pub mod port_swap;
pub mod terminal_geometry;
pub mod terminal_leg_alignment;
pub mod terminal_stub;
pub mod validation;
