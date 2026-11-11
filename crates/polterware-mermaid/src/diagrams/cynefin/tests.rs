//! Tests ported from the upstream cynefin specs:
//! `packages/mermaid/src/diagrams/cynefin/cynefin.spec.ts` (the database and the boundary
//! geometry) and `cynefin.integration.spec.ts` (the grammar through the database), plus
//! grammar edge cases checked against the generated Langium parser.
//!
//! Skipped, and why:
//! - `cynefin.spec.ts`'s `Cynefin Boundaries` describe (`seededRandom`, `hashString`,
//!   `generateFoldPath`, `generateHorizontalBoundary`, `generateCliffPath`,
//!   `generateConfusionPath`): deterministic SVG path geometry for `cynefinRenderer.ts`;
//!   nothing here renders.
//! - `resolveSeed` (`cynefin.spec.ts`): render seed resolution, reading configuration and
//!   hashing the diagram id; configuration and rendering are out of scope.
//! - `should return config` (`cynefin.spec.ts`): `getConfig`; this crate owns no
//!   configuration.
//! - `should handle null/undefined blocks`: Rust has no `null`, and [`parse`] always hands the
//!   database arrays; the empty case is covered by `handles_empty_input_without_blocks`.
//! - `db.clear()` in every `beforeEach`: [`parse`] returns a fresh [`Ast`], so there is no
//!   module-level state to clear. `keeps_fresh_state_across_parses` covers it.

use super::parse;
use super::{Ast, DomainName};
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

fn labels(ast: &Ast, name: DomainName) -> Vec<&str> {
    ast.domain(name)
        .map(|domain| {
            domain
                .items
                .iter()
                .map(|item| item.label.as_str())
                .collect()
        })
        .unwrap_or_default()
}

fn domain_names(ast: &Ast) -> Vec<&str> {
    ast.domains
        .iter()
        .map(|domain| domain.name.as_str())
        .collect()
}

// cynefin.integration.spec.ts

#[test]
fn parses_a_basic_diagram_with_domains() {
    let ast = ast(
        "cynefin-beta\n  complex\n    \"Emergent practice\"\n    \"Probe-sense-respond\"\n  clear\n    \"Best practice\"\n",
    );
    assert_eq!(ast.domains.len(), 2);
    assert_eq!(
        labels(&ast, DomainName::Complex),
        ["Emergent practice", "Probe-sense-respond"]
    );
    assert_eq!(labels(&ast, DomainName::Clear), ["Best practice"]);
}

#[test]
fn parses_all_five_domains() {
    let ast = ast(
        "cynefin-beta\n  complex\n    \"A\"\n  complicated\n    \"B\"\n  clear\n    \"C\"\n  chaotic\n    \"D\"\n  confusion\n    \"E\"\n",
    );
    assert_eq!(ast.domains.len(), 5);
    for (name, label) in [
        (DomainName::Complex, "A"),
        (DomainName::Complicated, "B"),
        (DomainName::Clear, "C"),
        (DomainName::Chaotic, "D"),
        (DomainName::Confusion, "E"),
    ] {
        assert_eq!(labels(&ast, name), [label], "domain: {}", name.as_str());
    }
}

#[test]
fn parses_empty_domains() {
    let ast = ast("cynefin-beta\n  complex\n");
    assert!(ast.domain(DomainName::Complex).is_some());
    assert!(labels(&ast, DomainName::Complex).is_empty());
}

#[test]
fn parses_a_domain_with_multiple_items() {
    let ast = ast(
        "cynefin-beta\n  complex\n    \"First item\"\n    \"Second item\"\n    \"Third item\"\n",
    );
    assert_eq!(
        labels(&ast, DomainName::Complex),
        ["First item", "Second item", "Third item"]
    );
}

#[test]
fn parses_comments() {
    let ast = ast(
        "cynefin-beta\n  %% This is a comment\n  complex\n    \"Item\"\n  %% Another comment\n",
    );
    assert_eq!(ast.domains.len(), 1);
    assert_eq!(labels(&ast, DomainName::Complex), ["Item"]);
}

#[test]
fn parses_a_transition_between_domains() {
    let ast = ast(
        "cynefin-beta\n  complex\n    \"Practice\"\n  complicated\n    \"Analysis\"\n  complex --> complicated : \"Pattern found\"\n",
    );
    assert_eq!(ast.transitions.len(), 1);
    let transition = &ast.transitions[0];
    assert_eq!(transition.from, DomainName::Complex);
    assert_eq!(transition.to, DomainName::Complicated);
    assert_eq!(transition.label.as_deref(), Some("Pattern found"));
}

#[test]
fn parses_a_transition_without_a_label() {
    let ast = ast(
        "cynefin-beta\n  complex\n    \"A\"\n  complicated\n    \"B\"\n  complex --> complicated\n",
    );
    assert_eq!(ast.transitions.len(), 1);
    assert_eq!(ast.transitions[0].from, DomainName::Complex);
    assert_eq!(ast.transitions[0].to, DomainName::Complicated);
    assert_eq!(ast.transitions[0].label, None);
}

#[test]
fn parses_multiple_transitions() {
    let ast = ast(
        "cynefin-beta\n  complex\n    \"A\"\n  complicated\n    \"B\"\n  clear\n    \"C\"\n  complex --> complicated : \"Analyzed\"\n  complicated --> clear : \"Codified\"\n  chaotic --> complex : \"Stabilized\"\n",
    );
    assert_eq!(ast.transitions.len(), 3);
    assert_eq!(ast.transitions[0].from, DomainName::Complex);
    assert_eq!(ast.transitions[1].from, DomainName::Complicated);
    assert_eq!(ast.transitions[2].from, DomainName::Chaotic);
}

#[test]
fn parses_acc_title() {
    let ast =
        ast("cynefin-beta\n  accTitle: Cynefin Framework Overview\n  complex\n    \"Emergent\"\n");
    assert_eq!(ast.acc_title, "Cynefin Framework Overview");
}

#[test]
fn parses_acc_descr() {
    let ast = ast(
        "cynefin-beta\n  accDescr: A diagram showing the five Cynefin domains\n  complex\n    \"Emergent\"\n",
    );
    assert_eq!(
        ast.acc_description,
        "A diagram showing the five Cynefin domains"
    );
}

#[test]
fn parses_a_title() {
    let ast = ast("cynefin-beta\n  title My Cynefin Map\n  complex\n    \"Emergent\"\n");
    assert_eq!(ast.diagram_title, "My Cynefin Map");
}

#[test]
fn parses_a_full_diagram_with_all_features() {
    let ast = ast(
        "cynefin-beta\n  title Team Practices\n  accTitle: Cynefin for team practices\n  complex\n    \"Retrospectives\"\n    \"Pair programming\"\n  complicated\n    \"Code review\"\n    \"Architecture decisions\"\n  clear\n    \"Deployment checklist\"\n  chaotic\n    \"Incident response\"\n  confusion\n    \"New initiative\"\n  complex --> complicated : \"Pattern emerges\"\n  complicated --> clear : \"Best practice found\"\n  chaotic --> complex : \"Stabilized\"\n  confusion --> chaotic : \"Crisis detected\"\n",
    );
    assert_eq!(ast.diagram_title, "Team Practices");
    assert_eq!(ast.acc_title, "Cynefin for team practices");

    assert_eq!(ast.domains.len(), 5);
    assert_eq!(labels(&ast, DomainName::Complex).len(), 2);
    assert_eq!(labels(&ast, DomainName::Complicated).len(), 2);
    assert_eq!(labels(&ast, DomainName::Clear).len(), 1);
    assert_eq!(labels(&ast, DomainName::Chaotic).len(), 1);
    assert_eq!(labels(&ast, DomainName::Confusion).len(), 1);

    assert_eq!(ast.transitions.len(), 4);
    assert_eq!(ast.transitions[0].from, DomainName::Complex);
    assert_eq!(ast.transitions[0].to, DomainName::Complicated);
    assert_eq!(ast.transitions[0].label.as_deref(), Some("Pattern emerges"));
    assert_eq!(ast.transitions[3].from, DomainName::Confusion);
    assert_eq!(ast.transitions[3].to, DomainName::Chaotic);
    assert_eq!(ast.transitions[3].label.as_deref(), Some("Crisis detected"));
}

#[test]
fn drops_self_loop_transitions_and_keeps_valid_ones() {
    let ast = ast(
        "cynefin-beta\n  complex\n    \"A\"\n  complicated\n    \"B\"\n  complex --> complex : \"Self-reflection\"\n  complex --> complicated : \"Pattern found\"\n  complicated --> complicated\n",
    );
    assert_eq!(ast.transitions.len(), 1);
    assert_eq!(ast.transitions[0].from, DomainName::Complex);
    assert_eq!(ast.transitions[0].to, DomainName::Complicated);
    assert_eq!(ast.transitions[0].label.as_deref(), Some("Pattern found"));
}

#[test]
fn produces_zero_transitions_when_all_are_self_loops() {
    let ast = ast("cynefin-beta\n  complex\n    \"A\"\n  complex --> complex\n");
    assert!(ast.transitions.is_empty());
}

#[test]
fn keeps_fresh_state_across_parses() {
    let first = ast(
        "cynefin-beta\n  title First Diagram\n  complex\n    \"Alpha\"\n    \"Beta\"\n  complicated\n    \"Gamma\"\n  complex --> complicated : \"Move\"\n",
    );
    assert_eq!(first.domains.len(), 2);
    assert_eq!(first.transitions.len(), 1);
    assert_eq!(first.diagram_title, "First Diagram");

    let second = ast("cynefin-beta\n  title Second Diagram\n  chaotic\n    \"Delta\"\n");
    assert_eq!(second.domains.len(), 1);
    assert!(second.domain(DomainName::Chaotic).is_some());
    assert!(second.domain(DomainName::Complex).is_none());
    assert!(second.transitions.is_empty());
    assert_eq!(second.diagram_title, "Second Diagram");
}

// cynefin.spec.ts

#[test]
fn starts_empty() {
    let ast = ast("cynefin-beta");
    assert!(ast.domains.is_empty());
    assert!(ast.transitions.is_empty());
    assert_eq!(ast.diagram_title, "");
    assert_eq!(ast.acc_title, "");
    assert_eq!(ast.acc_description, "");
}

#[test]
fn sets_domains_from_ast_blocks() {
    let ast = ast("cynefin-beta\n  complex\n    \"Test\"\n  clear\n");
    assert_eq!(ast.domains.len(), 2);
    assert_eq!(labels(&ast, DomainName::Complex).len(), 1);
    assert!(labels(&ast, DomainName::Clear).is_empty());
}

#[test]
fn sets_transitions() {
    let ast = ast("cynefin-beta\n  complex --> complicated : \"Pattern found\"\n");
    assert_eq!(ast.transitions.len(), 1);
    assert_eq!(ast.transitions[0].from, DomainName::Complex);
    assert_eq!(ast.transitions[0].to, DomainName::Complicated);
    assert_eq!(ast.transitions[0].label.as_deref(), Some("Pattern found"));
}

#[test]
fn sets_transitions_without_labels() {
    let ast = ast("cynefin-beta\n  chaotic --> complex\n");
    assert_eq!(ast.transitions[0].label, None);
}

#[test]
fn filters_self_loop_transitions() {
    let ast = ast(
        "cynefin-beta\n  complex --> complex\n  complex --> complicated : \"Pattern found\"\n  clear --> clear\n",
    );
    assert_eq!(ast.transitions.len(), 1);
    assert_eq!(ast.transitions[0].from, DomainName::Complex);
    assert_eq!(ast.transitions[0].to, DomainName::Complicated);
}

#[test]
fn handles_a_list_of_only_self_loops_gracefully() {
    let ast = ast("cynefin-beta\n  complex --> complex\n  chaotic --> chaotic\n");
    assert!(ast.transitions.is_empty());
}

#[test]
fn handles_empty_input_without_blocks() {
    let ast = ast("cynefin-beta\n");
    assert!(ast.domains.is_empty());
    assert!(ast.transitions.is_empty());
}

#[test]
fn clears_all_data() {
    let populated = ast("cynefin-beta\n  complex\n    \"A\"\n  complex --> chaotic\n");
    assert_eq!(populated.domains.len(), 1);
    assert_eq!(populated.transitions.len(), 1);

    let cleared = ast("cynefin-beta");
    assert!(cleared.domains.is_empty());
    assert!(cleared.transitions.is_empty());
}

#[test]
fn handles_multiple_domains() {
    let ast = ast("cynefin-beta\n  complex\n  complicated\n  clear\n  chaotic\n  confusion\n");
    assert_eq!(ast.domains.len(), 5);
    assert_eq!(
        domain_names(&ast),
        ["complex", "complicated", "clear", "chaotic", "confusion"]
    );
}

#[test]
fn handles_a_domain_with_multiple_items() {
    let ast = ast("cynefin-beta\n  complex\n    \"A\"\n    \"B\"\n    \"C\"\n    \"D\"\n");
    assert_eq!(labels(&ast, DomainName::Complex).len(), 4);
}

// Grammar edge cases, checked against the generated Langium parser.

#[test]
fn parses_the_header_with_or_without_a_colon() {
    for body in [
        "cynefin-beta",
        "cynefin-beta:",
        "  cynefin-beta  ",
        "\n\tcynefin-beta\n\n",
        "cynefin-beta\n",
    ] {
        let parsed = ast(body);
        assert!(parsed.domains.is_empty(), "{body:?}");
        assert!(parsed.transitions.is_empty(), "{body:?}");
    }

    assert_eq!(
        labels(&ast("cynefin-beta complex \"A\""), DomainName::Complex),
        ["A"]
    );
    assert_eq!(
        labels(
            &ast("cynefin-beta: complex\n    \"A\""),
            DomainName::Complex
        ),
        ["A"]
    );
    assert_eq!(ast("cynefin-beta: title T\n  complex").diagram_title, "T");
}

#[test]
fn items_may_share_the_domain_line() {
    let ast = ast("cynefin-beta\n  complex \"A\" \"B\"\n");
    assert_eq!(labels(&ast, DomainName::Complex), ["A", "B"]);
}

#[test]
fn titles_and_accessibility_lines_may_follow_domains() {
    let ast = ast(
        "cynefin-beta\n  complex\n    \"A\"\n  title Later Title\n  accTitle: later title\n  accDescr: later description\n",
    );
    assert_eq!(ast.diagram_title, "Later Title");
    assert_eq!(ast.acc_title, "later title");
    assert_eq!(ast.acc_description, "later description");
}

#[test]
fn repeated_domains_keep_the_first_position_and_the_last_items() {
    let ast = ast("cynefin-beta\n  complex\n    \"A\"\n  clear\n    \"B\"\n  complex\n    \"C\"\n");
    assert_eq!(domain_names(&ast), ["complex", "clear"]);
    assert_eq!(labels(&ast, DomainName::Complex), ["C"]);
}

#[test]
fn converts_title_and_accessibility_values() {
    let ast = ast(
        "cynefin-beta\n  title  A   B \n  accTitle:   At  Title  \n  accDescr:   De  sc  \n  complex\n",
    );
    assert_eq!(ast.diagram_title, "A B");
    assert_eq!(ast.acc_title, "At Title");
    assert_eq!(ast.acc_description, "De sc");
}

#[test]
fn cuts_title_and_accessibility_values_at_a_comment() {
    let ast = ast(
        "cynefin-beta\n  title My Map %% note\n  accTitle: at %% note\n  accDescr: ad %% note\n  complex\n",
    );
    assert_eq!(ast.diagram_title, "My Map");
    assert_eq!(ast.acc_title, "at");
    assert_eq!(ast.acc_description, "ad");
}

#[test]
fn normalizes_a_multiline_acc_descr() {
    let ast = ast("cynefin-beta\n  accDescr {\n    line  one\n\n    line two  \n  }\n  complex\n");
    assert_eq!(ast.acc_description, "line one\nline two");
}

#[test]
fn a_later_empty_title_or_accessibility_line_clears_the_value() {
    let ast = ast("cynefin-beta\n  title One\n  title\n  accTitle:\n  accDescr:\n  complex\n");
    assert_eq!(ast.diagram_title, "");
    assert_eq!(ast.acc_title, "");
    assert_eq!(ast.acc_description, "");
}

#[test]
fn parses_crlf_and_blank_lines() {
    let ast = ast("\n\ncynefin-beta\r\n\r\n  complex\r\n\r\n    \"A\"\r\n\r\n");
    assert_eq!(labels(&ast, DomainName::Complex), ["A"]);
}

#[test]
fn parses_single_quoted_and_escaped_labels_without_collapsing_them() {
    let ast = ast(
        "cynefin-beta\n  complex\n    'Single quoted'\n    \"a \\\"quote\\\"\"\n    \"a  b\"\n",
    );
    assert_eq!(
        labels(&ast, DomainName::Complex),
        ["Single quoted", "a \"quote\"", "a  b"]
    );
}

#[test]
fn keeps_an_empty_label() {
    let ast = ast("cynefin-beta\n  complex\n    \"\"\n  complex --> chaotic : \"\"\n");
    assert_eq!(labels(&ast, DomainName::Complex), [""]);
    assert_eq!(ast.transitions[0].label, None);
}

#[test]
fn parses_an_arrow_without_surrounding_spaces() {
    let ast = ast("cynefin-beta\n  complex-->complicated\n");
    assert_eq!(ast.transitions.len(), 1);
    assert_eq!(ast.transitions[0].to, DomainName::Complicated);
}

#[test]
fn requires_the_header_keyword_to_be_delimited() {
    error("cynefin-betax\n  complex\n");
    error("cynefin-beta;\n  complex\n");
}

#[test]
fn rejects_an_unknown_domain() {
    error("cynefin-beta\n  foo\n    \"A\"\n");
    error("cynefin-beta\n  COMPLEX\n    \"A\"\n");
}

#[test]
fn rejects_a_prefix_of_a_domain_name() {
    // `complex` is a `DOMAIN_NAME` prefix of `complexity`; Chevrotain's longest match keeps
    // `complex` and leaves `ity`, which the generated lexer reports as junk.
    error("cynefin-beta\n  complexity\n    \"A\"\n");
}

#[test]
fn rejects_junk_after_an_item() {
    error("cynefin-beta\n  complex\n    \"A\" junk\n");
}

#[test]
fn rejects_a_malformed_transition() {
    error("cynefin-beta\n  complex -->\n");
    error("cynefin-beta\n  complex --> complicated :\n");
    error("cynefin-beta\n  --> complicated\n");
    // The arrow must follow its `from` domain on the same line; otherwise the first line is a
    // `DomainBlock` and the arrow starts no alternative.
    error("cynefin-beta\n  complex\n  --> complicated\n");
}

#[test]
fn accepts_a_transition_at_the_end_of_input() {
    let ast = ast("cynefin-beta\n  complex --> complicated");
    assert_eq!(ast.transitions.len(), 1);
    assert_eq!(ast.transitions[0].to, DomainName::Complicated);
}

#[test]
fn rejects_items_after_a_transition() {
    error("cynefin-beta\n  complex --> complicated\n    \"A\"\n");
}

#[test]
fn rejects_a_missing_header() {
    error("");
    error("complex\n");
    error("%% only a comment\n");
}

#[test]
fn errors_carry_the_position_of_the_unexpected_token() {
    let junk = error("cynefin-beta\n  complex\n    junk\n");
    let span = junk.span.expect("a span");
    assert_eq!((span.line, span.column), (3, 5));

    let missing_target = error("cynefin-beta\n  complex -->\n");
    assert!(missing_target.span.is_some());
}

#[test]
fn sanitizes_the_title_like_the_common_database() {
    let ast = ast("cynefin-beta\n  title <script>alert(1)</script>A<br/>B\n  complex\n");
    assert_eq!(ast.diagram_title, "A<br>B");
}

#[test]
fn accepts_comments_before_and_between_lines() {
    let ast = ast("%% leading\ncynefin-beta\n  %% inside\n  complex\n    \"A\" %% trailing\n");
    assert_eq!(labels(&ast, DomainName::Complex), ["A"]);
}
