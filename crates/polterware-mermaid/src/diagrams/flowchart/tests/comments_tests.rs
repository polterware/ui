//! Port of `parser/flow-comments.spec.js`.
//!
//! The spec wraps every input in upstream's `cleanupComments`, which belongs to preprocessing,
//! so [`super::cleanup_comments`] stands in for it.

use super::{ast, cleanup_comments, edge, vertex};

fn assert_ab_edge(body: &str) {
    let ast = ast(&cleanup_comments(body));
    assert_eq!(vertex(&ast, "A").id, "A");
    assert_eq!(vertex(&ast, "B").id, "B");
    assert_eq!(ast.edges.len(), 1);
    assert_eq!(edge(&ast, 0).start, "A");
    assert_eq!(edge(&ast, 0).end, "B");
    assert_eq!(
        edge(&ast, 0).type_name.unwrap(),
        super::super::ArrowType::Point
    );
    assert_eq!(edge(&ast, 0).text, "");
}

#[test]
fn should_handle_comments() {
    assert_ab_edge("graph TD;\n%% Comment\n A-->B;");
}

#[test]
fn should_handle_comments_at_the_start() {
    assert_ab_edge("%% Comment\ngraph TD;\n A-->B;");
}

#[test]
fn should_handle_comments_at_the_end() {
    assert_ab_edge("graph TD;\n A-->B\n %% Comment at the end\n");
}

#[test]
fn should_handle_comments_at_the_end_no_trailing_newline() {
    assert_ab_edge("graph TD;\n A-->B\n%% Comment");
}

#[test]
fn should_handle_comments_at_the_end_many_trailing_newlines() {
    assert_ab_edge("graph TD;\n A-->B\n%% Comment\n\n\n");
}

#[test]
fn should_handle_no_trailing_newlines() {
    assert_ab_edge("graph TD;\n A-->B");
}

#[test]
fn should_handle_many_trailing_newlines() {
    assert_ab_edge("graph TD;\n A-->B\n\n");
}

#[test]
fn should_handle_a_comment_with_blank_rows_in_between() {
    assert_ab_edge("graph TD;\n\n\n %% Comment\n A-->B;");
}

#[test]
fn should_handle_a_comment_with_mermaid_code_in_it() {
    assert_ab_edge(
        "graph TD;\n\n\n %% Test od>Odd shape]-->|Two line<br>edge comment|ro;\n A-->B;",
    );
}
