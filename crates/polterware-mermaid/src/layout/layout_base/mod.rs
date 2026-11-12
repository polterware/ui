//! Port of `layout-base` 1.0.2, the object model and geometry that cose-base 1.0.3
//! (and so cytoscape-cose-bilkent 4.1.0) is built on: `src/` in
//! `research/mermaid-port/upstream/mermaid/node_modules/.pnpm/layout-base@1.0.2`
//! (MIT, Copyright: i-Vis Research Group, Bilkent University, 2007 - present).
//!
//! The files map one to one: [`util`] holds the geometry and collection helpers
//! ([`util::dimension_d`], [`util::i_geometry`], [`util::linked_list`], ...), and this
//! module the object model ([`l_graph_object`], [`l_node`], [`l_edge`], [`l_graph`],
//! [`l_graph_manager`], [`layout`]), and [`fd`] the force-directed base that cose-base
//! extends.
//!
//! Where upstream's object graph has references (nodes to their owner graph and child
//! graph, edges to their endpoints, graphs to their parent node), this port owns every
//! object in an arena inside [`LGraphManager`] and refers to them by typed index
//! ([`NodeId`], [`EdgeId`], [`GraphId`]). The manager is the graph manager of exactly
//! one layout, so upstream's `graphManager` back-references are gone.
//!
//! Each file keeps the methods of the JavaScript prototype it ports: methods that only
//! touch their own object are inherent methods on [`LNode`], [`LEdge`], or [`LGraph`],
//! and methods that reach other objects are inherent methods on [`LGraphManager`]
//! (written in the file of the prototype they come from, prefixed with the receiver)
//! because the manager owns the arenas.
//!
//! Iteration orders are upstream's. JavaScript arrays and `Map`/`Set` keep insertion
//! order, so node lists, edge lists, and the visited sets of the graph walks are
//! [`Vec`]s here.
//!
//! Upstream's `HashMap`, `HashSet`, and `Emitter` are not ported as types: the object
//! model only needs the standard collections, and no consumer adds a listener to a
//! layout. Upstream's `UniqueIDGeneretor` (object identity) is replaced by the typed
//! indices.
//!
//! Upstream throws strings; the fallible methods return [`LayoutError`] carrying the
//! same message. Where upstream's code throws a JavaScript `TypeError` from a bug
//! (reading `nodes[0]` of an empty graph, calling `Array.prototype.add`), the port does
//! what the code around the bug intends and says so at the site.

pub mod fd;
pub mod l_edge;
pub mod l_graph;
pub mod l_graph_manager;
pub mod l_graph_object;
pub mod l_node;
pub mod layout;
pub mod layout_constants;
pub mod util;

pub use fd::FDLayout;
pub use l_edge::{EdgeId, LEdge};
pub use l_graph::{GraphId, LGraph};
pub use l_graph_manager::LGraphManager;
pub use l_graph_object::LGraphObject;
pub use l_node::{LNode, LabelPos, NodeId};
pub use layout::{Layout, LayoutObject, LayoutObjectRef, Updatable};

/// A failure upstream throws as a string. The payload is upstream's message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutError(pub &'static str);

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for LayoutError {}
