//! Mermaid in pure Rust, a port of [mermaid](https://github.com/mermaid-js/mermaid) 12.3.0
//! (MIT, Copyright (c) 2014 - 2022 Knut Sveidqvist), with its layout engines
//! [dagre](https://github.com/dagrejs/dagre) and [graphlib](https://github.com/dagrejs/graphlib)
//! (MIT, Copyright (c) 2012-2014 Chris Pettitt). The upstream sources this port follows are
//! checked out in the workspace at `research/mermaid-port/upstream`.
//!
//! The crate has no GPUI: it parses text into a [`diagrams::Diagram`], lays it out, and
//! describes a [`scene`]. `polterware-ui` paints the scene; the Polterware core uses the parser
//! to validate what agents write.

pub mod common;
pub mod config;
pub mod diagrams;
pub mod error;
pub mod layout;
pub mod measure;
pub mod preprocess;
pub mod render;
pub mod scene;
pub mod theme;

pub use diagrams::{Diagram, DiagramKind};
pub use error::{ParseError, RenderError, Span};
pub use measure::{
    FallbackMeasure, Font, MarkdownLine, MarkdownWord, MarkdownWordKind, TextDimensions, TextError,
    TextMeasure, TextOptions, markdown_to_lines, non_markdown_to_lines, shape_text,
    split_line_to_fit_width, split_line_to_words, split_text_to_chars, split_word_to_fit_width,
};
pub use scene::{Scene, SourceKind, SourceRef};

/// Preprocesses `text`, detects its type, and parses it.
pub fn parse(text: &str) -> Result<Diagram, ParseError> {
    let source = preprocess::preprocess(text);
    let kind = diagrams::detect(&source.body)
        .ok_or_else(|| ParseError::new("No diagram type detected", None))?;
    diagrams::parse_kind(kind, &source.body)
}

/// Renders `source` to a [`Scene`]: detects the diagram type, parses it, lays it out, and
/// describes what a painter draws.
///
/// The source's frontmatter and `%%{init}%%` directives are applied over `config` before the
/// diagram is drawn. The types with a renderer arm below are the diagram types this entry
/// draws for now; another type reports [`RenderError::Unsupported`].
pub fn render_diagram(source: &str, config: &config::Config) -> Result<Scene, RenderError> {
    let preprocessed = preprocess::preprocess(source);
    if let Some(message) = preprocessed.error {
        return Err(RenderError::Parse(ParseError::new(message, None)));
    }
    let kind = diagrams::detect(&preprocessed.body)
        .ok_or_else(|| ParseError::new("No diagram type detected", None))?;
    let mut config = config.clone();
    config.apply(&preprocessed.config)?;
    match kind {
        DiagramKind::Flowchart => {
            let scene = diagrams::flowchart::render::render(&preprocessed.body, &config)?;
            Ok(scene)
        }
        DiagramKind::State => {
            let scene = diagrams::state::render::render(&preprocessed.body, &config)?;
            Ok(scene)
        }
        DiagramKind::Sequence => {
            let scene = diagrams::sequence::render::render(&preprocessed.body, &config)?;
            Ok(scene)
        }
        DiagramKind::Class => {
            let scene = diagrams::class::render::render(&preprocessed.body, &config)?;
            Ok(scene)
        }
        DiagramKind::Er => {
            let scene = diagrams::er::render::render(&preprocessed.body, &config)?;
            Ok(scene)
        }
        DiagramKind::Pie => {
            let scene = diagrams::pie::render::render(&preprocessed.body, &config)?;
            Ok(scene)
        }
        DiagramKind::Sankey => {
            let scene = diagrams::sankey::render::render(&preprocessed.body, &config)?;
            Ok(scene)
        }
        DiagramKind::Requirement => {
            let scene = diagrams::requirement::render::render(&preprocessed.body, &config)?;
            Ok(scene)
        }
        DiagramKind::Info => {
            let scene = diagrams::info::render::render(&preprocessed.body, &config)?;
            Ok(scene)
        }
        DiagramKind::Xychart => {
            let scene = diagrams::xychart::render::render(&preprocessed.body, &config)?;
            Ok(scene)
        }
        DiagramKind::Kanban => {
            let scene = diagrams::kanban::render::render(&preprocessed.body, &config)?;
            Ok(scene)
        }
        DiagramKind::UserJourney => {
            let scene = diagrams::user_journey::render::render(&preprocessed.body, &config)?;
            Ok(scene)
        }
        other => Err(RenderError::Unsupported(format!("{other:?}"))),
    }
}
