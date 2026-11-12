//! Port of `cose-base` 1.0.3, the compound spring embedder that
//! cytoscape-cose-bilkent 4.1.0 (and so Mermaid's `cose-bilkent` layout) runs.
//!
//! The files map one to one: [`co_se_constants`], [`co_se_edge`], [`co_se_graph`],
//! [`co_se_graph_manager`], [`co_se_layout`], and [`co_se_node`]. Upstream's
//! `CoSEEdge`, `CoSEGraph`, `CoSEGraphManager`, and `CoSENode` only extend their
//! `layout-base` counterparts with a constructor, so they are documented aliases here;
//! [`CoSELayout`] is the algorithm.
//!
//! ```no_run
//! use polterware_mermaid::layout::cose_base::CoSELayout;
//! use polterware_mermaid::layout::layout_base::util::dimension_d::DimensionD;
//! use polterware_mermaid::layout::layout_base::util::point::Point;
//!
//! # fn build() -> Result<(), polterware_mermaid::layout::layout_base::LayoutError> {
//! let mut layout = CoSELayout::<()>::new();
//! layout.new_graph_manager();
//! let root = layout.fd.add_root();
//!
//! let a = layout.new_node(None);
//! layout.fd.layout.graph_manager.graph_add_node(root, a)?;
//! layout.fd.layout.graph_manager.node_mut(a).set_rect(
//!     &Point::new(0.0, 0.0),
//!     &DimensionD::new(40.0, 40.0),
//! );
//! layout.fd.node_mut(a).id = Some("a".into());
//!
//! let b = layout.new_node(None);
//! layout.fd.layout.graph_manager.graph_add_node(root, b)?;
//! layout.fd.layout.graph_manager.node_mut(b).set_rect(
//!     &Point::new(100.0, 0.0),
//!     &DimensionD::new(40.0, 40.0),
//! );
//! layout.fd.node_mut(b).id = Some("b".into());
//!
//! let edge = layout.new_edge(None);
//! layout.fd.layout.graph_manager.add_edge(edge, a, b)?;
//!
//! layout.run_layout()?;
//! # Ok(())
//! # }
//! ```

pub mod co_se_constants;
pub mod co_se_edge;
pub mod co_se_graph;
pub mod co_se_graph_manager;
pub mod co_se_layout;
pub mod co_se_node;

pub use co_se_layout::CoSELayout;
