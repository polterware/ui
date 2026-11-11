//! Tests ported from the upstream xychart specs:
//! `packages/mermaid/src/diagrams/xychart/parser/xychart.jison.spec.ts` (the grammar) and
//! `packages/mermaid/src/diagrams/xychart/xychartDb.spec.ts` (the database). Inputs and
//! expectations are upstream's, with the two adaptations the mock database forces:
//!
//! * The jison spec replaces `parser.yy` with spies, so it asserts what the grammar hands the
//!   database (`setXAxisTitle({ text: '  name  ' })`, `setLineData(title, points)`) before the
//!   database trims, sanitizes, and transforms it. Here `parse` answers the database's state
//!   after those setters ran, so the assertions read the stored title and the transformed
//!   points. Where truncation or the axis defaults change an expectation, the test says so.
//! * `xychartDb.spec.ts` resets the module-level database between cases; every `parse` call
//!   here starts from `Ast::default`, which is the same state as `db.clear()`.
//!
//! Skipped, and why:
//! * The rendering assertions: `chartBuilder/components/chartTitle.spec.ts`,
//!   `legend.spec.ts`, and `chartBuilder/textDimensionCalculator.spec.ts` (they assert SVG
//!   drawables, sizes, and theme colours).
//! * `getDrawableElem`'s no-plot error and `getChartConfig`/`getChartThemeConfig`: configuration
//!   and the renderer, not parsing.

use super::Ast;
use super::ast::{Axis, Orientation, Plot, PlotKind};
use super::parse;
use crate::error::ParseError;

fn ast(body: &str) -> Ast {
    parse(body).unwrap_or_else(|error| panic!("expected {body:?} to parse, got {error}"))
}

fn error(body: &str) -> ParseError {
    match parse(body) {
        Ok(ast) => panic!("expected {body:?} to fail, parsed {ast:?}"),
        Err(error) => error,
    }
}

fn band(ast: &Ast) -> (&str, Vec<&str>) {
    match &ast.x_axis {
        Axis::Band { title, categories } => {
            (title, categories.iter().map(String::as_str).collect())
        }
        other => panic!("expected a band x axis, got {other:?}"),
    }
}

fn linear_x(ast: &Ast) -> (&str, f64, f64) {
    match &ast.x_axis {
        Axis::Linear { title, min, max } => (title, *min, *max),
        other => panic!("expected a linear x axis, got {other:?}"),
    }
}

fn linear_y(ast: &Ast) -> (&str, f64, f64) {
    match &ast.y_axis {
        Axis::Linear { title, min, max } => (title, *min, *max),
        other => panic!("expected a linear y axis, got {other:?}"),
    }
}

fn values(plot: &Plot) -> Vec<f64> {
    plot.data
        .iter()
        .map(|point| point.value.expect("every plotted point has a value"))
        .collect()
}

fn point_labels(plot: &Plot) -> Option<Vec<&str>> {
    plot.point_labels
        .as_ref()
        .map(|labels| labels.iter().map(String::as_str).collect())
}

// packages/mermaid/src/diagrams/xychart/parser/xychart.jison.spec.ts

#[test]
fn rejects_text_without_the_diagram_header() {
    assert!(parse("xychart-1").is_err());
}

#[test]
fn accepts_the_header_alone() {
    let ast = ast("xychart");
    assert_eq!(ast, Ast::default());
}

#[test]
fn parses_the_title_with_quotes() {
    let ast = ast("xychart \n title \"This is a title\"");
    assert_eq!(ast.diagram_title, "This is a title");
}

#[test]
fn parses_the_title_without_quotes() {
    let ast = ast("xychart \n title oneLinertitle");
    assert_eq!(ast.diagram_title, "oneLinertitle");
}

#[test]
fn parses_the_vertical_orientation() {
    let ast = ast("xychart vertical");
    assert_eq!(ast.orientation, Orientation::Vertical);
}

#[test]
fn parses_the_orientation_with_spaces() {
    let ast = ast("xychart        horizontal        ");
    assert_eq!(ast.orientation, Orientation::Horizontal);

    assert!(parse("xychart abc").is_err());
}

#[test]
fn parses_the_x_axis_title() {
    let ast = ast("xychart \nx-axis xAxisName\n");
    let (title, categories) = band(&ast);
    assert_eq!(title, "xAxisName");
    assert!(categories.is_empty());
}

#[test]
fn parses_the_x_axis_title_with_spaces() {
    let ast = ast("xychart \nx-axis        xAxisName     \n");
    assert_eq!(band(&ast).0, "xAxisName");
}

#[test]
fn parses_a_quoted_x_axis_title_with_spaces() {
    let ast = ast("xychart \n    x-axis \"xAxisName has space\"\n");
    assert_eq!(band(&ast).0, "xAxisName has space");
}

#[test]
fn parses_a_quoted_x_axis_title_keeping_its_inner_spaces() {
    // The grammar hands `  xAxisName has space   ` to `setXAxisTitle`; the database trims it.
    let ast = ast("xychart \n   x-axis    \"  xAxisName has space   \"         \n");
    assert_eq!(band(&ast).0, "xAxisName has space");
}

#[test]
fn parses_an_x_axis_title_with_a_range() {
    let ast = ast("xychart \nx-axis xAxisName    45.5   -->   33   \n");
    assert_eq!(linear_x(&ast), ("xAxisName", 45.5, 33.0));
}

#[test]
fn rejects_an_x_axis_range_that_does_not_start_with_a_number() {
    assert!(parse("xychart \nx-axis xAxisName    aaa   -->   33   \n").is_err());
}

#[test]
fn parses_an_x_axis_range_with_a_decimal_only_maximum() {
    let ast = ast("xychart \nx-axis xAxisName    45.5   -->   .34   \n");
    assert_eq!(linear_x(&ast), ("xAxisName", 45.5, 0.34));
}

#[test]
fn parses_an_x_axis_range_without_a_title() {
    let ast = ast("xychart \nx-axis   45.5   -->   1.34   \n");
    assert_eq!(linear_x(&ast), ("", 45.5, 1.34));
}

#[test]
fn parses_x_axis_categories() {
    let ast = ast("xychart \nx-axis xAxisName    [  \"cat1\"  ,   cat2a  ]   \n   ");
    assert_eq!(band(&ast), ("xAxisName", vec!["cat1", "cat2a"]));
}

#[test]
fn parses_x_axis_categories_without_a_title() {
    let ast = ast("xychart \nx-axis    [  \"cat1\"  ,   cat2a  ]   \n   ");
    assert_eq!(band(&ast), ("", vec!["cat1", "cat2a"]));
}

#[test]
fn rejects_unbalanced_x_axis_categories() {
    assert!(parse("xychart \nx-axis xAxisName    [  \"cat1\"  [   cat2a  ]   \n   ").is_err());
    assert!(parse("xychart \nx-axis xAxisName    [  \"cat1\"  ,   cat2a ] ]   \n   ").is_err());
}

#[test]
fn parses_the_complete_x_axis_variant_one() {
    let ast = ast("xychart \n x-axis \"this is x axis\" [category1, \"category 2\", category3]\n");
    assert_eq!(
        band(&ast),
        (
            "this is x axis",
            vec!["category1", "category 2", "category3"]
        )
    );
}

#[test]
fn parses_the_complete_x_axis_variant_two() {
    let ast =
        ast("xychart \nx-axis xAxisName    [  \"cat1  with space\"  ,   cat2 , cat3]   \n   ");
    assert_eq!(
        band(&ast),
        ("xAxisName", vec!["cat1  with space", "cat2", "cat3"])
    );
}

#[test]
fn parses_the_complete_x_axis_variant_three() {
    let ast =
        ast("xychart \nx-axis xAxisName    [  \"cat1  with space\"  ,   cat2 asdf , cat3]   \n   ");
    assert_eq!(
        band(&ast),
        ("xAxisName", vec!["cat1  with space", "cat2asdf", "cat3"])
    );
}

#[test]
fn parses_the_y_axis_title() {
    let ast = ast("xychart \ny-axis yAxisName\n");
    assert_eq!(linear_y(&ast).0, "yAxisName");
}

#[test]
fn parses_the_y_axis_title_with_spaces() {
    let ast = ast("xychart \ny-axis        yAxisName     \n");
    assert_eq!(linear_y(&ast).0, "yAxisName");
}

#[test]
fn parses_a_quoted_y_axis_title_with_spaces() {
    let ast = ast("xychart \n    y-axis \"yAxisName has space\"\n");
    assert_eq!(linear_y(&ast).0, "yAxisName has space");
}

#[test]
fn parses_a_quoted_y_axis_title_keeping_its_inner_spaces() {
    let ast = ast("xychart \n   y-axis    \"  yAxisName has space   \"         \n");
    assert_eq!(linear_y(&ast).0, "yAxisName has space");
}

#[test]
fn parses_a_y_axis_title_with_a_range() {
    let ast = ast("xychart \ny-axis yAxisName    45.5   -->   33   \n");
    assert_eq!(linear_y(&ast), ("yAxisName", 45.5, 33.0));
}

#[test]
fn parses_a_y_axis_range_without_a_title() {
    let ast = ast("xychart \ny-axis    45.5   -->   33   \n");
    assert_eq!(linear_y(&ast), ("", 45.5, 33.0));
}

#[test]
fn parses_a_y_axis_range_with_a_decimal_only_maximum() {
    let ast = ast("xychart \ny-axis yAxisName    45.5   -->   .33   \n");
    assert_eq!(linear_y(&ast), ("yAxisName", 45.5, 0.33));
}

#[test]
fn rejects_an_invalid_number_in_the_y_axis_range() {
    assert!(parse("xychart \ny-axis yAxisName    45.5   -->   abc   \n").is_err());
}

#[test]
fn rejects_y_axis_categories() {
    assert!(parse("xychart \ny-axis yAxisName    [ 45.3,   33 ]   \n").is_err());
}

#[test]
fn parses_both_axes_at_once() {
    let ast = ast("xychart\nx-axis xAxisName\ny-axis yAxisName\n");
    assert_eq!(band(&ast).0, "xAxisName");
    assert_eq!(linear_y(&ast).0, "yAxisName");
}

#[test]
fn parses_line_data() {
    let ast = ast("xychart\nx-axis xAxisName\ny-axis yAxisName\n line lineTitle [23, 45, 56.6]");
    assert_eq!(ast.plots.len(), 1);
    let plot = &ast.plots[0];
    assert_eq!(plot.kind, PlotKind::Line);
    assert_eq!(plot.title, "lineTitle");
    // The x axis had a title but no data, so the first plot makes it linear from 1 to 3.
    assert_eq!(linear_x(&ast), ("xAxisName", 1.0, 3.0));
    assert_eq!(values(plot), [23.0, 45.0, 56.6]);
    assert_eq!(linear_y(&ast), ("yAxisName", 23.0, 56.6));
}

#[test]
fn parses_line_data_with_spaces_and_signs() {
    let ast = ast(
        "xychart\nx-axis xAxisName\ny-axis yAxisName\n line \"lineTitle with space\"   [  +23 , -45  , 56.6 ]   ",
    );
    let plot = &ast.plots[0];
    assert_eq!(plot.title, "lineTitle with space");
    assert_eq!(values(plot), [23.0, -45.0, 56.6]);
}

#[test]
fn parses_line_data_without_a_title() {
    let ast =
        ast("xychart\nx-axis xAxisName\ny-axis yAxisName\n line [  +23 , -45  , 56.6 , .33]   ");
    let plot = &ast.plots[0];
    assert_eq!(plot.title, "");
    assert_eq!(values(plot), [23.0, -45.0, 56.6, 0.33]);
}

#[test]
fn rejects_unbalanced_line_data() {
    assert!(
        parse("xychart\nx-axis xAxisName\ny-axis yAxisName\n line \"lineTitle with space\"   [  +23 [ -45  , 56.6 ]   ")
            .is_err()
    );
    assert!(
        parse("xychart\nx-axis xAxisName\ny-axis yAxisName\n line \"lineTitle with space\"   [  +23 , -45  ] 56.6 ]   ")
            .is_err()
    );
}

#[test]
fn rejects_line_data_that_is_not_provided() {
    assert!(
        parse("xychart\nx-axis xAxisName\ny-axis yAxisName\n line \"lineTitle with space\"   ")
            .is_err()
    );
}

#[test]
fn rejects_empty_line_data() {
    assert!(
        parse("xychart\nx-axis xAxisName\ny-axis yAxisName\n line \"lineTitle with space\"  [ ] ")
            .is_err()
    );
}

#[test]
fn rejects_a_misplaced_comma_in_line_data() {
    assert!(
        parse("xychart\nx-axis xAxisName\ny-axis yAxisName\n line \"lineTitle with space\"   [  +23 ,  , -45  , 56.6 ]   ")
            .is_err()
    );
}

#[test]
fn rejects_a_non_number_in_line_data() {
    assert!(
        parse("xychart\nx-axis xAxisName\ny-axis yAxisName\n line \"lineTitle with space\"   [  +23 , -4aa5  , 56.6 ]   ")
            .is_err()
    );
}

#[test]
fn parses_bar_data() {
    let ast = ast("xychart\nx-axis xAxisName\ny-axis yAxisName\n bar barTitle [23, 45, 56.6, .22]");
    let plot = &ast.plots[0];
    assert_eq!(plot.kind, PlotKind::Bar);
    assert_eq!(plot.title, "barTitle");
    assert_eq!(values(plot), [23.0, 45.0, 56.6, 0.22]);
}

#[test]
fn parses_bar_data_with_spaces_and_signs() {
    let ast = ast(
        "xychart\nx-axis xAxisName\ny-axis yAxisName\n bar \"barTitle with space\"   [  +23 , -45  , 56.6 ]   ",
    );
    let plot = &ast.plots[0];
    assert_eq!(plot.title, "barTitle with space");
    assert_eq!(values(plot), [23.0, -45.0, 56.6]);
}

#[test]
fn parses_bar_data_without_a_title() {
    let ast = ast("xychart\nx-axis xAxisName\ny-axis yAxisName\n bar   [  +23 , -45  , 56.6 ]   ");
    assert_eq!(ast.plots[0].title, "");
    assert_eq!(values(&ast.plots[0]), [23.0, -45.0, 56.6]);
}

#[test]
fn rejects_unbalanced_bar_data() {
    assert!(
        parse("xychart\nx-axis xAxisName\ny-axis yAxisName\n bar \"barTitle with space\"   [  +23 [ -45  , 56.6 ]   ")
            .is_err()
    );
    assert!(
        parse("xychart\nx-axis xAxisName\ny-axis yAxisName\n bar \"barTitle with space\"   [  +23 , -45  ] 56.6 ]   ")
            .is_err()
    );
}

#[test]
fn rejects_bar_data_that_is_not_provided() {
    assert!(
        parse("xychart\nx-axis xAxisName\ny-axis yAxisName\n bar \"barTitle with space\"    ")
            .is_err()
    );
}

#[test]
fn rejects_empty_bar_data() {
    assert!(
        parse(
            "xychart\nx-axis xAxisName\ny-axis yAxisName\n bar \"barTitle with space\"   [   ]   "
        )
        .is_err()
    );
}

#[test]
fn rejects_a_misplaced_comma_in_bar_data() {
    assert!(
        parse("xychart\nx-axis xAxisName\ny-axis yAxisName\n bar \"barTitle with space\"   [  +23 , , -45  , 56.6 ]   ")
            .is_err()
    );
}

#[test]
fn rejects_a_non_number_in_bar_data() {
    assert!(
        parse("xychart\nx-axis xAxisName\ny-axis yAxisName\n bar \"barTitle with space\"   [  +23 , -4aa5  , 56.6 ]   ")
            .is_err()
    );
}

#[test]
fn parses_multiple_bars_and_lines_variant_one() {
    let ast = ast(
        "xychart\nx-axis xAxisName\ny-axis yAxisName\n bar barTitle1 [23, 45, 56.6] \n line lineTitle1 [11, 45.5, 67, 23] \n bar barTitle2 [13, 42, 56.89] \n line lineTitle2 [45, 99, 012]",
    );
    let plots: Vec<(&str, Vec<f64>)> = ast
        .plots
        .iter()
        .map(|plot| (plot.title.as_str(), values(plot)))
        .collect();
    assert_eq!(
        plots,
        [
            ("barTitle1", vec![23.0, 45.0, 56.6]),
            ("lineTitle1", vec![11.0, 45.5, 67.0, 23.0]),
            ("barTitle2", vec![13.0, 42.0, 56.89]),
            ("lineTitle2", vec![45.0, 99.0, 12.0]),
        ]
    );
    assert_eq!(ast.plots[0].kind, PlotKind::Bar);
    assert_eq!(ast.plots[3].kind, PlotKind::Line);
    // The first plot fixed the x range at 1..3 and the y range only widens.
    assert_eq!(linear_x(&ast), ("xAxisName", 1.0, 3.0));
    assert_eq!(linear_y(&ast), ("yAxisName", 11.0, 99.0));
}

#[test]
fn parses_multiple_bars_and_lines_variant_two() {
    let ast = ast("
    xychart horizontal
    title \"Basic xychart\"
    x-axis \"this is x axis\" [category1, \"category 2\", category3]
    y-axis yaxisText 10 --> 150
 bar barTitle1 [23, 45, 56.6]
 line lineTitle1 [11, 45.5, 67, 23]
 bar barTitle2 [13, 42, 56.89]
    line lineTitle2 [45, 99, 012]");
    assert_eq!(ast.orientation, Orientation::Horizontal);
    assert_eq!(ast.diagram_title, "Basic xychart");
    assert_eq!(
        band(&ast),
        (
            "this is x axis",
            vec!["category1", "category 2", "category3"]
        )
    );
    assert_eq!(linear_y(&ast), ("yaxisText", 10.0, 150.0));
    let plots: Vec<(&str, Vec<f64>)> = ast
        .plots
        .iter()
        .map(|plot| (plot.title.as_str(), values(plot)))
        .collect();
    // The database truncates each plot to the three band categories, so the line plots keep
    // the first three of their four values (the jison spec spies on the untruncated argument).
    assert_eq!(
        plots,
        [
            ("barTitle1", vec![23.0, 45.0, 56.6]),
            ("lineTitle1", vec![11.0, 45.5, 67.0]),
            ("barTitle2", vec![13.0, 42.0, 56.89]),
            ("lineTitle2", vec![45.0, 99.0, 12.0]),
        ]
    );
    let labels: Vec<&str> = ast.plots[0]
        .data
        .iter()
        .map(|point| point.label.as_str())
        .collect();
    assert_eq!(labels, ["category1", "category 2", "category3"]);
}

#[test]
fn parses_line_data_with_point_labels() {
    let ast = ast(
        "xychart\nx-axis xAxisName\ny-axis yAxisName\n line [10 \"First\", 20 \"Second\", 30 \"Third\"]",
    );
    let plot = &ast.plots[0];
    assert_eq!(values(plot), [10.0, 20.0, 30.0]);
    assert_eq!(point_labels(plot), Some(vec!["First", "Second", "Third"]));
}

#[test]
fn parses_line_data_with_mixed_labels_and_no_labels() {
    let ast =
        ast("xychart\nx-axis xAxisName\ny-axis yAxisName\n line [10 \"Start\", 20, 30 \"End\"]");
    assert_eq!(values(&ast.plots[0]), [10.0, 20.0, 30.0]);
    assert_eq!(point_labels(&ast.plots[0]), Some(vec!["Start", "", "End"]));
}

#[test]
fn parses_line_data_with_labels_containing_spaces() {
    let ast = ast(
        "xychart\nx-axis xAxisName\ny-axis yAxisName\n line [10 \"Hello World\", 20 \"Another Label\"]",
    );
    assert_eq!(
        point_labels(&ast.plots[0]),
        Some(vec!["Hello World", "Another Label"])
    );
}

#[test]
fn parses_line_data_with_a_title_and_point_labels() {
    let ast =
        ast("xychart\nx-axis xAxisName\ny-axis yAxisName\n line \"My Line\" [10 \"A\", 20 \"B\"]");
    assert_eq!(ast.plots[0].title, "My Line");
    assert_eq!(point_labels(&ast.plots[0]), Some(vec!["A", "B"]));
}

#[test]
fn parses_bar_data_with_point_labels() {
    let ast =
        ast("xychart\nx-axis xAxisName\ny-axis yAxisName\n bar [10 \"A\", 20 \"B\", 30 \"C\"]");
    let plot = &ast.plots[0];
    assert_eq!(values(plot), [10.0, 20.0, 30.0]);
    // `setBarData` never reads the labels.
    assert_eq!(plot.point_labels, None);
}

#[test]
fn parses_line_data_with_decimal_values_and_labels() {
    let ast = ast(
        "xychart\nx-axis xAxisName\ny-axis yAxisName\n line [3.8 \"Phi-3\", 7 \"Mistral\", 540 \"PaLM\"]",
    );
    assert_eq!(values(&ast.plots[0]), [3.8, 7.0, 540.0]);
    assert_eq!(
        point_labels(&ast.plots[0]),
        Some(vec!["Phi-3", "Mistral", "PaLM"])
    );
}

#[test]
fn parses_acc_title() {
    let ast = ast("xychart\naccTitle: Accessible Title\nx-axis [Q1, Q2]\nline [1, 2]");
    assert_eq!(ast.acc_title, "Accessible Title");
}

#[test]
fn parses_a_single_line_acc_descr() {
    let ast = ast("xychart\naccDescr: This is a description\nx-axis [Q1, Q2]\nline [1, 2]");
    assert_eq!(ast.acc_description, "This is a description");
}

#[test]
fn parses_a_multiline_acc_descr() {
    // The grammar hands `This is a multiline\n  description` to `setAccDescription`; the
    // database then collapses the indentation after the newline (`commonDb.ts`).
    let ast = ast("xychart
accDescr {
  This is a multiline
  description
}
x-axis [Q1, Q2]
line [1, 2]");
    assert_eq!(ast.acc_description, "This is a multiline\ndescription");
}

#[test]
fn parses_acc_title_acc_descr_and_title_together() {
    let ast = ast("xychart
accTitle: Sales Overview
accDescr: Revenue report for Q1-Q4
title \"Revenue Report\"
x-axis [Q1, Q2, Q3, Q4]
line [45, 67, 89, 55]");
    assert_eq!(ast.acc_title, "Sales Overview");
    assert_eq!(ast.acc_description, "Revenue report for Q1-Q4");
    assert_eq!(ast.diagram_title, "Revenue Report");
    // The band axis has four categories, so all four values are plotted.
    assert_eq!(values(&ast.plots[0]), [45.0, 67.0, 89.0, 55.0]);
}

// packages/mermaid/src/diagrams/xychart/xychartDb.spec.ts

#[test]
fn preserves_sanitized_line_and_bar_titles_for_legends() {
    let ast = ast("xychart
x-axis [90d, 60d]
line \" avg \" [48.1, 41.5]
bar \" p95 \" [112.2, 75.3]");
    let titles: Vec<(&str, PlotKind)> = ast
        .plots
        .iter()
        .map(|plot| (plot.title.as_str(), plot.kind))
        .collect();
    assert_eq!(titles, [("avg", PlotKind::Line), ("p95", PlotKind::Bar)]);
}

#[test]
fn truncates_bar_data_to_the_number_of_x_axis_categories() {
    let ast = ast("xychart
x-axis [cat1, cat2, cat3]
bar testBar [10, 20, 30, 40, 50]");
    let plot = &ast.plots[0];
    assert_eq!(plot.data.len(), 3);
    let data: Vec<(&str, Option<f64>)> = plot
        .data
        .iter()
        .map(|point| (point.label.as_str(), point.value))
        .collect();
    assert_eq!(
        data,
        [
            ("cat1", Some(10.0)),
            ("cat2", Some(20.0)),
            ("cat3", Some(30.0))
        ]
    );
    assert_eq!(linear_y(&ast), ("", 10.0, 30.0));
}

#[test]
fn truncates_line_data_to_the_number_of_x_axis_categories() {
    let ast = ast("xychart
x-axis [A, B]
line testLine [100, 200, 300, 400]");
    let plot = &ast.plots[0];
    assert_eq!(plot.data.len(), 2);
    let data: Vec<(&str, Option<f64>)> = plot
        .data
        .iter()
        .map(|point| (point.label.as_str(), point.value))
        .collect();
    assert_eq!(data, [("A", Some(100.0)), ("B", Some(200.0))]);
}

#[test]
fn keeps_data_that_matches_the_category_count() {
    let ast = ast("xychart
x-axis [X, Y, Z]
bar exact [5, 10, 15]");
    let plot = &ast.plots[0];
    assert_eq!(plot.data.len(), 3);
    let data: Vec<(&str, Option<f64>)> = plot
        .data
        .iter()
        .map(|point| (point.label.as_str(), point.value))
        .collect();
    assert_eq!(
        data,
        [("X", Some(5.0)), ("Y", Some(10.0)), ("Z", Some(15.0))]
    );
}

#[test]
fn keeps_a_category_without_a_value_when_the_data_is_shorter() {
    let ast = ast("xychart
x-axis [A, B, C, D]
bar short [1, 2]");
    let plot = &ast.plots[0];
    // `categories.map` produces one entry per category, with `undefined` for the missing ones.
    assert_eq!(plot.data.len(), 4);
    let data: Vec<(&str, Option<f64>)> = plot
        .data
        .iter()
        .map(|point| (point.label.as_str(), point.value))
        .collect();
    assert_eq!(
        data,
        [("A", Some(1.0)), ("B", Some(2.0)), ("C", None), ("D", None)]
    );
}

#[test]
fn computes_the_y_axis_range_only_from_visible_data() {
    let ast = ast("xychart
x-axis [Q1, Q2]
bar sales [10, 50, 999, 800]");
    assert_eq!(linear_y(&ast), ("", 10.0, 50.0));
}

// Behavior of the lexer and the database the specs above do not pin down.

#[test]
fn keywords_are_case_insensitive() {
    let ast = ast("XYCHART\nTITLE \"Sales\"\nX-AXIS [Q1]\nLINE [1]");
    assert_eq!(ast.diagram_title, "Sales");
    assert_eq!(band(&ast), ("", vec!["Q1"]));
    assert_eq!(values(&ast.plots[0]), [1.0]);
}

#[test]
fn only_the_exact_horizontal_orientation_is_horizontal() {
    // The lexer is case-insensitive, but `setOrientation` compares the token text exactly.
    assert_eq!(ast("xychart HORIZONTAL").orientation, Orientation::Vertical);
    assert_eq!(
        ast("xychart horizontal").orientation,
        Orientation::Horizontal
    );
}

#[test]
fn skips_comments_and_semicolons() {
    let ast = ast("xychart\n%% a comment\ntitle \"T\"; line [1, 2]\n");
    assert_eq!(ast.diagram_title, "T");
    assert_eq!(values(&ast.plots[0]), [1.0, 2.0]);
}

#[test]
fn accepts_the_empty_document_the_grammar_allows() {
    // `start: document` and `document: /* empty */`: the header is optional in the grammar
    // itself; the diagram detectors are what require it.
    assert_eq!(ast(""), Ast::default());
    assert_eq!(ast("title \"Only a title\"").diagram_title, "Only a title");
}

#[test]
fn reads_a_markdown_string() {
    let ast = ast("xychart\ntitle \"`marked`\"");
    assert_eq!(ast.diagram_title, "marked");
}

#[test]
fn the_orientation_must_be_on_the_header_line() {
    assert!(parse("xychart\nvertical").is_err());
}

#[test]
fn a_plot_before_the_x_axis_uses_the_default_linear_axis() {
    let ast = ast("xychart\nline [10, 20]");
    assert_eq!(linear_x(&ast), ("", 1.0, 2.0));
    let labels: Vec<&str> = ast.plots[0]
        .data
        .iter()
        .map(|point| point.label.as_str())
        .collect();
    assert_eq!(labels, ["1", "2"]);
}

#[test]
fn a_later_x_axis_does_not_relabel_an_earlier_plot() {
    let ast = ast("xychart\nline [10, 20]\nx-axis [a, b]");
    let labels: Vec<&str> = ast.plots[0]
        .data
        .iter()
        .map(|point| point.label.as_str())
        .collect();
    assert_eq!(labels, ["1", "2"]);
    assert_eq!(band(&ast), ("", vec!["a", "b"]));
}

#[test]
fn a_single_point_does_not_divide_by_zero() {
    let ast = ast("xychart\nx-axis 0 --> 10\nline [5]");
    assert_eq!(ast.plots[0].data.len(), 1);
    assert_eq!(ast.plots[0].data[0].label, "0");
    assert_eq!(ast.plots[0].data[0].value, Some(5.0));
}

#[test]
fn linear_axis_labels_are_the_number_formatted_like_javascript() {
    let chart = ast("xychart\nx-axis 0 --> 100\nline [1, 2, 3]");
    let labels: Vec<&str> = chart.plots[0]
        .data
        .iter()
        .map(|point| point.label.as_str())
        .collect();
    assert_eq!(labels, ["0", "50", "100"]);

    let chart = ast("xychart\nx-axis 1.5 --> 3\nbar [1, 2]");
    let labels: Vec<&str> = chart.plots[0]
        .data
        .iter()
        .map(|point| point.label.as_str())
        .collect();
    assert_eq!(labels, ["1.5", "3"]);
}

#[test]
fn parses_crlf_line_endings() {
    let ast = ast("xychart\r\nx-axis [a, b]\r\nline [1, 2]\r\n");
    assert_eq!(band(&ast), ("", vec!["a", "b"]));
    assert_eq!(values(&ast.plots[0]), [1.0, 2.0]);
}

#[test]
fn a_stray_character_reports_its_span() {
    let error = error("xychart\nline [1, x]");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (2, 10));
}

#[test]
fn an_unterminated_category_list_reports_the_end_of_input() {
    let error = error("xychart\nx-axis [a");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (2, 10));
}
