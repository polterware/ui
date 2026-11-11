//! Port of `parser/flow-text.spec.js`.

use super::super::{ArrowType, Stroke};
use super::{ast, edge, parse_error, text, vertex};

#[test]
fn should_handle_text_on_edges() {
    // Text without space, with space, CAPS, and the `|...|` spelling.
    for (body, type_name, expected) in [
        ("graph TD;A--x|textNoSpace|B;", ArrowType::Cross, ""),
        (
            "graph TD;A--x|text including space|B;",
            ArrowType::Cross,
            "",
        ),
        ("graph TD;A --x|textNoSpace| B;", ArrowType::Cross, ""),
        (
            "graph TD;A--x|text including CAPS space|B;",
            ArrowType::Cross,
            "",
        ),
        (
            "graph TD;A--x|text with / should work|B;",
            ArrowType::Cross,
            "text with / should work",
        ),
        (
            "graph TD;A--x|text including URL space|B;",
            ArrowType::Cross,
            "text including URL space",
        ),
        (
            "graph TD;A--x|text including R TD space|B;",
            ArrowType::Cross,
            "text including R TD space",
        ),
        (
            "graph TD;A--x|text including `|B;",
            ArrowType::Cross,
            "text including `",
        ),
        (
            "graph TD;A--x|text including graph space|B;",
            ArrowType::Cross,
            "text including graph space",
        ),
    ] {
        let ast = ast(body);
        assert_eq!(edge(&ast, 0).type_name.unwrap(), type_name, "{body}");
        if !expected.is_empty() {
            assert_eq!(edge(&ast, 0).text, expected, "{body}");
        }
    }
}

#[test]
fn should_handle_unquoted_edge_text() {
    let tight = ast("graph TD;A--text including URL space and send-->B;");
    assert_eq!(edge(&tight, 0).type_name.unwrap(), ArrowType::Point);
    assert_eq!(edge(&tight, 0).text, "text including URL space and send");

    let spaced = ast("graph TD;A-- text including URL space and send -->B;");
    assert_eq!(edge(&spaced, 0).type_name.unwrap(), ArrowType::Point);
    assert_eq!(edge(&spaced, 0).text, "text including URL space and send");
}

#[test]
fn should_handle_v_in_node_ids() {
    for (body, id) in [
        ("graph TD;A--xv(my text);", "v"),
        ("graph TD;A--xcsv(my text);", "csv"),
        ("graph TD;A--xava(my text);", "ava"),
        ("graph TD;A--xva(my text);", "va"),
    ] {
        let ast = ast(body);
        assert_eq!(edge(&ast, 0).type_name.unwrap(), ArrowType::Cross, "{body}");
        assert_eq!(text(&ast, id), "my text", "{body}");
    }
}

#[test]
fn should_handle_text_in_square_brackets() {
    let ast = ast("graph TD;V-->a[v]");
    assert_eq!(text(&ast, "a"), "v");
}

#[test]
fn should_handle_quoted_edge_text() {
    let ast = ast("graph TD;V-- \"test string()\" -->a[v]");
    assert_eq!(edge(&ast, 0).text, "test string()");
}

#[test]
fn should_handle_text_on_lines() {
    assert_eq!(
        ast("graph TD;A-- test text with == -->B;").edges[0]
            .stroke
            .unwrap(),
        Stroke::Normal
    );
    assert_eq!(
        ast("graph TD;A-. test text with == .->B;").edges[0]
            .stroke
            .unwrap(),
        Stroke::Dotted
    );
    assert_eq!(
        ast("graph TD;A== test text with - ==>B;").edges[0]
            .stroke
            .unwrap(),
        Stroke::Thick
    );
}

#[test]
fn should_handle_text_on_edges_using_the_new_notation() {
    for (body, type_name, expected) in [
        ("graph TD;A-- textNoSpace --xB;", ArrowType::Cross, ""),
        ("graph TD;A--    textNoSpace --xB;", ArrowType::Cross, ""),
        (
            "graph TD;A-- text including space --xB;",
            ArrowType::Cross,
            "",
        ),
        (
            "graph TD;A -- text with / should work --x B;",
            ArrowType::Cross,
            "text with / should work",
        ),
        ("graph TD;A -- textNoSpace --x B;", ArrowType::Cross, ""),
        (
            "graph TD;A-- text including CAPS space --xB;",
            ArrowType::Cross,
            "",
        ),
        (
            "graph TD;A-- text including URL space --xB;",
            ArrowType::Cross,
            "text including URL space",
        ),
        (
            "graph TD;A-- text including R TD space --xB;",
            ArrowType::Cross,
            "text including R TD space",
        ),
        (
            "graph TD;A-- text including graph space and v --xB;",
            ArrowType::Cross,
            "text including graph space and v",
        ),
        (
            "graph TD;A-- text including graph space and v --xB[blav]",
            ArrowType::Cross,
            "text including graph space and v",
        ),
    ] {
        let ast = ast(body);
        assert_eq!(edge(&ast, 0).type_name.unwrap(), type_name, "{body}");
        if !expected.is_empty() {
            assert_eq!(edge(&ast, 0).text, expected, "{body}");
        }
    }
}

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

struct Shape {
    start: &'static str,
    end: &'static str,
    name: &'static str,
}

const SHAPES: [Shape; 14] = [
    Shape {
        start: "[",
        end: "]",
        name: "square",
    },
    Shape {
        start: "(",
        end: ")",
        name: "round",
    },
    Shape {
        start: "{",
        end: "}",
        name: "diamond",
    },
    Shape {
        start: "(-",
        end: "-)",
        name: "ellipse",
    },
    Shape {
        start: "([",
        end: "])",
        name: "stadium",
    },
    Shape {
        start: ">",
        end: "]",
        name: "odd",
    },
    Shape {
        start: "[(",
        end: ")]",
        name: "cylinder",
    },
    Shape {
        start: "(((",
        end: ")))",
        name: "doublecircle",
    },
    Shape {
        start: "[/",
        end: "\\]",
        name: "trapezoid",
    },
    Shape {
        start: "[\\",
        end: "/]",
        name: "inv_trapezoid",
    },
    Shape {
        start: "[/",
        end: "/]",
        name: "lean_right",
    },
    Shape {
        start: "[\\",
        end: "\\]",
        name: "lean_left",
    },
    Shape {
        start: "[[",
        end: "]]",
        name: "subroutine",
    },
    Shape {
        start: "{{",
        end: "}}",
        name: "hexagon",
    },
];

#[test]
fn should_handle_keywords_in_every_shape() {
    for shape in &SHAPES {
        for keyword in KEYWORDS {
            let body = format!(
                "graph TD;A_{keyword}_node-->B{}This node has a {keyword} as text{};",
                shape.start, shape.end
            );
            let ast = ast(&body);
            assert_eq!(
                vertex(&ast, "B").type_name.as_deref(),
                Some(shape.name),
                "{body}"
            );
            assert_eq!(
                text(&ast, "B"),
                format!("This node has a {keyword} as text"),
                "{body}"
            );
        }
    }
}

#[test]
fn should_handle_keywords_in_rect_vertex() {
    for keyword in KEYWORDS {
        let body = format!(
            "graph TD;A_{keyword}_node-->B[|borders:lt|This node has a {keyword} as text];"
        );
        let ast = ast(&body);
        assert_eq!(
            vertex(&ast, "B").type_name.as_deref(),
            Some("rect"),
            "{body}"
        );
        assert_eq!(
            text(&ast, "B"),
            format!("This node has a {keyword} as text"),
            "{body}"
        );
    }
}

#[test]
fn should_handle_odd_vertex_with_node_id_ending_with_minus() {
    let ast = ast("graph TD;A_node-->odd->Vertex Text];");
    assert_eq!(vertex(&ast, "odd-").type_name.as_deref(), Some("odd"));
    assert_eq!(text(&ast, "odd-"), "Vertex Text");
}

#[test]
fn should_allow_forward_slashes_in_lean_right_vertices() {
    let ast = ast("graph TD;A_node-->B[/This node has a / as text/];");
    assert_eq!(vertex(&ast, "B").type_name.as_deref(), Some("lean_right"));
    assert_eq!(text(&ast, "B"), "This node has a / as text");
}

#[test]
fn should_allow_backslashes_in_lean_left_vertices() {
    let ast = ast("graph TD;A_node-->B[\\This node has a \\ as text\\];");
    assert_eq!(vertex(&ast, "B").type_name.as_deref(), Some("lean_left"));
    assert_eq!(text(&ast, "B"), "This node has a \\ as text");
}

#[test]
fn should_handle_diacritics_and_minus_in_a_diamond() {
    let ast = ast("graph TD;A-->C{Chimpansen hoppar åäö-ÅÄÖ};");
    assert_eq!(vertex(&ast, "C").type_name.as_deref(), Some("diamond"));
    assert_eq!(text(&ast, "C"), "Chimpansen hoppar åäö-ÅÄÖ");
}

#[test]
fn should_handle_diacritics_minus_space_and_br() {
    let ast = ast("graph TD;A-->C(Chimpansen hoppar åäö  <br> -  ÅÄÖ);");
    assert_eq!(vertex(&ast, "C").type_name.as_deref(), Some("round"));
    assert_eq!(text(&ast, "C"), "Chimpansen hoppar åäö  <br> -  ÅÄÖ");
}

#[test]
fn should_handle_unicode_chars() {
    let ast = ast("graph TD;A-->C(Начало);");
    assert_eq!(text(&ast, "C"), "Начало");
}

#[test]
fn should_handle_backslash_in_text() {
    let ast = ast("graph TD;A-->C(c:\\windows);");
    assert_eq!(text(&ast, "C"), "c:\\windows");
}

#[test]
fn should_handle_text_in_vertices() {
    let chart = ast("graph TD;A-->C(Chimpansen hoppar);");
    assert_eq!(vertex(&chart, "C").type_name.as_deref(), Some("round"));
    assert_eq!(text(&chart, "C"), "Chimpansen hoppar");
    let chart = ast("graph TD;A-->C(some CAPS);");
    assert_eq!(text(&chart, "C"), "some CAPS");
    let chart = ast("graph TD;A-->C(some URL);");
    assert_eq!(text(&chart, "C"), "some URL");
    let chart = ast("graph TD;A[chimpansen hoppar]-->C;");
    assert_eq!(vertex(&chart, "A").type_name.as_deref(), Some("square"));
    assert_eq!(text(&chart, "A"), "chimpansen hoppar");
    let chart = ast("graph TD;A[chimpansen hoppar] --> C;");
    assert_eq!(text(&chart, "A"), "chimpansen hoppar");
    let chart = ast("graph TD;A[chimpansen_hoppar] --> C;");
    assert_eq!(text(&chart, "A"), "chimpansen_hoppar");
    let chart = ast("graph TD;A[\"chimpansen hoppar ()[]\"] --> C;");
    assert_eq!(vertex(&chart, "A").type_name.as_deref(), Some("square"));
    assert_eq!(text(&chart, "A"), "chimpansen hoppar ()[]");
    let chart = ast("graph TD;A((chimpansen hoppar))-->C;");
    assert_eq!(vertex(&chart, "A").type_name.as_deref(), Some("circle"));
    assert_eq!(text(&chart, "A"), "chimpansen hoppar");
    let chart = ast("graph TD\nA(-this is an ellipse-)-->B");
    assert_eq!(vertex(&chart, "A").type_name.as_deref(), Some("ellipse"));
    assert_eq!(text(&chart, "A"), "this is an ellipse");
    let chart = ast("graph TD;A(chimpansen hoppar)-->C;");
    assert_eq!(vertex(&chart, "A").type_name.as_deref(), Some("round"));
    assert_eq!(text(&chart, "A"), "chimpansen hoppar");
}

#[test]
fn should_handle_special_characters_in_labels() {
    let chart = ast("graph TD;A(?)-->|?|C;");
    assert_eq!(text(&chart, "A"), "?");
    assert_eq!(edge(&chart, 0).text, "?");
    let chart = ast("graph TD;A(éèêàçô)-->|éèêàçô|C;");
    assert_eq!(text(&chart, "A"), "éèêàçô");
    assert_eq!(edge(&chart, 0).text, "éèêàçô");
    let chart = ast("graph TD;A(,.?!+-*)-->|,.?!+-*|C;");
    assert_eq!(text(&chart, "A"), ",.?!+-*");
    assert_eq!(edge(&chart, 0).text, ",.?!+-*");
}

#[test]
fn should_handle_multi_line_text() {
    let ast = ast("graph TD;A--o|text space|B;\n B-->|more text with space|C;");
    assert_eq!(edge(&ast, 0).type_name.unwrap(), ArrowType::Circle);
    assert_eq!(edge(&ast, 1).type_name.unwrap(), ArrowType::Point);
    assert_eq!(vertex(&ast, "A").id, "A");
    assert_eq!(vertex(&ast, "B").id, "B");
    assert_eq!(vertex(&ast, "C").id, "C");
    assert_eq!(ast.edges.len(), 2);
    assert_eq!(edge(&ast, 0).start, "A");
    assert_eq!(edge(&ast, 0).end, "B");
    assert_eq!(edge(&ast, 1).start, "B");
    assert_eq!(edge(&ast, 1).end, "C");
    assert_eq!(edge(&ast, 1).text, "more text with space");
}

#[test]
fn should_not_freeze_when_ellipse_text_has_a_parenthesis() {
    let error = parse_error("graph\nX(- My Text (");
    assert!(!error.to_string().is_empty());
}

#[test]
fn should_throw_at_nested_brackets() {
    let error = parse_error("graph TD; A[This is a () in text];");
    assert!(error.to_string().contains("PS"), "{error}");
}

#[test]
fn should_throw_for_strings_and_text_at_the_same_time() {
    let error = parse_error(
        "graph TD;A(this node has \"string\" and text)-->|this link has \"string\" and text|C;",
    );
    assert!(error.to_string().contains("STR"), "{error}");
}

#[test]
fn should_throw_for_escaping_quotes_in_text_state() {
    let error = parse_error("graph TD; A[This is a \"()\" in text];");
    assert!(error.to_string().contains("STR"), "{error}");
}

#[test]
fn should_throw_for_nested_quotation_marks() {
    let error = parse_error("graph TD; A[\"This is a \"()\" in text\"];");
    assert!(error.to_string().contains("SQE"), "{error}");
}

#[test]
fn should_throw_for_a_closing_parenthesis_inside_square_text() {
    let error = parse_error("graph TD; node[hello ) world] --> works");
    assert!(error.to_string().contains("PE"), "{error}");
}
