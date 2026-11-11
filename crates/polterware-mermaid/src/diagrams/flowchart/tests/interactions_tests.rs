//! Port of `parser/flow-interactions.spec.js`.
//!
//! Upstream spies on `FlowDB.setClickEvent`, `setLink`, and `setTooltip`. The database keeps
//! the observable results of those calls (the clickable class, the tooltip, the link and its
//! target); the callback name and arguments only reach a DOM handler, which is renderer work.

use super::{ast, vertex};

#[test]
fn should_be_possible_to_use_click_to_a_callback() {
    let ast = ast("graph TD\nA-->B\nclick A callback");
    assert!(vertex(&ast, "A").classes.contains(&"clickable".to_string()));
    assert_eq!(ast.edges.len(), 1);
}

#[test]
fn should_be_possible_to_use_click_to_a_click_and_call_callback() {
    let ast = ast("graph TD\nA-->B\nclick A call callback()");
    assert!(vertex(&ast, "A").classes.contains(&"clickable".to_string()));
}

#[test]
fn should_be_possible_to_use_click_to_a_callback_with_tooltip() {
    let ast = ast("graph TD\nA-->B\nclick A callback \"tooltip\"");
    assert!(vertex(&ast, "A").classes.contains(&"clickable".to_string()));
    assert_eq!(ast.tooltip("A"), Some("tooltip"));
}

#[test]
fn should_be_possible_to_use_click_to_a_click_and_call_callback_with_tooltip() {
    let ast = ast("graph TD\nA-->B\nclick A call callback() \"tooltip\"");
    assert!(vertex(&ast, "A").classes.contains(&"clickable".to_string()));
    assert_eq!(ast.tooltip("A"), Some("tooltip"));
}

#[test]
fn should_be_possible_to_use_click_to_a_callback_with_an_arbitrary_number_of_args() {
    let ast = ast("graph TD\nA-->B\nclick A call callback(\"test0\", test1, test2)");
    assert!(vertex(&ast, "A").classes.contains(&"clickable".to_string()));
}

#[test]
fn should_handle_interaction_click_to_a_link() {
    let ast = ast("graph TD\nA-->B\nclick A \"click.html\"");
    assert_eq!(vertex(&ast, "A").link.as_deref(), Some("click.html"));
    assert!(vertex(&ast, "A").classes.contains(&"clickable".to_string()));
}

#[test]
fn should_handle_interaction_click_to_a_click_and_href_link() {
    let ast = ast("graph TD\nA-->B\nclick A href \"click.html\"");
    assert_eq!(vertex(&ast, "A").link.as_deref(), Some("click.html"));
}

#[test]
fn should_handle_interaction_click_to_a_link_with_tooltip() {
    let ast = ast("graph TD\nA-->B\nclick A \"click.html\" \"tooltip\"");
    assert_eq!(vertex(&ast, "A").link.as_deref(), Some("click.html"));
    assert_eq!(ast.tooltip("A"), Some("tooltip"));
}

#[test]
fn should_handle_interaction_click_to_a_click_and_href_link_with_tooltip() {
    let ast = ast("graph TD\nA-->B\nclick A href \"click.html\" \"tooltip\"");
    assert_eq!(vertex(&ast, "A").link.as_deref(), Some("click.html"));
    assert_eq!(ast.tooltip("A"), Some("tooltip"));
}

#[test]
fn should_handle_interaction_click_to_a_link_with_target() {
    let ast = ast("graph TD\nA-->B\nclick A \"click.html\" _blank");
    assert_eq!(vertex(&ast, "A").link.as_deref(), Some("click.html"));
    assert_eq!(vertex(&ast, "A").link_target.as_deref(), Some("_blank"));
}

#[test]
fn should_handle_interaction_click_to_a_click_and_href_link_with_target() {
    let ast = ast("graph TD\nA-->B\nclick A href \"click.html\" _blank");
    assert_eq!(vertex(&ast, "A").link.as_deref(), Some("click.html"));
    assert_eq!(vertex(&ast, "A").link_target.as_deref(), Some("_blank"));
}

#[test]
fn should_handle_interaction_click_to_a_link_with_tooltip_and_target() {
    let ast = ast("graph TD\nA-->B\nclick A \"click.html\" \"tooltip\" _blank");
    assert_eq!(vertex(&ast, "A").link.as_deref(), Some("click.html"));
    assert_eq!(vertex(&ast, "A").link_target.as_deref(), Some("_blank"));
    assert_eq!(ast.tooltip("A"), Some("tooltip"));
}

#[test]
fn should_handle_interaction_click_to_a_click_and_href_link_with_tooltip_and_target() {
    let ast = ast("graph TD\nA-->B\nclick A href \"click.html\" \"tooltip\" _blank");
    assert_eq!(vertex(&ast, "A").link.as_deref(), Some("click.html"));
    assert_eq!(vertex(&ast, "A").link_target.as_deref(), Some("_blank"));
    assert_eq!(ast.tooltip("A"), Some("tooltip"));
}
