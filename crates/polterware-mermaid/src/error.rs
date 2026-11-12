//! Errors carry a position so an editor or an agent can point at the line.

/// A place in the source: 1-based line and column, and the byte range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub line: u32,
    pub column: u32,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub span: Option<Span>,
}

impl ParseError {
    pub fn new(message: impl Into<String>, span: Option<Span>) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }

    pub fn at(message: impl Into<String>, span: Span) -> Self {
        Self::new(message, Some(span))
    }

    pub fn unsupported(kind: &str) -> Self {
        Self::new(format!("{kind} diagrams are not supported yet"), None)
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.span {
            Some(span) => write!(f, "{}:{}: {}", span.line, span.column, self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for ParseError {}

/// What can stop [`crate::render_diagram`]: the source, the configuration, the diagram type,
/// or the layout pipeline.
#[derive(Clone, Debug, PartialEq)]
pub enum RenderError {
    Parse(ParseError),
    Config(crate::config::ConfigError),
    Layout(crate::render::layout_dagre::LayoutError),
    /// A diagram type the renderer does not draw yet (the detector's kind name).
    Unsupported(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(f),
            Self::Config(error) => error.fmt(f),
            Self::Layout(error) => error.fmt(f),
            Self::Unsupported(kind) => write!(f, "{kind} diagrams are not supported yet"),
        }
    }
}

impl std::error::Error for RenderError {}

impl From<ParseError> for RenderError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<crate::config::ConfigError> for RenderError {
    fn from(error: crate::config::ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<crate::render::layout_dagre::LayoutError> for RenderError {
    fn from(error: crate::render::layout_dagre::LayoutError) -> Self {
        Self::Layout(error)
    }
}

impl From<crate::diagrams::flowchart::FlowchartRenderError> for RenderError {
    fn from(error: crate::diagrams::flowchart::FlowchartRenderError) -> Self {
        match error {
            crate::diagrams::flowchart::FlowchartRenderError::Parse(error) => Self::Parse(error),
            crate::diagrams::flowchart::FlowchartRenderError::Layout(error) => Self::Layout(error),
        }
    }
}

impl From<crate::diagrams::sequence::SequenceRenderError> for RenderError {
    fn from(error: crate::diagrams::sequence::SequenceRenderError) -> Self {
        match error {
            crate::diagrams::sequence::SequenceRenderError::Parse(error) => Self::Parse(error),
            crate::diagrams::sequence::SequenceRenderError::Layout(error) => Self::Layout(error),
        }
    }
}

impl From<crate::diagrams::class::render::ClassRenderError> for RenderError {
    fn from(error: crate::diagrams::class::render::ClassRenderError) -> Self {
        match error {
            crate::diagrams::class::render::ClassRenderError::Parse(error) => Self::Parse(error),
            crate::diagrams::class::render::ClassRenderError::Layout(error) => Self::Layout(error),
        }
    }
}

impl From<crate::diagrams::er::render::ErRenderError> for RenderError {
    fn from(error: crate::diagrams::er::render::ErRenderError) -> Self {
        match error {
            crate::diagrams::er::render::ErRenderError::Parse(error) => Self::Parse(error),
            crate::diagrams::er::render::ErRenderError::Layout(error) => Self::Layout(error),
        }
    }
}
