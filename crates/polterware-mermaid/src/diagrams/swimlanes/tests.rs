//! Tests ported from the upstream swimlanes specs in
//! `packages/mermaid/src/diagrams/swimlanes/`: `swimlanesDiagram.spec.ts` and
//! `lanePalette.spec.ts`. Inputs are upstream's.
//!
//! Upstream's swimlanes diagram parses with flowchart's grammar and database
//! (`swimlanesDiagram.ts` uses `createFlowDiagram`), so the parse-level assertions of the
//! lane cases are the flowchart database's; the layout half of each case is not ported.
//!
//! Skipped, and why:
//! - `swimlanesDiagram.spec.ts`'s `describe('swimlanesDiagram')`: all five cases assert
//!   `getConfig().layout` (the `swimlane` schema default, and the global, diagram-scoped, and
//!   frontmatter overrides). Configuration is not owned by this crate and nothing here lays
//!   out.
//! - `swimlanesDiagram.spec.ts`'s `describe('swimlane-beta lanes that parse but cannot be
//!   drawn as written')`: the `prepareLayoutForSwimlanes` half of each case — marking
//!   top-level subgraphs `shape: 'swimlane'`, dropping a node from a later lane, ordering
//!   lanes (`['l2', 'l1']`), and the two `log.warn` messages — lives in
//!   `rendering-util/layout-algorithms/swimlanes/helpers.ts`, which is layout. The parse-level
//!   structure each case starts from is ported below.
//! - `lanePalette.spec.ts`: every case asserts the CSS `styles.ts` emits and the theme
//!   palettes behind it; `styles.ts` is rendering.

use super::Ast;
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

fn lane_ids(ast: &Ast) -> Vec<&str> {
    ast.sub_graphs
        .iter()
        .map(|sub_graph| sub_graph.id.as_str())
        .collect()
}

fn lane_members(ast: &Ast, id: &str) -> Vec<String> {
    ast.sub_graph(id)
        .unwrap_or_else(|| panic!("lane {id:?} not found in {:?}", lane_ids(ast)))
        .nodes
        .clone()
}

fn parent(ast: &Ast, id: &str) -> Option<String> {
    ast.node(id)
        .unwrap_or_else(|| panic!("node {id:?} not found in {:?}", lane_ids(ast)))
        .parent_id
        .clone()
}

// swimlanesDiagram.spec.ts

/// The spec's `SWIMLANE` constant: the same body a flowchart parses, with the
/// `swimlane-beta` header `flow.jison` accepts as a `GRAPH` token.
#[test]
fn parses_the_spec_swimlane_example() {
    let ast = ast("swimlane-beta TD\n  A --> B");
    let ids: Vec<&str> = ast
        .vertices
        .iter()
        .map(|vertex| vertex.id.as_str())
        .collect();
    assert_eq!(ids, ["A", "B"]);
    assert_eq!(ast.edges.len(), 1);
    assert_eq!(ast.edges[0].start, "A");
    assert_eq!(ast.edges[0].end, "B");
}

#[test]
fn parses_every_header_direction() {
    // `flow.jison`'s `<dir>` rules; `FlowDB.setDirection` rewrites TD to TB.
    for (written, stored) in [
        ("TB", "TB"),
        ("TD", "TB"),
        ("BT", "BT"),
        ("RL", "RL"),
        ("LR", "LR"),
        ("BR", "BR"),
    ] {
        let body = format!("swimlane-beta {written}\n  A --> B");
        assert_eq!(
            ast(&body).direction.as_deref(),
            Some(stored),
            "direction {written}"
        );
    }
}

#[test]
fn defaults_to_top_bottom_without_a_direction() {
    // `graphConfig : GRAPH NODIR` calls `setDirection('TB')`.
    assert_eq!(
        ast("swimlane-beta\n  A --> B").direction.as_deref(),
        Some("TB")
    );
}

#[test]
fn parses_like_the_flowchart_it_reuses() {
    // `flow.jison` gives `flowchart-elk`, `swimlane-beta`, `graph`, and `flowchart` the same
    // `GRAPH` token, and `swimlanesDiagram.ts` reuses flowchart's parser and database.
    let flowchart =
        crate::diagrams::flowchart::parse("flowchart TD\n  A --> B").expect("flowchart");
    for header in ["swimlane-beta", "graph", "flowchart-elk"] {
        let body = format!("{header} TD\n  A --> B");
        assert_eq!(ast(&body), flowchart, "header {header}");
    }
}

#[test]
fn rejects_a_header_without_a_direction() {
    let error = error("swimlane-beta XX\n  A --> B");
    assert_eq!(error.span.map(|span| span.line), Some(1));
}

// describe('swimlane-beta lanes that parse but cannot be drawn as written (issue #8386)')

#[test]
fn keeps_a_node_listed_in_two_lanes_in_the_first_one() {
    let ast = ast(
        "swimlane-beta LR\nsubgraph lane1[Performer A]\n  a[\"step one\"]\nend\nsubgraph phase1[Phase 1]\n  a\nend",
    );

    assert_eq!(lane_ids(&ast), ["lane1", "phase1"]);
    assert_eq!(lane_members(&ast, "lane1"), ["a"]);
    // `makeUniq` drops a member an earlier subgraph already lists.
    assert!(lane_members(&ast, "phase1").is_empty());
    assert_eq!(parent(&ast, "a").as_deref(), Some("lane1"));
    assert_eq!(ast.node("a").expect("node a").label, "step one");
    assert_eq!(ast.node("lane1").expect("lane1").label, "Performer A");
    assert_eq!(ast.dropped_sub_graph_members.len(), 1);
    let dropped = &ast.dropped_sub_graph_members[0];
    assert_eq!(dropped.node, "a");
    assert_eq!(dropped.kept_by, "lane1");
    assert_eq!(dropped.dropped_from, "phase1");
}

#[test]
fn keeps_a_nested_subgraph_inside_its_lane() {
    let ast = ast(
        "swimlane-beta LR\nsubgraph lane1\n  subgraph inner\n    a\n  end\nend\nsubgraph lane2\n  a\nend",
    );

    // A nested subgraph is added when its `end` is reached, before its lane.
    assert_eq!(lane_ids(&ast), ["inner", "lane1", "lane2"]);
    assert_eq!(lane_members(&ast, "lane1"), ["inner"]);
    assert_eq!(lane_members(&ast, "inner"), ["a"]);
    assert!(lane_members(&ast, "lane2").is_empty());
    assert_eq!(parent(&ast, "inner").as_deref(), Some("lane1"));
    assert_eq!(parent(&ast, "a").as_deref(), Some("inner"));
    assert_eq!(ast.dropped_sub_graph_members.len(), 1);
    let dropped = &ast.dropped_sub_graph_members[0];
    assert_eq!(dropped.node, "a");
    assert_eq!(dropped.kept_by, "inner");
    assert_eq!(dropped.dropped_from, "lane2");
}

#[test]
fn keeps_a_lane_the_author_declared_empty() {
    let ast = ast("swimlane-beta LR\nsubgraph l1\n  a\nend\nsubgraph l2\nend");

    assert_eq!(lane_ids(&ast), ["l1", "l2"]);
    assert_eq!(lane_members(&ast, "l1"), ["a"]);
    assert!(lane_members(&ast, "l2").is_empty());
    let empty = ast.node("l2").expect("node l2");
    assert_eq!(empty.label, "l2");
    assert!(empty.is_group);
    // Marking a lane `swimlane` is `prepareLayoutForSwimlanes`, not the parser.
    assert_eq!(empty.shape, "rect");
    assert_eq!(parent(&ast, "a").as_deref(), Some("l1"));
}

#[test]
fn draws_a_subgraph_nested_in_a_lane_as_a_plain_box() {
    let ast = ast(
        "swimlane-beta LR\nsubgraph lane1[Performer A]\n  subgraph ph1[Phase 1]\n    a[\"step one\"]\n  end\nend",
    );

    assert_eq!(lane_ids(&ast), ["ph1", "lane1"]);
    assert_eq!(lane_members(&ast, "lane1"), ["ph1"]);
    assert_eq!(lane_members(&ast, "ph1"), ["a"]);
    assert_eq!(parent(&ast, "ph1").as_deref(), Some("lane1"));
    assert_eq!(parent(&ast, "a").as_deref(), Some("ph1"));
    let nested = ast.node("ph1").expect("node ph1");
    assert_eq!(nested.label, "Phase 1");
    assert!(nested.is_group);
    assert_eq!(nested.shape, "rect");
    assert_eq!(ast.node("a").expect("node a").label, "step one");
}
