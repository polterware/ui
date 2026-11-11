//! Port of `parser/flow-lines.spec.js`.

use super::{ast, edge};

#[test]
fn should_handle_line_interpolation_default_definitions() {
    let ast = ast("graph TD\nA-->B\nlinkStyle default interpolate basis");
    assert_eq!(ast.default_interpolate.as_deref(), Some("basis"));
    let _ = edge(&ast, 0);
}

#[test]
fn should_handle_line_interpolation_numbered_definitions() {
    let ast = ast(
        "graph TD\nA-->B\nA-->C\nlinkStyle 0 interpolate basis\nlinkStyle 1 interpolate cardinal",
    );
    assert_eq!(edge(&ast, 0).interpolate.as_deref(), Some("basis"));
    assert_eq!(edge(&ast, 1).interpolate.as_deref(), Some("cardinal"));
}

#[test]
fn should_handle_edge_curve_properties_using_edge_id() {
    let ast = ast(
        "graph TD\nA e1@-->B\nA uniqueName@-->C\ne1@{curve: basis}\nuniqueName@{curve: cardinal}",
    );
    assert_eq!(edge(&ast, 0).interpolate.as_deref(), Some("basis"));
    assert_eq!(edge(&ast, 1).interpolate.as_deref(), Some("cardinal"));
}

#[test]
fn should_handle_edge_curve_properties_using_edge_id_without_overriding_default() {
    let ast = ast(
        "graph TD\nA e1@-->B\nA-->C\nlinkStyle default interpolate linear\ne1@{curve: stepAfter}",
    );
    assert_eq!(edge(&ast, 0).interpolate.as_deref(), Some("stepAfter"));
    assert_eq!(ast.default_interpolate.as_deref(), Some("linear"));
}

#[test]
fn should_handle_edge_curve_properties_using_edge_id_mixed_with_line_interpolation() {
    let ast = ast(
        "graph TD\nA e1@-->B-->D\nA-->C e4@-->D-->E\nlinkStyle default interpolate linear\nlinkStyle 1 interpolate basis\ne1@{curve: monotoneX}\ne4@{curve: stepBefore}",
    );
    assert_eq!(edge(&ast, 0).interpolate.as_deref(), Some("monotoneX"));
    assert_eq!(edge(&ast, 1).interpolate.as_deref(), Some("basis"));
    assert_eq!(ast.default_interpolate.as_deref(), Some("linear"));
    assert_eq!(edge(&ast, 3).interpolate.as_deref(), Some("stepBefore"));
}

#[test]
fn should_handle_line_interpolation_multi_numbered_definitions() {
    let ast = ast("graph TD\nA-->B\nA-->C\nlinkStyle 0,1 interpolate basis");
    assert_eq!(edge(&ast, 0).interpolate.as_deref(), Some("basis"));
    assert_eq!(edge(&ast, 1).interpolate.as_deref(), Some("basis"));
}

#[test]
fn should_handle_line_interpolation_default_with_style() {
    let ast = ast("graph TD\nA-->B\nlinkStyle default interpolate basis stroke-width:1px;");
    assert_eq!(ast.default_interpolate.as_deref(), Some("basis"));
}

#[test]
fn should_handle_line_interpolation_numbered_with_style() {
    let ast = ast(
        "graph TD\nA-->B\nA-->C\nlinkStyle 0 interpolate basis stroke-width:1px;\nlinkStyle 1 interpolate cardinal stroke-width:1px;",
    );
    assert_eq!(edge(&ast, 0).interpolate.as_deref(), Some("basis"));
    assert_eq!(edge(&ast, 1).interpolate.as_deref(), Some("cardinal"));
}

#[test]
fn should_handle_line_interpolation_multi_numbered_with_style() {
    let ast = ast("graph TD\nA-->B\nA-->C\nlinkStyle 0,1 interpolate basis stroke-width:1px;");
    assert_eq!(edge(&ast, 0).interpolate.as_deref(), Some("basis"));
    assert_eq!(edge(&ast, 1).interpolate.as_deref(), Some("basis"));
}

#[test]
fn should_handle_regular_lines() {
    let ast = ast("graph TD;A-->B;");
    assert_eq!(edge(&ast, 0).stroke.unwrap(), super::super::Stroke::Normal);
}

#[test]
fn should_handle_dotted_lines() {
    let ast = ast("graph TD;A-.->B;");
    assert_eq!(edge(&ast, 0).stroke.unwrap(), super::super::Stroke::Dotted);
}

#[test]
fn should_handle_thick_lines() {
    let ast = ast("graph TD;A==>B;");
    assert_eq!(edge(&ast, 0).stroke.unwrap(), super::super::Stroke::Thick);
}
