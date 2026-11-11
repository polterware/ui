//! Tests ported from the upstream wardley specs: `wardleyParser.spec.ts` (the parser and
//! database) and `wardleyBuilder.spec.ts` (`resolveNodeId`). Inputs and expectations are
//! upstream's; value comparisons that upstream makes with `toBeCloseTo` use a tolerance here.
//!
//! Skipped, and why:
//! - `wardleyBuilder.spec.ts`'s "prefers exact id over label match when both exist" cannot
//!   be expressed through the grammar: a component's id is its label, and the only nodes
//!   whose id and label differ are pipeline components, which are inserted after every
//!   component. `resolve_node_id_uses_the_exact_id` is the closest document that reaches the
//!   exact-id branch.
//! - `wardleyRenderer.ts` draws the map; nothing here lays out or renders.
//! - `wardleyDiagram.ts` and `styles.ts` are diagram wiring and CSS.
//! - `wardleyDetector.ts` is the detector already ported in `crate::diagrams::detect`.
//!
//! The extra tests at the end pin behavior the specs leave implicit: the database errors,
//! the merge of duplicate nodes, the lexer's keyword shadowing, and line-ending handling.

use super::{Accelerator, Ast, Flow, Node, SourceStrategy, parse};
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

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}

fn node<'a>(ast: &'a Ast, label: &str) -> &'a Node {
    ast.nodes
        .iter()
        .find(|node| node.label == label)
        .unwrap_or_else(|| panic!("node {label:?} not found in {:?}", ast.nodes))
}

fn stage_names(ast: &Ast) -> Vec<&str> {
    ast.axes
        .stages
        .as_ref()
        .map(|stages| stages.iter().map(String::as_str).collect())
        .unwrap_or_default()
}

// packages/mermaid/src/diagrams/wardley/wardleyParser.spec.ts

#[test]
fn parses_components_and_links() {
    let ast = ast(
        "wardley-beta\ntitle Example\ncomponent Alpha [0.2, 0.1]\ncomponent Beta [0.4, 0.3]\nAlpha -> Beta",
    );
    assert_eq!(ast.nodes.len(), 2);
    assert_eq!(ast.links.len(), 1);
    assert_eq!(ast.diagram_title, "Example");
}

#[test]
fn parses_custom_evolution_stages() {
    let ast = ast(
        "wardley-beta\ntitle Test\nevolution Genesis -> Custom -> Product -> Commodity\ncomponent A [0.5, 0.5]",
    );
    assert_eq!(
        stage_names(&ast),
        ["Genesis", "Custom", "Product", "Commodity"]
    );
}

#[test]
fn parses_dual_label_evolution_stages_with_slashes() {
    let ast = ast(
        "wardley-beta\ntitle Test\nevolution Genesis / Concept -> Custom / Emerging -> Product / Converging -> Commodity / Accepted\ncomponent A [0.5, 0.5]",
    );
    assert_eq!(
        stage_names(&ast),
        [
            "Genesis / Concept",
            "Custom / Emerging",
            "Product / Converging",
            "Commodity / Accepted"
        ]
    );
}

#[test]
fn parses_pipeline_blocks_with_single_coordinate_components() {
    let ast = ast(
        "wardley-beta\ntitle Test Pipeline\ncomponent Kettle [0.45, 0.57]\npipeline Kettle {\n  component Campfire Kettle [0.35] label [-60, 35]\n  component Electric Kettle [0.53]\n}",
    );
    let kettle = node(&ast, "Kettle");
    let campfire = node(&ast, "Campfire Kettle");
    let electric = node(&ast, "Electric Kettle");
    assert_eq!(campfire.y, kettle.y);
    assert_eq!(electric.y, kettle.y);
    assert_eq!(campfire.x, 35.0);
    assert_eq!(electric.x, 53.0);
    assert_eq!(ast.pipelines.len(), 1);
    assert_eq!(ast.pipelines[0].node_id, "Kettle");
    assert_eq!(
        ast.pipelines[0].component_ids,
        ["Kettle_Campfire Kettle", "Kettle_Electric Kettle"]
    );
}

#[test]
fn parses_custom_stage_widths_with_boundary_notation() {
    let ast = ast(
        "wardley-beta\ntitle Test Custom Widths\nevolution Genesis@0.3 -> Custom@0.6 -> Product@0.85 -> Commodity@1.0\ncomponent A [0.5, 0.5]",
    );
    assert_eq!(
        stage_names(&ast),
        ["Genesis", "Custom", "Product", "Commodity"]
    );
    assert_eq!(ast.axes.stage_boundaries, Some(vec![0.3, 0.6, 0.85, 1.0]));
}

#[test]
fn parses_notes_with_quoted_text() {
    let ast = ast(
        "wardley-beta\ntitle Test Notes\ncomponent API [0.6, 0.7]\nnote \"Critical decision point\" [0.65, 0.55]",
    );
    assert_eq!(ast.notes.len(), 1);
    assert_eq!(ast.notes[0].text, "Critical decision point");
}

#[test]
fn parses_annotations_with_quoted_text() {
    let ast = ast(
        "wardley-beta\ntitle Test Annotations\ncomponent API [0.6, 0.7]\nannotations [0.1, 0.9]\nannotation 1,[0.6, 0.65] \"Critical component\"\nannotation 2,[0.5, 0.5] \"Performance layer\"",
    );
    assert_eq!(ast.annotations.len(), 2);
    assert_eq!(
        ast.annotations[0].text.as_deref(),
        Some("Critical component")
    );
    assert_eq!(
        ast.annotations[1].text.as_deref(),
        Some("Performance layer")
    );
}

#[test]
fn parses_anchors() {
    let ast = ast(
        "wardley-beta\ntitle Test Anchors\nanchor Business [0.95, 0.63]\nanchor Public [0.95, 0.78]\ncomponent Tea [0.63, 0.81]",
    );
    let anchors: Vec<&Node> = ast
        .nodes
        .iter()
        .filter(|node| node.class_name.as_deref() == Some("anchor"))
        .collect();
    assert_eq!(anchors.len(), 2);
    assert_eq!(anchors[0].label, "Business");
}

#[test]
fn parses_evolve_statements() {
    let ast =
        ast("wardley-beta\ntitle Test Evolve\ncomponent Kettle [0.35, 0.43]\nevolve Kettle 0.62");
    assert_eq!(ast.trends.len(), 1);
    close(ast.trends[0].target_x, 62.0);
    close(ast.trends[0].target_y, 35.0);
}

#[test]
fn parses_component_decorators() {
    let ast = ast(
        "wardley-beta\ntitle Test Decorators\ncomponent API [0.6, 0.7] (build)\ncomponent Database [0.4, 0.5] (buy)\ncomponent Cache [0.5, 0.6] (outsource)",
    );
    assert_eq!(
        node(&ast, "API").source_strategy,
        Some(SourceStrategy::Build)
    );
    assert_eq!(
        node(&ast, "Database").source_strategy,
        Some(SourceStrategy::Buy)
    );
    assert_eq!(
        node(&ast, "Cache").source_strategy,
        Some(SourceStrategy::Outsource)
    );
}

#[test]
fn parses_size_directive() {
    let ast = ast("wardley-beta\ntitle Test Size\nsize [1200, 900]\ncomponent A [0.5, 0.5]");
    let size = ast.size.expect("the size directive is stored");
    assert_eq!(size.width, 1200.0);
    assert_eq!(size.height, 900.0);
}

#[test]
fn handles_quoted_identifiers_inline_labels_and_converts_coordinates_to_percentages() {
    let ast = ast(
        "wardley-beta\ntitle Coordinate Handling\ncomponent \"Mobile App\" [0.2, 0.4] (build) (inertia)\ncomponent API [0.3, 0.5]\n\"Mobile App\" +<> API; constraint\nnote \"Check dependencies\" [0.25, 0.45]\nannotations [0.10, 0.90]\nannotation 1,[0.60, 0.65] \"Critical component\"\naccelerator \"Cloud Native\" [0.20, 0.85]\ndeaccelerator \"Legacy Data\" [0.40, 0.35]",
    );
    let mobile = node(&ast, "Mobile App");
    close(mobile.x, 40.0);
    close(mobile.y, 20.0);
    assert_eq!(mobile.source_strategy, Some(SourceStrategy::Build));
    assert!(mobile.inertia);

    assert_eq!(ast.links.len(), 1);
    assert_eq!(ast.links[0].flow, Some(Flow::Bidirectional));
    assert_eq!(ast.links[0].label.as_deref(), Some("constraint"));

    close(ast.notes[0].x, 45.0);
    close(ast.notes[0].y, 25.0);

    let annotations_box = ast.annotations_box.expect("annotations box");
    close(annotations_box.x, 90.0);
    close(annotations_box.y, 10.0);

    assert_eq!(
        ast.annotations[0].text.as_deref(),
        Some("Critical component")
    );
    close(ast.annotations[0].coordinates[0].x, 65.0);
    close(ast.annotations[0].coordinates[0].y, 60.0);

    assert_eq!(ast.accelerators.len(), 1);
    assert_eq!(ast.accelerators[0].name, "Cloud Native");
    close(ast.accelerators[0].x, 85.0);
    close(ast.accelerators[0].y, 20.0);
    close(ast.deaccelerators[0].x, 35.0);
    close(ast.deaccelerators[0].y, 40.0);
}

#[test]
fn accepts_hyphens_in_unquoted_component_names() {
    let ast = ast(
        "wardley-beta\ncomponent real-time processing [0.5, 0.5]\ncomponent end-user [0.8, 0.9]",
    );
    let realtime = node(&ast, "real-time processing");
    close(realtime.x, 50.0);
    close(realtime.y, 50.0);
    let enduser = node(&ast, "end-user");
    close(enduser.x, 90.0);
    close(enduser.y, 80.0);
}

#[test]
fn accepts_hyphens_in_link_endpoints() {
    let ast = ast(
        "wardley-beta\ncomponent real-time processing [0.5, 0.5]\ncomponent end-user [0.8, 0.9]\nreal-time processing -> end-user",
    );
    assert_eq!(ast.links.len(), 1);
    assert_eq!(ast.links[0].source, "real-time processing");
    assert_eq!(ast.links[0].target, "end-user");
}

#[test]
fn accepts_hyphens_in_anchor_names() {
    let ast = ast("wardley-beta\nanchor on-call engineer [0.9, 0.95]");
    let anchor = node(&ast, "on-call engineer");
    assert_eq!(anchor.class_name.as_deref(), Some("anchor"));
    close(anchor.x, 95.0);
    close(anchor.y, 90.0);
}

#[test]
fn still_tokenises_a_to_b_as_an_arrow() {
    let ast = ast("wardley-beta\ncomponent A [0.1, 0.1]\ncomponent B [0.2, 0.2]\nA->B");
    assert_eq!(ast.links.len(), 1);
    assert_eq!(ast.links[0].source, "A");
    assert_eq!(ast.links[0].target, "B");
}

#[test]
fn parses_foo_bar_arrow_baz_as_a_link_from_foo_bar_to_baz() {
    let ast =
        ast("wardley-beta\ncomponent foo-bar [0.3, 0.3]\ncomponent baz [0.6, 0.6]\nfoo-bar->baz");
    assert_eq!(ast.links.len(), 1);
    assert_eq!(ast.links[0].source, "foo-bar");
    assert_eq!(ast.links[0].target, "baz");
}

#[test]
fn accepts_hyphens_in_pipeline_component_names_with_label_overrides() {
    let ast = ast(
        "wardley-beta\ncomponent Data Store [0.5, 0.5]\npipeline Data Store {\n  component real-time queue [0.3] label [-40, 20]\n  component batch-loader [0.7]\n}",
    );
    let realtime = node(&ast, "real-time queue");
    let batch = node(&ast, "batch-loader");
    assert_eq!(realtime.class_name.as_deref(), Some("pipeline-component"));
    assert_eq!(realtime.label_offset_x, Some(-40.0));
    assert_eq!(realtime.label_offset_y, Some(20.0));
    close(realtime.x, 30.0);
    assert_eq!(batch.class_name.as_deref(), Some("pipeline-component"));
    close(batch.x, 70.0);
}

#[test]
fn accepts_consecutive_hyphens_inside_a_name() {
    let ast = ast("wardley-beta\ncomponent foo--bar [0.3, 0.4]");
    let node = node(&ast, "foo--bar");
    close(node.x, 40.0);
    close(node.y, 30.0);
}

#[test]
fn accepts_a_trailing_hyphen_in_a_name() {
    let ast = ast("wardley-beta\ncomponent foo- [0.2, 0.3]");
    let node = node(&ast, "foo-");
    close(node.x, 30.0);
    close(node.y, 20.0);
}

// packages/mermaid/src/diagrams/wardley/wardleyBuilder.spec.ts

#[test]
fn resolve_node_id_returns_the_id_when_a_node_exists_with_that_exact_id() {
    let ast = ast("wardley-beta\ncomponent API [0.6, 0.7]\nAPI -> API");
    assert_eq!(ast.links[0].source, "API");
    assert_eq!(ast.links[0].target, "API");
}

#[test]
fn resolve_node_id_falls_back_to_label_match_for_pipeline_style_synthetic_ids() {
    let ast = ast(
        "wardley-beta\ncomponent Kettle [0.45, 0.57]\npipeline Kettle {\n  component Campfire Kettle [0.35]\n}\nCampfire Kettle -> Kettle",
    );
    assert_eq!(ast.links[0].source, "Kettle_Campfire Kettle");
    assert_eq!(ast.links[0].target, "Kettle");
}

#[test]
fn resolve_node_id_returns_the_input_unchanged_when_no_id_or_label_matches() {
    let ast = ast("wardley-beta\ncomponent API [0.6, 0.7]\nUnknown -> API");
    assert_eq!(ast.links[0].source, "Unknown");
}

#[test]
fn resolve_node_id_uses_the_exact_id() {
    let ast = ast(
        "wardley-beta\ncomponent Kettle [0.45, 0.57]\npipeline Kettle {\n  component Kettle [0.35]\n}\nKettle -> Kettle",
    );
    assert_eq!(ast.links[0].source, "Kettle");
    assert_eq!(ast.links[0].target, "Kettle");
    assert!(ast.node("Kettle_Kettle").is_some());
}

// Behavior the specs leave implicit.

#[test]
fn reports_the_visibility_range_with_the_upstream_message() {
    let error = error("wardley-beta\ncomponent A [150.0, 0.5]");
    assert_eq!(
        error.message,
        "Component \"A\" visibility must be between 0-1 (decimal) or 0-100 (percentage). Received: 150"
    );
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (2, 14));
}

#[test]
fn reports_the_evolution_range() {
    let error = error("wardley-beta\ncomponent A [0.5, 101.0]");
    assert_eq!(
        error.message,
        "Component \"A\" evolution must be between 0-1 (decimal) or 0-100 (percentage). Received: 101"
    );
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (2, 19));
}

#[test]
fn a_coordinate_cannot_carry_a_sign() {
    let error = error("wardley-beta\nanchor A [-0.5, 0.5]");
    assert_eq!(error.message, "expected WARDLEY_NUMBER, got '-'");
}

#[test]
fn reports_a_pipeline_without_a_parent() {
    let error = error("wardley-beta\npipeline Nope {\n  component x [0.2]\n}");
    assert_eq!(
        error.message,
        "Pipeline \"Nope\" must reference an existing component with coordinates."
    );
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (2, 10));
}

#[test]
fn reports_an_out_of_range_annotation_with_its_number() {
    let error = error("wardley-beta\nannotation 1,[150.0, 0] \"x\"");
    assert_eq!(
        error.message,
        "Annotation 1 visibility must be between 0-1 (decimal) or 0-100 (percentage). Received: 150"
    );
}

#[test]
fn reports_an_out_of_range_pipeline_component() {
    let error =
        error("wardley-beta\ncomponent A [0.5, 0.5]\npipeline A {\n  component x [101.0]\n}");
    assert_eq!(
        error.message,
        "Pipeline component \"x\" evolution must be between 0-1 (decimal) or 0-100 (percentage). Received: 101"
    );
}

#[test]
fn a_later_size_replaces_an_earlier_one() {
    let ast = ast("wardley-beta\nsize [10, 20]\nsize [30, 40]\ncomponent A [0.5, 0.5]");
    let size = ast.size.expect("the size directive is stored");
    assert_eq!((size.width, size.height), (30.0, 40.0));
}

#[test]
fn a_later_evolution_replaces_an_earlier_one() {
    let ast = ast("wardley-beta\nevolution A -> B\nevolution C -> D\ncomponent X [0.5, 0.5]");
    assert_eq!(stage_names(&ast), ["C", "D"]);
    assert_eq!(ast.axes.stage_boundaries, Some(Vec::new()));
}

#[test]
fn only_stages_with_a_boundary_contribute_one() {
    let ast = ast("wardley-beta\nevolution A -> B@0.4 -> C -> D@0.8\ncomponent X [0.5, 0.5]");
    assert_eq!(ast.axes.stage_boundaries, Some(vec![0.4, 0.8]));
}

#[test]
fn only_the_first_annotations_statement_sets_the_box() {
    let ast = ast("wardley-beta\nannotations [0.1, 0.9]\nannotations [0.5, 0.5]");
    let annotations_box = ast.annotations_box.expect("annotations box");
    close(annotations_box.x, 90.0);
    close(annotations_box.y, 10.0);
}

#[test]
fn the_annotations_box_accepts_integers() {
    let ast = ast("wardley-beta\nannotations [1, 0]");
    let annotations_box = ast.annotations_box.expect("annotations box");
    close(annotations_box.x, 0.0);
    close(annotations_box.y, 100.0);
}

#[test]
fn a_duplicate_component_merges_like_the_builder() {
    let ast =
        ast("wardley-beta\ncomponent A [0.1, 0.2] label [5, 6] (build)\ncomponent A [0.3, 0.4]");
    assert_eq!(ast.nodes.len(), 1);
    let node = &ast.nodes[0];
    close(node.x, 40.0);
    close(node.y, 30.0);
    assert_eq!(node.class_name.as_deref(), Some("component"));
    assert_eq!(node.label_offset_x, Some(5.0));
    assert_eq!(node.label_offset_y, Some(6.0));
    assert_eq!(node.source_strategy, None);
}

#[test]
fn a_later_evolve_for_the_same_component_replaces_the_trend() {
    let ast = ast("wardley-beta\ncomponent A [0.5, 0.5]\nevolve A 0.4\nevolve A 0.7");
    assert_eq!(ast.trends.len(), 1);
    close(ast.trends[0].target_x, 70.0);
}

#[test]
fn an_evolve_for_a_missing_component_is_ignored() {
    let ast = ast("wardley-beta\nevolve Nope 0.5");
    assert!(ast.trends.is_empty());
}

#[test]
fn marks_pipeline_parents_and_components() {
    let ast = ast(
        "wardley-beta\ncomponent A [0.5, 0.5]\npipeline A {\n  component x [0.2]\n  component y [0.3]\n}",
    );
    assert!(node(&ast, "A").is_pipeline_parent);
    assert!(node(&ast, "x").in_pipeline);
    assert!(node(&ast, "y").in_pipeline);
    assert!(!node(&ast, "x").is_pipeline_parent);
}

#[test]
fn recognises_dashed_and_plain_links() {
    let ast = ast(
        "wardley-beta\ncomponent A [0.1, 0.1]\ncomponent B [0.1, 0.1]\nA -> B\nA --> B\nA -.-> B",
    );
    let dashed: Vec<bool> = ast.links.iter().map(|link| link.dashed).collect();
    assert_eq!(dashed, [false, false, true]);
}

#[test]
fn a_port_flow_wins_over_the_arrow_flow() {
    let ast =
        ast("wardley-beta\ncomponent A [0.1, 0.1]\ncomponent B [0.1, 0.1]\nA +< +'backup'> B");
    assert_eq!(ast.links[0].flow, Some(Flow::Backward));
    assert_eq!(ast.links[0].label.as_deref(), Some("backup"));
}

#[test]
fn a_link_annotation_becomes_the_label() {
    let ast =
        ast("wardley-beta\ncomponent A [0.1, 0.1]\ncomponent B [0.1, 0.1]\nA -> B; constraint");
    assert_eq!(ast.links[0].flow, None);
    assert_eq!(ast.links[0].label.as_deref(), Some("constraint"));
}

#[test]
fn an_empty_link_annotation_is_an_empty_label() {
    let ast = ast("wardley-beta\ncomponent A [0.1, 0.1]\ncomponent B [0.1, 0.1]\nA -> B;   ");
    assert_eq!(ast.links[0].label.as_deref(), Some(""));
}

#[test]
fn an_arrow_label_supplies_the_flow_and_the_label() {
    let ast = ast("wardley-beta\ncomponent A [0.1, 0.1]\ncomponent B [0.1, 0.1]\nA +'backup'> B");
    assert_eq!(ast.links[0].flow, Some(Flow::Forward));
    assert_eq!(ast.links[0].label.as_deref(), Some("backup"));
}

#[test]
fn titles_and_accessibility_lines_are_stored() {
    let ast = ast(
        "wardley-beta\ntitle A map\naccTitle: Accessible\naccDescr {\n  First line\n  Second line\n}\ncomponent A [0.5, 0.5]",
    );
    assert_eq!(ast.diagram_title, "A map");
    assert_eq!(ast.acc_title, "Accessible");
    assert_eq!(ast.acc_description, "First line\nSecond line");
}

#[test]
fn a_title_stops_before_a_comment() {
    let ast = ast("wardley-beta\ntitle Hello %% trailing comment\n");
    assert_eq!(ast.diagram_title, "Hello");
}

#[test]
fn an_empty_title_leaves_the_title_empty() {
    let ast = ast("wardley-beta\ntitle\ncomponent A [0.5, 0.5]");
    assert_eq!(ast.diagram_title, "");
}

#[test]
fn statements_may_share_the_header_line() {
    let ast = ast("wardley-beta component A [0.1, 0.1]");
    assert_eq!(node(&ast, "A").x, 10.0);
}

#[test]
fn comments_are_skipped() {
    let ast = ast("wardley-beta\n%% a comment\ncomponent A [0.1, 0.1] %% trailing comment\n");
    assert!(ast.node("A").is_some());
}

#[test]
fn parses_crlf_line_endings() {
    let ast = ast("wardley-beta\r\ncomponent A [0.5, 0.5]\r\n");
    assert_eq!(node(&ast, "A").x, 50.0);
}

#[test]
fn keywords_shadow_names() {
    assert!(parse("wardley-beta\ncomponent note [0.5, 0.5]").is_err());
    assert!(parse("wardley-beta\ncomponent size [0.5, 0.5]").is_err());
}

#[test]
fn an_integer_where_a_decimal_is_required_is_an_error() {
    let error = error("wardley-beta\ncomponent A [1, 0]");
    assert_eq!(error.message, "expected WARDLEY_NUMBER, got 'INT'");
}

#[test]
fn a_leading_zero_in_size_is_two_integers() {
    let error = error("wardley-beta\nsize [0120, 80]");
    assert_eq!(error.message, "expected ,, got 'INT'");
}

#[test]
fn a_semicolon_alone_is_a_lexer_error() {
    let error = error("wardley-beta\ncomponent A [0.5, 0.5]\ncomponent B [0.5, 0.5]\nA -> B ;");
    assert_eq!(error.message, "unexpected character ';'");
    let span = error.span.expect("the error carries a span");
    assert_eq!(span.line, 4);
}

#[test]
fn an_unknown_character_reports_its_span() {
    let error = error("wardley-beta\ncomponent Café [0.5, 0.5]");
    assert_eq!(error.message, "unexpected character 'é'");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (2, 14));
}

#[test]
fn a_pipeline_needs_a_newline_after_the_opening_brace() {
    assert!(
        parse("wardley-beta\ncomponent A [0.5, 0.5]\npipeline A { component x [0.2] }").is_err()
    );
}

#[test]
fn a_pipeline_needs_at_least_one_component() {
    assert!(parse("wardley-beta\ncomponent A [0.5, 0.5]\npipeline A {\n}").is_err());
}

#[test]
fn parses_the_tea_shop_example() {
    let ast = ast(
        "wardley-beta\ntitle Tea Shop\nsize [1100, 800]\n\nanchor Business [0.95, 0.63]\nanchor Public [0.95, 0.78]\ncomponent Cup of Tea [0.79, 0.61] label [19, -4]\ncomponent Cup [0.73, 0.78]\ncomponent Tea [0.63, 0.81]\ncomponent Hot Water [0.52, 0.80]\ncomponent Water [0.38, 0.82]\ncomponent Kettle [0.43, 0.35] label [-57, 4]\ncomponent Power [0.1, 0.7] label [-27, 20]\n\nBusiness -> Cup of Tea\nPublic -> Cup of Tea\nCup of Tea -> Cup\nCup of Tea -> Tea\nCup of Tea -> Hot Water\nHot Water -> Water\nHot Water -> Kettle\nKettle -> Power\n\nevolve Kettle 0.62\nevolve Power 0.89\n\nnote \"Standardising power allows Kettles to evolve faster\" [0.30, 0.49]\nnote \"Hot water is obvious and well known\" [0.48, 0.80]\nnote \"A generic note appeared\" [0.23, 0.33]",
    );
    assert_eq!(ast.nodes.len(), 9);
    assert_eq!(ast.links.len(), 8);
    assert_eq!(ast.trends.len(), 2);
    assert_eq!(ast.notes.len(), 3);
    assert_eq!(node(&ast, "Cup of Tea").label_offset_x, Some(19.0));
    assert_eq!(node(&ast, "Cup of Tea").label_offset_y, Some(-4.0));
    assert_eq!(node(&ast, "Kettle").label_offset_x, Some(-57.0));
    close(node(&ast, "Power").x, 70.0);
    close(node(&ast, "Power").y, 10.0);
}

#[test]
fn parses_a_map_with_parenthesised_names_pipelines_and_annotations() {
    let ast = ast(
        "wardley-beta\ntitle GPT Tokeniser\nsize [1100, 800]\nanchor GPT Tokeniser [0.90, 0.58]\ncomponent tokeniser [0.81, 0.58]\ncomponent byte pair encoding (BPE) [0.53, 0.76] label [-27, 19]\npipeline tokeniser {\n  component tokeniser v1 [0.11]\n  component tokeniser v2 [0.80]\n}\ndeaccelerator License Play [0.13, 0.78]\nannotations [1, 0]\nannotation 1,[0.57, 0.16] \"Alternative algos in research\"\nnote \"Voting members\" [0.04, 0.35]",
    );
    assert_eq!(
        node(&ast, "byte pair encoding (BPE)").label,
        "byte pair encoding (BPE)"
    );
    assert_eq!(
        ast.pipelines[0].component_ids,
        ["tokeniser_tokeniser v1", "tokeniser_tokeniser v2"]
    );
    let deaccelerator = &ast.deaccelerators[0];
    assert_eq!(deaccelerator.name, "License Play");
    close(deaccelerator.x, 78.0);
    close(deaccelerator.y, 13.0);
    let annotations_box = ast.annotations_box.expect("annotations box");
    close(annotations_box.x, 0.0);
    close(annotations_box.y, 100.0);
    assert_eq!(ast.annotations[0].number, 1.0);
    assert_eq!(ast.notes[0].text, "Voting members");
}

#[test]
fn resolve_node_id_is_available_on_the_ast() {
    let ast = ast(
        "wardley-beta\ncomponent Kettle [0.5, 0.5]\npipeline Kettle {\n  component Campfire Kettle [0.3]\n}",
    );
    assert_eq!(ast.resolve_node_id("Kettle"), "Kettle");
    assert_eq!(
        ast.resolve_node_id("Campfire Kettle"),
        "Kettle_Campfire Kettle"
    );
    assert_eq!(ast.resolve_node_id("Missing"), "Missing");
}

#[test]
fn accelerators_keep_their_order() {
    let ast = ast("wardley-beta\naccelerator First [0.1, 0.2]\naccelerator Second [0.3, 0.4]");
    let names: Vec<&str> = ast
        .accelerators
        .iter()
        .map(|accelerator: &Accelerator| accelerator.name.as_str())
        .collect();
    assert_eq!(names, ["First", "Second"]);
}

#[test]
fn a_market_decorator_is_stored() {
    let ast = ast("wardley-beta\ncomponent A [0.5, 0.5] (market)");
    assert_eq!(
        node(&ast, "A").source_strategy,
        Some(SourceStrategy::Market)
    );
}

#[test]
fn labeled_arrows_declare_each_direction() {
    let ast =
        ast("wardley-beta\ncomponent A [0.1, 0.1]\ncomponent B [0.1, 0.1]\nA +'x'< B\nA +'y'<> B");
    assert_eq!(ast.links[0].flow, Some(Flow::Backward));
    assert_eq!(ast.links[0].label.as_deref(), Some("x"));
    assert_eq!(ast.links[1].flow, Some(Flow::Bidirectional));
    assert_eq!(ast.links[1].label.as_deref(), Some("y"));
}

#[test]
fn a_plain_greater_than_arrow_is_a_link() {
    let ast = ast("wardley-beta\ncomponent A [0.1, 0.1]\ncomponent B [0.1, 0.1]\nA > B");
    assert_eq!(ast.links[0].flow, None);
    assert!(!ast.links[0].dashed);
}

#[test]
fn a_single_line_acc_description_is_stored() {
    let ast = ast("wardley-beta\naccDescr: a neat description\ncomponent A [0.5, 0.5]");
    assert_eq!(ast.acc_description, "a neat description");
}

#[test]
fn blank_lines_inside_a_pipeline_are_ignored() {
    let ast = ast(
        "wardley-beta\ncomponent A [0.5, 0.5]\npipeline A {\n\n  component x [0.2]\n\n  component y [0.3]\n\n}",
    );
    assert_eq!(ast.pipelines[0].component_ids.len(), 2);
}

#[test]
fn a_stage_boundary_may_be_spaced() {
    let ast = ast("wardley-beta\nevolution A @ 0.3 -> B\ncomponent X [0.5, 0.5]");
    assert_eq!(ast.axes.stage_boundaries, Some(vec![0.3]));
}

#[test]
fn a_title_collapses_runs_of_spaces() {
    let ast = ast("wardley-beta\ntitle A   B\ncomponent X [0.5, 0.5]");
    assert_eq!(ast.diagram_title, "A B");
}

#[test]
fn an_annotation_coordinate_may_be_an_integer() {
    let ast = ast("wardley-beta\nannotation 3,[1, 0] \"x\"");
    close(ast.annotations[0].coordinates[0].x, 0.0);
    close(ast.annotations[0].coordinates[0].y, 100.0);
}

#[test]
fn an_empty_map_has_empty_collections() {
    let ast = ast("wardley-beta");
    assert!(ast.nodes.is_empty());
    assert!(ast.links.is_empty());
    assert!(ast.trends.is_empty());
    assert!(ast.pipelines.is_empty());
    assert!(ast.annotations.is_empty());
    assert!(ast.notes.is_empty());
    assert!(ast.accelerators.is_empty());
    assert!(ast.deaccelerators.is_empty());
    assert_eq!(ast.annotations_box, None);
    assert_eq!(ast.axes.stages, None);
    assert_eq!(ast.axes.stage_boundaries, None);
    assert_eq!(ast.size, None);
    assert_eq!(ast.diagram_title, "");
}
