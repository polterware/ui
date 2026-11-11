//! Tests ported from the upstream C4 parser specs in
//! `packages/mermaid/src/diagrams/c4/parser/`: `c4Diagram.spec.ts`, `c4Person.spec.ts`,
//! `c4PersonExt.spec.ts`, `c4System.spec.ts`, `c4Container.spec.ts`, `c4Component.spec.ts`,
//! `c4Boundary.spec.ts`, and `c4NamedAttributes.spec.ts`. Inputs and expectations are
//! upstream's.
//!
//! Skipped, and why:
//! - `setConfig({ securityLevel: 'strict' })` and `c4.parser.yy.clear()` are setup; each
//!   `parse` call here builds a fresh database.
//! - `expect(rendered).toBe(true)` is jison's return value; `parse` returning `Ok` is the
//!   equivalent assertion, and `parse` has no "rendered" flag.
//! - `yield exactly the shape`'s `toEqual` object only lists the keys JavaScript happened to
//!   set; the Rust struct is compared whole, with the remaining fields at their default.
//! - `c4Renderer.spec.ts` and any rendering or configuration assertion are out of scope, as
//!   are `getConfig`/`setWrap`.

use super::ast::C4Element;
use super::{Ast, Boundary, Rel, Shape, Text, TextValue, parse};
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

fn only_shape(ast: &Ast) -> &Shape {
    assert_eq!(ast.shapes.len(), 1, "shapes: {:?}", ast.shapes);
    &ast.shapes[0]
}

fn only_rel(ast: &Ast) -> &Rel {
    assert_eq!(ast.rels.len(), 1, "rels: {:?}", ast.rels);
    &ast.rels[0]
}

fn named(key: &str, value: &str) -> Text {
    Text {
        text: TextValue::Named {
            key: key.to_string(),
            value: value.to_string(),
        },
    }
}

// c4Diagram.spec.ts

#[test]
fn handles_trailing_whitespace_after_statements() {
    let whitespace = " ";
    let body = format!(
        "C4Context{whitespace}\ntitle System Context diagram for Internet Banking System{whitespace}\nPerson(customerA, \"Banking Customer A\", \"A customer of the bank, with personal bank accounts.\"){whitespace}"
    );
    ast(&body);
}

#[test]
fn handles_parameter_names_that_are_keywords() {
    let ast = ast("C4Context\ntitle title\nPerson(Person, \"Person\", \"Person\")");
    assert_eq!(ast.title, "title");

    let shape = only_shape(&ast);
    assert_eq!(shape.alias, "Person");
    assert_eq!(shape.descr, Some(Text::plain("Person")));
    assert_eq!(shape.label, Text::plain("Person"));
}

#[test]
fn allows_default_in_the_parameters() {
    let ast = ast("C4Context\nPerson(default, \"default\", \"default\")");
    let shape = only_shape(&ast);
    assert_eq!(shape.alias, "default");
    assert_eq!(shape.descr, Some(Text::plain("default")));
    assert_eq!(shape.label, Text::plain("default"));
}

// c4Person.spec.ts

#[test]
fn parses_a_diagram_with_one_person() {
    let ast = ast(
        "C4Context\ntitle System Context diagram for Internet Banking System\nPerson(customerA, \"Banking Customer A\", \"A customer of the bank, with personal bank accounts.\")",
    );
    assert_eq!(
        ast.title,
        "System Context diagram for Internet Banking System"
    );
    assert_eq!(
        *only_shape(&ast),
        Shape {
            alias: "customerA".to_string(),
            label: Text::plain("Banking Customer A"),
            type_c4_shape: Text::plain("person"),
            parent_boundary: "global".to_string(),
            wrap: false,
            descr: Some(Text::plain(
                "A customer of the bank, with personal bank accounts."
            )),
            ..Shape::default()
        }
    );
}

#[test]
fn parses_a_person_alias() {
    let ast = ast("C4Context\nPerson(customerA, \"Banking Customer A\")");
    assert_eq!(only_shape(&ast).alias, "customerA");
}

#[test]
fn parses_a_person_label() {
    let ast = ast("C4Context\nPerson(customerA, \"Banking Customer A\")");
    assert_eq!(only_shape(&ast).label, Text::plain("Banking Customer A"));
}

#[test]
fn parses_a_person_description() {
    let ast = ast(
        "C4Context\nPerson(customerA, \"\", \"A customer of the bank, with personal bank accounts.\")",
    );
    assert_eq!(
        only_shape(&ast).descr,
        Some(Text::plain(
            "A customer of the bank, with personal bank accounts."
        ))
    );
}

#[test]
fn parses_a_person_sprite() {
    let ast = ast("C4Context\nPerson(customerA, $sprite=\"users\")");
    assert_eq!(only_shape(&ast).label, named("sprite", "users"));
}

#[test]
fn parses_a_person_link() {
    let ast = ast("C4Context\nPerson(customerA, $link=\"https://github.com/mermaidjs\")");
    assert_eq!(
        only_shape(&ast).label,
        named("link", "https://github.com/mermaidjs")
    );
}

#[test]
fn parses_person_tags() {
    let ast = ast("C4Context\nPerson(customerA, $tags=\"tag1,tag2\")");
    assert_eq!(only_shape(&ast).label, named("tags", "tag1,tag2"));
}

#[test]
fn keeps_named_attributes_that_land_in_an_earlier_positional_slot() {
    let ast = ast(
        "C4Context\nPerson(customerA, \"Banking Customer A\", \"A customer.\", $tags=\"v1.0+internal\", $link=\"https://example.com\")",
    );
    let shape = only_shape(&ast);
    assert_eq!(shape.alias, "customerA");
    assert_eq!(shape.tags.as_deref(), Some("v1.0+internal"));
    assert_eq!(shape.link.as_deref(), Some("https://example.com"));
}

// c4PersonExt.spec.ts

#[test]
fn parses_a_diagram_with_one_person_ext() {
    let ast = ast(
        "C4Context\ntitle System Context diagram for Internet Banking System\nPerson_Ext(customerA, \"Banking Customer A\", \"A customer of the bank, with personal bank accounts.\")",
    );
    assert_eq!(
        *only_shape(&ast),
        Shape {
            alias: "customerA".to_string(),
            label: Text::plain("Banking Customer A"),
            type_c4_shape: Text::plain("external_person"),
            parent_boundary: "global".to_string(),
            wrap: false,
            descr: Some(Text::plain(
                "A customer of the bank, with personal bank accounts."
            )),
            ..Shape::default()
        }
    );
}

#[test]
fn parses_a_person_ext_alias() {
    let ast = ast("C4Context\nPerson_Ext(customerA, \"Banking Customer A\")");
    assert_eq!(only_shape(&ast).alias, "customerA");
}

#[test]
fn parses_a_person_ext_label() {
    let ast = ast("C4Context\nPerson_Ext(customerA, \"Banking Customer A\")");
    assert_eq!(only_shape(&ast).label, Text::plain("Banking Customer A"));
}

#[test]
fn parses_a_person_ext_description() {
    let ast = ast(
        "C4Context\nPerson_Ext(customerA, \"\", \"A customer of the bank, with personal bank accounts.\")",
    );
    assert_eq!(
        only_shape(&ast).descr,
        Some(Text::plain(
            "A customer of the bank, with personal bank accounts."
        ))
    );
}

#[test]
fn parses_a_person_ext_sprite() {
    let ast = ast("C4Context\nPerson_Ext(customerA, $sprite=\"users\")");
    assert_eq!(only_shape(&ast).label, named("sprite", "users"));
}

#[test]
fn parses_a_person_ext_link() {
    let ast = ast("C4Context\nPerson_Ext(customerA, $link=\"https://github.com/mermaidjs\")");
    assert_eq!(
        only_shape(&ast).label,
        named("link", "https://github.com/mermaidjs")
    );
}

#[test]
fn parses_person_ext_tags() {
    let ast = ast("C4Context\nPerson_Ext(customerA, $tags=\"tag1,tag2\")");
    assert_eq!(only_shape(&ast).label, named("tags", "tag1,tag2"));
}

// c4System.spec.ts

const SYSTEM_MACROS: &[(&str, &str)] = &[
    ("System", "system"),
    ("SystemDb", "system_db"),
    ("SystemQueue", "system_queue"),
    ("System_Ext", "external_system"),
    ("SystemDb_Ext", "external_system_db"),
    ("SystemQueue_Ext", "external_system_queue"),
];

#[test]
fn parses_a_diagram_with_one_system() {
    for (macro_name, element_name) in SYSTEM_MACROS {
        let body = format!(
            "C4Context\ntitle System Context diagram for Internet Banking System\n{macro_name}(SystemAA, \"Internet Banking System\", \"Allows customers to view information about their bank accounts, and make payments.\")"
        );
        let ast = ast(&body);
        assert_eq!(
            *only_shape(&ast),
            Shape {
                alias: "SystemAA".to_string(),
                label: Text::plain("Internet Banking System"),
                type_c4_shape: Text::plain(*element_name),
                parent_boundary: "global".to_string(),
                wrap: false,
                descr: Some(Text::plain(
                    "Allows customers to view information about their bank accounts, and make payments."
                )),
                ..Shape::default()
            },
            "macro: {macro_name}"
        );
    }
}

#[test]
fn parses_a_system_alias() {
    for (macro_name, _) in SYSTEM_MACROS {
        let ast = ast(&format!(
            "C4Context\n{macro_name}(SystemAA, \"Internet Banking System\")"
        ));
        assert_eq!(only_shape(&ast).alias, "SystemAA", "macro: {macro_name}");
    }
}

#[test]
fn parses_a_system_label() {
    for (macro_name, _) in SYSTEM_MACROS {
        let ast = ast(&format!(
            "C4Context\n{macro_name}(SystemAA, \"Internet Banking System\")"
        ));
        assert_eq!(
            only_shape(&ast).label,
            Text::plain("Internet Banking System"),
            "macro: {macro_name}"
        );
    }
}

#[test]
fn parses_a_system_description() {
    for (macro_name, _) in SYSTEM_MACROS {
        let ast = ast(&format!(
            "C4Context\n{macro_name}(SystemAA, \"\", \"Allows customers to view information about their bank accounts, and make payments.\")"
        ));
        assert_eq!(
            only_shape(&ast).descr,
            Some(Text::plain(
                "Allows customers to view information about their bank accounts, and make payments."
            )),
            "macro: {macro_name}"
        );
    }
}

#[test]
fn parses_a_system_sprite() {
    for (macro_name, _) in SYSTEM_MACROS {
        let ast = ast(&format!(
            "C4Context\n{macro_name}(SystemAA, $sprite=\"users\")"
        ));
        assert_eq!(
            only_shape(&ast).label,
            named("sprite", "users"),
            "macro: {macro_name}"
        );
    }
}

#[test]
fn parses_a_system_link() {
    for (macro_name, _) in SYSTEM_MACROS {
        let ast = ast(&format!(
            "C4Context\n{macro_name}(SystemAA, $link=\"https://github.com/mermaidjs\")"
        ));
        assert_eq!(
            only_shape(&ast).label,
            named("link", "https://github.com/mermaidjs"),
            "macro: {macro_name}"
        );
    }
}

#[test]
fn parses_system_tags() {
    for (macro_name, _) in SYSTEM_MACROS {
        let ast = ast(&format!(
            "C4Context\n{macro_name}(SystemAA, $tags=\"tag1,tag2\")"
        ));
        assert_eq!(
            only_shape(&ast).label,
            named("tags", "tag1,tag2"),
            "macro: {macro_name}"
        );
    }
}

// c4Container.spec.ts

const CONTAINER_MACROS: &[(&str, &str)] = &[
    ("Container", "container"),
    ("ContainerDb", "container_db"),
    ("ContainerQueue", "container_queue"),
    ("Container_Ext", "external_container"),
    ("ContainerDb_Ext", "external_container_db"),
    ("ContainerQueue_Ext", "external_container_queue"),
];

#[test]
fn parses_a_diagram_with_one_container() {
    for (macro_name, element_name) in CONTAINER_MACROS {
        let body = format!(
            "C4Context\ntitle Container diagram for Internet Banking Container\n{macro_name}(ContainerAA, \"Internet Banking Container\", \"Technology\", \"Allows customers to view information about their bank accounts, and make payments.\")"
        );
        let ast = ast(&body);
        assert_eq!(
            *only_shape(&ast),
            Shape {
                alias: "ContainerAA".to_string(),
                label: Text::plain("Internet Banking Container"),
                type_c4_shape: Text::plain(*element_name),
                parent_boundary: "global".to_string(),
                wrap: false,
                techn: Some(Text::plain("Technology")),
                descr: Some(Text::plain(
                    "Allows customers to view information about their bank accounts, and make payments."
                )),
                ..Shape::default()
            },
            "macro: {macro_name}"
        );
    }
}

#[test]
fn parses_a_container_alias() {
    for (macro_name, _) in CONTAINER_MACROS {
        let ast = ast(&format!(
            "C4Context\n{macro_name}(ContainerAA, \"Internet Banking Container\")"
        ));
        assert_eq!(only_shape(&ast).alias, "ContainerAA", "macro: {macro_name}");
    }
}

#[test]
fn parses_a_container_label() {
    for (macro_name, _) in CONTAINER_MACROS {
        let ast = ast(&format!(
            "C4Context\n{macro_name}(ContainerAA, \"Internet Banking Container\")"
        ));
        assert_eq!(
            only_shape(&ast).label,
            Text::plain("Internet Banking Container"),
            "macro: {macro_name}"
        );
    }
}

#[test]
fn parses_a_container_technology() {
    for (macro_name, _) in CONTAINER_MACROS {
        let ast = ast(&format!(
            "C4Context\n{macro_name}(ContainerAA, \"\", \"Java\")"
        ));
        assert_eq!(
            only_shape(&ast).techn,
            Some(Text::plain("Java")),
            "macro: {macro_name}"
        );
    }
}

#[test]
fn parses_a_container_description() {
    for (macro_name, _) in CONTAINER_MACROS {
        let ast = ast(&format!(
            "C4Context\n{macro_name}(ContainerAA, \"\", \"\", \"Allows customers to view information about their bank accounts, and make payments.\")"
        ));
        assert_eq!(
            only_shape(&ast).descr,
            Some(Text::plain(
                "Allows customers to view information about their bank accounts, and make payments."
            )),
            "macro: {macro_name}"
        );
    }
}

#[test]
fn parses_a_container_sprite() {
    for (macro_name, _) in CONTAINER_MACROS {
        let ast = ast(&format!(
            "C4Context\n{macro_name}(ContainerAA, $sprite=\"users\")"
        ));
        assert_eq!(
            only_shape(&ast).label,
            named("sprite", "users"),
            "macro: {macro_name}"
        );
    }
}

#[test]
fn parses_a_container_link() {
    for (macro_name, _) in CONTAINER_MACROS {
        let ast = ast(&format!(
            "C4Context\n{macro_name}(ContainerAA, $link=\"https://github.com/mermaidjs\")"
        ));
        assert_eq!(
            only_shape(&ast).label,
            named("link", "https://github.com/mermaidjs"),
            "macro: {macro_name}"
        );
    }
}

#[test]
fn parses_container_tags() {
    for (macro_name, _) in CONTAINER_MACROS {
        let ast = ast(&format!(
            "C4Context\n{macro_name}(ContainerAA, $tags=\"tag1,tag2\")"
        ));
        assert_eq!(
            only_shape(&ast).label,
            named("tags", "tag1,tag2"),
            "macro: {macro_name}"
        );
    }
}

// c4Component.spec.ts

const COMPONENT_MACROS: &[(&str, &str)] = &[
    ("Component", "component"),
    ("ComponentDb", "component_db"),
    ("ComponentQueue", "component_queue"),
    ("Component_Ext", "external_component"),
    ("ComponentDb_Ext", "external_component_db"),
    ("ComponentQueue_Ext", "external_component_queue"),
];

#[test]
fn parses_a_diagram_with_one_component() {
    for (macro_name, element_name) in COMPONENT_MACROS {
        let body = format!(
            "C4Component\ntitle Component diagram for Internet Banking Component\n{macro_name}(ComponentAA, \"Internet Banking Component\", \"Technology\", \"Allows customers to view information about their bank accounts, and make payments.\")"
        );
        let ast = ast(&body);
        assert_eq!(ast.c4_type.as_deref(), Some("C4Component"));
        let shape = only_shape(&ast);
        assert_eq!(shape.alias, "ComponentAA", "macro: {macro_name}");
        assert_eq!(shape.label, Text::plain("Internet Banking Component"));
        assert_eq!(
            *shape.descr.as_ref().expect("descr"),
            Text::plain(
                "Allows customers to view information about their bank accounts, and make payments."
            )
        );
        assert_eq!(shape.techn, Some(Text::plain("Technology")));
        assert_eq!(shape.type_c4_shape, Text::plain(*element_name));
    }
}

#[test]
fn handles_trailing_whitespace_after_a_component() {
    let whitespace = " ";
    for (macro_name, _) in COMPONENT_MACROS {
        let body = format!(
            "C4Component{whitespace}\ntitle Component diagram for Internet Banking Component{whitespace}\n{macro_name}(ComponentAA, \"Internet Banking Component\", \"Technology\", \"Allows customers to view information about their bank accounts, and make payments.\"){whitespace}"
        );
        ast(&body);
    }
}

// c4Boundary.spec.ts

fn boundary_body(label: &str, type_or_attribute: &str) -> String {
    format!(
        "C4Context\nBoundary(b1, \"{label}\"{type_or_attribute}) {{\nSystem(SystemAA, \"Internet Banking System\")\n}}"
    )
}

#[test]
fn parses_a_diagram_with_one_boundary() {
    let ast = ast(&boundary_body("BankBoundary", ""));
    assert_eq!(ast.boundaries.len(), 2);
    assert_eq!(
        ast.boundaries[1],
        Boundary {
            alias: "b1".to_string(),
            label: Text::plain("BankBoundary"),
            type_name: Some(Text::plain("system")),
            parent_boundary: "global".to_string(),
            wrap: false,
            ..Boundary::default()
        }
    );
}

#[test]
fn parses_the_boundary_alias() {
    let ast = ast(&boundary_body("BankBoundary", ""));
    assert_eq!(ast.boundaries[1].alias, "b1");
}

#[test]
fn parses_the_boundary_label() {
    let ast = ast(&boundary_body("BankBoundary", ""));
    assert_eq!(ast.boundaries[1].label, Text::plain("BankBoundary"));
}

#[test]
fn parses_the_boundary_type() {
    let ast = ast(
        "C4Context\nBoundary(b1, \"\", \"company\") {\nSystem(SystemAA, \"Internet Banking System\")\n}",
    );
    assert_eq!(ast.boundaries[1].type_name, Some(Text::plain("company")));
}

#[test]
fn parses_a_boundary_link() {
    let ast = ast(
        "C4Context\nBoundary(b1, $link=\"https://github.com/mermaidjs\") {\nSystem(SystemAA, \"Internet Banking System\")\n}",
    );
    assert_eq!(
        ast.boundaries[1].label,
        named("link", "https://github.com/mermaidjs")
    );
}

#[test]
fn parses_boundary_tags() {
    let ast = ast(
        "C4Context\nBoundary(b1, $tags=\"tag1,tag2\") {\nSystem(SystemAA, \"Internet Banking System\")\n}",
    );
    assert_eq!(ast.boundaries[1].label, named("tags", "tag1,tag2"));
}

#[test]
fn resolves_a_boundary_alias_as_a_relation_endpoint() {
    let ast = ast(
        "C4Context\nBoundary(b1, \"BankBoundary\") {\nSystem(SystemAA, \"Internet Banking System\")\n}\nRel(b1, SystemAA, \"Contains\")",
    );
    let boundary = ast.boundary("b1").expect("boundary");
    match ast.c4_shape("b1") {
        Some(C4Element::Boundary(resolved)) => {
            assert!(std::ptr::eq(resolved, boundary));
        }
        other => panic!("expected the boundary, got {other:?}"),
    }
    let system = ast.shape("SystemAA").expect("shape");
    assert_eq!(system.type_c4_shape, Text::plain("system"));
    assert_eq!(ast.c4_shape("missing"), None);
}

// c4NamedAttributes.spec.ts

#[test]
fn keeps_tags_a_string_when_the_description_is_omitted() {
    let ast = ast("C4Context\nSystem(s, \"S\", $tags=\"cylinder\")");
    assert_eq!(only_shape(&ast).tags.as_deref(), Some("cylinder"));
}

#[test]
fn keeps_tags_a_string_when_the_description_is_given() {
    let ast = ast("C4Context\nSystem(s, \"S\", \"desc\", $tags=\"cylinder\")");
    let shape = only_shape(&ast);
    assert_eq!(shape.tags.as_deref(), Some("cylinder"));
    assert_eq!(shape.descr, Some(Text::plain("desc")));
}

#[test]
fn still_stores_the_description_as_text_when_it_is_the_one_named() {
    let ast = ast("C4Context\nSystem(s, \"S\", $descr=\"a description\")");
    assert_eq!(only_shape(&ast).descr, Some(Text::plain("a description")));
}

#[test]
fn keeps_sprite_and_link_strings_on_a_container() {
    let ast =
        ast("C4Container\nContainer(c, \"C\", $sprite=\"browser\", $link=\"https://x.test\")");
    let shape = only_shape(&ast);
    assert_eq!(shape.sprite.as_deref(), Some("browser"));
    assert_eq!(shape.link.as_deref(), Some("https://x.test"));
}

#[test]
fn keeps_a_relationship_tags_a_string_when_techn_and_descr_are_omitted() {
    let ast =
        ast("C4Context\nPerson(a, \"A\")\nPerson(b, \"B\")\nRel(a, b, \"uses\", $tags=\"async\")");
    assert_eq!(only_rel(&ast).tags.as_deref(), Some("async"));
}

#[test]
fn keeps_a_named_descr_that_lands_in_an_earlier_slot() {
    let ast = ast("C4Container\nContainer(c, \"C\", $descr=\"a description\")");
    assert_eq!(only_shape(&ast).descr, Some(Text::plain("a description")));
}

#[test]
fn still_defaults_descr_to_empty_when_it_is_genuinely_absent() {
    let ast = ast("C4Container\nContainer(c, \"C\", $techn=\"Java\")");
    let shape = only_shape(&ast);
    assert_eq!(shape.techn, Some(Text::plain("Java")));
    assert_eq!(shape.descr, Some(Text::plain("")));
}

#[test]
fn keeps_a_named_descr_on_a_relationship_where_techn_also_comes_first() {
    let ast = ast(
        "C4Context\nPerson(a, \"A\")\nPerson(b, \"B\")\nRel(a, b, \"uses\", $descr=\"how it is used\")",
    );
    assert_eq!(only_rel(&ast).descr, Some(Text::plain("how it is used")));
}

fn expect_shape_field(shape: &Shape, field: &str) {
    match field {
        "descr" => assert_eq!(shape.descr, Some(Text::plain("V")), "{field}"),
        "techn" => assert_eq!(shape.techn, Some(Text::plain("V")), "{field}"),
        "tags" => assert_eq!(shape.tags.as_deref(), Some("V"), "{field}"),
        "sprite" => assert_eq!(shape.sprite.as_deref(), Some("V"), "{field}"),
        "link" => assert_eq!(shape.link.as_deref(), Some("V"), "{field}"),
        other => panic!("unexpected shape field {other}"),
    }
}

fn expect_boundary_field(boundary: &Boundary, field: &str) {
    match field {
        "type" => assert_eq!(boundary.type_name, Some(Text::plain("V")), "{field}"),
        "descr" => assert_eq!(boundary.descr, Some(Text::plain("V")), "{field}"),
        "tags" => assert_eq!(boundary.tags.as_deref(), Some("V"), "{field}"),
        "sprite" => assert_eq!(boundary.sprite.as_deref(), Some("V"), "{field}"),
        "link" => assert_eq!(boundary.link.as_deref(), Some("V"), "{field}"),
        other => panic!("unexpected boundary field {other}"),
    }
}

fn expect_rel_field(rel: &Rel, field: &str) {
    match field {
        "techn" => assert_eq!(rel.techn, Some(Text::plain("V")), "{field}"),
        "descr" => assert_eq!(rel.descr, Some(Text::plain("V")), "{field}"),
        "tags" => assert_eq!(rel.tags.as_deref(), Some("V"), "{field}"),
        "sprite" => assert_eq!(rel.sprite.as_deref(), Some("V"), "{field}"),
        "link" => assert_eq!(rel.link.as_deref(), Some("V"), "{field}"),
        other => panic!("unexpected relation field {other}"),
    }
}

#[test]
fn one_named_attribute_on_an_element_keeps_its_own_shape() {
    let cases: &[(&str, &str, &[&str])] = &[
        (
            "C4Context",
            "System(x, \"L\", $F=\"V\")",
            &["descr", "tags", "sprite", "link"],
        ),
        (
            "C4Context",
            "Person(x, \"L\", $F=\"V\")",
            &["descr", "tags", "sprite", "link"],
        ),
        (
            "C4Container",
            "Container(x, \"L\", $F=\"V\")",
            &["descr", "techn", "tags", "sprite", "link"],
        ),
        (
            "C4Component",
            "Component(x, \"L\", $F=\"V\")",
            &["descr", "techn", "tags", "sprite", "link"],
        ),
    ];
    for (header, template, fields) in cases {
        for field in *fields {
            let body = format!("{header}\n{}", template.replace("$F", &format!("${field}")));
            let ast = ast(&body);
            expect_shape_field(only_shape(&ast), field);
        }
    }
}

#[test]
fn one_named_attribute_on_a_boundary_keeps_its_own_shape() {
    let cases: &[(&str, &str, &[&str])] = &[
        (
            "C4Context",
            "System_Boundary(x, \"L\", $F=\"V\")",
            &["type", "tags", "link"],
        ),
        (
            "C4Container",
            "Container_Boundary(x, \"L\", $F=\"V\")",
            &["type", "tags", "link"],
        ),
        (
            "C4Deployment",
            "Node(x, \"L\", $F=\"V\")",
            &["type", "descr", "tags", "sprite", "link"],
        ),
    ];
    for (header, template, fields) in cases {
        for field in *fields {
            let body = format!(
                "{header}\n{} {{\nContainer(i, \"I\")\n}}",
                template.replace("$F", &format!("${field}"))
            );
            let ast = ast(&body);
            assert_eq!(
                ast.boundaries.len(),
                2,
                "header {header}, template {template}"
            );
            expect_boundary_field(&ast.boundaries[1], field);
        }
    }
}

#[test]
fn one_named_attribute_on_a_relation_keeps_its_own_shape() {
    for field in ["techn", "descr", "tags", "sprite", "link"] {
        let body = format!(
            "C4Context\nPerson(a, \"A\")\nPerson(b, \"B\")\nRel(a, b, \"uses\", ${field}=\"V\")"
        );
        let ast = ast(&body);
        expect_rel_field(only_rel(&ast), field);
    }
}

// Database behaviour with no upstream parser spec of its own: the rest of `c4Db.ts`.

#[test]
fn a_deployment_node_keeps_its_own_fields_and_drops_the_positional_sprite() {
    let ast = ast(
        "C4Deployment\nNode(n, \"N\", \"T\", \"D\", \"sprite.png\", \"tag\", \"link\") {\nContainer(i, \"I\")\n}",
    );
    let node = ast.boundary("n").expect("node");
    assert_eq!(node.node_type.as_deref(), Some("node"));
    assert_eq!(node.type_name, Some(Text::plain("T")));
    assert_eq!(node.descr, Some(Text::plain("D")));
    assert_eq!(node.sprite, None);
    assert_eq!(node.tags.as_deref(), Some("tag"));
    assert_eq!(node.link.as_deref(), Some("link"));
    assert_eq!(node.parent_boundary, "global");
}

#[test]
fn updates_an_element_by_alias_instead_of_appending_it() {
    let ast = ast("C4Context\nPerson(a, \"First\")\nPerson(a, \"Second\", \"Description\")");
    assert_eq!(ast.shapes.len(), 1);
    let shape = &ast.shapes[0];
    assert_eq!(shape.label, Text::plain("Second"));
    assert_eq!(shape.descr, Some(Text::plain("Description")));
    assert_eq!(shape.type_c4_shape, Text::plain("person"));
}

#[test]
fn merges_a_relationship_between_the_same_endpoints() {
    let ast = ast(
        "C4Context\nPerson(a, \"A\")\nPerson(b, \"B\")\nRel(a, b, \"uses\", \"HTTP\")\nRel(a, b, \"calls\", \"JSON\")",
    );
    let rel = only_rel(&ast);
    assert_eq!(rel.label, Text::plain("calls"));
    assert_eq!(rel.techn, Some(Text::plain("JSON")));
}

#[test]
fn nests_boundaries_and_restores_the_parent() {
    let ast = ast(
        "C4Context\nSystem_Boundary(outer, \"Outer\") {\nContainer_Boundary(inner, \"Inner\") {\nContainer(c, \"C\")\n}\nSystem(s, \"S\")\n}\nPerson(p, \"P\")",
    );
    assert_eq!(
        ast.boundary("outer").expect("outer").parent_boundary,
        "global"
    );
    assert_eq!(
        ast.boundary("inner").expect("inner").parent_boundary,
        "outer"
    );
    assert_eq!(ast.shape("c").expect("c").parent_boundary, "inner");
    assert_eq!(ast.shape("s").expect("s").parent_boundary, "outer");
    assert_eq!(ast.shape("p").expect("p").parent_boundary, "global");
}

#[test]
fn update_layout_config_keeps_values_of_at_least_one() {
    let configured =
        ast("C4Context\nUpdateLayoutConfig($c4ShapeInRow=\"3\", $c4BoundaryInRow=\"1\")");
    assert_eq!(configured.shape_in_row, 3);
    assert_eq!(configured.boundary_in_row, 1);

    let unchanged =
        ast("C4Context\nUpdateLayoutConfig($c4ShapeInRow=\"0\", $c4BoundaryInRow=\"nope\")");
    assert_eq!(unchanged.shape_in_row, 4);
    assert_eq!(unchanged.boundary_in_row, 2);

    let defaults = ast("C4Context\nPerson(a, \"A\")");
    assert_eq!(defaults.shape_in_row, 4);
    assert_eq!(defaults.boundary_in_row, 2);
}

#[test]
fn update_el_style_sets_the_style_fields_of_a_shape() {
    let ast = ast(
        "C4Context\nPerson(a, \"A\")\nUpdateElementStyle(a, $bgColor=\"red\", $fontColor=\"white\", $borderColor=\"blue\", $shadowing=\"true\", $shape=\"box\", $sprite=\"users\", $techn=\"Go\", $legendText=\"Legend\", $legendSprite=\"icon\")",
    );
    let shape = only_shape(&ast);
    assert_eq!(shape.bg_color.as_deref(), Some("red"));
    assert_eq!(shape.font_color.as_deref(), Some("white"));
    assert_eq!(shape.border_color.as_deref(), Some("blue"));
    assert_eq!(shape.shadowing.as_deref(), Some("true"));
    assert_eq!(shape.shape.as_deref(), Some("box"));
    assert_eq!(shape.sprite.as_deref(), Some("users"));
    // `UpdateElementStyle` stores the positional `techn` as a bare string upstream.
    assert_eq!(shape.techn, Some(Text::bare("Go")));
    assert_eq!(shape.legend_text.as_deref(), Some("Legend"));
    assert_eq!(shape.legend_sprite.as_deref(), Some("icon"));
}

#[test]
fn update_el_style_sets_the_style_fields_of_a_boundary() {
    let ast = ast(
        "C4Context\nBoundary(b, \"B\") {\nSystem(s, \"S\")\n}\nUpdateElementStyle(b, $bgColor=\"red\")",
    );
    assert_eq!(
        ast.boundary("b").expect("b").bg_color.as_deref(),
        Some("red")
    );
}

#[test]
fn update_el_style_ignores_an_unknown_element() {
    let ast = ast("C4Context\nUpdateElementStyle(missing, $bgColor=\"red\")");
    assert!(ast.shapes.is_empty());
    assert_eq!(ast.boundaries.len(), 1);
}

#[test]
fn update_rel_style_sets_colors_and_offsets() {
    let ast = ast(
        "C4Context\nPerson(a, \"A\")\nPerson(b, \"B\")\nRel(a, b, \"uses\")\nUpdateRelStyle(a, b, $textColor=\"red\", $lineColor=\"blue\", $offsetX=\"10\", $offsetY=\"-40\")",
    );
    let rel = only_rel(&ast);
    assert_eq!(rel.text_color.as_deref(), Some("red"));
    assert_eq!(rel.line_color.as_deref(), Some("blue"));
    assert_eq!(rel.offset_x, Some(10));
    assert_eq!(rel.offset_y, Some(-40));
}

#[test]
fn update_rel_style_ignores_an_unknown_relationship() {
    let ast = ast("C4Context\nUpdateRelStyle(a, b, $textColor=\"red\")");
    assert!(ast.rels.is_empty());
}

#[test]
fn acc_title_sets_the_title_and_acc_descr_sets_the_description() {
    let ast = ast("C4Context\naccTitle: Accessible title\naccDescr: Accessible description");
    assert_eq!(ast.title, "Accessible title");
    assert_eq!(ast.acc_description, "Accessible description");
}

#[test]
fn acc_descr_accepts_a_multiline_block() {
    let ast = ast("C4Context\naccDescr {\n  first line\n  second line\n}");
    assert_eq!(ast.acc_description, "first line\nsecond line");
}

#[test]
fn acc_description_statement_sets_the_description() {
    let ast = ast("C4Context\naccDescription an accessible description");
    assert_eq!(ast.acc_description, "an accessible description");
}

#[test]
fn rel_index_ignores_its_index_argument() {
    let ast = ast("C4Context\nPerson(a, \"A\")\nPerson(b, \"B\")\nRelIndex(1, a, b, \"uses\")");
    let rel = only_rel(&ast);
    assert_eq!(rel.from, "a");
    assert_eq!(rel.to, "b");
    assert_eq!(rel.label, Text::plain("uses"));
}

#[test]
fn rel_up_and_its_siblings_are_not_shadowed_by_rel() {
    let ast = ast(
        "C4Context\nPerson(a, \"A\")\nPerson(b, \"B\")\nPerson(c, \"C\")\nRel_Up(a, b, \"uses\")\nBiRel(b, c, \"uses\")\nRel_Back(c, a, \"uses\")",
    );
    assert_eq!(ast.rels[0].type_name, "rel_u");
    assert_eq!(ast.rels[1].type_name, "birel");
    assert_eq!(ast.rels[2].type_name, "rel_b");
}

#[test]
fn comments_are_skipped() {
    let ast = ast("C4Context\n%% a comment\ntitle Title\n%% another comment\nPerson(a, \"A\")");
    assert_eq!(ast.title, "Title");
    assert_eq!(only_shape(&ast).alias, "a");
}

/// A `%%` after a statement is part of the statement: the title rule runs before the comment
/// rule and `%` is not one of its stop characters.
#[test]
fn a_comment_inside_a_title_line_stays_in_the_title() {
    let ast = ast("C4Context\ntitle Title %% trailing\nPerson(a, \"A\")");
    assert_eq!(ast.title, "Title %% trailing");
}

#[test]
fn a_direction_statement_sets_the_direction() {
    let ast = ast("direction TB");
    assert_eq!(ast.direction.as_deref(), Some("TB"));
    assert_eq!(ast.c4_type, None);
}

#[test]
fn a_header_without_a_newline_is_an_error() {
    let error = error("C4Context");
    assert!(
        error.message.contains("NEWLINE"),
        "message: {}",
        error.message
    );
}

#[test]
fn a_header_without_statements_is_an_error() {
    let error = error("C4Context\n");
    assert!(
        error.message.contains("statement"),
        "message: {}",
        error.message
    );
}

#[test]
fn a_statement_without_a_separator_is_an_error() {
    let error = error("C4Context\nPerson(a, \"A\") Person(b, \"B\")");
    assert!(
        error.message.contains("NEWLINE"),
        "message: {}",
        error.message
    );
}

#[test]
fn a_boundary_without_a_newline_after_its_brace_is_an_error() {
    let error = error("C4Context\nBoundary(b, \"B\") { System(s, \"S\") }");
    assert!(
        error.message.contains("NEWLINE"),
        "message: {}",
        error.message
    );
}

#[test]
fn an_unclosed_boundary_is_an_error() {
    let error = error("C4Context\nBoundary(b, \"B\") {\nSystem(s, \"S\")");
    assert!(
        error.message.contains("RBRACE") || error.message.contains("EOF"),
        "message: {}",
        error.message
    );
}

#[test]
fn an_unknown_statement_is_an_error() {
    let error = error("C4Context\nNonsense(a, \"b\")");
    assert!(!error.message.is_empty());
    assert!(error.span.is_some());
}

#[test]
fn a_named_value_without_a_value_is_an_error() {
    let error = error("C4Context\nPerson(a, $tags=\"\")");
    assert!(
        error.message.contains("STR_VALUE"),
        "message: {}",
        error.message
    );
}
