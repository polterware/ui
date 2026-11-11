//! Tests ported from the upstream sankey spec, `packages/mermaid/src/diagrams/sankey/parser/
//! sankey.spec.ts` (the grammar and the database it fills; upstream has no separate
//! `sankeyDB.spec.ts`). Inputs and expectations are upstream's.
//!
//! Nothing in that spec is skipped: it renders nothing, and it asserts no configuration. Its
//! `beforeEach` calls `db.clear()`, which a fresh [`Ast`] per parse replaces here, and it feeds
//! `prepareTextForParsing(cleanupComments(...))`, which [`parse`] reproduces itself (see
//! [`super::lexer`] and [`super::parser`]), so the fixtures are passed as upstream wrote them.
//! The `energy.csv` and issue #7528 fixtures are inlined because the crate cannot read upstream
//! checkouts at test time.
//!
//! After the ported cases come tests for behavior the spec does not pin down: field trimming,
//! doubled quotes, node order, comment lines, `parseFloat` values, the lexical errors, and the
//! spans the errors carry.

use super::{Ast, parse};
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

fn ids(ast: &Ast) -> Vec<&str> {
    ast.nodes.iter().map(|node| node.id.as_str()).collect()
}

fn values(ast: &Ast) -> Vec<f64> {
    ast.links.iter().map(|link| link.value).collect()
}

/// `packages/mermaid/src/diagrams/sankey/parser/energy.csv`, verbatim.
const ENERGY_CSV: &str = r#"%% There are leading and trailing spaces, do not crop
    Agricultural 'waste',Bio-conversion,124.729
%% line with a comment

%% Normal line
Bio-conversion,Liquid,0.597

%% Line with unquoted sankey keyword
sankey,target,10

%% Quoted sankey keyword
"sankey",target,10

%% Another normal line
Bio-conversion,Losses,26.862

%% Line with integer amount
Bio-conversion,Solid,280

%% Some blank lines in the middle of CSV


%% Another normal line
Bio-conversion,Gas,81.144

%% Quoted line
"Biofuel imports",Liquid,35

%% Quoted line with escaped quotes inside
"""Biomass imports""",Solid,35

%% Lines containing commas inside
%% Quoted and unquoted values should be equal in terms of graph
"District heating","Heating and cooling, commercial",22.505
District heating,"Heating and cooling, homes",46.184

%% A bunch of lines, normal CSV
Coal imports,Coal,11.606
Coal reserves,Coal,63.965
Coal,Solid,75.571
District heating,Industry,10.639
Electricity grid,Over generation / exports,104.453
Electricity grid,Heating and cooling - homes,113.726
Electricity grid,H2 conversion,27.14
Electricity grid,Industry,342.165
Electricity grid,Road transport,37.797
Electricity grid,Agriculture,4.412
Electricity grid,Heating and cooling - commercial,40.858
Electricity grid,Losses,56.691
Electricity grid,Rail transport,7.863
Electricity grid,Lighting & appliances - commercial,90.008
Electricity grid,Lighting & appliances - homes,93.494
Gas imports,Ngas,40.719
Gas reserves,Ngas,82.233
Gas,Heating and cooling - commercial,0.129
Gas,Losses,1.401
Gas,Thermal generation,151.891
Gas,Agriculture,2.096
Gas,Industry,48.58
Geothermal,Electricity grid,7.013
H2 conversion,H2,20.897
H2 conversion,Losses,6.242
H2,Road transport,20.897
Hydro,Electricity grid,6.995
Liquid,Industry,121.066
Liquid,International shipping,128.69
Liquid,Road transport,135.835
Liquid,Domestic aviation,14.458
Liquid,International aviation,206.267
Liquid,Agriculture,3.64
Liquid,National navigation,33.218
Liquid,Rail transport,4.413
Marine algae,Bio-conversion,4.375
Ngas,Gas,122.952
Nuclear,Thermal generation,839.978
Oil imports,Oil,504.287
Oil reserves,Oil,107.703
Oil,Liquid,611.99
Other waste,Solid,56.587
Other waste,Bio-conversion,77.81
Pumped heat,Heating and cooling - homes,193.026
Pumped heat,Heating and cooling - commercial,70.672
Solar PV,Electricity grid,59.901
Solar Thermal,Heating and cooling - homes,19.263
Solar,Solar Thermal,19.263
Solar,Solar PV,59.901
Solid,Agriculture,0.882
Solid,Thermal generation,400.12
Solid,Industry,46.477
Thermal generation,Electricity grid,525.531
Thermal generation,Losses,787.129
Thermal generation,District heating,79.329
Tidal,Electricity grid,9.452
UK land based bioenergy,Bio-conversion,182.01
"""Wave""",Electricity grid,19.013
"""Wind""",Electricity grid,289.366

%% lines at the end, do not remove
"#;

/// The issue #7528 fixture from the spec, indentation and all.
const ISSUE_7528: &str = "
      Agricultural 'waste',Bio-conversion,124.729
      Bio-conversion,Liquid,0.597
      Bio-conversion,Losses,26.862
      Bio-conversion,Solid,280.322
      Bio-conversion,Gas,81.144
      Biofuel imports,Liquid,35
      Biomass imports,Solid,35
      Coal imports,Coal,11.606
      Coal reserves,Coal,63.965
      Coal,Solid,75.571
      District heating,Industry,10.639
      District heating,Heating and cooling - homes,22.505
      District heating,Heating and cooling - commercial,46.184
      Electricity grid,Over generation / exports,104.453
      Electricity grid,Lighting & appliances - commercial,113.726
      Electricity grid,Lighting & appliances - homes,27.14
      ";

// packages/mermaid/src/diagrams/sankey/parser/sankey.spec.ts

#[test]
fn parses_csv_with_sankey_beta_syntax() {
    let ast = ast(&format!("sankey-beta\n\n {ENERGY_CSV}"));
    assert_eq!(ast.nodes.len(), 52);
    assert_eq!(ast.links.len(), 70);
    assert_eq!(ids(&ast)[0], "Agricultural 'waste'");
    // The unquoted and the quoted `sankey` lines name the same node.
    assert!(ast.node("sankey").is_some());
    assert!(ast.node("target").is_some());
    // A quoted field inside quotes keeps its quotes; doubled quotes collapse.
    assert!(ast.node("\"Biomass imports\"").is_some());
    assert!(ast.node("\"Wave\"").is_some());
    assert!(ast.node("Heating and cooling, commercial").is_some());
    // Two identical records are two links, not one.
    let ten: Vec<f64> = ast
        .links
        .iter()
        .filter(|link| link.source == "sankey" && link.target == "target")
        .map(|link| link.value)
        .collect();
    assert_eq!(ten, [10.0, 10.0]);
}

#[test]
fn parses_csv_with_sankey_syntax() {
    let ast = ast(&format!("sankey\n\n {ENERGY_CSV}"));
    assert_eq!(ast.nodes.len(), 52);
    assert_eq!(ast.links.len(), 70);
    assert_eq!(ids(&ast)[0], "Agricultural 'waste'");
}

#[test]
fn allows_dunder_proto_as_id_with_sankey_beta_syntax() {
    let ast = ast("sankey-beta
      __proto__,A,0.597
      A,__proto__,0.403
      ");
    assert_eq!(ids(&ast), ["__proto__", "A"]);
    assert_eq!(ast.links.len(), 2);
    assert_eq!(values(&ast), [0.597, 0.403]);
}

#[test]
fn allows_dunder_proto_as_id_with_sankey_syntax() {
    let ast = ast("sankey
      __proto__,A,0.597
      A,__proto__,0.403
      ");
    assert_eq!(ids(&ast), ["__proto__", "A"]);
    assert_eq!(ast.links.len(), 2);
}

#[test]
fn parses_the_issue_7528_sample_with_both_syntaxes() {
    let special = [
        "Agricultural 'waste'",
        "Heating and cooling - homes",
        "Heating and cooling - commercial",
        "Over generation / exports",
        "Lighting & appliances - commercial",
        "Lighting & appliances - homes",
    ];
    for syntax in ["sankey-beta", "sankey"] {
        let ast = ast(&format!("{syntax}{ISSUE_7528}"));
        assert_eq!(ast.nodes.len(), 19, "syntax: {syntax}");
        assert_eq!(ast.links.len(), 16, "syntax: {syntax}");
        for id in special {
            assert!(ast.node(id).is_some(), "syntax: {syntax}, id: {id:?}");
        }
    }
}

// Behavior of the grammar, the database, and the lexer the spec does not pin down.

#[test]
fn a_quoted_field_can_hold_commas() {
    let ast = ast("sankey\nPumped heat,\"Heating and cooling, homes\",193.026");
    assert_eq!(ids(&ast), ["Pumped heat", "Heating and cooling, homes"]);
    assert_eq!(values(&ast), [193.026]);
}

#[test]
fn doubled_quotes_inside_a_quoted_field_collapse() {
    let first = ast("sankey\n\"a\"\"b\",c,1");
    assert_eq!(ids(&first), ["a\"b", "c"]);
    let second = ast("sankey\n\"\"\"a\"\"\",c,1");
    assert_eq!(ids(&second), ["\"a\"", "c"]);
}
#[test]
fn nodes_keep_the_order_they_were_first_seen_in() {
    let ast = ast("sankey\nB,A,1\nA,C,2\nB,C,3");
    assert_eq!(ids(&ast), ["B", "A", "C"]);
    assert_eq!(ast.links.len(), 3);
    assert_eq!(ast.links[2].source, "B");
    assert_eq!(ast.links[2].target, "C");
    assert_eq!(ast.links[2].value, 3.0);
}

#[test]
fn fields_are_trimmed() {
    let ast = ast("sankey\n  A  ,  B  ,  3  ");
    assert_eq!(ids(&ast), ["A", "B"]);
    assert_eq!(values(&ast), [3.0]);
}

#[test]
fn empty_lines_between_records_are_allowed() {
    let ast = ast("sankey\n\nA,B,1\n\n\nC,D,2\n\n");
    assert_eq!(ast.links.len(), 2);
    assert_eq!(ids(&ast), ["A", "B", "C", "D"]);
}

#[test]
fn comment_lines_are_removed() {
    let body = "%% a leading comment\nsankey\nA,B,1\n  %% between records\nC,D,2\n%% last";
    let ast = ast(body);
    assert_eq!(ast.links.len(), 2);
    assert_eq!(ids(&ast), ["A", "B", "C", "D"]);
}

#[test]
fn a_comment_line_inside_a_quoted_field_is_removed() {
    let ast = ast("sankey\nA,\"b\n%% removed\nc\",1");
    assert_eq!(ids(&ast), ["A", "b\nc"]);
}

#[test]
fn a_newline_run_inside_a_quoted_field_collapses_to_one() {
    let ast = ast("sankey\nA,\"b\n\n\nc\",1");
    assert_eq!(ids(&ast), ["A", "b\nc"]);
}

#[test]
fn the_header_is_case_insensitive() {
    for header in [
        "sankey",
        "SANKEY",
        "Sankey",
        "sankey-beta",
        "SANKEY-BETA",
        "Sankey-Beta",
    ] {
        let ast = ast(&format!("{header}\nA,B,1"));
        assert_eq!(ast.links.len(), 1, "header: {header}");
    }
}

#[test]
fn crlf_line_endings_are_accepted() {
    let ast = ast("sankey\r\nA,B,1\r\nC,D,2\r\n");
    assert_eq!(ast.links.len(), 2);
}

#[test]
fn a_trailing_newline_is_ignored() {
    assert_eq!(ast("sankey\nA,B,1\n").links.len(), 1);
    assert_eq!(ast("sankey\nA,B,1\n   \n").links.len(), 1);
}

#[test]
fn leading_blank_lines_and_whitespace_before_the_header_are_ignored() {
    let ast = ast("\n  \n\tsankey\nA,B,1");
    assert_eq!(ast.links.len(), 1);
}

#[test]
fn a_non_numeric_value_is_nan() {
    assert_eq!(ast("sankey\nA,B,\"1,2\"").links[0].value, 1.0);
    assert!(ast("sankey\nA,B,abc").links[0].value.is_nan());
    let cases = [("0x10", 0.0), ("1e3", 1000.0), ("1e", 1.0), (".5", 0.5)];
    for (value, expected) in cases {
        let parsed = ast(&format!("sankey\nA,B,{value}"));
        assert_eq!(parsed.links[0].value, expected, "value: {value}");
    }
}

#[test]
fn a_script_element_is_removed_from_a_node_id() {
    let ast = ast("sankey\n<script>alert(1)</script>,B,1");
    assert_eq!(ids(&ast), ["", "B"]);
}

#[test]
fn a_stray_less_than_in_a_node_id_is_escaped() {
    let ast = ast("sankey\na<b,B,1");
    assert_eq!(ids(&ast), ["a&lt;b", "B"]);
}

#[test]
fn a_missing_header_is_an_error() {
    for body in ["", "graph\nA,B,1", "\n\n"] {
        let error = error(body);
        assert!(
            error.message.contains("sankey"),
            "body: {body:?}, message: {}",
            error.message
        );
    }
    let error = error("graph\nA,B,1");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (1, 1));
}

#[test]
fn the_header_needs_a_line_break_and_a_record() {
    for body in ["sankey", "sankey\n", "sankey\n\n\n", "   sankey   "] {
        assert!(parse(body).is_err(), "body: {body:?}");
    }
}

#[test]
fn a_record_needs_exactly_three_fields() {
    for body in [
        "sankey\nA,B,1,2",
        "sankey\nA,B",
        "sankey\nA",
        "sankey\nA,B,1,",
    ] {
        assert!(parse(body).is_err(), "body: {body:?}");
    }
}

#[test]
fn an_empty_field_is_an_error() {
    for body in [
        "sankey\n,B,1",
        "sankey\nA,,1",
        "sankey\nA,B,",
        "sankey\n\"\",B,1",
    ] {
        assert!(parse(body).is_err(), "body: {body:?}");
    }
}

#[test]
fn a_whitespace_only_line_between_records_is_not_an_empty_line() {
    // `prepareTextForParsing` strips only the ends of the whole text, so a line of spaces is
    // still a record line upstream, and the grammar rejects it.
    assert!(parse("sankey\n   \nA,B,1").is_err());
    assert!(parse("sankey   \nA,B,1").is_err());
}

#[test]
fn an_unterminated_quoted_field_reports_where_it_opened() {
    let error = error("sankey\n\"a");
    assert_eq!(error.message, "unterminated quoted field");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (2, 1));
}

#[test]
fn a_character_outside_text_data_is_a_lexical_error() {
    let tab = error("sankey\nA\tB,C,1");
    assert!(tab.message.contains('\t'));
    let span = tab.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (2, 2));

    let unicode = error("sankey\ncafé,b,1");
    assert!(unicode.message.contains('é'));
    let span = unicode.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (2, 4));
}

#[test]
fn an_extra_field_reports_its_comma() {
    let error = error("sankey\nA,B,1,2");
    assert_eq!(
        error.message,
        "expected a newline or end of input, got 'COMMA'"
    );
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (2, 6));
}

#[test]
fn a_short_record_reports_the_line_break() {
    let error = error("sankey\nA,B\nC,D,2");
    assert_eq!(error.message, "expected COMMA, got 'NEWLINE'");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (2, 4));
}

#[test]
fn parses_through_the_crate_entry_point() {
    let text =
        "---\ntitle: energy\nconfig:\n  sankey:\n    showValues: true\n---\nsankey-beta\n\nA,B,1\n";
    match crate::parse(text).expect("the crate detects and parses a sankey body") {
        crate::Diagram::Sankey(ast) => {
            assert_eq!(ast.links.len(), 1);
            assert_eq!(ast.links[0].source, "A");
            assert_eq!(ast.links[0].target, "B");
        }
        other => panic!("expected a sankey diagram, got {other:?}"),
    }
}
