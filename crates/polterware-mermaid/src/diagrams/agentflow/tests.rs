//! The parser and database specs of upstream `parser/*.spec.ts`, ported.
//!
//! Rendering and configuration assertions are not ported: this crate neither draws nor owns
//! configuration. The skipped cases are named in the final report and in the section comments
//! below.

use super::*;
use serde_json::json;

fn parsed(body: &str) -> Ast {
    parse(body).unwrap_or_else(|error| panic!("parse failed: {error}\n{body}"))
}

fn vertex<'a>(ast: &'a Ast, id: &str) -> &'a Vertex {
    ast.vertex(id)
        .unwrap_or_else(|| panic!("missing vertex {id}"))
}

fn mapping<'a>(ast: &'a Ast, id: &str) -> &'a ElementMapping {
    ast.element_by_id(id)
        .unwrap_or_else(|| panic!("missing mapping {id}"))
}

fn node<'a>(ast: &'a Ast, id: &str) -> &'a Node {
    ast.nodes
        .iter()
        .find(|node| node.id == id)
        .unwrap_or_else(|| panic!("missing node {id}"))
}

// ─── agentflow.spec.ts ───────────────────────────────────────────────────

#[test]
fn parses_an_empty_diagram_with_just_the_keyword_and_newline() {
    let ast = parsed("agentflow-beta TB\n");
    assert!(ast.nodes.is_empty());
    assert!(ast.layout_edges.is_empty());
}

#[test]
fn parses_two_nodes_and_an_edge() {
    let ast = parsed("agentflow-beta TB\n  a[\"Alpha\"]\n  b[\"Beta\"]\n  a --> b");
    assert_eq!(ast.nodes.len(), 2);
    assert_eq!(ast.layout_edges.len(), 1);
    assert_eq!(ast.layout_edges[0].start, "a");
    assert_eq!(ast.layout_edges[0].end, "b");
}

#[test]
fn captures_node_labels() {
    let ast = parsed("agentflow-beta TB\n  research[\"Research\"]\n  research --> write");
    assert_eq!(vertex(&ast, "research").text.as_deref(), Some("Research"));
}

#[test]
fn parses_lr_direction() {
    assert_eq!(
        parsed("agentflow-beta LR\n  a --> b").direction.as_deref(),
        Some("LR")
    );
}

#[test]
fn renders_a_self_loop_edge() {
    let ast = parsed("agentflow-beta TB\n  a[\"Alpha\"]\n  a --> a");
    assert_eq!(ast.layout_edges.len(), 1);
    assert_eq!(ast.layout_edges[0].start, "a");
    assert_eq!(ast.layout_edges[0].end, "a");
}

#[test]
fn keeps_a_self_loop_alongside_a_normal_edge() {
    let ast = parsed("agentflow-beta TB\n  a[\"Alpha\"]\n  b[\"Beta\"]\n  a --> b\n  b --> b");
    assert_eq!(ast.layout_edges.len(), 2);
    assert!(
        ast.layout_edges
            .iter()
            .any(|edge| edge.start == "b" && edge.end == "b")
    );
}

#[test]
fn drops_a_collapse_induced_self_loop() {
    let ast = parsed(
        "agentflow-beta TB\n  flow p[\"P\"]\n    a --> b\n  end\n  p@{ view: \"collapsed\" }",
    );
    assert!(ast.layout_edges.is_empty());
}

#[test]
fn parses_a_flow_with_a_quoted_label() {
    let ast = parsed("agentflow-beta TB\n  flow pipeline[\"Pipeline\"]\n    a --> b\n  end");
    let sub_graphs = &ast.sub_graphs;
    assert_eq!(sub_graphs.len(), 1);
    assert_eq!(sub_graphs[0].id, "pipeline");
    assert_eq!(sub_graphs[0].title, "Pipeline");
    assert_eq!(sub_graphs[0].type_name.as_deref(), Some("flow"));
}

#[test]
fn parses_a_flow_without_a_label() {
    let ast = parsed("agentflow-beta TB\n  flow pipeline\n    a --> b\n  end");
    assert_eq!(ast.sub_graphs.len(), 1);
    assert_eq!(ast.sub_graphs[0].id, "pipeline");
}

#[test]
fn parses_nested_flows() {
    let ast = parsed(
        "agentflow-beta TB\n  flow outer[\"Outer\"]\n    flow inner[\"Inner\"]\n      a --> b\n    end\n  end",
    );
    assert!(ast.sub_graphs.len() >= 2);
    assert!(ast.sub_graph("inner").is_some());
    assert!(ast.sub_graph("outer").is_some());
}

#[test]
fn exposes_flow_as_a_group_node() {
    let ast = parsed("agentflow-beta TB\n  flow pipeline[\"Pipeline\"]\n    a --> b\n  end");
    let pipeline = node(&ast, "pipeline");
    assert!(pipeline.is_group);
}

#[test]
fn parses_arrow_point_as_sequence() {
    let ast = parsed("agentflow-beta TB\n  a --> b");
    let edge = &ast.edges[0];
    assert_eq!(edge.type_name, Some(ArrowType::Point));
    assert_eq!(edge.stroke, Some(Stroke::Normal));
    assert_eq!(edge.edge_semantic, Some(EdgeSemantic::Sequence));
}

#[test]
fn parses_cross_as_failure() {
    let ast = parsed("agentflow-beta TB\n  a --x b");
    let edge = &ast.edges[0];
    assert_eq!(edge.type_name, Some(ArrowType::Cross));
    assert_eq!(edge.edge_semantic, Some(EdgeSemantic::Failure));
}

#[test]
fn parses_dotted_as_a_reference_edge() {
    let ast = parsed("agentflow-beta TB\n  a -.- b");
    let edge = &ast.edges[0];
    assert_eq!(edge.stroke, Some(Stroke::Dotted));
    assert_eq!(edge.edge_semantic, Some(EdgeSemantic::Reference));
}

#[test]
fn accepts_labels_on_arrows() {
    let ast = parsed("agentflow-beta TB\n  a -- yes --> b");
    assert_eq!(ast.edges.len(), 1);
    assert_eq!(ast.edges[0].text, "yes");
}

#[test]
fn accepts_shape_tool() {
    let ast =
        parsed("agentflow-beta TB\n  s[\"Search\"]\n  s@{ shape: tool, returns: \"Result\" }");
    assert!(ast.is_tool_definition(vertex(&ast, "s")));
}

#[test]
fn accepts_shape_action() {
    let ast = parsed("agentflow-beta TB\n  a[\"Action\"]\n  a@{ shape: action }");
    let model = ast.semantic_model();
    assert_eq!(
        model
            .vertices
            .iter()
            .find(|vertex| vertex.id == "a")
            .map(|vertex| vertex.vertex_kind),
        Some(VertexKind::Action)
    );
}

#[test]
fn accepts_shape_input() {
    let ast = parsed("agentflow-beta TB\n  i[\"x\"]\n  i@{ shape: input, value: \"literal\" }");
    let model = ast.semantic_model();
    assert_eq!(
        model
            .vertices
            .iter()
            .find(|vertex| vertex.id == "i")
            .map(|vertex| vertex.vertex_kind),
        Some(VertexKind::Input)
    );
}

#[test]
fn accepts_shape_refdoc() {
    let ast = parsed("agentflow-beta TB\n  r[\"Guide\"]\n  r@{ shape: refdoc }");
    let model = ast.semantic_model();
    assert_eq!(
        model
            .vertices
            .iter()
            .find(|vertex| vertex.id == "r")
            .map(|vertex| vertex.vertex_kind),
        Some(VertexKind::Refdoc)
    );
}

#[test]
fn accepts_shape_decision_and_inline_diamond_syntax() {
    let ast = parsed("agentflow-beta TB\n  d{\"is_valid?\"}");
    let model = ast.semantic_model();
    assert_eq!(
        model
            .vertices
            .iter()
            .find(|vertex| vertex.id == "d")
            .map(|vertex| vertex.vertex_kind),
        Some(VertexKind::Decision)
    );
}

#[test]
fn parses_a_connector_declaration() {
    let ast = parsed(
        "agentflow-beta TB\n  connector github[\"GitHub\"]\n  github@{ protocol: \"mcp\", transport: \"stdio\" }",
    );
    let connectors = ast.connectors();
    assert_eq!(connectors.len(), 1);
    assert_eq!(connectors[0].id, "github");
    assert_eq!(
        connectors[0]
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("protocol")),
        Some(&json!("mcp"))
    );
}

#[test]
fn tracks_multiple_connectors_separately() {
    let ast =
        parsed("agentflow-beta TB\n  connector github[\"GitHub\"]\n  connector slack[\"Slack\"]");
    let mut ids: Vec<&str> = ast
        .connectors()
        .iter()
        .map(|connector| connector.id.as_str())
        .collect();
    ids.sort();
    assert_eq!(ids, ["github", "slack"]);
}

#[test]
fn parses_inline_metadata_on_a_labeled_connector() {
    let ast = parsed(
        "agentflow-beta TB\n  connector github[\"GitHub\"]@{ protocol: \"mcp\", transport: \"stdio\" }",
    );
    let connectors = ast.connectors();
    assert_eq!(connectors.len(), 1);
    let metadata = connectors[0].metadata.as_ref().expect("metadata");
    assert_eq!(connectors[0].text.as_deref(), Some("GitHub"));
    assert_eq!(metadata.get("protocol"), Some(&json!("mcp")));
    assert_eq!(metadata.get("transport"), Some(&json!("stdio")));
}

#[test]
fn parses_inline_metadata_on_a_bare_connector() {
    let ast = parsed("agentflow-beta TB\n  connector github@{ protocol: \"mcp\" }");
    let connectors = ast.connectors();
    assert_eq!(connectors.len(), 1);
    assert_eq!(
        connectors[0]
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("protocol")),
        Some(&json!("mcp"))
    );
}

#[test]
fn merges_inline_and_standalone_connector_metadata() {
    let ast = parsed(
        "agentflow-beta TB\n  connector github[\"GitHub\"]@{ protocol: \"mcp\" }\n  github@{ transport: \"stdio\" }",
    );
    let metadata = ast.connectors()[0].metadata.as_ref().expect("metadata");
    assert_eq!(metadata.get("protocol"), Some(&json!("mcp")));
    assert_eq!(metadata.get("transport"), Some(&json!("stdio")));
}

#[test]
fn parses_multiline_inline_metadata_on_a_connector() {
    let ast = parsed(
        "agentflow-beta TB\n  connector github[\"GitHub\"]@{\n    protocol: \"mcp\"\n    transport: \"stdio\"\n  }\n  a --> b",
    );
    let metadata = ast.connectors()[0].metadata.as_ref().expect("metadata");
    assert_eq!(metadata.get("protocol"), Some(&json!("mcp")));
    assert_eq!(metadata.get("transport"), Some(&json!("stdio")));
}

#[test]
fn expands_parallel_fan_out() {
    let ast = parsed("agentflow-beta TB\n  a --> b & c");
    assert_eq!(ast.edges.len(), 2);
    let mut ends: Vec<&str> = ast.edges.iter().map(|edge| edge.end.as_str()).collect();
    ends.sort();
    assert_eq!(ends, ["b", "c"]);
}

#[test]
fn parses_flat_metadata_on_a_vertex() {
    let ast = parsed(
        "agentflow-beta TB\n  a[\"Alpha\"]\n  a@{ description: \"alpha node\", instruction: \"be careful\" }",
    );
    let metadata = vertex(&ast, "a").metadata.as_ref().expect("metadata");
    assert_eq!(metadata.get("description"), Some(&json!("alpha node")));
    assert_eq!(metadata.get("instruction"), Some(&json!("be careful")));
}

#[test]
fn merges_multiple_metadata_statements() {
    let ast = parsed(
        "agentflow-beta TB\n  a[\"A\"]\n  a@{ description: \"first\" }\n  a@{ instruction: \"second\" }",
    );
    let metadata = vertex(&ast, "a").metadata.as_ref().expect("metadata");
    assert_eq!(metadata.get("description"), Some(&json!("first")));
    assert_eq!(metadata.get("instruction"), Some(&json!("second")));
}

#[test]
fn parses_metadata_on_a_flow_container() {
    let ast = parsed(
        "agentflow-beta TB\n  flow pipeline[\"P\"]\n    a --> b\n  end\n  pipeline@{ model: \"claude-opus-4-6\", memory: \"shared\" }",
    );
    let metadata = ast
        .sub_graph("pipeline")
        .and_then(|sub_graph| sub_graph.metadata.as_ref())
        .expect("metadata");
    assert_eq!(metadata.get("model"), Some(&json!("claude-opus-4-6")));
    assert_eq!(metadata.get("memory"), Some(&json!("shared")));
}

#[test]
fn marks_a_subgraph_collapsed() {
    let ast = parsed(
        "agentflow-beta TB\n  flow pipeline[\"P\"]\n    a --> b\n  end\n  pipeline@{ view: \"collapsed\" }",
    );
    let metadata = ast
        .sub_graph("pipeline")
        .and_then(|sub_graph| sub_graph.metadata.as_ref())
        .expect("metadata");
    assert_eq!(metadata.get("view"), Some(&json!("collapsed")));
}

// ─── agentflow-flowchart-parity.spec.ts ──────────────────────────────────

const SHARED_SYNTAX: [&str; 7] = [
    "  A@{ label: \"hello\" }",
    "  A@{ shape: rect }",
    "  A@{ label: a^b }",
    "  A@{\n    label: \"multi\"\n  }",
    "  A[\"Label\"] --> B[\"Other\"]",
    "  A -- text --> B",
    "  A[\"a (paren) b\"]",
];

#[test]
fn agrees_with_flowchart_on_shared_syntax() {
    for body in SHARED_SYNTAX {
        let result = parse(&format!("agentflow-beta TB\n{body}\n"));
        let should_accept = !body.contains('^');
        assert_eq!(result.is_ok(), should_accept, "body: {body:?}");
    }
}

#[test]
fn rejects_a_caret_in_shape_data_like_flowchart_does() {
    assert!(parse("agentflow-beta TB\n  A@{ label: a^b }\n").is_err());
}

// ─── agentflow-global.spec.ts ────────────────────────────────────────────

const GLOBAL_SOURCE: &str = "agentflow-beta\n\nglobal\n  A\nend\n\nflow\n A --> B --> C\nend";

#[test]
fn global_excludes_a_node_from_flow_membership() {
    let ast = parsed(GLOBAL_SOURCE);
    assert_eq!(ast.sub_graphs.len(), 1);
    let flow = &ast.sub_graphs[0];
    assert!(!flow.nodes.contains(&"A".to_string()));
    assert!(flow.nodes.contains(&"B".to_string()));
    assert!(flow.nodes.contains(&"C".to_string()));
}

#[test]
fn global_node_has_no_parent_and_flow_members_do() {
    let ast = parsed(GLOBAL_SOURCE);
    let flow_id = ast.sub_graphs[0].id.clone();
    assert_eq!(node(&ast, "A").parent_id, None);
    assert_eq!(node(&ast, "B").parent_id, Some(flow_id.clone()));
    assert_eq!(node(&ast, "C").parent_id, Some(flow_id));
}

#[test]
fn global_keeps_flow_edges_intact() {
    let ast = parsed(GLOBAL_SOURCE);
    assert!(
        ast.layout_edges
            .iter()
            .any(|edge| edge.start == "A" && edge.end == "B")
    );
    assert!(
        ast.layout_edges
            .iter()
            .any(|edge| edge.start == "B" && edge.end == "C")
    );
}

#[test]
fn global_block_emits_no_container_node() {
    let ast = parsed(GLOBAL_SOURCE);
    let groups: Vec<&Node> = ast.nodes.iter().filter(|node| node.is_group).collect();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].id, ast.sub_graphs[0].id);
}

#[test]
fn global_is_order_independent_after_the_flow() {
    let ast =
        parsed("agentflow-beta\n\nflow pipeline[\"Pipeline\"]\n  A --> B\nend\n\nglobal\n  A\nend");
    let flow = ast.sub_graph("pipeline").expect("flow");
    assert!(!flow.nodes.contains(&"A".to_string()));
    assert!(flow.nodes.contains(&"B".to_string()));
    assert_eq!(node(&ast, "A").parent_id, None);
    assert_eq!(node(&ast, "B").parent_id, Some("pipeline".to_string()));
}

#[test]
fn global_allows_edges_and_both_endpoints_become_global() {
    let ast = parsed(
        "agentflow-beta\n\nglobal\n  A --> X\nend\n\nflow f[\"Flow\"]\n  A --> B\n  X --> B\nend",
    );
    let flow = ast.sub_graph("f").expect("flow");
    assert_eq!(flow.nodes, ["B"]);
    assert_eq!(node(&ast, "A").parent_id, None);
    assert_eq!(node(&ast, "X").parent_id, None);
    assert!(
        ast.layout_edges
            .iter()
            .any(|edge| edge.start == "A" && edge.end == "X")
    );
}

#[test]
fn global_acts_as_an_escape_hatch_nested_in_a_flow() {
    let ast = parsed("agentflow-beta\n\nflow f[\"Flow\"]\n  global\n    A\n  end\n  A --> B\nend");
    let flow = ast.sub_graph("f").expect("flow");
    assert_eq!(flow.nodes, ["B"]);
    assert_eq!(node(&ast, "A").parent_id, None);
    assert_eq!(node(&ast, "B").parent_id, Some("f".to_string()));
}

#[test]
fn global_supports_labels_and_shapes() {
    let ast = parsed(
        "agentflow-beta\n\nglobal\n  A[\"Shared input\"]\n  D{\"Decision\"}\nend\n\nflow f[\"Flow\"]\n  A --> B --> D\nend",
    );
    assert_eq!(node(&ast, "A").parent_id, None);
    assert_eq!(node(&ast, "A").label, "Shared input");
    assert_eq!(node(&ast, "D").parent_id, None);
    assert_eq!(node(&ast, "D").shape, "diamond");
}

#[test]
fn global_merges_multiple_blocks() {
    let ast = parsed(
        "agentflow-beta\n\nglobal\n  A\nend\n\nglobal\n  B\nend\n\nflow f[\"Flow\"]\n  A --> B --> C\nend",
    );
    assert_eq!(ast.sub_graph("f").expect("flow").nodes, ["C"]);
}

#[test]
fn global_is_usable_inside_bracketed_labels() {
    let ast =
        parsed("agentflow-beta\n\nflow f[\"global settings\"]\n  a[\"a global thing\"] --> b\nend");
    assert_eq!(ast.sub_graphs[0].title, "global settings");
    assert_eq!(node(&ast, "a").label, "a global thing");
}

// ─── agentflow-attachment-mapping.spec.ts ────────────────────────────────

#[test]
fn standalone_attachment_maps_as_attachment() {
    let ast = parsed(
        "agentflow-beta TB\n\nflow demo\n  helper[\"Helper\"]\nend\n\nhelper@{ instruction: \"Answer precisely.\" }",
    );
    let mut types: Vec<&str> = ast
        .element_mappings
        .iter()
        .filter(|mapping| mapping.id == "helper")
        .map(|mapping| mapping.type_name.as_str())
        .collect();
    types.sort();
    assert_eq!(types, ["attachment", "vertex"]);
    let declaration = ast
        .element_mappings
        .iter()
        .find(|mapping| mapping.id == "helper" && mapping.type_name == MappingType::Vertex)
        .expect("declaration");
    assert_eq!(declaration.position.start_line, 4);
    let attachment = ast
        .element_mappings
        .iter()
        .find(|mapping| mapping.id == "helper" && mapping.type_name == MappingType::Attachment)
        .expect("attachment");
    assert_eq!(attachment.position.start_line, 7);
    assert_eq!(attachment.position.start_column, 0);
    assert_eq!(attachment.position.end_line, 7);
}

#[test]
fn flow_attachment_maps_as_attachment() {
    let ast = parsed(
        "agentflow-beta TB\n  flow pipeline[\"Pipeline\"]\n    a --> b\n  end\n  pipeline@{ view: \"collapsed\" }",
    );
    let mut types: Vec<&str> = ast
        .element_mappings
        .iter()
        .filter(|mapping| mapping.id == "pipeline")
        .map(|mapping| mapping.type_name.as_str())
        .collect();
    types.sort();
    assert_eq!(types, ["attachment", "subgraph"]);
}

#[test]
fn connector_attachment_maps_as_attachment() {
    let ast = parsed(
        "agentflow-beta TB\n  connector github[\"GitHub\"]\n  github@{ protocol: \"mcp\", transport: \"stdio\" }",
    );
    let mut types: Vec<&str> = ast
        .element_mappings
        .iter()
        .filter(|mapping| mapping.id == "github")
        .map(|mapping| mapping.type_name.as_str())
        .collect();
    types.sort();
    assert_eq!(types, ["attachment", "connector"]);
}

#[test]
fn edge_metadata_attachment_maps_as_attachment() {
    let ast = parsed(
        "agentflow-beta TB\n  a[\"A\"]\n  b[\"B\"]\n  a e1@--> b\n  e1@{ instruction: \"hand off\" }",
    );
    let types: Vec<&str> = ast
        .element_mappings
        .iter()
        .filter(|mapping| mapping.id == "e1" && mapping.type_name != MappingType::Edge)
        .map(|mapping| mapping.type_name.as_str())
        .collect();
    assert_eq!(types, ["attachment"]);
}

#[test]
fn multiline_standalone_block_spans_and_is_hit_testable() {
    let ast =
        parsed("agentflow-beta TB\n  a[\"Alpha\"]\n  a@{\n    instruction: \"do the thing\"\n  }");
    let attachment = ast
        .element_mappings
        .iter()
        .find(|mapping| mapping.id == "a" && mapping.type_name == MappingType::Attachment)
        .expect("attachment");
    assert_eq!(attachment.position.start_line, 3);
    assert_eq!(attachment.position.end_line, 5);
    let hit = ast.element_at_position(4, 8).expect("hit");
    assert_eq!(hit.id, "a");
    assert_eq!(hit.type_name, MappingType::Attachment);
}

#[test]
fn inline_metadata_keeps_a_single_widened_vertex_mapping() {
    let ast = parsed("agentflow-beta TB\n  a[\"A\"]\n  b[\"B\"]@{ shape: rect }\n  a --> b");
    assert!(
        ast.element_mappings
            .iter()
            .filter(|mapping| mapping.id == "b" && mapping.type_name != MappingType::Edge)
            .all(|mapping| mapping.type_name == MappingType::Vertex)
    );
}

#[test]
fn a_bare_reference_stays_a_vertex_mapping() {
    let ast = parsed("agentflow-beta TB\n  a --> b");
    assert_eq!(mapping(&ast, "a").type_name, MappingType::Vertex);
}

#[test]
fn mapping_stats_count_attachments_separately() {
    let ast = parsed(
        "agentflow-beta TB\n  flow p[\"P\"]\n    a --> b\n  end\n  p@{ view: \"collapsed\" }",
    );
    let stats = ast.mapping_stats();
    assert_eq!(stats.attachments, 1);
    assert_eq!(
        stats.total_elements,
        stats.vertices + stats.edges + stats.subgraphs + stats.connectors + stats.attachments
    );
}

// ─── agentflow-blank-line-fold.spec.ts ───────────────────────────────────

#[test]
fn a_blank_line_after_a_block_does_not_shift_the_next_node() {
    let ast =
        parsed("agentflow-beta\nflow f[\"F\"]\n  a[\"A\"]@{ shape: rounded }\n\n  b[\"B\"]\nend");
    let b = mapping(&ast, "b");
    assert_eq!(b.position.start_line, 5);
    assert_eq!(b.position.end_line, 5);
}

#[test]
fn two_blank_lines_after_a_block_do_not_shift_by_two() {
    let ast =
        parsed("agentflow-beta\nflow f[\"F\"]\n  a[\"A\"]@{ shape: rounded }\n\n\n  b[\"B\"]\nend");
    assert_eq!(mapping(&ast, "b").position.start_line, 6);
}

#[test]
fn a_comment_line_after_a_block_keeps_the_next_line() {
    let ast = parsed(
        "agentflow-beta\nflow f[\"F\"]\n  a[\"A\"]@{ shape: rounded }\n  %% a note\n  b[\"B\"]\nend",
    );
    assert_eq!(mapping(&ast, "b").position.start_line, 5);
}

#[test]
fn an_edge_after_a_block_and_blank_keeps_its_line() {
    let ast = parsed("agentflow-beta TB\n  a[\"A\"]@{ shape: rounded }\n\n  c[\"C\"]\n  a --> c");
    let edge = ast
        .element_mappings
        .iter()
        .find(|mapping| mapping.type_name == MappingType::Edge)
        .expect("edge");
    assert_eq!(edge.position.start_line, 5);
}

#[test]
fn a_parse_error_reports_the_source_line() {
    let error = parse(
        "agentflow-beta TB\n  a[\"A\"]@{ shape: rounded }\n\n  b[\"B\"]@{ shape: rounded }\n\n  c[\"C\"]\"]\n",
    )
    .expect_err("syntax error expected");
    assert_eq!(error.span.expect("span").line, 6);
}

#[test]
fn strips_trailing_whitespace_after_a_block_without_folding_a_blank() {
    let ast = parsed("agentflow-beta TB\n  a[\"A\"]@{ shape: rounded }   \n\n  b[\"B\"]");
    assert!(ast.vertex("a").is_some());
    assert_eq!(mapping(&ast, "b").position.start_line, 4);
}

// ─── agentflow-comments.spec.ts ──────────────────────────────────────────

#[test]
fn parses_a_comment_before_the_keyword() {
    let ast = parsed("%% leading comment\nagentflow-beta TB\n  a --> b");
    assert!(ast.vertex("a").is_some());
    assert!(ast.vertex("b").is_some());
}

#[test]
fn parses_multiple_comments_before_the_header() {
    let ast = parsed("%% first\n%% second\n%% third\nagentflow-beta TB\n  a --> b");
    assert!(ast.vertex("a").is_some());
}

#[test]
fn parses_a_comment_right_after_the_keyword() {
    let ast = parsed("agentflow-beta TB\n%% trailing header comment\n  a --> b");
    assert_eq!(ast.vertices.len(), 2);
}

#[test]
fn parses_a_comment_between_statements() {
    let ast = parsed("agentflow-beta TB\n  a --> b\n%% mid comment\n  b --> c");
    assert_eq!(ast.vertices.len(), 3);
}

#[test]
fn parses_back_to_back_comments() {
    let ast = parsed("agentflow-beta TB\n  a --> b\n%% first\n%% second\n  b --> c");
    assert_eq!(ast.vertices.len(), 3);
}

#[test]
fn parses_a_trailing_comment() {
    let ast = parsed("agentflow-beta TB\n  a --> b\n%% trailing");
    assert_eq!(ast.vertices.len(), 2);
}

#[test]
fn parses_a_comment_without_a_space() {
    let ast = parsed("agentflow-beta TB\n  a --> b\n%%no-space comment\n  b --> c");
    assert_eq!(ast.vertices.len(), 3);
}

#[test]
fn parses_an_empty_comment_line() {
    let ast = parsed("agentflow-beta TB\n  a --> b\n%%\n  b --> c");
    assert_eq!(ast.vertices.len(), 3);
}

#[test]
fn strips_a_comment_inside_a_square_label() {
    let ast = parsed("agentflow-beta TB\n  a[Plain %% not part of the label\nlabel]\n  b --> a");
    let text = vertex(&ast, "a").text.clone().expect("label");
    assert!(!text.contains("%%"), "{text:?}");
    assert!(!text.contains("not part of the label"), "{text:?}");
    assert!(text.contains("Plain"), "{text:?}");
    assert!(text.contains("label"), "{text:?}");
}

#[test]
fn strips_a_comment_inside_an_ellipse_label() {
    let ast =
        parsed("agentflow-beta TB\n  a(-ellipse %% not part of the label\nlabel-)\n  b --> a");
    let text = vertex(&ast, "a").text.clone().expect("label");
    assert!(!text.contains("%%"), "{text:?}");
    assert!(!text.contains("not part of the label"), "{text:?}");
    assert!(text.contains("ellipse"), "{text:?}");
}

#[test]
fn strips_a_comment_inside_a_trapezoid_label() {
    let ast = parsed("agentflow-beta TB\n  a[/trap %% not part of the label\nlabel/]\n  b --> a");
    let text = vertex(&ast, "a").text.clone().expect("label");
    assert!(!text.contains("%%"), "{text:?}");
    assert!(!text.contains("not part of the label"), "{text:?}");
    assert!(text.contains("trap"), "{text:?}");
}

#[test]
fn parses_comments_around_a_flow() {
    let ast = parsed(
        "agentflow-beta TB\n%% before flow\nflow f1[\"Flow One\"]\n%% inside flow\n  a --> b\nend\n%% after flow",
    );
    assert_eq!(ast.vertices.len(), 2);
}

#[test]
fn parses_comments_around_connectors() {
    let ast = parsed(
        "agentflow-beta TB\n%% before connector\nconnector github[\"GitHub\"]\n%% after connector\n  a --> b",
    );
    assert_eq!(ast.vertices.len(), 3);
}

// ─── agentflow-containment-cycle.spec.ts ─────────────────────────────────

fn mutual(view_a: &str, view_b: &str) -> String {
    format!(
        "agentflow-beta TB\n  flow A[\"Flow A\"]{view_a}\n    a1[\"Task A1\"]\n    a1 --> B\n  end\n  flow B[\"Flow B\"]{view_b}\n    b1[\"Task B1\"]\n    b1 --> A\n  end"
    )
}

#[test]
fn mutual_containment_emits_no_parent_cycle() {
    let ast = parsed(&mutual("", ""));
    for item in &ast.nodes {
        let mut seen = std::collections::HashSet::new();
        let mut current = item.parent_id.clone();
        while let Some(parent) = current {
            assert!(seen.insert(parent.clone()), "cycle at {parent}");
            current = node(&ast, &parent).parent_id.clone();
        }
    }
}

#[test]
fn mutual_containment_keeps_the_first_parent_and_drops_the_loop() {
    let ast = parsed(&mutual("", ""));
    assert_eq!(node(&ast, "A").parent_id, Some("B".to_string()));
    assert_eq!(node(&ast, "B").parent_id, None);
    assert_eq!(node(&ast, "a1").parent_id, Some("A".to_string()));
    assert_eq!(node(&ast, "b1").parent_id, Some("B".to_string()));
}

#[test]
fn mutual_containment_reports_a_violation() {
    let ast = parsed(&mutual("", ""));
    let diagnostic = ast
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.id == super::ast::CONTAINMENT_VIOLATION
                && diagnostic.node_id.as_deref() == Some("B")
        })
        .expect("diagnostic");
    assert_eq!(diagnostic.severity, Severity::Warning);
}

#[test]
fn mutual_containment_with_one_collapsed_container_terminates() {
    let ast = parsed(&mutual("@{ view: collapsed }", ""));
    assert!(ast.nodes.iter().any(|node| node.id == "A"));
    let collapsed = node(&ast, "A");
    assert_eq!(collapsed.shape, "collapsedGroup");
    assert_eq!(collapsed.parent_id, None);
}

#[test]
fn mutual_containment_with_both_collapsed_draws_something() {
    let ast = parsed(&mutual("@{ view: collapsed }", "@{ view: collapsed }"));
    assert!(!ast.nodes.is_empty());
}

#[test]
fn three_container_cycle_terminates() {
    let ast = parsed(
        "agentflow-beta TB\n  flow A[\"A\"]@{ view: collapsed }\n    a1[\"a1\"]\n    a1 --> B\n  end\n  flow B[\"B\"]\n    b1[\"b1\"]\n    b1 --> C\n  end\n  flow C[\"C\"]\n    c1[\"c1\"]\n    c1 --> A\n  end",
    );
    assert!(!ast.nodes.is_empty());
}

// ─── agentflow-edge-instruction.spec.ts ──────────────────────────────────

const INSTRUCTION: &str = "Pass the distilled question and any relevant context to the next step";

fn instruction_diagram() -> String {
    format!(
        "agentflow-beta TB\n  a[\"A\"]\n  b[\"B\"]\n  a e1@--> b\n  e1@{{ instruction: \"{INSTRUCTION}\" }}"
    )
}

#[test]
fn edge_instruction_survives_on_the_raw_edge() {
    let ast = parsed(&instruction_diagram());
    let edge = ast.edge("e1").expect("edge");
    assert_eq!(
        edge.metadata
            .as_ref()
            .and_then(|metadata| metadata.get("instruction")),
        Some(&json!(INSTRUCTION))
    );
}

#[test]
fn edge_instruction_survives_in_the_layout_projection() {
    let ast = parsed(&instruction_diagram());
    let edge = ast
        .layout_edges
        .iter()
        .find(|edge| edge.id == "e1")
        .expect("edge");
    assert_eq!(
        edge.metadata
            .as_ref()
            .and_then(|metadata| metadata.get("instruction")),
        Some(&json!(INSTRUCTION))
    );
}

#[test]
fn edge_instruction_survives_in_the_semantic_model() {
    let ast = parsed(&instruction_diagram());
    let model = ast.semantic_model();
    let edge = model
        .edges
        .iter()
        .find(|edge| edge.id.as_deref() == Some("e1"))
        .expect("edge");
    assert_eq!(
        edge.metadata
            .as_ref()
            .and_then(|metadata| metadata.get("instruction")),
        Some(&json!(INSTRUCTION))
    );
}

#[test]
fn edge_metadata_stays_absent_without_a_block() {
    let ast = parsed("agentflow-beta TB\n  a[\"A\"]\n  b[\"B\"]\n  a --> b");
    assert!(ast.layout_edges[0].metadata.is_none());
}

// ─── agentflow-edge-semantic.spec.ts ─────────────────────────────────────

fn edge_for(operator: &str) -> Ast {
    parsed(&format!(
        "agentflow-beta TB\n  a[\"A\"]\n  b[\"B\"]\n  a {operator} b"
    ))
}

#[test]
fn canonical_operators_map_to_their_semantic() {
    assert_eq!(
        edge_for("-->").edges[0].edge_semantic,
        Some(EdgeSemantic::Sequence)
    );
    assert_eq!(
        edge_for("-.-").edges[0].edge_semantic,
        Some(EdgeSemantic::Reference)
    );
    assert_eq!(
        edge_for("--x").edges[0].edge_semantic,
        Some(EdgeSemantic::Failure)
    );
}

#[test]
fn a_longer_arrow_keeps_its_semantic() {
    assert_eq!(
        edge_for("---->").edges[0].edge_semantic,
        Some(EdgeSemantic::Sequence)
    );
}

#[test]
fn edge_type_and_stroke_are_unchanged() {
    let sequence = edge_for("-->");
    assert_eq!(sequence.edges[0].type_name, Some(ArrowType::Point));
    assert_eq!(sequence.edges[0].stroke, Some(Stroke::Normal));
    let reference = edge_for("-.-");
    assert_eq!(reference.edges[0].stroke, Some(Stroke::Dotted));
    let failure = edge_for("--x");
    assert_eq!(failure.edges[0].type_name, Some(ArrowType::Cross));
    assert_eq!(failure.edges[0].stroke, Some(Stroke::Normal));
}

#[test]
fn semantic_model_surfaces_edge_semantic() {
    let ast = parsed("agentflow-beta TB\n  a[\"A\"]\n  b[\"B\"]\n  c[\"C\"]\n  a --> b\n  b --x c");
    let model = ast.semantic_model();
    assert_eq!(model.edges.len(), 2);
    assert_eq!(model.edges[0].edge_semantic, Some(EdgeSemantic::Sequence));
    assert_eq!(model.edges[1].edge_semantic, Some(EdgeSemantic::Failure));
}

// ─── agentflow-empty-metadata.spec.ts ────────────────────────────────────

#[test]
fn multiline_empty_block_parses() {
    let ast = parsed("agentflow-beta TB\n  newtask[\"Example\"]@{\n}");
    assert_eq!(vertex(&ast, "newtask").text.as_deref(), Some("Example"));
}

#[test]
fn whitespace_only_multiline_block_parses() {
    let ast = parsed("agentflow-beta TB\n  a[\"A\"]@{\n\n}");
    assert!(ast.vertex("a").is_some());
}

#[test]
fn whitespace_only_single_line_block_parses() {
    let ast = parsed("agentflow-beta TB\n  a[\"A\"]@{   }");
    assert!(ast.vertex("a").is_some());
}

#[test]
fn multiline_empty_block_matches_single_line() {
    let single = parsed("agentflow-beta TB\n  a[\"A\"]@{}");
    let multi = parsed("agentflow-beta TB\n  a[\"A\"]@{\n}");
    assert_eq!(vertex(&single, "a").metadata, vertex(&multi, "a").metadata);
    assert_eq!(
        vertex(&single, "a").type_name,
        vertex(&multi, "a").type_name
    );
}

#[test]
fn empty_blocks_on_a_flow_match() {
    let single = parsed("agentflow-beta TB\n  flow f[\"F\"]\n    a --> b\n  end\n  f@{}");
    let multi = parsed("agentflow-beta TB\n  flow f[\"F\"]\n    a --> b\n  end\n  f@{\n}");
    assert_eq!(
        single.sub_graph("f").and_then(|sg| sg.metadata.clone()),
        multi.sub_graph("f").and_then(|sg| sg.metadata.clone())
    );
    assert_eq!(single.vertex("f"), multi.vertex("f"));
}

#[test]
fn a_malformed_block_reports_a_positioned_error() {
    let error = parse("agentflow-beta TB\n  a[\"A\"]@{\n:\n}").expect_err("YAML error");
    let message = error.to_string();
    assert!(message.contains('('), "{message}");
}

#[test]
fn a_non_string_shape_is_rejected() {
    let error = parse("agentflow-beta TB\n  a[\"A\"]@{ shape: 123 }").expect_err("shape error");
    assert!(error.message.contains("No such shape"), "{}", error.message);
}

#[test]
fn a_valid_metadata_block_is_unaffected() {
    let ast = parsed("agentflow-beta TB\n  a[\"A\"]@{ shape: task }");
    assert_eq!(vertex(&ast, "a").type_name.as_deref(), Some("roundedRect"));
}

// ─── agentflow-flow-metadata.spec.ts ─────────────────────────────────────

#[test]
fn attaches_multiline_inline_metadata_to_the_flow() {
    let ast = parsed(
        "agentflow-beta TB\n  flow article_generation@{\n         model: \"claude-opus-4\"\n       }\n        A --> B\n  end",
    );
    let sub_graph = ast.sub_graph("article_generation").expect("flow");
    assert_eq!(
        sub_graph
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("model")),
        Some(&json!("claude-opus-4"))
    );
    assert!(sub_graph.nodes.contains(&"A".to_string()));
    assert!(sub_graph.nodes.contains(&"B".to_string()));
    assert_eq!(ast.edges.len(), 1);
}

#[test]
fn attaches_single_line_inline_metadata_to_the_flow() {
    let ast = parsed(
        "agentflow-beta TB\n  flow f@{ model: \"claude-opus-4\", memory: \"shared\" }\n    a --> b\n  end",
    );
    let metadata = ast
        .sub_graph("f")
        .and_then(|sub_graph| sub_graph.metadata.as_ref())
        .expect("metadata");
    assert_eq!(metadata.get("model"), Some(&json!("claude-opus-4")));
    assert_eq!(metadata.get("memory"), Some(&json!("shared")));
}

#[test]
fn supports_the_labelled_inline_form_and_keeps_the_title() {
    let ast = parsed(
        "agentflow-beta TB\n  flow f[\"Article Generation\"]@{ model: \"claude-opus-4\" }\n    a --> b\n  end",
    );
    let sub_graph = ast.sub_graph("f").expect("flow");
    assert_eq!(sub_graph.title, "Article Generation");
    assert_eq!(
        sub_graph
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("model")),
        Some(&json!("claude-opus-4"))
    );
}

#[test]
fn carries_flow_metadata_verbatim_without_validation() {
    let ast = parsed(
        "agentflow-beta TB\n  flow f@{ model: \"m\", memory: \"shared\", execution: \"parallel\", custom: 1 }\n    a --> b\n  end",
    );
    let metadata = ast
        .sub_graph("f")
        .and_then(|sub_graph| sub_graph.metadata.as_ref())
        .expect("metadata");
    assert_eq!(metadata.get("model"), Some(&json!("m")));
    assert_eq!(metadata.get("memory"), Some(&json!("shared")));
    assert_eq!(metadata.get("execution"), Some(&json!("parallel")));
    assert_eq!(metadata.get("custom"), Some(&json!(1)));
    assert!(ast.diagnostics.is_empty());
}

// ─── agentflow-flow-self-reference.spec.ts ───────────────────────────────

#[test]
fn a_flow_is_not_a_member_of_itself() {
    let ast = parsed(
        "agentflow-beta TB\n  flow myFlow[\"My Flow\"]\n    a[\"Task A\"]\n    a --> myFlow\n  end",
    );
    let sub_graph = ast.sub_graph("myFlow").expect("flow");
    assert!(sub_graph.nodes.contains(&"a".to_string()));
    assert!(!sub_graph.nodes.contains(&"myFlow".to_string()));
}

#[test]
fn a_self_referencing_flow_has_no_self_parent() {
    let ast = parsed(
        "agentflow-beta TB\n  flow myFlow[\"My Flow\"]\n    a[\"Task A\"]\n    a --> myFlow\n  end",
    );
    assert_eq!(node(&ast, "myFlow").parent_id, None);
}

#[test]
fn a_self_referencing_flow_keeps_its_task_and_edge() {
    let ast = parsed(
        "agentflow-beta TB\n  flow myFlow[\"My Flow\"]\n    a[\"Task A\"]\n    a --> myFlow\n  end",
    );
    assert_eq!(node(&ast, "a").parent_id, Some("myFlow".to_string()));
    assert!(
        ast.layout_edges
            .iter()
            .any(|edge| edge.start == "a" && edge.end == "myFlow")
    );
}

#[test]
fn a_nested_flow_referencing_itself_keeps_its_parent() {
    let ast = parsed(
        "agentflow-beta TB\n  flow outer[\"Outer\"]\n    flow inner[\"Inner\"]\n      b[\"Task B\"]\n      b --> inner\n    end\n  end",
    );
    assert!(
        !ast.sub_graph("inner")
            .expect("inner")
            .nodes
            .contains(&"inner".to_string())
    );
    assert_eq!(node(&ast, "inner").parent_id, Some("outer".to_string()));
}

#[test]
fn a_mid_level_self_reference_keeps_the_whole_chain() {
    let ast = parsed(
        "agentflow-beta TB\n  flow top[\"Top\"]\n    flow mid[\"Mid\"]\n      flow leafFlow[\"Leaf\"]\n        c[\"Task C\"]\n      end\n      m[\"Task M\"]\n      m --> mid\n    end\n  end",
    );
    assert!(
        !ast.sub_graph("mid")
            .expect("mid")
            .nodes
            .contains(&"mid".to_string())
    );
    assert_eq!(node(&ast, "leafFlow").parent_id, Some("mid".to_string()));
    assert_eq!(node(&ast, "mid").parent_id, Some("top".to_string()));
    assert_eq!(node(&ast, "top").parent_id, None);
    assert_eq!(node(&ast, "m").parent_id, Some("mid".to_string()));
    assert_eq!(node(&ast, "c").parent_id, Some("leafFlow".to_string()));
}

#[test]
fn self_references_at_two_levels_are_both_dropped() {
    let ast = parsed(
        "agentflow-beta TB\n  flow outer[\"Outer\"]\n    flow inner[\"Inner\"]\n      b[\"Task B\"]\n      b --> inner\n    end\n    a[\"Task A\"]\n    a --> outer\n  end",
    );
    assert!(
        !ast.sub_graph("outer")
            .expect("outer")
            .nodes
            .contains(&"outer".to_string())
    );
    assert!(
        !ast.sub_graph("inner")
            .expect("inner")
            .nodes
            .contains(&"inner".to_string())
    );
    assert_eq!(node(&ast, "outer").parent_id, None);
    assert_eq!(node(&ast, "inner").parent_id, Some("outer".to_string()));
    assert!(
        ast.layout_edges
            .iter()
            .any(|edge| edge.start == "b" && edge.end == "inner")
    );
    assert!(
        ast.layout_edges
            .iter()
            .any(|edge| edge.start == "a" && edge.end == "outer")
    );
}

#[test]
fn normal_flow_membership_is_untouched() {
    let ast = parsed("agentflow-beta TB\n  flow f[\"F\"]\n    a --> b\n  end");
    let sub_graph = ast.sub_graph("f").expect("flow");
    assert!(sub_graph.nodes.contains(&"a".to_string()));
    assert!(sub_graph.nodes.contains(&"b".to_string()));
    assert_eq!(node(&ast, "a").parent_id, Some("f".to_string()));
    assert_eq!(node(&ast, "f").parent_id, None);
}

// ─── agentflow-inline-metadata-mapping.spec.ts ───────────────────────────

#[test]
fn inline_metadata_spans_the_block_for_a_vertex() {
    let ast = parsed(
        "agentflow-beta TB\n  flow f[\"F\"]\n    a[\"Alpha\"]@{\n      instruction: \"do the thing\"\n    }\n  end",
    );
    let a = mapping(&ast, "a");
    assert_eq!(a.type_name, MappingType::Vertex);
    assert_eq!(a.position.start_line, 3);
    assert_eq!(a.position.end_line, 5);
    let hit = ast.element_at_position(4, 8).expect("hit");
    assert_eq!(hit.id, "a");
    assert_eq!(hit.type_name, MappingType::Vertex);
}

#[test]
fn inline_metadata_on_an_edge_destination_spans_the_block() {
    let ast =
        parsed("agentflow-beta TB\n  x --> b[\"Beta\"]@{\n    instruction: \"hand off\"\n  }");
    let b = mapping(&ast, "b");
    assert_eq!(b.type_name, MappingType::Vertex);
    assert_eq!(b.position.start_line, 2);
    assert_eq!(b.position.end_line, 4);
    assert_eq!(
        ast.element_at_position(3, 6).map(|hit| hit.id.as_str()),
        Some("b")
    );
}

#[test]
fn single_line_inline_metadata_keeps_the_declaration_line() {
    let ast = parsed("agentflow-beta TB\n  a[\"A\"]\n  b[\"B\"]@{ shape: rect }\n  a --> b");
    let b = mapping(&ast, "b");
    assert_eq!(b.position.start_line, 3);
    assert_eq!(b.position.end_line, 3);
}

#[test]
fn a_node_without_inline_metadata_keeps_its_declaration_line() {
    let ast = parsed("agentflow-beta TB\n  a[\"Alpha\"]\n  a --> b");
    let a = mapping(&ast, "a");
    assert_eq!(a.position.start_line, 2);
    assert_eq!(a.position.end_line, 2);
}

// ─── agentflow-mappings.spec.ts ──────────────────────────────────────────

#[test]
fn a_fresh_parse_starts_with_no_mappings() {
    let ast = parsed("agentflow-beta TB\n");
    assert!(ast.element_mappings.is_empty());
    assert_eq!(ast.mapping_stats().total_elements, 0);
}

#[test]
fn captures_a_position_for_a_labelled_vertex() {
    let ast = parsed("agentflow-beta TB\n  a[\"Alpha\"]\n  b --> a");
    let a = mapping(&ast, "a");
    assert_eq!(a.type_name, MappingType::Vertex);
    assert_eq!(a.position.start_line, 2);
    assert_eq!(a.position.end_line, 2);
}

#[test]
fn captures_vertices_for_inline_shapes() {
    let ast =
        parsed("agentflow-beta TB\n  a[\"sq\"]\n  c{\"diamond\"}\n  d[/\"trap\"/]\n  f --> a");
    for id in ["a", "c", "d"] {
        let mapping = ast
            .element_by_id(id)
            .unwrap_or_else(|| panic!("missing {id}"));
        assert_eq!(mapping.type_name, MappingType::Vertex);
    }
}

#[test]
fn captures_an_edge_statement_position() {
    let ast = parsed("agentflow-beta TB\n  a --> b");
    let edges: Vec<&ElementMapping> = ast
        .element_mappings
        .iter()
        .filter(|mapping| mapping.type_name == MappingType::Edge)
        .collect();
    assert!(!edges.is_empty());
    assert!(edges.iter().any(|edge| edge.position.start_line == 2));
}

#[test]
fn captures_separate_edge_mappings() {
    let ast = parsed("agentflow-beta TB\n  a --> b\n  b --> c\n  c --> d");
    assert_eq!(
        ast.element_mappings
            .iter()
            .filter(|mapping| mapping.type_name == MappingType::Edge)
            .count(),
        3
    );
}

#[test]
fn captures_a_container_position() {
    let ast = parsed("agentflow-beta TB\n  flow pipeline[\"Pipeline\"]\n    a --> b\n  end");
    let pipeline = mapping(&ast, "pipeline");
    assert_eq!(pipeline.type_name, MappingType::SubGraph);
    assert_eq!(pipeline.position.start_line, 2);
    assert_eq!(pipeline.position.end_line, 4);
}

#[test]
fn captures_positions_for_nested_containers() {
    let ast = parsed(
        "agentflow-beta TB\n  flow outer[\"Outer\"]\n    flow inner[\"Inner\"]\n      n1 --> n2\n    end\n  end",
    );
    for id in ["outer", "inner"] {
        assert_eq!(
            ast.element_by_id(id)
                .unwrap_or_else(|| panic!("missing {id}"))
                .type_name,
            MappingType::SubGraph
        );
    }
}

#[test]
fn shifts_positions_by_the_frontmatter_offset() {
    let ast = parse_with_frontmatter_offset("agentflow-beta TB\n  a --> b", 3).expect("parse");
    let a = mapping(&ast, "a");
    assert_eq!(a.position.start_line, 5);
    assert_eq!(a.position.end_line, 5);
}

#[test]
fn reports_unshifted_positions_without_an_offset() {
    let ast = parsed("agentflow-beta TB\n  a --> b");
    assert_eq!(mapping(&ast, "a").position.start_line, 2);
}

#[test]
fn elements_on_line_returns_every_intersection() {
    let ast = parsed("agentflow-beta TB\n  a --> b\n  c --> d");
    let on_line = ast.elements_on_line(2);
    assert!(on_line.len() >= 2);
    assert!(
        on_line
            .iter()
            .any(|mapping| mapping.type_name == MappingType::Vertex && mapping.id == "a")
    );
}

#[test]
fn element_at_position_prefers_the_innermost_span() {
    let ast = parsed("agentflow-beta TB\n  flow pipeline[\"Pipeline\"]\n    inner --> next\n  end");
    let inner = mapping(&ast, "inner");
    let hit = ast
        .element_at_position(inner.position.start_line, inner.position.start_column)
        .expect("hit");
    assert_eq!(hit.id, "inner");
    assert_eq!(hit.type_name, MappingType::Vertex);
}

#[test]
fn mapping_stats_report_counts_per_statement_type() {
    let ast = parsed("agentflow-beta TB\n  flow p[\"P\"]\n    a --> b\n  end");
    let stats = ast.mapping_stats();
    assert!(stats.vertices >= 2);
    assert!(stats.edges >= 1);
    assert_eq!(stats.subgraphs, 1);
    assert_eq!(
        stats.total_elements,
        stats.vertices + stats.edges + stats.subgraphs + stats.connectors + stats.attachments
    );
}

// ─── agentflow-metadata-no-validation.spec.ts ────────────────────────────

#[test]
fn keeps_an_unknown_vertex_key_without_warning() {
    let ast = parsed("agentflow-beta TB\n  a[\"A\"]\n  a@{ myExtension: 42 }");
    assert_eq!(
        vertex(&ast, "a")
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("myExtension")),
        Some(&json!(42))
    );
    assert!(ast.diagnostics.is_empty());
}

#[test]
fn keeps_an_unknown_flow_key_without_warning() {
    let ast = parsed(
        "agentflow-beta TB\n  flow f[\"F\"]\n    a --> b\n  end\n  f@{ totallyCustomKey: \"opaque\" }",
    );
    assert_eq!(
        ast.sub_graph("f")
            .and_then(|sub_graph| sub_graph.metadata.as_ref())
            .and_then(|metadata| metadata.get("totallyCustomKey")),
        Some(&json!("opaque"))
    );
    assert!(ast.diagnostics.is_empty());
}

#[test]
fn keeps_a_misapplied_key_without_warning() {
    let ast = parsed(
        "agentflow-beta TB\n  flow f[\"F\"]\n    a --> b\n  end\n  f@{ execution: \"parallel\" }",
    );
    assert_eq!(
        ast.sub_graph("f")
            .and_then(|sub_graph| sub_graph.metadata.as_ref())
            .and_then(|metadata| metadata.get("execution")),
        Some(&json!("parallel"))
    );
    assert!(ast.diagnostics.is_empty());
}

#[test]
fn carries_arbitrary_edge_metadata() {
    let ast =
        parsed("agentflow-beta TB\n  a e1@--> b\n  e1@{ instruction: \"hand off\", weight: 5 }");
    let metadata = ast
        .edge("e1")
        .and_then(|edge| edge.metadata.as_ref())
        .expect("metadata");
    assert_eq!(metadata.get("instruction"), Some(&json!("hand off")));
    assert_eq!(metadata.get("weight"), Some(&json!(5)));
    assert!(ast.diagnostics.is_empty());
}

#[test]
fn carries_prompt_as_an_ordinary_key() {
    let ast = parsed("agentflow-beta TB\n  a[\"A\"]\n  a@{ prompt: \"do the thing\" }");
    let metadata = vertex(&ast, "a").metadata.as_ref().expect("metadata");
    assert_eq!(metadata.get("prompt"), Some(&json!("do the thing")));
    assert_eq!(metadata.get("instruction"), None);
    assert!(ast.diagnostics.is_empty());
}

#[test]
fn does_not_resolve_connector_refs() {
    let ast = parsed(
        "agentflow-beta TB\n  call[\"call\"]@{ shape: action, connectorRef: \"nonexistent.op\" }\n  input[\"in\"]@{ shape: input }\n  input --> call",
    );
    assert_eq!(
        vertex(&ast, "call")
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("connectorRef")),
        Some(&json!("nonexistent.op"))
    );
    assert!(ast.diagnostics.is_empty());
}

#[test]
fn does_not_warn_on_a_flow_with_no_input_node() {
    let ast = parsed("agentflow-beta TB\n  flow f[\"F\"]\n    a --> b\n  end");
    assert!(ast.diagnostics.is_empty());
}

#[test]
fn does_not_warn_on_a_duplicate_id() {
    let ast = parsed("agentflow-beta TB\n  a[\"First\"]\n  a[\"Second\"]\n  a --> b");
    assert!(ast.diagnostics.is_empty());
}

#[test]
fn does_not_warn_on_the_reserved_connectors_id() {
    let ast = parsed("agentflow-beta TB\n  connectors[\"My group\"]\n  connectors --> b");
    assert!(ast.diagnostics.is_empty());
}

#[test]
fn still_rejects_an_invalid_shape_name() {
    let error =
        parse("agentflow-beta TB\n  a[\"A\"]\n  a@{ shape: NotAShape }").expect_err("shape");
    assert!(error.message.contains("No such shape"), "{}", error.message);
}

#[test]
fn still_records_view_collapsed() {
    let ast = parsed(
        "agentflow-beta TB\n  flow f[\"F\"]\n    a --> b\n  end\n  f@{ view: \"collapsed\" }",
    );
    assert_eq!(
        ast.sub_graph("f")
            .and_then(|sub_graph| sub_graph.metadata.as_ref())
            .and_then(|metadata| metadata.get("view")),
        Some(&json!("collapsed"))
    );
}

// ─── agentflow-metadata-prototype-keys.spec.ts ───────────────────────────

#[test]
fn drops_an_authored_proto_key() {
    let ast =
        parsed("agentflow-beta TB\n  a[\"A\"]\n  a@{\n    __proto__:\n      polluted: true\n  }");
    let metadata = vertex(&ast, "a").metadata.clone().unwrap_or_default();
    assert!(!metadata.contains_key("__proto__"));
}

#[test]
fn drops_constructor_and_prototype_keys() {
    let ast = parsed(
        "agentflow-beta TB\n  a[\"A\"]\n  a@{ constructor: \"x\", prototype: \"y\", description: \"kept\" }",
    );
    let metadata = vertex(&ast, "a").metadata.clone().unwrap_or_default();
    let keys: Vec<&String> = metadata.keys().collect();
    assert_eq!(keys, ["description"]);
    assert_eq!(metadata.get("description"), Some(&json!("kept")));
}

#[test]
fn strips_nested_proto_keys_and_keeps_the_structure() {
    let ast = parsed(
        "agentflow-beta TB\n  a[\"A\"]\n  a@{\n    params:\n      - name: first\n        __proto__:\n          polluted: true\n    description: \"kept\"\n  }",
    );
    let metadata = vertex(&ast, "a").metadata.clone().unwrap_or_default();
    assert_eq!(metadata.get("description"), Some(&json!("kept")));
    let params = metadata
        .get("params")
        .and_then(|value| value.as_array())
        .expect("params");
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].get("name"), Some(&json!("first")));
    assert!(params[0].get("__proto__").is_none());
}

#[test]
fn leaves_ordinary_metadata_untouched() {
    let ast = parsed(
        "agentflow-beta TB\n  a[\"A\"]\n  a@{ shape: rounded, description: \"hello\", myExtension: 42 }",
    );
    let metadata = vertex(&ast, "a").metadata.clone().unwrap_or_default();
    assert_eq!(metadata.get("description"), Some(&json!("hello")));
    assert_eq!(metadata.get("myExtension"), Some(&json!(42)));
}

// ─── agentflow-metadata-trailing-comma.spec.ts ───────────────────────────

#[test]
fn inline_connector_with_every_line_comma_terminated() {
    let ast = parsed(
        "agentflow-beta TB\n  connector github[\"GitHub\"]@{\n    protocol: \"mcp\",\n    transport: \"stdio\",\n  }",
    );
    let metadata = ast.connectors()[0].metadata.as_ref().expect("metadata");
    assert_eq!(metadata.get("protocol"), Some(&json!("mcp")));
    assert_eq!(metadata.get("transport"), Some(&json!("stdio")));
}

#[test]
fn inline_connector_with_mixed_commas() {
    let ast = parsed(
        "agentflow-beta TB\n  connector github[\"GitHub\"]@{\n    protocol: \"mcp\",\n    endpoint: \"https://api.github.com\"\n    token_required: true,\n  }",
    );
    let metadata = ast.connectors()[0].metadata.as_ref().expect("metadata");
    assert_eq!(metadata.get("protocol"), Some(&json!("mcp")));
    assert_eq!(
        metadata.get("endpoint"),
        Some(&json!("https://api.github.com"))
    );
    assert_eq!(metadata.get("token_required"), Some(&json!(true)));
}

#[test]
fn standalone_vertex_attachment_with_trailing_commas() {
    let ast = parsed(
        "agentflow-beta TB\n  a[\"Alpha\"]\n  a@{\n    description: \"alpha node\",\n    instruction: \"be careful\",\n  }",
    );
    let metadata = vertex(&ast, "a").metadata.as_ref().expect("metadata");
    assert_eq!(metadata.get("description"), Some(&json!("alpha node")));
    assert_eq!(metadata.get("instruction"), Some(&json!("be careful")));
}

#[test]
fn flow_header_inline_metadata_with_trailing_commas() {
    let ast = parsed(
        "agentflow-beta TB\n  flow f[\"Flow\"]@{\n    model: \"claude-opus-4\",\n    memory: \"shared\",\n  }\n    a --> b\n  end",
    );
    let metadata = ast
        .sub_graph("f")
        .and_then(|sub_graph| sub_graph.metadata.as_ref())
        .expect("metadata");
    assert_eq!(metadata.get("model"), Some(&json!("claude-opus-4")));
    assert_eq!(metadata.get("memory"), Some(&json!("shared")));
}

#[test]
fn strips_a_comma_before_a_line_trailing_comment() {
    let ast = parsed(
        "agentflow-beta TB\n  connector github[\"GitHub\"]@{\n    protocol: \"mcp\", # main protocol\n    transport: \"stdio\"\n  }",
    );
    let metadata = ast.connectors()[0].metadata.as_ref().expect("metadata");
    assert_eq!(metadata.get("protocol"), Some(&json!("mcp")));
    assert_eq!(metadata.get("transport"), Some(&json!("stdio")));
}

#[test]
fn keeps_commas_inside_block_scalar_content() {
    let ast = parsed(
        "agentflow-beta TB\n  a[\"Alpha\"]\n  a@{\n    instruction: |\n      Do a thing,\n      then stop\n    protocol: \"mcp\",\n  }",
    );
    let metadata = vertex(&ast, "a").metadata.as_ref().expect("metadata");
    assert_eq!(
        metadata.get("instruction"),
        Some(&json!("Do a thing,\nthen stop\n"))
    );
    assert_eq!(metadata.get("protocol"), Some(&json!("mcp")));
}

#[test]
fn keeps_separator_commas_in_multiline_flow_sequences() {
    let ast = parsed(
        "agentflow-beta TB\n  a[\"Alpha\"]\n  a@{\n    tools: [\"read\",\n      \"write\"],\n    protocol: \"mcp\"\n  }",
    );
    let metadata = vertex(&ast, "a").metadata.as_ref().expect("metadata");
    assert_eq!(metadata.get("tools"), Some(&json!(["read", "write"])));
    assert_eq!(metadata.get("protocol"), Some(&json!("mcp")));
}

#[test]
fn keeps_a_comma_inside_a_multiline_quoted_scalar() {
    let ast = parsed(
        "agentflow-beta TB\n  a[\"Alpha\"]\n  a@{\n    note: 'first,\n      second',\n    protocol: \"mcp\"\n  }",
    );
    let metadata = vertex(&ast, "a").metadata.as_ref().expect("metadata");
    assert_eq!(metadata.get("note"), Some(&json!("first, second")));
    assert_eq!(metadata.get("protocol"), Some(&json!("mcp")));
}

#[test]
fn parses_a_full_diagram_around_a_trailing_comma_connector() {
    let ast = parsed(
        "agentflow-beta LR\n  connector github[\"GitHub API\"]@{\n    protocol: \"http\",\n    endpoint: \"https://api.github.com\",\n    token_required: true,\n  }\n\n  query[\"query\"]\n  query@{ shape: input }\n  fetch_issues[\"fetch_issues\"]\n  fetch_issues@{ shape: tool, connectorRef: \"github.list_issues\" }\n  query --> fetch_issues",
    );
    let metadata = ast.connectors()[0].metadata.as_ref().expect("metadata");
    assert_eq!(metadata.get("protocol"), Some(&json!("http")));
    assert_eq!(
        metadata.get("endpoint"),
        Some(&json!("https://api.github.com"))
    );
    assert_eq!(metadata.get("token_required"), Some(&json!(true)));
    assert_eq!(
        vertex(&ast, "fetch_issues")
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("connectorRef")),
        Some(&json!("github.list_issues"))
    );
    assert_eq!(ast.edges.len(), 1);
}

#[test]
fn single_line_form_tolerates_a_trailing_comma() {
    let ast = parsed("agentflow-beta TB\n  connector github[\"GitHub\"]@{ protocol: \"mcp\", }");
    assert_eq!(
        ast.connectors()[0]
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("protocol")),
        Some(&json!("mcp"))
    );
}

#[test]
fn invalid_yaml_that_commas_cannot_fix_still_throws() {
    assert!(
        parse("agentflow-beta TB\n  a[\"Alpha\"]\n  a@{\n    protocol: *undefined_alias,\n  }")
            .is_err()
    );
}

// ─── agentflow-subgraph-span.spec.ts ─────────────────────────────────────

#[test]
fn sibling_flows_do_not_overlap() {
    let ast = parsed(
        "agentflow-beta TB\n\n  flow f2[\"F2\"]\n    a --> b\n  end\n\n  flow f1[\"F1\"]\n    c --> d\n  end",
    );
    let f2 = mapping(&ast, "f2").position;
    let f1 = mapping(&ast, "f1").position;
    assert_eq!((f2.start_line, f2.end_line), (3, 5));
    assert_eq!((f1.start_line, f1.end_line), (7, 9));
    assert!(f2.end_line < f1.start_line);
}

#[test]
fn a_nested_inner_flow_ends_at_its_own_end() {
    let ast = parsed(
        "agentflow-beta TB\n  flow outer[\"Outer\"]\n    flow inner[\"Inner\"]\n      a --> b\n    end\n    c --> d\n  end",
    );
    let inner = mapping(&ast, "inner").position;
    let outer = mapping(&ast, "outer").position;
    assert_eq!((inner.start_line, inner.end_line), (3, 5));
    assert_eq!((outer.start_line, outer.end_line), (2, 7));
}

#[test]
fn a_flow_whose_end_is_the_last_line_maps() {
    let ast = parsed("agentflow-beta TB\n  flow f[\"F\"]\n    a --> b\n  end");
    let f = mapping(&ast, "f").position;
    assert_eq!((f.start_line, f.end_line), (2, 4));
    assert!(ast.vertex("a").is_some());
    assert!(ast.vertex("b").is_some());
}

#[test]
fn innermost_match_prefers_the_tighter_span() {
    let ast = parsed(
        "agentflow-beta TB\n  flow outer[\"Outer\"]\n    flow inner[\"Inner\"]\n      a[\"Alpha\"]\n    end\n  end",
    );
    assert_eq!(
        ast.element_at_position(4, 8).map(|hit| hit.id.as_str()),
        Some("a")
    );
    assert_eq!(
        ast.element_at_position(3, 5).map(|hit| hit.id.as_str()),
        Some("inner")
    );
}

#[test]
fn innermost_match_handles_a_label_wider_than_1000_columns() {
    let wide = "W".repeat(1200);
    let ast = parsed(&format!(
        "agentflow-beta TB\n  flow outer[\"Outer\"]\n    a[\"{wide}\"] --> b[\"B\"]\n  end"
    ));
    assert_eq!(
        ast.element_at_position(3, 10).map(|hit| hit.id.as_str()),
        Some("a")
    );
    assert_eq!(
        ast.element_at_position(3, 1216).map(|hit| hit.id.as_str()),
        Some("b")
    );
}

// ─── agentflow-tool-validators.spec.ts ───────────────────────────────────

#[test]
fn recognises_the_canonical_subroutine_shape() {
    let ast = parsed(
        "agentflow-beta TB\n  search[\"search\"]\n  search@{ shape: subroutine, returns: \"Result\" }",
    );
    assert!(ast.is_tool_definition(vertex(&ast, "search")));
}

#[test]
fn recognises_the_tool_alias() {
    let ast = parsed("agentflow-beta TB\n  search[\"search\"]\n  search@{ shape: tool }");
    assert!(ast.is_tool_definition(vertex(&ast, "search")));
}

#[test]
fn recognises_the_subprocess_alias() {
    let ast = parsed("agentflow-beta TB\n  search[\"search\"]\n  search@{ shape: subprocess }");
    assert!(ast.is_tool_definition(vertex(&ast, "search")));
}

#[test]
fn a_default_shape_is_not_a_tool() {
    let ast = parsed("agentflow-beta TB\n  plain[\"plain node\"]");
    assert!(!ast.is_tool_definition(vertex(&ast, "plain")));
}

#[test]
fn input_and_action_nodes_are_not_tools() {
    let ast = parsed(
        "agentflow-beta TB\n  i[\"i\"]\n  i@{ shape: input }\n  h[\"h\"]\n  h@{ shape: action }",
    );
    assert!(!ast.is_tool_definition(vertex(&ast, "i")));
    assert!(!ast.is_tool_definition(vertex(&ast, "h")));
}

#[test]
fn tools_returns_every_tool_shaped_vertex() {
    let ast = parsed(
        "agentflow-beta TB\n  a[\"a\"]\n  a@{ shape: subroutine }\n  b[\"b\"]\n  b@{ shape: tool }\n  c[\"c\"]\n  d[\"d\"]\n  d@{ shape: framed-rectangle }",
    );
    let mut tools: Vec<&str> = ast
        .tools()
        .iter()
        .map(|vertex| vertex.id.as_str())
        .collect();
    tools.sort();
    assert_eq!(tools, ["a", "b", "d"]);
}

#[test]
fn semantic_model_surfaces_tool_kind_and_metadata() {
    let ast = parsed(
        "agentflow-beta TB\n  search[\"search\"]\n  search@{ shape: tool, returns: \"Result\" }",
    );
    let model = ast.semantic_model();
    let search = model
        .vertices
        .iter()
        .find(|vertex| vertex.id == "search")
        .expect("vertex");
    assert_eq!(search.vertex_kind, VertexKind::Tool);
    assert_eq!(
        search
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("returns")),
        Some(&json!("Result"))
    );
}

#[test]
fn alias_shaped_tools_surface_tool_kind() {
    let ast = parsed(
        "agentflow-beta TB\n  a[\"a\"]\n  a@{ shape: subprocess }\n  b[\"b\"]\n  b@{ shape: framed-rectangle }",
    );
    let model = ast.semantic_model();
    assert_eq!(
        model
            .vertices
            .iter()
            .find(|vertex| vertex.id == "a")
            .expect("a")
            .vertex_kind,
        VertexKind::Tool
    );
    assert_eq!(
        model
            .vertices
            .iter()
            .find(|vertex| vertex.id == "b")
            .expect("b")
            .vertex_kind,
        VertexKind::Tool
    );
}

#[test]
fn tool_retry_and_cache_metadata_survive() {
    let ast = parsed(
        "agentflow-beta TB\n  search[\"search\"]\n  search@{ shape: tool, retry: 3, cache: \"24h\", returns: \"Result\" }",
    );
    let metadata = vertex(&ast, "search").metadata.as_ref().expect("metadata");
    assert_eq!(metadata.get("retry"), Some(&json!(3)));
    assert_eq!(metadata.get("cache"), Some(&json!("24h")));
    assert_eq!(metadata.get("returns"), Some(&json!("Result")));
}

// ─── agentflow-yaml-error-position.spec.ts ───────────────────────────────
//
// Upstream's js-yaml excerpt gutter, `mark`, and `hash.loc` are not reproduced: this crate's
// error carries a `Span`. The cases below assert the source line the span names.

#[test]
fn a_single_line_block_error_reports_its_source_line() {
    let error =
        parse("agentflow-beta TB\n\n  content_ref@{ shape: procs,* type: \"BilingualContent\" }")
            .expect_err("YAML error");
    assert_eq!(error.span.expect("span").line, 3);
    assert!(error.message.contains("alias"), "{}", error.message);
}

#[test]
fn a_multiline_block_error_reports_the_body_line() {
    let error = parse(
        "agentflow-beta\nflow f[\"F\"]\n  a[\"A\"]\n  b[\"B\"]\n  c[\"C\"]@{\n\tshape: rounded\n  }\nend",
    )
    .expect_err("shape error");
    assert_eq!(error.span.expect("span").line, 6);
}

#[test]
fn an_error_on_the_block_line_maps_to_the_block() {
    let error =
        parse("agentflow-beta TB\n  a[\"A\"]@{ x: *,\n    y: 1 }").expect_err("alias error");
    assert_eq!(error.span.expect("span").line, 2);
}

#[test]
fn a_block_below_a_blank_keeps_its_source_line() {
    let error = parse(
        "agentflow-beta TB\n  a[\"A\"]@{ shape: rounded }\n\n  b[\"B\"]@{ shape: procs,* x: 1 }",
    )
    .expect_err("alias error");
    assert_eq!(error.span.expect("span").line, 4);
}

#[test]
fn a_shape_error_reports_the_block_line() {
    let error = parse("agentflow-beta TB\n  a[\"A\"]@{ shape: Rounded }").expect_err("shape error");
    assert!(error.message.contains("No such shape"), "{}", error.message);
    assert_eq!(error.span.expect("span").line, 2);
}

#[test]
fn a_frontmatter_offset_applies_to_errors_too() {
    let error =
        parse_with_frontmatter_offset("agentflow-beta TB\n  a[\"A\"]@{ shape: Rounded }", 3)
            .expect_err("shape error");
    assert!(error.message.contains("No such shape"), "{}", error.message);
}

#[test]
fn a_well_formed_block_does_not_throw() {
    let ast = parsed("agentflow-beta TB\n  a[\"A\"]@{ shape: rounded }\n\n  b[\"B\"]");
    assert!(ast.vertex("b").is_some());
}

// ─── grammar surface not covered by the parser specs ─────────────────────

#[test]
fn parses_a_pipe_label_after_the_arrow() {
    let ast = parsed("agentflow-beta TB\n  a -->|ready| b");
    assert_eq!(ast.edges[0].text, "ready");
    assert_eq!(ast.edges[0].label_type, LabelType::Text);
}

#[test]
fn parses_every_inline_vertex_shape() {
    for (source, expected) in [
        ("a[\"square\"]", "square"),
        ("a((\"circle\"))", "circle"),
        ("a(\"round\")", "round"),
        ("a(-ellipse-)", "ellipse"),
        ("a([\"stadium\"])", "stadium"),
        ("a[[\"subroutine\"]]", "subroutine"),
        ("a[(\"cylinder\")]", "cylinder"),
        ("a{\"diamond\"}", "diamond"),
        ("a{{\"hexagon\"}}", "hexagon"),
        ("a>odd]", "odd"),
        ("a[/trap\\]", "trapezoid"),
        ("a[/lean/]", "lean_right"),
        ("a[\\inv/]", "inv_trapezoid"),
        ("a[\\lean\\]", "lean_left"),
    ] {
        let ast = parsed(&format!("agentflow-beta TB\n  {source}"));
        assert_eq!(
            vertex(&ast, "a").type_name.as_deref(),
            Some(expected),
            "source: {source}"
        );
    }
}

#[test]
fn parses_vertex_props() {
    let ast = parsed("agentflow-beta TB\n  a[|field:value|label]");
    assert_eq!(vertex(&ast, "a").props.get("field"), Some(&json!("value")));
    assert_eq!(vertex(&ast, "a").text.as_deref(), Some("label"));
}

#[test]
fn parses_click_link_and_marks_the_node_clickable() {
    let ast = parsed("agentflow-beta TB\n  a[\"A\"]\n  click a href \"https://example.com\"");
    let a = vertex(&ast, "a");
    assert_eq!(a.link.as_deref(), Some("https://example.com"));
    assert!(a.classes.iter().any(|class| class == "clickable"));
}

#[test]
fn parses_click_callback() {
    let ast = parsed("agentflow-beta TB\n  a[\"A\"]\n  click a call handler()");
    assert!(
        vertex(&ast, "a")
            .classes
            .iter()
            .any(|class| class == "clickable")
    );
}

#[test]
fn parses_style_and_class_statements() {
    let ast = parsed(
        "agentflow-beta TB\n  a[\"A\"]\n  style a fill:#f9f\n  classDef pretty fill:#333\n  class a pretty",
    );
    assert_eq!(vertex(&ast, "a").styles, ["fill:#f9f"]);
    assert_eq!(
        ast.class("pretty").map(|class| class.styles.clone()),
        Some(vec!["fill:#333".to_string()])
    );
    assert!(
        vertex(&ast, "a")
            .classes
            .iter()
            .any(|class| class == "pretty")
    );
}

#[test]
fn parses_link_style_and_rejects_an_out_of_bounds_index() {
    let ast = parsed("agentflow-beta TB\n  a --> b\n  linkStyle default stroke:#f00");
    assert_eq!(ast.default_style, Some(vec!["stroke:#f00".to_string()]));
    let error = parse("agentflow-beta TB\n  a --> b\n  linkStyle 3 stroke:#f00")
        .expect_err("out of bounds");
    assert!(error.message.contains("out of bounds"), "{}", error.message);
}

#[test]
fn parses_accessibility_statements() {
    let ast = parsed("agentflow-beta TB\n  accTitle: The flow\n  accDescr: A description");
    assert_eq!(ast.acc_title, "The flow");
    assert_eq!(ast.acc_description, "A description");
}
