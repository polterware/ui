//! Tests ported from the upstream pie specs: `packages/parser/tests/pie.test.ts` (the Langium
//! grammar) and `packages/mermaid/src/diagrams/pie/pie.spec.ts` (the database). Inputs and
//! expectations are upstream's.
//!
//! Skipped, and why:
//! - `expectNoErrorsOrAlternatives` checks Langium's lexer/parser error arrays and that no
//!   `console.log` ran. Here `parse` returning `Ok` is the error-free assertion, and there is
//!   no console.
//! - `result.value.$type` is Langium's generated AST type marker, not database data.
//! - `pie.spec.ts`'s `config` describe: `setConfig` and `resetConfig` are `it.todo` upstream,
//!   and `getConfig` compares with `DEFAULT_PIE_DB.config`; this crate owns no configuration.
//! - `pie.spec.ts`'s `setConfig({ securityLevel: 'strict' })` setup is configuration.
//! - `pieRenderer.spec.ts` renders; nothing here renders.

use super::Ast;
use super::parse;
use crate::error::ParseError;

fn ast(body: &str) -> Ast {
    parse(body).unwrap_or_else(|error| panic!("expected {body:?} to parse, got {error}"))
}

fn value(ast: &Ast, label: &str) -> f64 {
    ast.section(label)
        .unwrap_or_else(|| panic!("section {label:?} not found in {:?}", ast.sections))
}

fn error(body: &str) -> ParseError {
    match parse(body) {
        Ok(ast) => panic!("expected {body:?} to fail, parsed {ast:?}"),
        Err(error) => error,
    }
}

// packages/parser/tests/pie.test.ts

#[test]
fn parses_pie_with_or_without_surrounding_whitespace() {
    for body in ["pie", "  pie  ", "\tpie\t", "\n    \tpie\n    "] {
        let ast = ast(body);
        assert!(ast.sections.is_empty(), "{body:?}");
        assert!(!ast.show_data, "{body:?}");
        assert_eq!(ast.diagram_title, "");
        assert_eq!(ast.acc_title, "");
        assert_eq!(ast.acc_description, "");
    }
}

#[test]
fn parses_show_data_with_or_without_surrounding_whitespace() {
    for body in [
        "pie showData",
        "  pie  showData  ",
        "\tpie\tshowData\t",
        "\n    pie\tshowData\n    ",
    ] {
        assert!(ast(body).show_data, "{body:?}");
    }
}

#[test]
fn parses_a_title_on_the_header_line() {
    for body in [
        "pie title sample title",
        "  pie  title sample title  ",
        "\tpie\ttitle sample title\t",
        "pie\n        \ttitle sample title\n        ",
    ] {
        assert_eq!(ast(body).diagram_title, "sample title", "{body:?}");
    }
}

#[test]
fn parses_a_title_on_its_own_line() {
    for body in [
        "pie\n        title sample title",
        "pie\n        title sample title\n        ",
        "pie\n        title sample title",
        "pie\n        title sample title\n        ",
    ] {
        assert_eq!(ast(body).diagram_title, "sample title", "{body:?}");
    }
}

#[test]
fn parses_show_data_and_a_title_on_the_header_line() {
    for body in [
        "pie showData title sample title",
        "pie showData title sample title\n        ",
    ] {
        let ast = ast(body);
        assert!(ast.show_data, "{body:?}");
        assert_eq!(ast.diagram_title, "sample title", "{body:?}");
    }
}

#[test]
fn parses_show_data_and_a_title_on_separate_lines() {
    for body in [
        "pie showData\n        title sample title",
        "pie showData\n        title sample title\n        ",
        "pie showData\n        title sample title",
        "pie showData\n        title sample title\n        ",
    ] {
        let ast = ast(body);
        assert!(ast.show_data, "{body:?}");
        assert_eq!(ast.diagram_title, "sample title", "{body:?}");
    }
}

#[test]
fn parses_sections_with_any_whitespace_around_the_colon() {
    for body in [
        "pie\n        \"GitHub\":100\n        \"GitLab\":50",
        "pie\n        \"GitHub\"   :   100\n        \"GitLab\"   :   50",
        "pie\n        \"GitHub\"\t:\t100\n        \"GitLab\"\t:\t50",
        "pie\n        \t\"GitHub\" \t : \t 100\n        \t\"GitLab\" \t : \t  50\n        ",
    ] {
        let ast = ast(body);
        assert_eq!(ast.sections.len(), 2, "{body:?}");
        assert_eq!(ast.sections[0].label, "GitHub");
        assert_eq!(ast.sections[0].value, 100.0);
        assert_eq!(ast.sections[1].label, "GitLab");
        assert_eq!(ast.sections[1].value, 50.0);
    }
}

#[test]
fn parses_sections_with_show_data() {
    let ast = ast("pie showData\n        \"GitHub\": 100\n        \"GitLab\": 50");
    assert!(ast.show_data);
    assert_eq!(value(&ast, "GitHub"), 100.0);
    assert_eq!(value(&ast, "GitLab"), 50.0);
}

#[test]
fn parses_sections_with_a_title() {
    let ast = ast("pie title sample wow\n        \"GitHub\": 100\n        \"GitLab\": 50");
    assert_eq!(ast.diagram_title, "sample wow");
    assert_eq!(value(&ast, "GitHub"), 100.0);
    assert_eq!(value(&ast, "GitLab"), 50.0);
}

#[test]
fn parses_a_positive_decimal_section_value() {
    let ast = ast("pie\n        \"ash\": 60.67\n        \"bat\": 40");
    assert_eq!(value(&ast, "ash"), 60.67);
    assert_eq!(value(&ast, "bat"), 40.0);
}

#[test]
fn parses_sections_with_an_acc_title() {
    let ast = ast("pie accTitle: sample wow\n        \"GitHub\": 100\n        \"GitLab\": 50");
    assert_eq!(ast.acc_title, "sample wow");
    assert_eq!(value(&ast, "GitHub"), 100.0);
    assert_eq!(value(&ast, "GitLab"), 50.0);
}

#[test]
fn parses_sections_with_a_single_line_acc_description() {
    let ast = ast("pie accDescr: sample wow\n        \"GitHub\": 100\n        \"GitLab\": 50");
    assert_eq!(ast.acc_description, "sample wow");
    assert_eq!(value(&ast, "GitHub"), 100.0);
    assert_eq!(value(&ast, "GitLab"), 50.0);
}

#[test]
fn parses_sections_with_a_multi_line_acc_description() {
    let ast = ast(
        "pie accDescr {\n            sample wow\n        }\n        \"GitHub\": 100\n        \"GitLab\": 50",
    );
    assert_eq!(ast.acc_description, "sample wow");
    assert_eq!(value(&ast, "GitHub"), 100.0);
    assert_eq!(value(&ast, "GitLab"), 50.0);
}

// packages/mermaid/src/diagrams/pie/pie.spec.ts

#[test]
fn handles_a_very_simple_pie() {
    let ast = ast("pie\n      \"ash\": 100\n      ");
    assert_eq!(value(&ast, "ash"), 100.0);
}

#[test]
fn handles_a_simple_pie() {
    let ast = ast("pie\n      \"ash\" : 60\n      \"bat\" : 40\n      ");
    assert_eq!(value(&ast, "ash"), 60.0);
    assert_eq!(value(&ast, "bat"), 40.0);
}

#[test]
fn handles_a_simple_pie_with_show_data() {
    let ast = ast("pie showData\n      \"ash\" : 60\n      \"bat\" : 40\n      ");
    assert!(ast.show_data);
    assert_eq!(value(&ast, "ash"), 60.0);
    assert_eq!(value(&ast, "bat"), 40.0);
}

#[test]
fn handles_a_simple_pie_with_comments() {
    let ast = ast("pie\n      %% comments\n      \"ash\" : 60\n      \"bat\" : 40\n      ");
    assert_eq!(value(&ast, "ash"), 60.0);
    assert_eq!(value(&ast, "bat"), 40.0);
}

#[test]
fn handles_a_simple_pie_with_a_title() {
    let ast = ast("pie title a 60/40 pie\n      \"ash\" : 60\n      \"bat\" : 40\n      ");
    assert_eq!(ast.diagram_title, "a 60/40 pie");
    assert_eq!(value(&ast, "ash"), 60.0);
    assert_eq!(value(&ast, "bat"), 40.0);
}

#[test]
fn handles_a_simple_pie_with_an_acc_title() {
    let ast = ast(
        "pie title a neat chart\n      accTitle: a neat acc title\n      \"ash\" : 60\n      \"bat\" : 40\n      ",
    );
    assert_eq!(ast.diagram_title, "a neat chart");
    assert_eq!(ast.acc_title, "a neat acc title");
    assert_eq!(value(&ast, "ash"), 60.0);
    assert_eq!(value(&ast, "bat"), 40.0);
}

#[test]
fn handles_a_simple_pie_with_an_acc_description() {
    let ast = ast(
        "pie title a neat chart\n      accDescr: a neat description\n      \"ash\" : 60\n      \"bat\" : 40\n      ",
    );
    assert_eq!(ast.diagram_title, "a neat chart");
    assert_eq!(ast.acc_description, "a neat description");
    assert_eq!(value(&ast, "ash"), 60.0);
    assert_eq!(value(&ast, "bat"), 40.0);
}

#[test]
fn handles_a_simple_pie_with_a_multiline_acc_description() {
    let ast = ast(
        "pie title a neat chart\n      accDescr {\n        a neat description\n        on multiple lines\n      }\n      \"ash\" : 60\n      \"bat\" : 40\n    ",
    );
    assert_eq!(ast.diagram_title, "a neat chart");
    assert_eq!(ast.acc_description, "a neat description\non multiple lines");
    assert_eq!(value(&ast, "ash"), 60.0);
    assert_eq!(value(&ast, "bat"), 40.0);
}

#[test]
fn handles_a_simple_pie_with_a_positive_decimal() {
    let ast = ast("pie\n      \"ash\" : 60.67\n      \"bat\" : 40\n      ");
    assert_eq!(value(&ast, "ash"), 60.67);
    assert_eq!(value(&ast, "bat"), 40.0);
}

#[test]
fn rejects_a_negative_decimal() {
    let error = error("pie\n        \"ash\" : -60.67\n        \"bat\" : 40.12\n        ");
    assert!(error.message.contains("Negative values are not allowed"));
}

#[test]
fn handles_a_zero_slice_value() {
    let ast = ast(
        "pie title Default text position: Animal adoption\n        accTitle: simple pie char demo\n        accDescr: pie chart with 3 sections: dogs, cats, rats. Most are dogs.\n         \"dogs\" : 0\n        \"rats\" : 40.12\n      ",
    );
    assert_eq!(value(&ast, "dogs"), 0.0);
    assert_eq!(value(&ast, "rats"), 40.12);
}

#[test]
fn rejects_a_negative_slice_value() {
    let error = error(
        "pie title Default text position: Animal adoption\n        accTitle: simple pie char demo\n        accDescr: pie chart with 3 sections: dogs, cats, rats. Most are dogs.\n         \"dogs\" : -60.67\n        \"rats\" : 40.12\n    ",
    );
    assert_eq!(
        error.message,
        "\"dogs\" has invalid value: -60.67. Negative values are not allowed in pie charts. All slice values must be >= 0."
    );
}

#[test]
fn handles_unsafe_property_names() {
    let ast = ast(
        "pie title Unsafe props test\n        \"__proto__\" : 386\n        \"constructor\" : 85\n        \"prototype\" : 15",
    );
    let labels: Vec<&str> = ast
        .sections
        .iter()
        .map(|section| section.label.as_str())
        .collect();
    assert_eq!(labels, ["__proto__", "constructor", "prototype"]);
}

// Behavior of the database and lexer the specs above do not pin down.

#[test]
fn a_duplicate_label_keeps_the_first_value() {
    let ast = ast("pie\n  \"ash\": 10\n  \"ash\": 20");
    assert_eq!(ast.sections.len(), 1);
    assert_eq!(value(&ast, "ash"), 10.0);
}

#[test]
fn a_later_title_replaces_an_earlier_one() {
    let ast = ast("pie\n  title first\n  title second");
    assert_eq!(ast.diagram_title, "second");
}

#[test]
fn a_title_without_text_stays_empty() {
    let ast = ast("pie title");
    assert_eq!(ast.diagram_title, "");
}

#[test]
fn a_section_label_interprets_backslash_escapes() {
    let ast = ast("pie\n  \"a \\\"quoted\\\" name\": 1");
    assert_eq!(ast.sections[0].label, "a \"quoted\" name");
}

#[test]
fn a_negative_value_reports_the_number_token_span() {
    let error = error("pie\n\"ash\": -1");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (2, 8));
}

#[test]
fn a_stray_character_reports_its_span() {
    let error = error("pie\n  @");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (2, 3));
    assert!(error.message.contains('@'));
}

#[test]
fn show_data_must_follow_the_header_on_the_same_line() {
    assert!(parse("pie\n  showData\n  \"ash\": 1").is_err());
}

#[test]
fn parses_crlf_line_endings() {
    let ast = ast("pie\r\n  \"ash\": 1\r\n");
    assert_eq!(value(&ast, "ash"), 1.0);
}
