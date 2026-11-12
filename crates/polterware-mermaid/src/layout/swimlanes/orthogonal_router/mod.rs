//! Port of upstream mermaid 12.3.0
//! `rendering-util/layout-algorithms/swimlanes/orthogonalRouter`: the orthogonal edge router
//! the swimlanes layout runs after the Sugiyama phases.
//!
//! [`router`] follows `router.ts` one function at a time: ports are placed on the cardinal
//! faces of the node borders, sibling edges are distributed over the less loaded faces, each
//! edge is routed as an axis-aligned polyline through a visibility grid, parallel runs are
//! spread over tracks, and the polyline is rebuilt from the assigned track coordinates.
//! [`line_jumps`] is the layout-side half of `rendering-elements/lineJump.ts`: the arc/gap
//! geometry is computed from the routed polylines, and the renderer rewrites the painted path.

pub mod line_jumps;
pub mod router;

pub use router::route_edges_orthogonal;
