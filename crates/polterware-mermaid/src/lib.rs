//! Mermaid in pure Rust, a port of [mermaid](https://github.com/mermaid-js/mermaid) 12.3.0
//! (MIT, Copyright (c) 2014 - 2022 Knut Sveidqvist), with its layout engines
//! [dagre](https://github.com/dagrejs/dagre) and [graphlib](https://github.com/dagrejs/graphlib)
//! (MIT, Copyright (c) 2012-2014 Chris Pettitt). The upstream sources this port follows are
//! checked out in the workspace at `research/mermaid-port/upstream`.
//!
//! The crate has no GPUI: it parses text into a [`diagrams::Diagram`], lays it out, and
//! describes a [`scene`]. `polterware-ui` paints the scene; the Polterware core uses the parser
//! to validate what agents write.

pub mod config;
pub mod diagrams;
pub mod error;
pub mod layout;
pub mod preprocess;
pub mod scene;
pub mod theme;

pub use diagrams::{Diagram, DiagramKind};
pub use error::{ParseError, Span};

/// Preprocesses `text`, detects its type, and parses it.
pub fn parse(text: &str) -> Result<Diagram, ParseError> {
    let source = preprocess::preprocess(text);
    let kind = diagrams::detect(&source.body)
        .ok_or_else(|| ParseError::new("No diagram type detected", None))?;
    diagrams::parse_kind(kind, &source.body)
}
