//! Port of `parser/flow.spec.js`.

use super::{ast, cleanup_comments, edge, text, vertex};

#[test]
fn should_handle_trailing_whitespace_after_statements() {
    let ast = ast(&cleanup_comments(
        "graph TD;\n\n\n %% Comment\n A-->B; \n B-->C;",
    ));
    assert_eq!(vertex(&ast, "A").id, "A");
    assert_eq!(vertex(&ast, "B").id, "B");
    assert_eq!(ast.edges.len(), 2);
    assert_eq!(edge(&ast, 0).start, "A");
    assert_eq!(edge(&ast, 0).end, "B");
    assert_eq!(
        edge(&ast, 0).type_name.unwrap(),
        super::super::ArrowType::Point
    );
    assert_eq!(edge(&ast, 0).text, "");
}

#[test]
fn should_accept_swimlane_beta_as_a_graph_keyword() {
    let ast = ast("swimlane-beta LR;A-->B;");
    assert_eq!(ast.direction.as_deref(), Some("LR"));
    assert_eq!(vertex(&ast, "A").id, "A");
    assert_eq!(vertex(&ast, "B").id, "B");
    assert_eq!(ast.edges.len(), 1);
    assert_eq!(edge(&ast, 0).start, "A");
    assert_eq!(edge(&ast, 0).end, "B");
}

#[test]
fn should_handle_node_names_with_end_substring() {
    let ast = ast("graph TD\nendpoint --> sender");
    assert_eq!(vertex(&ast, "endpoint").id, "endpoint");
    assert_eq!(vertex(&ast, "sender").id, "sender");
    assert_eq!(edge(&ast, 0).start, "endpoint");
    assert_eq!(edge(&ast, 0).end, "sender");
}

#[test]
fn should_handle_node_names_ending_with_keywords() {
    let ast = ast("graph TD\nblend --> monograph");
    assert_eq!(vertex(&ast, "blend").id, "blend");
    assert_eq!(vertex(&ast, "monograph").id, "monograph");
    assert_eq!(edge(&ast, 0).start, "blend");
    assert_eq!(edge(&ast, 0).end, "monograph");
}

#[test]
fn should_allow_default_in_the_node_name() {
    let ast = ast("graph TD\ndefault --> monograph");
    assert_eq!(vertex(&ast, "default").id, "default");
    assert_eq!(vertex(&ast, "monograph").id, "monograph");
    assert_eq!(edge(&ast, 0).start, "default");
    assert_eq!(edge(&ast, 0).end, "monograph");
}

#[test]
fn should_handle_special_characters_inside_a_round_label() {
    let cases = [
        (".", "."),
        ("Start 103a.a1", "Start 103a.a1"),
        (":", ":"),
        (",", ","),
        ("a-b", "a-b"),
        ("+", "+"),
        ("*", "*"),
        ("<", "&lt;"),
        ("&", "&"),
    ];
    for (input, expected) in cases {
        let body = format!("graph TD;A({input})-->B;");
        let ast = ast(&body);
        assert_eq!(vertex(&ast, "A").id, "A");
        assert_eq!(vertex(&ast, "B").id, "B");
        assert_eq!(text(&ast, "A"), expected, "{body}");
    }
}

#[test]
fn should_allow_direction_in_node_ids() {
    let node_direction = ast("graph TD;\n  node1TB\n");
    assert_eq!(vertex(&node_direction, "node1TB").id, "node1TB");
    let chart = ast("graph TD;A--x|text including URL space|B;");
    assert_eq!(vertex(&chart, "A").id, "A");
}

#[test]
fn should_allow_numbers_as_labels() {
    let ast = ast("graph TB;subgraph \"number as labels\";1;end;");
    assert_eq!(vertex(&ast, "1").id, "1");
}

#[test]
fn should_add_acc_title_and_description() {
    let ast = ast(
        "graph LR\n      accTitle: Big decisions\n      accDescr: Flow chart of the decision making process\n      A[Hard] -->|Text| B(Round)\n      B --> C{Decision}\n      C -->|One| D[Result 1]\n      C -->|Two| E[Result 2]\n      ",
    );
    assert_eq!(ast.acc_title, "Big decisions");
    assert_eq!(
        ast.acc_description,
        "Flow chart of the decision making process"
    );
}

#[test]
fn should_add_acc_title_and_multiline_acc_description() {
    let ast = ast(
        "graph LR\n      accTitle: Big decisions\n\n      accDescr {\n        Flow chart of the decision making process\n        with a second line\n      }\n\n      A[Hard] -->|Text| B(Round)\n      B --> C{Decision}\n      C -->|One| D[Result 1]\n      C -->|Two| E[Result 2]\n",
    );
    assert_eq!(ast.acc_title, "Big decisions");
    assert_eq!(
        ast.acc_description,
        "Flow chart of the decision making process\nwith a second line"
    );
}

#[test]
fn should_work_with_unsafe_property_names() {
    for unsafe_prop in ["__proto__", "constructor"] {
        ast(&format!("graph LR\n      {unsafe_prop} --> A;"));
        ast(&format!(
            "graph LR\n      click {unsafe_prop} callback \"{unsafe_prop}\";"
        ));
        ast(&format!(
            "graph LR\n      {unsafe_prop} --> A;\n      classDef {unsafe_prop} color:#ffffff,fill:#000000;\n      class {unsafe_prop} {unsafe_prop};"
        ));
        ast(&format!(
            "graph LR\n      {unsafe_prop} --> A;\n      subgraph {unsafe_prop}\n        C --> D;\n      end;"
        ));
    }
}

#[test]
fn errors_carry_a_span() {
    let error = super::parse_error("graph TD\nA-->");
    let span = error.span.expect("a span");
    assert_eq!((span.line, span.column), (2, 5));
    assert!(error.to_string().starts_with("2:5:"));
}
