//! Tests ported from the upstream radar specs: `packages/parser/tests/radar.test.ts` (the
//! Langium grammar) and `packages/mermaid/src/diagrams/radar/radar.spec.ts` (the database).
//! Inputs and expectations are upstream's.
//!
//! Skipped, and why:
//! - `expectNoErrorsOrAlternatives` checks Langium's lexer/parser error arrays and that no
//!   `console.log` ran. Here `parse` returning `Ok` is the error-free assertion, and there is
//!   no console.
//! - `result.value.$type` is Langium's generated AST type marker, not database data.
//! - `radar.spec.ts`'s `renderer` describe: `relativeRadius` and `closedRoundCurve` are
//!   rendering geometry, and `draw` renders; nothing here renders.
//! - The `config`/theme assertions are configuration, which this crate does not own. The three
//!   documents are still parsed, through [`crate::preprocess`] the way the public entry point
//!   does, so the directive-removal path is covered.
//! - `MermaidParseError`'s message: upstream prints `Parse error on line X, column Y:` and
//!   `?` when a token has no location; [`ParseError`] carries a [`Span`](crate::error::Span)
//!   and displays `line:column: message`, so the tests assert the span's line and column.
//!
//! Two upstream curve fixtures under `should not allow curves without axes` and `should not
//! allow curves without entries` accidentally repeat the `radar-beta` header (`radar-beta` is
//! prepended to a fixture that already has it), so they pass by failing at the doubled
//! keyword. The port uses the single-header documents those tests meant to check.

use super::parser::{self, ParsedEntries, ParsedRadar};
use super::{Ast, Graticule, parse};
use crate::error::ParseError;

fn ast(body: &str) -> Ast {
    parse(body).unwrap_or_else(|error| panic!("expected {body:?} to parse, got {error}"))
}

fn document(body: &str) -> ParsedRadar {
    parser::parse_document(body)
        .unwrap_or_else(|error| panic!("expected {body:?} to parse, got {error}"))
}

fn error(body: &str) -> ParseError {
    match parse(body) {
        Ok(ast) => panic!("expected {body:?} to fail, parsed {ast:?}"),
        Err(error) => error,
    }
}

fn span_line_column(error: &ParseError) -> (u32, u32) {
    let span = error.span.expect("the error carries a span");
    (span.line, span.column)
}

/// The four spacing variants upstream's `mutateGlobalSpacing` produces.
fn spaced(context: &str) -> [String; 4] {
    [
        context.to_string(),
        format!("  {context}  "),
        format!("\t{context}\t"),
        format!("\n    \t{context}\n    "),
    ]
}

// packages/parser/tests/radar.test.ts

#[test]
fn parses_the_regular_header_in_every_form_and_surrounding_whitespace() {
    for header in ["radar-beta", "radar-beta:", "radar-beta :"] {
        for body in spaced(header) {
            let ast = ast(&body);
            assert!(ast.axes.is_empty(), "{body:?}");
            assert!(ast.curves.is_empty(), "{body:?}");
            assert_eq!(ast.options, super::Options::default(), "{body:?}");
        }
    }
}

#[test]
fn parses_a_title_on_the_header_line_or_its_own_line() {
    for context in [" title My Title", "\n  title My Title"] {
        for spacing in spaced(context) {
            let body = format!("radar-beta{spacing}");
            assert_eq!(ast(&body).diagram_title, "My Title", "{body:?}");
        }
    }
}

#[test]
fn parses_an_accessible_description_on_the_header_line_or_its_own_line() {
    for context in [
        " accDescr: My Accessible Description",
        "\n  accDescr: My Accessible Description",
    ] {
        for spacing in spaced(context) {
            let body = format!("radar-beta{spacing}");
            assert_eq!(
                ast(&body).acc_description,
                "My Accessible Description",
                "{body:?}"
            );
        }
    }
}

#[test]
fn parses_an_accessible_title_on_the_header_line_or_its_own_line() {
    for context in [
        " accTitle: My Accessible Title",
        "\n  accTitle: My Accessible Title",
    ] {
        for spacing in spaced(context) {
            let body = format!("radar-beta{spacing}");
            assert_eq!(ast(&body).acc_title, "My Accessible Title", "{body:?}");
        }
    }
}

#[test]
fn parses_a_title_and_both_accessibility_lines() {
    for context in [
        " title My Title\n  accDescr: My Accessible Description\n  accTitle: My Accessible Title",
        "\n  title My Title\n  accDescr: My Accessible Description\n  accTitle: My Accessible Title",
    ] {
        for spacing in spaced(context) {
            let body = format!("radar-beta{spacing}");
            let ast = ast(&body);
            assert_eq!(ast.diagram_title, "My Title", "{body:?}");
            assert_eq!(ast.acc_description, "My Accessible Description", "{body:?}");
            assert_eq!(ast.acc_title, "My Accessible Title", "{body:?}");
        }
    }
}

#[test]
fn parses_one_axis_with_and_without_a_label() {
    for context in ["axis my-axis", "axis my-axis[\"My Axis Label\"]"] {
        let body = format!("radar-beta\n{context}");
        let document = document(&body);
        assert_eq!(document.axes.len(), 1, "{body:?}");
        assert_eq!(document.axes[0].name, "my-axis");
    }
    let labelled = document("radar-beta\naxis my-axis[\"My Axis Label\"]");
    assert_eq!(labelled.axes[0].label.as_deref(), Some("My Axis Label"));
    let unlabelled = document("radar-beta\naxis my-axis");
    assert_eq!(unlabelled.axes[0].label, None);
}

#[test]
fn parses_multiple_axes_on_one_line_or_on_separate_lines() {
    for context in [
        "axis my-axis[\"My Axis Label\"]\n      axis my-axis2",
        "axis my-axis, my-axis2",
        "axis my-axis[\"My Axis Label\"], my-axis2",
        "axis my-axis, my-axis2[\"My Second Axis Label\"]",
    ] {
        let body = format!("radar-beta\n{context}");
        let document = document(&body);
        assert_eq!(document.axes.len(), 2, "{body:?}");
        assert_eq!(document.axes[0].name, "my-axis");
        assert_eq!(document.axes[1].name, "my-axis2");
    }
}

#[test]
fn parses_axis_labels_with_or_without_space_before_the_bracket() {
    for context in [
        "axis my-axis[\"My Axis Label\"]\n      axis my-axis2[\"My Second Axis Label\"]",
        "axis my-axis [\"My Axis Label\"], my-axis2\t[\"My Second Axis Label\"]",
    ] {
        let body = format!("radar-beta\n{context}");
        let document = document(&body);
        assert_eq!(document.axes.len(), 2, "{body:?}");
        assert_eq!(document.axes[0].name, "my-axis");
        assert_eq!(document.axes[0].label.as_deref(), Some("My Axis Label"));
        assert_eq!(document.axes[1].name, "my-axis2");
        assert_eq!(
            document.axes[1].label.as_deref(),
            Some("My Second Axis Label")
        );
    }
}

#[test]
fn rejects_an_axis_without_a_name() {
    assert!(parse("radar-beta\n      axis").is_err());
}

#[test]
fn rejects_axes_that_are_not_comma_separated() {
    assert!(parse("radar-beta\n      axis my-axis my-axis2").is_err());
}

/// Upstream's title says "without axes", but the input has no `{` either; the missing entries
/// are what the parser rejects. See the module note about the doubled header.
#[test]
fn rejects_curves_without_entries() {
    for body in [
        "radar-beta\n      curve my-curve",
        "radar-beta\n      curve my-curve[\"My Curve Label\"]",
        "radar-beta\n      axis my-axis\n      curve my-curve",
        "radar-beta\n      axis my-axis\n      curve my-curve[\"My Curve Label\"]",
    ] {
        assert!(parse(body).is_err(), "{body:?}");
    }
}

#[test]
fn parses_one_curve_with_one_entry_in_any_layout() {
    for context in [
        "curve my-curve { 1 }",
        "curve my-curve {\n        1\n      }",
        "curve my-curve {\n\n      1\n\n      }",
    ] {
        let body = format!("radar-beta\naxis my-axis\n{context}");
        let document = document(&body);
        assert_eq!(document.curves.len(), 1, "{body:?}");
        assert_eq!(document.curves[0].name, "my-curve");
        match &document.curves[0].entries {
            ParsedEntries::Numbers(values) => assert_eq!(values, &[1.0]),
            ParsedEntries::Detailed(_) => panic!("expected number entries in {body:?}"),
        }
    }
}

#[test]
fn parses_one_curve_with_one_detailed_entry() {
    for context in [
        "curve my-curve { my-axis 1 }",
        "curve my-curve { my-axis : 1 }",
        "curve my-curve {\n        my-axis: 1\n      }",
    ] {
        let body = format!("radar-beta\naxis my-axis\n{context}");
        let document = document(&body);
        assert_eq!(document.curves.len(), 1, "{body:?}");
        assert_eq!(document.curves[0].name, "my-curve");
        match &document.curves[0].entries {
            ParsedEntries::Detailed(entries) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].axis, "my-axis");
                assert_eq!(entries[0].value, 1.0);
            }
            ParsedEntries::Numbers(_) => panic!("expected detailed entries in {body:?}"),
        }
    }
}

#[test]
fn parses_one_curve_with_multiple_detailed_entries() {
    for context in [
        "curve my-curve { ax1 1, ax2 2 }",
        "curve my-curve {\n        ax1 1,\n        ax2 2\n      }",
        "curve my-curve[\"My Curve Label\"] {\n        ax1: 1, ax2: 2\n      }",
    ] {
        let body = format!("radar-beta\naxis ax1, ax1\n{context}");
        let document = document(&body);
        assert_eq!(document.curves.len(), 1, "{body:?}");
        assert_eq!(document.curves[0].name, "my-curve");
        match &document.curves[0].entries {
            ParsedEntries::Detailed(entries) => {
                assert_eq!(entries.len(), 2);
                assert_eq!(entries[0].axis, "ax1");
                assert_eq!(entries[0].value, 1.0);
                assert_eq!(entries[1].axis, "ax2");
                assert_eq!(entries[1].value, 2.0);
            }
            ParsedEntries::Numbers(_) => panic!("expected detailed entries in {body:?}"),
        }
    }
}

#[test]
fn parses_multiple_curves_on_one_line_or_on_separate_lines() {
    for context in [
        "curve c1 { ax1 1, ax2 2 }\n      curve c2 { ax1 3, ax2 4 }",
        "curve c1 {\n        ax1 1,\n        ax2 2\n      }\n      curve c2 {\n        ax1 3,\n        ax2 4\n      }",
        "curve c1{ 1, 2 }, c2{ 3, 4 }",
    ] {
        let body = format!("radar-beta\naxis ax1, ax1\n{context}");
        let document = document(&body);
        assert_eq!(document.curves.len(), 2, "{body:?}");
        assert_eq!(document.curves[0].name, "c1");
        assert_eq!(document.curves[1].name, "c2");
    }
}

#[test]
fn rejects_an_empty_curve_name() {
    assert!(parse("radar-beta\n      axis my-axis\n      curve").is_err());
}

#[test]
fn rejects_number_and_detailed_entries_in_the_same_curve() {
    assert!(parse("radar-beta\n      axis ax1, ax2\n      curve my-curve { 1, ax1 2 }").is_err());
}

#[test]
fn rejects_entries_that_are_not_comma_separated() {
    assert!(
        parse("radar-beta\n      axis ax1, ax2\n      curve my-curve { ax1 1 ax2 2 }").is_err()
    );
}

#[test]
fn parses_numeric_options() {
    for (name, value) in [("ticks", 5.0), ("min", 50.0), ("max", 50.0)] {
        let body = format!(
            "radar-beta\n      axis ax1, ax2\n      curve c1 {{ ax1 1, ax2 2 }}\n      {name} {value}"
        );
        let ast = ast(&body);
        match name {
            "ticks" => assert_eq!(ast.options.ticks, value, "{body:?}"),
            "min" => assert_eq!(ast.options.min, value, "{body:?}"),
            _ => assert_eq!(ast.options.max, Some(value), "{body:?}"),
        }
    }
}

#[test]
fn parses_string_options() {
    for (context, expected) in [
        ("graticule circle", Graticule::Circle),
        ("graticule polygon", Graticule::Polygon),
    ] {
        let body = format!(
            "radar-beta\n      axis ax1, ax2\n      curve c1 {{ ax1 1, ax2 2 }}\n      {context}"
        );
        assert_eq!(ast(&body).options.graticule, expected, "{body:?}");
    }
}

#[test]
fn parses_boolean_options() {
    for (context, expected) in [("showLegend true", true), ("showLegend false", false)] {
        let body = format!(
            "radar-beta\n      axis ax1, ax2\n      curve c1 {{ ax1 1, ax2 2 }}\n      {context}"
        );
        assert_eq!(ast(&body).options.show_legend, expected, "{body:?}");
    }
}

#[test]
fn reports_a_span_for_a_missing_axis_after_a_comma() {
    // Upstream's fixture: the comma after `ambiance` is followed by a newline.
    let invalid = "radar-beta
  title Restaurant Comparison
  axis food[\"Food Quality\"], service[\"Service\"], price[\"Price\"]
  axis ambiance[\"Ambiance\"],

  curve a[\"Restaurant A\"]{4, 3, 2, 4}";
    let error = error(invalid);
    assert_eq!(span_line_column(&error), (4, 29));
}

#[test]
fn reports_a_span_for_missing_curve_entries() {
    let error = error("radar-beta\n  axis my-axis\n  curve my-curve");
    let (line, column) = span_line_column(&error);
    assert_eq!(line, 3);
    assert!(column > 0);
}

#[test]
fn reports_a_span_for_axis_syntax_without_commas() {
    let error = error("radar-beta\n  axis my-axis my-axis2");
    assert_eq!(span_line_column(&error), (2, 16));
}

/// Upstream lexes the whole input before parsing, so this document reports a lexer error at
/// `@` (line 4, column 10) and a parser error at `invalid`. This crate stops at the first
/// token, the `ID`, and reports its span.
#[test]
fn reports_a_span_for_an_unknown_character() {
    let error = error("radar-beta\n  axis A\n  curve B{1}\n  invalid@symbol");
    assert_eq!(span_line_column(&error), (4, 3));
}

#[test]
fn reports_the_span_of_a_stray_character() {
    let error = error("radar-beta\n  axis A\n  curve B{1}\n  @");
    assert_eq!(span_line_column(&error), (4, 3));
    assert!(error.message.contains('@'));
}

/// Upstream expects `Parse error on line ?, column ?:` here; this crate's span points at the
/// end of input instead.
#[test]
fn reports_a_span_at_end_of_input() {
    let error = error("radar-beta\n  axis");
    assert_eq!(span_line_column(&error), (2, 7));
}

// packages/mermaid/src/diagrams/radar/radar.spec.ts

#[test]
fn handles_a_simple_radar_definition() {
    let ast = ast("radar-beta\n    axis A,B,C\n    curve mycurve{1,2,3}");
    assert_eq!(ast.curves.len(), 1);
    assert_eq!(ast.curves[0].name, "mycurve");
    assert_eq!(ast.curves[0].entries, [1.0, 2.0, 3.0]);
}

#[test]
fn handles_a_diagram_with_data_and_title() {
    let ast = ast("radar-beta
    title Radar diagram
    accTitle: Radar accTitle
    accDescr: Radar accDescription
    axis A[\"Axis A\"], B[\"Axis B\"] ,C[\"Axis C\"]
    curve mycurve[\"My Curve\"]{1,2,3}
    ");
    assert_eq!(ast.diagram_title, "Radar diagram");
    assert_eq!(ast.acc_title, "Radar accTitle");
    assert_eq!(ast.acc_description, "Radar accDescription");
    assert_eq!(ast.axes.len(), 3);
    assert_eq!(ast.axes[0].name, "A");
    assert_eq!(ast.axes[0].label, "Axis A");
    assert_eq!(ast.axes[1].name, "B");
    assert_eq!(ast.axes[1].label, "Axis B");
    assert_eq!(ast.axes[2].name, "C");
    assert_eq!(ast.axes[2].label, "Axis C");
    assert_eq!(ast.curves.len(), 1);
    assert_eq!(ast.curves[0].name, "mycurve");
    assert_eq!(ast.curves[0].label, "My Curve");
    assert_eq!(ast.curves[0].entries, [1.0, 2.0, 3.0]);
    assert_eq!(ast.options, super::Options::default());
}

#[test]
fn handles_a_radar_diagram_with_options() {
    let ast = ast("radar-beta
    ticks 10
    showLegend false
    graticule polygon
    min 1
    max 10
    ");
    assert_eq!(
        ast.options,
        super::Options {
            show_legend: false,
            ticks: 10.0,
            max: Some(10.0),
            min: 1.0,
            graticule: Graticule::Polygon,
        }
    );
}

#[test]
fn handles_a_curve_with_detailed_data_in_any_order() {
    let ast = ast("radar-beta
    axis A,B,C
    curve mycurve{ C: 3, A: 1, B: 2 }");
    assert_eq!(ast.curves.len(), 1);
    assert_eq!(ast.curves[0].name, "mycurve");
    assert_eq!(ast.curves[0].label, "mycurve");
    assert_eq!(ast.curves[0].entries, [1.0, 2.0, 3.0]);
}

#[test]
fn handles_a_radar_diagram_with_comments() {
    let ast = ast("radar-beta
    %% This is a comment
    axis A,B,C
    %% This is another comment
    curve mycurve{1,2,3}
    ");
    assert_eq!(ast.axes.len(), 3);
    assert_eq!(ast.curves[0].entries, [1.0, 2.0, 3.0]);
}

#[test]
fn handles_a_radar_diagram_with_a_config_override() {
    let source = crate::preprocess::preprocess(
        "
    %%{init: {'radar': {'marginTop': 80, 'axisLabelFactor': 1.25}}}%%
    radar-beta
    axis A,B,C
    curve mycurve{1,2,3}
    ",
    );
    let ast = ast(&source.body);
    assert_eq!(ast.axes.len(), 3);
}

#[test]
fn handles_a_radar_diagram_with_a_theme_override() {
    let source = crate::preprocess::preprocess(
        "
    %%{init: { \"theme\": \"base\", \"themeVariables\": {'fontSize': 80, 'cScale0': '#123456' }}}%%
    radar-beta:
    axis A,B,C
    curve mycurve{1,2,3}
    ",
    );
    let ast = ast(&source.body);
    assert_eq!(ast.axes.len(), 3);
}

#[test]
fn handles_a_radar_diagram_with_a_radar_style_override() {
    let source = crate::preprocess::preprocess(
        "
    %%{init: { \"theme\": \"base\", \"themeVariables\": {'fontSize': 10, 'radar': { 'axisColor': '#FF0000' }}}}%%
    radar-beta
    axis A,B,C
    curve mycurve{1,2,3}
    ",
    );
    let ast = ast(&source.body);
    assert_eq!(ast.axes.len(), 3);
}

#[test]
fn limits_the_number_of_ticks() {
    for (ticks, expected) in [(12, 12.0), (32, 32.0), (33, 32.0)] {
        let body = format!("radar-beta\n      ticks {ticks}\n      ");
        assert_eq!(ast(&body).options.ticks, expected, "{ticks}");
    }
}

#[test]
fn parses_the_documented_example() {
    let ast = ast("radar-beta
  axis m[\"Math\"], s[\"Science\"], e[\"English\"]
  axis h[\"History\"], g[\"Geography\"], a[\"Art\"]
  curve a[\"Alice\"]{85, 90, 80, 70, 75, 90}
  curve b[\"Bob\"]{70, 75, 85, 80, 90, 85}

  max 100
  min 0");
    assert_eq!(ast.axes.len(), 6);
    assert_eq!(ast.axes[5].name, "a");
    assert_eq!(ast.axes[5].label, "Art");
    assert_eq!(ast.curves.len(), 2);
    assert_eq!(ast.curves[1].label, "Bob");
    assert_eq!(ast.curves[1].entries, [70.0, 75.0, 85.0, 80.0, 90.0, 85.0]);
    assert_eq!(ast.options.max, Some(100.0));
}

/// `radar.spec.ts`'s "should draw a complex radar diagram", with the drawing left out: number
/// and detailed curves, labels, and a graticule option in one document.
#[test]
fn parses_a_complex_radar_diagram() {
    let ast = ast("radar-beta
        title Radar diagram
        accTitle: Radar accTitle
        accDescr: Radar accDescription
        axis A[\"Axis A\"], B[\"Axis B\"] ,C[\"Axis C\"]
        curve mycurve[\"My Curve\"]{1,2,3}
        curve mycurve2[\"My Curve 2\"]{ C: 1, A: 2, B: 3 }
        graticule polygon
        ");
    assert_eq!(ast.curves.len(), 2);
    assert_eq!(ast.curves[0].entries, [1.0, 2.0, 3.0]);
    assert_eq!(ast.curves[1].entries, [2.0, 3.0, 1.0]);
    assert_eq!(ast.options.graticule, Graticule::Polygon);
}

// Behavior of the database and lexer the specs above do not pin down.

#[test]
fn a_missing_label_falls_back_to_the_name() {
    let ast = ast("radar-beta\naxis A\ncurve c{1}");
    assert_eq!(ast.axes[0].label, "A");
    assert_eq!(ast.curves[0].label, "c");
}

#[test]
fn a_detailed_curve_reports_the_label_of_a_missing_axis() {
    let error = error("radar-beta\naxis A[\"Axis A\"], B[\"Axis B\"]\ncurve c{ A: 1 }");
    assert_eq!(error.message, "Missing entry for axis Axis B");
    assert!(error.span.is_some());
}

#[test]
fn a_detailed_curve_needs_axes() {
    let error = error("radar-beta\ncurve c{ A: 1 }");
    assert_eq!(
        error.message,
        "Axes must be populated before curves for reference entries"
    );
    assert!(error.span.is_some());
}

#[test]
fn a_detailed_curve_can_name_axes_declared_after_it() {
    let ast = ast("radar-beta\ncurve c{ B: 2, A: 1 }\naxis A, B");
    assert_eq!(ast.curves[0].entries, [1.0, 2.0]);
}

#[test]
fn number_entries_are_not_checked_against_the_axis_count() {
    let shorter = ast("radar-beta\naxis A, B, C\ncurve c{1,2}");
    assert_eq!(shorter.curves[0].entries, [1.0, 2.0]);
    let longer = ast("radar-beta\naxis A, B, C\ncurve c{1,2,3,4}");
    assert_eq!(longer.curves[0].entries, [1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn the_last_repeated_option_wins() {
    let ast = ast("radar-beta\nmax 5\nmax 7\nticks 3, ticks 4");
    assert_eq!(ast.options.max, Some(7.0));
    assert_eq!(ast.options.ticks, 4.0);
}

#[test]
fn a_later_title_replaces_an_earlier_one() {
    let ast = ast("radar-beta\n  title first\n  title second");
    assert_eq!(ast.diagram_title, "second");
}

#[test]
fn a_comment_after_a_title_line_ends_the_title() {
    let ast = ast("radar-beta\n  title Radar %% not part of the title");
    assert_eq!(ast.diagram_title, "Radar");
}

#[test]
fn a_label_interprets_backslash_escapes() {
    let ast = ast("radar-beta\naxis A[\"a \\\"quoted\\\" axis\"]\ncurve c{1}");
    assert_eq!(ast.axes[0].label, "a \"quoted\" axis");
}

#[test]
fn a_statement_may_follow_the_header_on_its_line() {
    let ast = ast("radar-beta axis A, B curve c{1,2}");
    assert_eq!(ast.axes.len(), 2);
    assert_eq!(ast.curves[0].entries, [1.0, 2.0]);
}

#[test]
fn keyword_prefixed_words_are_identifiers_when_they_are_longer() {
    let document = document("radar-beta\naxis maximum, axis2");
    assert_eq!(document.axes[0].name, "maximum");
    assert_eq!(document.axes[1].name, "axis2");
}

#[test]
fn a_graticule_word_is_not_an_identifier() {
    // `GRATICULE` precedes `ID` in the generated lexer and has no longer alternative.
    assert!(parse("radar-beta\naxis circle").is_err());
}

#[test]
fn parses_crlf_line_endings() {
    let ast = ast("radar-beta\r\n  axis A\r\n  curve c{1, 2}\r\n");
    assert_eq!(ast.axes[0].name, "A");
    assert_eq!(ast.curves[0].entries, [1.0, 2.0]);
}

#[test]
fn parses_fractional_numbers() {
    let ast = ast("radar-beta\naxis A, B\ncurve c{1.5, 60.67}\nmax 3.25");
    assert_eq!(ast.curves[0].entries, [1.5, 60.67]);
    assert_eq!(ast.options.max, Some(3.25));
}
