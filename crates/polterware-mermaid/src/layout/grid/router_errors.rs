//! Port of Mermaid 12.3.0's
//! `rendering-util/layout-algorithms/grid/routerErrors.ts`, plus the plain geometry errors the
//! routing helpers throw.

use crate::layout::grid::router_instrumentation::GridRoutingFallbackReason;
use crate::layout::grid::types::GridError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchStateScope {
    Edge,
    Invocation,
}

impl SearchStateScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Edge => "edge",
            Self::Invocation => "invocation",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridRoutingResourceLimitError {
    pub reason: GridRoutingFallbackReason,
    pub message: String,
    pub search_state_scope: Option<SearchStateScope>,
}

impl GridRoutingResourceLimitError {
    pub fn new(
        reason: GridRoutingFallbackReason,
        message: impl Into<String>,
        search_state_scope: Option<SearchStateScope>,
    ) -> Self {
        Self {
            reason,
            message: message.into(),
            search_state_scope,
        }
    }
}

impl std::fmt::Display for GridRoutingResourceLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GridRoutingResourceLimitError {}

/// Routing errors: upstream throws a `GridRoutingResourceLimitError` for caps and a plain `Error`
/// for invalid geometry. Both map to one Rust error so callers that recover from caps can match
/// the variant.
#[derive(Clone, Debug, PartialEq)]
pub enum GridRoutingError {
    Geometry(String),
    /// An upstream `gridError`: the code, message, and structured details travel together so
    /// retries can inspect the failure reason.
    Grid(GridError),
    ResourceLimit(GridRoutingResourceLimitError),
}

impl GridRoutingError {
    pub fn geometry(message: impl Into<String>) -> Self {
        Self::Geometry(message.into())
    }

    pub fn resource_limit(reason: GridRoutingFallbackReason, message: impl Into<String>) -> Self {
        Self::ResourceLimit(GridRoutingResourceLimitError::new(reason, message, None))
    }

    pub fn is_resource_limit(&self) -> bool {
        matches!(self, Self::ResourceLimit(_))
    }
}

impl std::fmt::Display for GridRoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Geometry(message) => f.write_str(message),
            Self::Grid(error) => error.fmt(f),
            Self::ResourceLimit(error) => f.write_str(&error.message),
        }
    }
}

impl std::error::Error for GridRoutingError {}
