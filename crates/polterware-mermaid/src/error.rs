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
        Self { message: message.into(), span }
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
