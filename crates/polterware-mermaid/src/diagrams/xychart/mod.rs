//! `xychart` diagrams: a port of upstream mermaid 12.3.0
//! `packages/mermaid/src/diagrams/xychart/` — the jison lexer and grammar (`parser/xychart.jison`),
//! the database semantics (`xychartDb.ts`, with `chartBuilder/interfaces.ts`'s data shapes and
//! the accessibility fields of `diagrams/common/commonDb.ts`), and the two specs
//! (`parser/xychart.jison.spec.ts`, `xychartDb.spec.ts`).
//!
//! The grammar is jison, so `lexer.rs` reproduces its start conditions and longest-match rule
//! selection, and `parser.rs` is the recursive descent of its productions. [`Ast`] holds what
//! upstream's `XYChartDB` holds after parsing: the orientation, the two axes, the plots with
//! their transformed data, and the title and accessibility strings. The body starts with the
//! header line; frontmatter and `%%{init}%%` directives are already removed by
//! [`crate::preprocess`].
//!
//! # Where the port stops short of upstream
//!
//! * Configuration and theme are not owned by this crate, so `getChartConfig`,
//!   `getChartThemeConfig`, `getChartDefaultConfig`, `getChartDefaultThemeConfig`, the
//!   `plotColorPalette`, and the `strokeFill`/`fill`/`strokeWidth` a plot stores are absent.
//!   `setOrientation` and the default orientation are kept, because the grammar sets them.
//! * `sanitizeText` is approximated the way the other diagrams' ports approximate it:
//!   `script`/`style` elements are dropped and a stray `<` is escaped, but there is no
//!   DOMPurify and no `securityLevel`/`htmlLabels` configuration. This affects the diagram
//!   title, the axis titles, the band categories, the plot titles, the point labels, and the
//!   accessibility strings.
//! * `setTmpSVGG`, `getDrawableElem`, `XYChartBuilder`, and the whole `chartBuilder/` tree plus
//!   `xychartRenderer.ts` lay out and draw: [`render`] ports them. The `chartBuilder` spec files
//!   (`chartTitle.spec.ts`, `legend.spec.ts`, `textDimensionCalculator.spec.ts`) are rendering
//!   and are not ported as specs; the renderer's fixtures cover the same geometry.
//! * `getDrawableElem` throws `No Plot to render, please provide a plot with some data` when
//!   there is no plot; that is a rendering check, so parsing a chart without plots succeeds.
//! * The grammar's LALR error text is replaced by this crate's messages; the [`ParseError`]
//!   carries the [`Span`](crate::error::Span) of the offending token.
//! * A plot's `data` keeps the `label`s `transformDataWithoutCategory` builds, including the
//!   `String(number)` form of a linear axis; that formatting follows JavaScript's
//!   `Number::toString` for the ranges a chart can reach.
//! * `text`'s `markdown` type (`MD_STR`) is read but not stored: every database setter
//!   upstream receives uses only `.text`.
//!
//! No app breaks: [`crate::diagrams::Diagram::Xychart`] keeps its variant, and parsing an
//! xychart previously always answered `ParseError::unsupported`, so no app could hold an
//! [`Ast`] yet. Polterware, Polterware Dispatch, and Vitals are the apps that reach this crate.

mod ast;
mod db;
mod lexer;
mod parser;
pub mod render;

pub use ast::{Ast, Axis, Orientation, Plot, PlotKind, PlotPoint};

use crate::error::ParseError;

/// Parses the diagram body (see [`crate::preprocess`]).
pub fn parse(body: &str) -> Result<Ast, ParseError> {
    parser::parse(body)
}

#[cfg(test)]
mod tests;
