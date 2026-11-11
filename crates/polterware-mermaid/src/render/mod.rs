//! Port of upstream `rendering-util`: the shared rendering elements every diagram renderer
//! uses (node shapes, edges, markers, clusters, labels) and the unified layout-and-render
//! pipeline (`render.ts`, `layout-algorithms/dagre`). Output is a [`crate::scene::Scene`].

pub mod clusters;
pub mod edges;
pub mod intersect;
pub mod labels;
pub mod layout_dagre;
pub mod markers;
pub mod pipeline;
pub mod shapes;
