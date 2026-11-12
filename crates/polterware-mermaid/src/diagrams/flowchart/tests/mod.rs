//! Ported upstream specs, grouped by the file they came from.
//!
//! Upstream specs import the generated jison parser, reset a `FlowDB`, parse, and read the
//! database back. Here the same input goes through [`super::parse`], and the assertions read the
//! [`Ast`] (the getters' results). Assertions that need the browser, configuration, or a render
//! are noted in the port report instead of being reproduced.

mod arrows_tests;
mod chaining_tests;
mod comments_tests;
mod db_tests;
mod direction_tests;
mod edges_tests;
mod interactions_tests;
mod lexer_tests;
mod lines_tests;
mod md_string_tests;
mod node_data_tests;
mod parser_tests;
mod render_tests;
mod singlenode_tests;
mod style_tests;
mod subgraph_tests;
mod text_tests;

use super::{Ast, Edge, Vertex, parse};
use crate::error::ParseError;

fn ast(body: &str) -> Ast {
    parse(body).unwrap_or_else(|error| panic!("failed to parse {body:?}: {error}"))
}

fn parse_error(body: &str) -> ParseError {
    match parse(body) {
        Ok(ast) => panic!("expected {body:?} to fail, parsed {ast:?}"),
        Err(error) => error,
    }
}

fn vertex<'a>(ast: &'a Ast, id: &str) -> &'a Vertex {
    ast.vertex(id)
        .unwrap_or_else(|| panic!("no vertex {id:?} in {:?}", ast.vertices))
}

fn text<'a>(ast: &'a Ast, id: &str) -> &'a str {
    vertex(ast, id)
        .text
        .as_deref()
        .unwrap_or_else(|| panic!("vertex {id:?} has no text"))
}

fn edge(ast: &Ast, index: usize) -> &Edge {
    ast.edges
        .get(index)
        .unwrap_or_else(|| panic!("no edge {index} in {:?}", ast.edges))
}

/// Upstream's preprocessing removes `%%` comment lines before the parser runs
/// (`diagram-api/comments.ts` `cleanupComments`). Comment stripping lives in preprocessing, not
/// in the flowchart grammar, so the comment specs apply it the way upstream's specs do.
fn cleanup_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let is_directive = trimmed
            .strip_prefix("%%")
            .is_some_and(|rest| rest.starts_with('{'));
        let is_comment = trimmed.starts_with("%%") && !is_directive && trimmed.len() > 2;
        if !is_comment {
            out.push_str(line);
        }
    }
    out.trim_start().to_string()
}
