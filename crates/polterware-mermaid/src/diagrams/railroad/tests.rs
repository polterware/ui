//! Tests ported from the upstream railroad specs:
//! `packages/parser/tests/railroad.test.ts` (the Langium grammar),
//! `packages/mermaid/src/diagrams/railroad/parser/railroadDiagram.spec.ts` (the parser and
//! database together), `packages/mermaid/src/diagrams/railroad/railroadDb.spec.ts` (the
//! database on its own), and the `detector` describe of `railroadDetector.spec.ts`. Inputs
//! and expectations are upstream's.
//!
//! Skipped, and why:
//! - `expectNoErrorsOrAlternatives` checks Langium's lexer/parser error arrays and that no
//!   `console.log` ran. Here `parse` returning `Ok` is the error-free assertion, and there is
//!   no console.
//! - `result.value.$type` is Langium's generated AST type marker, not database data;
//!   [`kind`] names the equivalent [`Node`] variant.
//! - `railroadRenderer.spec.ts` needs an SVG DOM: every case measures elements and asserts
//!   the `viewBox`, marker positions, choice connectors, and arc orientations.
//! - `styles.spec.ts` is `getStyles` and CSS output, which is configuration and rendering.
//! - `railroadDetector.spec.ts`'s `loader` describe checks the diagram definition wiring (its
//!   id and that parser, database, and renderer are defined), not parsing.

use super::Ast;
use super::Node;
use super::Rule;
use super::db::Db;
use super::parse;
use crate::diagrams::DiagramKind;
use crate::diagrams::detect;
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

/// The variant name upstream's `$type` assertion would compare against.
fn kind(node: &Node) -> &'static str {
    match node {
        Node::Terminal { .. } => "terminal",
        Node::NonTerminal { .. } => "nonterminal",
        Node::Sequence { .. } => "sequence",
        Node::Choice { .. } => "choice",
        Node::Optional { .. } => "optional",
        Node::Repetition { .. } => "repetition",
        Node::Special { .. } => "special",
    }
}

fn terminal_value(node: &Node) -> &str {
    match node {
        Node::Terminal { value } => value,
        other => panic!("expected a terminal, got {other:?}"),
    }
}

fn nonterminal_name(node: &Node) -> &str {
    match node {
        Node::NonTerminal { name } => name,
        other => panic!("expected a nonterminal, got {other:?}"),
    }
}

fn elements(node: &Node) -> &[Node] {
    match node {
        Node::Sequence { elements } => elements,
        other => panic!("expected a sequence, got {other:?}"),
    }
}

fn alternatives(node: &Node) -> &[Node] {
    match node {
        Node::Choice { alternatives } => alternatives,
        other => panic!("expected a choice, got {other:?}"),
    }
}

fn repetition(node: &Node) -> (&Node, usize, Option<usize>) {
    match node {
        Node::Repetition {
            element, min, max, ..
        } => (element, *min, *max),
        other => panic!("expected a repetition, got {other:?}"),
    }
}

// packages/mermaid/src/diagrams/railroad/parser/railroadDiagram.spec.ts

#[test]
fn parses_terminal() {
    let input = r#"
        railroad-beta
        rule = terminal("hello") ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 1);
    assert_eq!(ast.rules[0].name, "rule");
    assert_eq!(kind(&ast.rules[0].definition), "terminal");
    assert_eq!(terminal_value(&ast.rules[0].definition), "hello");
}

#[test]
fn parses_nonterminal() {
    let input = r#"
        railroad-beta
        rule = nonterminal("expression") ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 1);
    assert_eq!(kind(&ast.rules[0].definition), "nonterminal");
    assert_eq!(nonterminal_name(&ast.rules[0].definition), "expression");
}

#[test]
fn parses_sequence() {
    let input = r#"
        railroad-beta
        rule = sequence(terminal("a"), terminal("b"), terminal("c")) ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 1);
    assert_eq!(kind(&ast.rules[0].definition), "sequence");
    assert_eq!(elements(&ast.rules[0].definition).len(), 3);
}

#[test]
fn collapses_single_element_sequence() {
    let input = r#"
        railroad-beta
        rule = sequence(terminal("a")) ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 1);
    assert_eq!(kind(&ast.rules[0].definition), "terminal");
}

#[test]
fn parses_choice() {
    let input = r#"
        railroad-beta
        rule = choice(terminal("a"), terminal("b"), terminal("c")) ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 1);
    assert_eq!(kind(&ast.rules[0].definition), "choice");
    assert_eq!(alternatives(&ast.rules[0].definition).len(), 3);
}

#[test]
fn collapses_single_element_choice() {
    let input = r#"
        railroad-beta
        rule = choice(terminal("a")) ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 1);
    assert_eq!(kind(&ast.rules[0].definition), "terminal");
}

#[test]
fn parses_optional() {
    let input = r#"
        railroad-beta
        rule = optional(terminal("a")) ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 1);
    assert_eq!(kind(&ast.rules[0].definition), "optional");
}

#[test]
fn parses_zero_or_more() {
    let input = r#"
        railroad-beta
        rule = zeroOrMore(terminal("a")) ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 1);
    assert_eq!(kind(&ast.rules[0].definition), "repetition");
    let (_, min, max) = repetition(&ast.rules[0].definition);
    assert_eq!(min, 0);
    assert_eq!(max, None);
}

#[test]
fn parses_one_or_more() {
    let input = r#"
        railroad-beta
        rule = oneOrMore(terminal("a")) ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 1);
    assert_eq!(kind(&ast.rules[0].definition), "repetition");
    let (_, min, _) = repetition(&ast.rules[0].definition);
    assert_eq!(min, 1);
}

#[test]
fn parses_special() {
    let input = r#"
        railroad-beta
        rule = special("any character") ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 1);
    assert_eq!(kind(&ast.rules[0].definition), "special");
    match &ast.rules[0].definition {
        Node::Special { text } => assert_eq!(text, "any character"),
        other => panic!("expected a special, got {other:?}"),
    }
}

#[test]
fn parses_title() {
    let input = r#"
        railroad-beta
        title "Test Grammar"
        rule = terminal("a") ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.diagram_title, "Test Grammar");
}

#[test]
fn parses_multiple_rules() {
    let input = r#"
        railroad-beta
        rule1 = terminal("a") ;
        rule2 = terminal("b") ;
        rule3 = terminal("c") ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 3);
    assert_eq!(ast.rules[0].name, "rule1");
    assert_eq!(ast.rules[1].name, "rule2");
    assert_eq!(ast.rules[2].name, "rule3");
}

#[test]
fn handles_nested_structures() {
    let input = r#"
        railroad-beta
        rule = sequence(
            choice(terminal("a"), terminal("b")),
            oneOrMore(choice(terminal("c"), terminal("d")))
        ) ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 1);
    assert_eq!(kind(&ast.rules[0].definition), "sequence");
}

#[test]
fn parses_complex_expression_grammar() {
    let input = r#"
        railroad-beta
        digit = choice(
            terminal("0"), terminal("1"), terminal("2"),
            terminal("3"), terminal("4"), terminal("5"),
            terminal("6"), terminal("7"), terminal("8"), terminal("9")
        ) ;
        number = oneOrMore(nonterminal("digit")) ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 2);
    assert_eq!(ast.rules[0].name, "digit");
    assert_eq!(ast.rules[1].name, "number");
}

#[test]
fn throws_on_missing_railroad_beta_keyword() {
    let input = r#"
        rule = terminal("a") ;
      "#;
    let error = error(input);
    assert!(error.span.is_some());
}

#[test]
fn throws_on_missing_semicolon() {
    let input = r#"
        railroad-beta
        rule = terminal("a")
      "#;
    let error = error(input);
    assert!(error.span.is_some());
}

#[test]
fn throws_on_unterminated_string() {
    let input = r#"
        railroad-beta
        rule = terminal("unclosed) ;
      "#;
    let error = error(input);
    assert!(error.span.is_some());
}

#[test]
fn handles_empty_rules_list() {
    let input = r#"
        railroad-beta
        title "Empty Grammar"
      "#;
    let ast = ast(input);
    assert!(ast.rules.is_empty());
    assert_eq!(ast.diagram_title, "Empty Grammar");
}

#[test]
fn handles_whitespace_in_strings() {
    let input = r#"
        railroad-beta
        rule = terminal("hello world") ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 1);
    assert_eq!(terminal_value(&ast.rules[0].definition), "hello world");
}

#[test]
fn handles_underscore_in_rule_names() {
    let input = r#"
        railroad-beta
        my_rule = terminal("test") ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 1);
    assert_eq!(ast.rules[0].name, "my_rule");
}

#[test]
fn handles_numbers_in_rule_names() {
    let input = r#"
        railroad-beta
        rule123 = terminal("test") ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 1);
    assert_eq!(ast.rules[0].name, "rule123");
}

#[test]
fn parses_deeply_nested_ir() {
    let input = r#"
        railroad-beta
        rule = sequence(
            choice(terminal("a"), terminal("b")),
            choice(terminal("c"), terminal("d"))
        ) ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 1);
    let definition = &ast.rules[0].definition;
    assert_eq!(kind(definition), "sequence");
    let elements = elements(definition);
    assert_eq!(elements.len(), 2);
    assert_eq!(kind(&elements[0]), "choice");
    assert_eq!(kind(&elements[1]), "choice");
}

#[test]
fn parses_zero_or_more_with_choice() {
    let input = r#"
        railroad-beta
        rule = zeroOrMore(choice(terminal("a"), terminal("b"))) ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 1);
    assert_eq!(kind(&ast.rules[0].definition), "repetition");
    let (element, _, _) = repetition(&ast.rules[0].definition);
    assert_eq!(kind(element), "choice");
}

#[test]
fn parses_block_comments() {
    let input = r#"
        railroad-beta
        /* this is a comment */
        rule = terminal("a") ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 1);
    assert_eq!(kind(&ast.rules[0].definition), "terminal");
    assert_eq!(terminal_value(&ast.rules[0].definition), "a");
}

#[test]
fn parses_an_expression_grammar() {
    let input = r#"
        railroad-beta
        title "Simple Expression"
        expression = sequence(
            nonterminal("term"),
            zeroOrMore(sequence(
                choice(terminal("+"), terminal("-")),
                nonterminal("term")
            ))
        ) ;
        term = sequence(
            nonterminal("factor"),
            zeroOrMore(sequence(
                choice(terminal("*"), terminal("/")),
                nonterminal("factor")
            ))
        ) ;
        factor = choice(
            nonterminal("number"),
            sequence(terminal("("), nonterminal("expression"), terminal(")"))
        ) ;
        number = oneOrMore(nonterminal("digit")) ;
        digit = choice(
            terminal("0"), terminal("1"), terminal("2"),
            terminal("3"), terminal("4"), terminal("5"),
            terminal("6"), terminal("7"), terminal("8"), terminal("9")
        ) ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 5);
    assert_eq!(ast.diagram_title, "Simple Expression");
    assert_eq!(ast.rules[0].name, "expression");
    assert_eq!(ast.rules[1].name, "term");
    assert_eq!(ast.rules[2].name, "factor");
    assert_eq!(ast.rules[3].name, "number");
    assert_eq!(ast.rules[4].name, "digit");
}

#[test]
fn parses_json_grammar_excerpt() {
    let input = r#"
        railroad-beta
        title "JSON Subset"
        value = choice(
            nonterminal("string"),
            nonterminal("number"),
            terminal("true"),
            terminal("false"),
            terminal("null")
        ) ;
        string = sequence(
            terminal("\""),
            zeroOrMore(nonterminal("character")),
            terminal("\"")
        ) ;
        number = oneOrMore(nonterminal("digit")) ;
        character = choice(nonterminal("letter"), nonterminal("digit")) ;
        letter = choice(terminal("a"), terminal("b"), terminal("c")) ;
        digit = choice(terminal("0"), terminal("1"), terminal("2")) ;
      "#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 6);
    assert_eq!(ast.diagram_title, "JSON Subset");
}

// packages/mermaid/src/diagrams/railroad/railroadDb.spec.ts

#[test]
fn db_stores_and_retrieves_rules() {
    let mut db = Db::new();
    let rule = Rule {
        name: "test".to_string(),
        definition: Node::Terminal {
            value: "test".to_string(),
        },
    };
    db.add_rule(rule.clone());
    assert_eq!(db.rules().len(), 1);
    assert_eq!(db.rule("test"), Some(&rule));
}

#[test]
fn db_handles_title() {
    let mut db = Db::new();
    db.set_title("Test Title");
    assert_eq!(db.title(), "Test Title");
}

#[test]
fn db_sanitizes_titles_before_storing_them() {
    let mut db = Db::new();
    db.set_title("<script>alert(1)</script>Safe Title");
    assert!(!db.title().contains("<script>"));
    assert!(db.title().contains("Safe Title"));
}

#[test]
fn db_clears_state() {
    let mut db = Db::new();
    db.set_title("Test");
    db.add_rule(Rule {
        name: "rule".to_string(),
        definition: Node::Terminal {
            value: "test".to_string(),
        },
    });
    db.clear();
    assert_eq!(db.title(), "");
    assert_eq!(db.rules().len(), 0);
}

#[test]
fn db_handles_duplicate_rules() {
    let mut db = Db::new();
    let rule1 = Rule {
        name: "test".to_string(),
        definition: Node::Terminal {
            value: "first".to_string(),
        },
    };
    let rule2 = Rule {
        name: "test".to_string(),
        definition: Node::Terminal {
            value: "second".to_string(),
        },
    };
    db.add_rule(rule1);
    db.add_rule(rule2.clone());
    assert_eq!(db.rules().len(), 2);
    assert_eq!(db.rule("test"), Some(&rule2));
}

#[test]
fn db_handles_accessibility_title() {
    let mut db = Db::new();
    db.set_acc_title("Accessible Title");
    assert_eq!(db.acc_title(), "Accessible Title");
}

#[test]
fn db_handles_accessibility_description() {
    let mut db = Db::new();
    db.set_acc_description("Accessible Description");
    assert_eq!(db.acc_description(), "Accessible Description");
}

#[test]
fn db_shares_state_between_set_title_and_set_diagram_title() {
    let mut db = Db::new();
    db.set_diagram_title("Diagram Title");
    assert_eq!(db.diagram_title(), "Diagram Title");
    assert_eq!(db.title(), "Diagram Title");
}

#[test]
fn db_returns_title_via_diagram_title() {
    let mut db = Db::new();
    db.set_title("Regular Title");
    assert_eq!(db.diagram_title(), "Regular Title");
}

#[test]
fn db_returns_none_for_a_missing_rule() {
    let db = Db::new();
    assert_eq!(db.rule("nonexistent"), None);
}

#[test]
fn db_sanitizes_rule_names_and_terminal_values() {
    let mut db = Db::new();
    db.add_rule(Rule {
        name: "rule<script>alert(1)</script>".to_string(),
        definition: Node::Terminal {
            value: "value<script>alert(1)</script>".to_string(),
        },
    });
    let stored = &db.rules()[0];
    assert!(!stored.name.contains("<script>"));
    assert!(stored.name.contains("rule"));
    assert_eq!(kind(&stored.definition), "terminal");
    let value = terminal_value(&stored.definition);
    assert!(!value.contains("<script>"));
    assert!(value.contains("value"));
}

#[test]
fn db_clears_all_state_including_accessibility_fields() {
    let mut db = Db::new();
    db.set_title("Test Title");
    db.set_acc_title("Acc Title");
    db.set_acc_description("Acc Desc");
    db.set_diagram_title("Diagram Title");
    db.add_rule(Rule {
        name: "rule".to_string(),
        definition: Node::Terminal {
            value: "test".to_string(),
        },
    });
    db.clear();
    assert_eq!(db.title(), "");
    assert_eq!(db.acc_title(), "");
    assert_eq!(db.acc_description(), "");
    assert_eq!(db.diagram_title(), "");
    assert_eq!(db.rules().len(), 0);
}

// packages/parser/tests/railroad.test.ts

#[test]
fn grammar_parses_title_and_accessibility_metadata() {
    let input = r#"railroad-beta
title Example Grammar
accTitle: Accessible Railroad
accDescr: Railroad description
rule = terminal("a") ;"#;
    let ast = ast(input);
    assert_eq!(ast.diagram_title, "Example Grammar");
    assert_eq!(ast.acc_title, "Accessible Railroad");
    assert_eq!(ast.acc_description, "Railroad description");
    assert_eq!(ast.rules.len(), 1);
}

#[test]
fn grammar_parses_ir_function_expressions() {
    let input = r#"railroad-beta
rule = sequence(terminal("a"), nonterminal("b")) ;"#;
    let ast = ast(input);
    let definition = &ast.rules[0].definition;
    assert_eq!(kind(definition), "sequence");
    let elements = elements(definition);
    assert_eq!(elements.len(), 2);
    assert_eq!(kind(&elements[0]), "terminal");
    assert_eq!(kind(&elements[1]), "nonterminal");
}

#[test]
fn grammar_parses_choice_expressions() {
    let input = r#"railroad-beta
rule = choice(terminal("a"), terminal("b"), terminal("c")) ;"#;
    let ast = ast(input);
    assert_eq!(kind(&ast.rules[0].definition), "choice");
    assert_eq!(alternatives(&ast.rules[0].definition).len(), 3);
}

#[test]
fn grammar_parses_optional_one_or_more_and_zero_or_more() {
    let input = r#"railroad-beta
r1 = optional(terminal("a")) ;
r2 = oneOrMore(terminal("b")) ;
r3 = zeroOrMore(terminal("c")) ;"#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 3);
    assert_eq!(kind(&ast.rules[0].definition), "optional");
    assert_eq!(kind(&ast.rules[1].definition), "repetition");
    assert_eq!(kind(&ast.rules[2].definition), "repetition");
    assert_eq!(repetition(&ast.rules[1].definition).1, 1);
    assert_eq!(repetition(&ast.rules[2].definition).1, 0);
}

#[test]
fn grammar_parses_special_expressions() {
    let input = r#"railroad-beta
rule = special("any character") ;"#;
    let ast = ast(input);
    assert_eq!(kind(&ast.rules[0].definition), "special");
    match &ast.rules[0].definition {
        Node::Special { text } => assert_eq!(text, "any character"),
        other => panic!("expected a special, got {other:?}"),
    }
}

#[test]
fn grammar_skips_c_style_block_comments() {
    let input = r#"railroad-beta
/* comment before the first rule */
rule = terminal("a") ;
next = nonterminal("rule") ;"#;
    let ast = ast(input);
    assert_eq!(ast.rules.len(), 2);
    assert_eq!(ast.rules[0].name, "rule");
    assert_eq!(ast.rules[1].name, "next");
}

#[test]
fn grammar_reports_malformed_input() {
    let input = r#"railroad-beta
rule = terminal("a")"#;
    assert!(parse(input).is_err());
}

// railroadDetector.spec.ts

#[test]
fn detector_matches_the_railroad_beta_keyword() {
    assert_eq!(
        detect("railroad-beta\nrule = \"test\" ;"),
        Some(DiagramKind::Railroad)
    );
}

#[test]
fn detector_matches_leading_whitespace() {
    assert_eq!(
        detect("  railroad-beta\nrule = \"test\" ;"),
        Some(DiagramKind::Railroad)
    );
}

#[test]
fn detector_matches_railroad_beta_case_insensitively() {
    assert_eq!(
        detect("RAILROAD-BETA\nrule = \"test\" ;"),
        Some(DiagramKind::Railroad)
    );
    assert_eq!(
        detect("Railroad-Beta\nrule = \"test\" ;"),
        Some(DiagramKind::Railroad)
    );
}

#[test]
fn detector_ignores_other_diagrams() {
    assert_ne!(detect("flowchart TD\nA --> B"), Some(DiagramKind::Railroad));
}

#[test]
fn detector_ignores_the_keyword_in_the_middle_of_the_text() {
    assert_eq!(detect("some text railroad-beta\nrule = \"test\" ;"), None);
}

#[test]
fn detector_ignores_an_empty_string() {
    assert_eq!(detect(""), None);
}

#[test]
fn detector_ignores_whitespace_only() {
    assert_eq!(detect("   \n  \t  "), None);
}

#[test]
fn detector_matches_after_leading_newlines() {
    assert_eq!(
        detect("\n\nrailroad-beta\nrule = \"test\" ;"),
        Some(DiagramKind::Railroad)
    );
}

// Tokenizer details the grammar tests rely on.

#[test]
fn keyword_prefix_is_read_as_an_identifier() {
    let ast = ast(r#"railroad-beta
sequenceRule = terminal("a") ;"#);
    assert_eq!(ast.rules[0].name, "sequenceRule");
}

#[test]
fn header_keyword_must_end_at_whitespace_or_a_comment() {
    assert!(
        parse(
            r#"railroad-betaX
rule = terminal("a") ;"#
        )
        .is_err()
    );
}

#[test]
fn a_line_starting_with_title_is_read_as_a_title_line() {
    let ast = ast("railroad-beta\ntitle = terminal(\"a\") ;");
    assert!(ast.rules.is_empty());
    assert_eq!(ast.diagram_title, "= terminal(\"a\") ;");
}

#[test]
fn quoted_titles_are_decoded_like_strings() {
    let ast = ast(r#"railroad-beta
title "say \"hi\"""#);
    assert_eq!(ast.diagram_title, "say \"hi\"");
}

#[test]
fn block_accessible_descriptions_lose_line_indentation() {
    let input = r#"railroad-beta
accDescr {
  line one
  line two
}
rule = terminal("a") ;"#;
    let ast = ast(input);
    assert_eq!(ast.acc_description, "line one\nline two");
}

#[test]
fn later_metadata_lines_overwrite_earlier_ones() {
    let ast = ast(r#"railroad-beta
title First
title Second
rule = terminal("a") ;"#);
    assert_eq!(ast.diagram_title, "Second");
}
