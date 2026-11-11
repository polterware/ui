//! Port of `parser/flow-edges.spec.js`.

use super::super::{ArrowType, Stroke};
use super::{ast, edge, parse_error, vertex};

const KEYWORDS: [&str; 19] = [
    "graph",
    "flowchart",
    "flowchart-elk",
    "style",
    "default",
    "linkStyle",
    "interpolate",
    "classDef",
    "class",
    "href",
    "call",
    "click",
    "_self",
    "_blank",
    "_parent",
    "_top",
    "end",
    "subgraph",
    "kitty",
];

struct EdgeType {
    start: &'static str,
    end: &'static str,
    stroke: Stroke,
    type_name: ArrowType,
}

const DOUBLE_ENDED: [EdgeType; 9] = [
    EdgeType {
        start: "x--",
        end: "--x",
        stroke: Stroke::Normal,
        type_name: ArrowType::DoubleCross,
    },
    EdgeType {
        start: "x==",
        end: "==x",
        stroke: Stroke::Thick,
        type_name: ArrowType::DoubleCross,
    },
    EdgeType {
        start: "x-.",
        end: ".-x",
        stroke: Stroke::Dotted,
        type_name: ArrowType::DoubleCross,
    },
    EdgeType {
        start: "o--",
        end: "--o",
        stroke: Stroke::Normal,
        type_name: ArrowType::DoubleCircle,
    },
    EdgeType {
        start: "o==",
        end: "==o",
        stroke: Stroke::Thick,
        type_name: ArrowType::DoubleCircle,
    },
    EdgeType {
        start: "o-.",
        end: ".-o",
        stroke: Stroke::Dotted,
        type_name: ArrowType::DoubleCircle,
    },
    EdgeType {
        start: "<--",
        end: "-->",
        stroke: Stroke::Normal,
        type_name: ArrowType::DoublePoint,
    },
    EdgeType {
        start: "<==",
        end: "==>",
        stroke: Stroke::Thick,
        type_name: ArrowType::DoublePoint,
    },
    EdgeType {
        start: "<-.",
        end: ".->",
        stroke: Stroke::Dotted,
        type_name: ArrowType::DoublePoint,
    },
];

const REGULAR: [EdgeType; 18] = [
    EdgeType {
        start: "--",
        end: "--x",
        stroke: Stroke::Normal,
        type_name: ArrowType::Cross,
    },
    EdgeType {
        start: "==",
        end: "==x",
        stroke: Stroke::Thick,
        type_name: ArrowType::Cross,
    },
    EdgeType {
        start: "-.",
        end: ".-x",
        stroke: Stroke::Dotted,
        type_name: ArrowType::Cross,
    },
    EdgeType {
        start: "--",
        end: "--o",
        stroke: Stroke::Normal,
        type_name: ArrowType::Circle,
    },
    EdgeType {
        start: "==",
        end: "==o",
        stroke: Stroke::Thick,
        type_name: ArrowType::Circle,
    },
    EdgeType {
        start: "-.",
        end: ".-o",
        stroke: Stroke::Dotted,
        type_name: ArrowType::Circle,
    },
    EdgeType {
        start: "--",
        end: "-->",
        stroke: Stroke::Normal,
        type_name: ArrowType::Point,
    },
    EdgeType {
        start: "==",
        end: "==>",
        stroke: Stroke::Thick,
        type_name: ArrowType::Point,
    },
    EdgeType {
        start: "-.",
        end: ".->",
        stroke: Stroke::Dotted,
        type_name: ArrowType::Point,
    },
    EdgeType {
        start: "--",
        end: "----x",
        stroke: Stroke::Normal,
        type_name: ArrowType::Cross,
    },
    EdgeType {
        start: "==",
        end: "====x",
        stroke: Stroke::Thick,
        type_name: ArrowType::Cross,
    },
    EdgeType {
        start: "-.",
        end: "...-x",
        stroke: Stroke::Dotted,
        type_name: ArrowType::Cross,
    },
    EdgeType {
        start: "--",
        end: "----o",
        stroke: Stroke::Normal,
        type_name: ArrowType::Circle,
    },
    EdgeType {
        start: "==",
        end: "====o",
        stroke: Stroke::Thick,
        type_name: ArrowType::Circle,
    },
    EdgeType {
        start: "-.",
        end: "...-o",
        stroke: Stroke::Dotted,
        type_name: ArrowType::Circle,
    },
    EdgeType {
        start: "--",
        end: "---->",
        stroke: Stroke::Normal,
        type_name: ArrowType::Point,
    },
    EdgeType {
        start: "==",
        end: "====>",
        stroke: Stroke::Thick,
        type_name: ArrowType::Point,
    },
    EdgeType {
        start: "-.",
        end: "...->",
        stroke: Stroke::Dotted,
        type_name: ArrowType::Point,
    },
];

#[test]
fn should_handle_open_ended_edges() {
    let ast = ast("graph TD;A---B;");
    assert_eq!(edge(&ast, 0).type_name.unwrap(), ArrowType::Open);
}

#[test]
fn should_handle_cross_ended_edges() {
    let ast = ast("graph TD;A--xB;");
    assert_eq!(edge(&ast, 0).type_name.unwrap(), ArrowType::Cross);
}

#[test]
fn should_handle_circle_ended_edges() {
    let ast = ast("graph TD;A--oB;");
    assert_eq!(edge(&ast, 0).type_name.unwrap(), ArrowType::Circle);
}

#[test]
fn should_handle_open_ended_edges_with_ids_and_labels() {
    for edge_type in &REGULAR {
        let body = format!(
            "flowchart TD;\nA e1@{}{} B;",
            edge_type.start, edge_type.end
        );
        let ast = ast(&body);
        assert_eq!(vertex(&ast, "A").id, "A");
        assert_eq!(vertex(&ast, "B").id, "B");
        assert_eq!(ast.edges.len(), 1, "{body}");
        assert_eq!(edge(&ast, 0).id.as_deref(), Some("e1"), "{body}");
        assert_eq!(edge(&ast, 0).start, "A");
        assert_eq!(edge(&ast, 0).end, "B");
        assert_eq!(
            edge(&ast, 0).type_name.unwrap(),
            edge_type.type_name,
            "{body}"
        );
        assert_eq!(edge(&ast, 0).text, "", "{body}");
        assert_eq!(edge(&ast, 0).stroke.unwrap(), edge_type.stroke, "{body}");
    }
}

#[test]
fn should_handle_normal_edges_where_a_node_also_has_metadata() {
    let ast = ast("flowchart LR\nA id1@-->B\nA@{ shape: 'rect' }\n");
    assert_eq!(edge(&ast, 0).id.as_deref(), Some("id1"));
}

#[test]
fn should_handle_double_ended_edges_with_ids_and_labels() {
    for edge_type in &DOUBLE_ENDED {
        let body = format!(
            "flowchart TD;\nA e1@{} label {} B;",
            edge_type.start, edge_type.end
        );
        let ast = ast(&body);
        assert_eq!(vertex(&ast, "A").id, "A");
        assert_eq!(vertex(&ast, "B").id, "B");
        assert_eq!(ast.edges.len(), 1, "{body}");
        assert_eq!(edge(&ast, 0).id.as_deref(), Some("e1"), "{body}");
        assert_eq!(edge(&ast, 0).start, "A");
        assert_eq!(edge(&ast, 0).end, "B");
        assert_eq!(
            edge(&ast, 0).type_name.unwrap(),
            edge_type.type_name,
            "{body}"
        );
        assert_eq!(edge(&ast, 0).text, "label", "{body}");
        assert_eq!(edge(&ast, 0).stroke.unwrap(), edge_type.stroke, "{body}");
    }
}

#[test]
fn should_handle_edge_ids_attached_to_the_preceding_node() {
    let cases = [
        "A[bla] e1@--> B",
        "A[bla]e1@--> B",
        "A@{shape:circle, label: bla}e1@--> B",
        "A@{shape:circle, label: bla} e1@--> B",
        "A@{shape:circle, }e1@--> B",
        "A@{shape:circle,}e1@--> B",
        "A@{shape:circle}e1@--> B",
        "A(bla)e1@--> B",
        "A{bla}e1@--> B",
        "A((bla))e1@--> B",
        "A>bla]e1@--> B",
        "A([bla])e1@--> B",
        "A[[bla]]e1@--> B",
        "A[(bla)]e1@--> B",
        "A{{bla}}e1@--> B",
        "A[/bla/]e1@--> B",
    ];
    for graph in cases {
        let body = format!("flowchart TD\n{graph}");
        let ast = ast(&body);
        assert_eq!(ast.edges.len(), 1, "{graph}");
        assert_eq!(edge(&ast, 0).id.as_deref(), Some("e1"), "{graph}");
        assert_eq!(edge(&ast, 0).start, "A", "{graph}");
        assert_eq!(edge(&ast, 0).end, "B", "{graph}");
    }
}

#[test]
fn should_handle_a_chain_of_shape_data_nodes_with_edge_ids() {
    let ast = ast("flowchart TD\nA@{shape:circle} e1@--> B@{shape:circle} e2@--> C");
    let mut ids: Vec<&str> = ast.nodes.iter().map(|node| node.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec!["A", "B", "C"]);
    assert_eq!(ast.edges.len(), 2);
    assert_eq!(edge(&ast, 0).id.as_deref(), Some("e1"));
    assert_eq!(edge(&ast, 0).start, "A");
    assert_eq!(edge(&ast, 0).end, "B");
    assert_eq!(edge(&ast, 1).id.as_deref(), Some("e2"));
    assert_eq!(edge(&ast, 1).start, "B");
    assert_eq!(edge(&ast, 1).end, "C");
}

#[test]
fn should_handle_a_shape_data_node_followed_by_trailing_spaces() {
    let ast = ast("flowchart TD\nA@{shape:circle}   ");
    assert_eq!(ast.nodes.len(), 1);
    assert_eq!(ast.nodes[0].id, "A");
    assert!(ast.edges.is_empty());
}

#[test]
fn should_keep_the_shape_data_of_nodes_followed_by_spaces() {
    let ast = ast("flowchart TD\nA@{shape: circle} e1@--> B@{shape: circle} e2@--> C");
    assert_eq!(ast.node("A").unwrap().shape, "circle");
    assert_eq!(ast.node("B").unwrap().shape, "circle");
    assert_eq!(ast.edges.len(), 2);
    assert_eq!(edge(&ast, 0).id.as_deref(), Some("e1"));
    assert_eq!(edge(&ast, 1).id.as_deref(), Some("e2"));
}

#[test]
fn should_keep_the_shape_data_of_a_node_at_the_end_of_a_line() {
    let ast = ast("flowchart TD\nA@{shape: circle}   ");
    assert_eq!(ast.nodes.len(), 1);
    assert_eq!(ast.nodes[0].id, "A");
    assert_eq!(ast.nodes[0].shape, "circle");
}

#[test]
fn should_reject_edge_ids_containing_node_bracket_characters() {
    for graph in [
        "A e[1]@--> B",
        "A e(1)@--> B",
        "A e{1}@--> B",
        "A e|1|@--> B",
    ] {
        let error = parse_error(&format!("flowchart TD\n{graph}"));
        assert!(!error.to_string().is_empty());
    }
}

#[test]
fn should_handle_double_ended_edges_without_text() {
    for edge_type in &DOUBLE_ENDED {
        let body = format!("graph TD;\nA {}{} B;", edge_type.start, edge_type.end);
        let ast = ast(&body);
        assert_eq!(ast.edges.len(), 1, "{body}");
        assert_eq!(
            edge(&ast, 0).type_name.unwrap(),
            edge_type.type_name,
            "{body}"
        );
        assert_eq!(edge(&ast, 0).text, "", "{body}");
        assert_eq!(edge(&ast, 0).stroke.unwrap(), edge_type.stroke, "{body}");
    }
}

#[test]
fn should_handle_double_ended_edges_with_text() {
    for edge_type in &DOUBLE_ENDED {
        let body = format!("graph TD;\nA {} text {} B;", edge_type.start, edge_type.end);
        let ast = ast(&body);
        assert_eq!(ast.edges.len(), 1, "{body}");
        assert_eq!(
            edge(&ast, 0).type_name.unwrap(),
            edge_type.type_name,
            "{body}"
        );
        assert_eq!(edge(&ast, 0).text, "text", "{body}");
        assert_eq!(edge(&ast, 0).stroke.unwrap(), edge_type.stroke, "{body}");
    }
}

#[test]
fn should_handle_double_ended_edges_with_keyword_text() {
    for edge_type in &DOUBLE_ENDED {
        for keyword in KEYWORDS {
            let body = format!(
                "graph TD;\nA {} {} {} B;",
                edge_type.start, keyword, edge_type.end
            );
            let ast = ast(&body);
            assert_eq!(ast.edges.len(), 1, "{body}");
            assert_eq!(
                edge(&ast, 0).type_name.unwrap(),
                edge_type.type_name,
                "{body}"
            );
            assert_eq!(edge(&ast, 0).text, keyword, "{body}");
            assert_eq!(edge(&ast, 0).stroke.unwrap(), edge_type.stroke, "{body}");
        }
    }
}

#[test]
fn should_handle_multiple_edges() {
    let ast = ast("graph TD;A---|This is the 123 s text|B;\nA---|This is the second edge|B;");
    assert_eq!(vertex(&ast, "A").id, "A");
    assert_eq!(vertex(&ast, "B").id, "B");
    assert_eq!(ast.edges.len(), 2);
    assert_eq!(edge(&ast, 0).start, "A");
    assert_eq!(edge(&ast, 0).end, "B");
    assert_eq!(edge(&ast, 0).type_name.unwrap(), ArrowType::Open);
    assert_eq!(edge(&ast, 0).text, "This is the 123 s text");
    assert_eq!(edge(&ast, 0).stroke.unwrap(), Stroke::Normal);
    assert_eq!(edge(&ast, 0).length, Some(1));
    assert_eq!(edge(&ast, 1).start, "A");
    assert_eq!(edge(&ast, 1).end, "B");
    assert_eq!(edge(&ast, 1).type_name.unwrap(), ArrowType::Open);
    assert_eq!(edge(&ast, 1).text, "This is the second edge");
    assert_eq!(edge(&ast, 1).stroke.unwrap(), Stroke::Normal);
    assert_eq!(edge(&ast, 1).length, Some(1));
}

fn edge_length_case(body: &str, type_name: ArrowType, stroke: Stroke, text: &str, length: i64) {
    let ast = ast(body);
    assert_eq!(vertex(&ast, "A").id, "A");
    assert_eq!(vertex(&ast, "B").id, "B");
    assert_eq!(ast.edges.len(), 1, "{body}");
    assert_eq!(edge(&ast, 0).start, "A");
    assert_eq!(edge(&ast, 0).end, "B");
    assert_eq!(edge(&ast, 0).type_name.unwrap(), type_name, "{body}");
    assert_eq!(edge(&ast, 0).text, text, "{body}");
    assert_eq!(edge(&ast, 0).stroke.unwrap(), stroke, "{body}");
    assert_eq!(edge(&ast, 0).length, Some(length), "{body}");
}

#[test]
fn should_handle_edge_lengths() {
    for length in 1..=3usize {
        let dashes = "-".repeat(length);
        edge_length_case(
            &format!("graph TD;\nA -{dashes}- B;"),
            ArrowType::Open,
            Stroke::Normal,
            "",
            length as i64,
        );
        edge_length_case(
            &format!("graph TD;\nA -- Label -{dashes}- B;"),
            ArrowType::Open,
            Stroke::Normal,
            "Label",
            length as i64,
        );
        edge_length_case(
            &format!("graph TD;\nA -{dashes}> B;"),
            ArrowType::Point,
            Stroke::Normal,
            "",
            length as i64,
        );
        edge_length_case(
            &format!("graph TD;\nA -- Label -{dashes}> B;"),
            ArrowType::Point,
            Stroke::Normal,
            "Label",
            length as i64,
        );
        edge_length_case(
            &format!("graph TD;\nA <-{dashes}> B;"),
            ArrowType::DoublePoint,
            Stroke::Normal,
            "",
            length as i64,
        );
        edge_length_case(
            &format!("graph TD;\nA <-- Label -{dashes}> B;"),
            ArrowType::DoublePoint,
            Stroke::Normal,
            "Label",
            length as i64,
        );
        edge_length_case(
            &format!("graph TD;\nA ={}= B;", "=".repeat(length)),
            ArrowType::Open,
            Stroke::Thick,
            "",
            length as i64,
        );
        edge_length_case(
            &format!("graph TD;\nA == Label ={}= B;", "=".repeat(length)),
            ArrowType::Open,
            Stroke::Thick,
            "Label",
            length as i64,
        );
        edge_length_case(
            &format!("graph TD;\nA ={}> B;", "=".repeat(length)),
            ArrowType::Point,
            Stroke::Thick,
            "",
            length as i64,
        );
        edge_length_case(
            &format!("graph TD;\nA == Label ={}> B;", "=".repeat(length)),
            ArrowType::Point,
            Stroke::Thick,
            "Label",
            length as i64,
        );
        edge_length_case(
            &format!("graph TD;\nA <={}> B;", "=".repeat(length)),
            ArrowType::DoublePoint,
            Stroke::Thick,
            "",
            length as i64,
        );
        edge_length_case(
            &format!("graph TD;\nA <== Label ={}> B;", "=".repeat(length)),
            ArrowType::DoublePoint,
            Stroke::Thick,
            "Label",
            length as i64,
        );
        let dots = ".".repeat(length);
        edge_length_case(
            &format!("graph TD;\nA -{dots}- B;"),
            ArrowType::Open,
            Stroke::Dotted,
            "",
            length as i64,
        );
        edge_length_case(
            &format!("graph TD;\nA -. Label {dots}- B;"),
            ArrowType::Open,
            Stroke::Dotted,
            "Label",
            length as i64,
        );
        edge_length_case(
            &format!("graph TD;\nA -{dots}-> B;"),
            ArrowType::Point,
            Stroke::Dotted,
            "",
            length as i64,
        );
        edge_length_case(
            &format!("graph TD;\nA -. Label {dots}-> B;"),
            ArrowType::Point,
            Stroke::Dotted,
            "Label",
            length as i64,
        );
        edge_length_case(
            &format!("graph TD;\nA <-{dots}-> B;"),
            ArrowType::DoublePoint,
            Stroke::Dotted,
            "",
            length as i64,
        );
        edge_length_case(
            &format!("graph TD;\nA <-. Label {dots}-> B;"),
            ArrowType::DoublePoint,
            Stroke::Dotted,
            "Label",
            length as i64,
        );
    }
}
