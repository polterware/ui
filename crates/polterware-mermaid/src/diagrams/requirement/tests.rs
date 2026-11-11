//! Ports of upstream `parser/requirementDiagram.spec.js` and `requirementDb.spec.ts`
//! (mermaid 12.3.0): every input is upstream's, and every assertion is upstream's unless it
//! needs the renderer.
//!
//! Skipped upstream tests, and why:
//! - `requirementDb.spec.ts` "should create unique edge ids for parallel relationships"
//!   reads `getData().edges`, the renderer's projection. This crate has no layout or
//!   rendering, so the edge ids and the `getData` node fields do not exist here; the
//!   relationships themselves are kept in [`Ast::relationships`].
//!
//! The `port_specific` modules check behavior the jison lexer and grammar have that the
//! upstream specs do not assert, because it is observable and easy to regress.

use super::db::Db;
use super::*;

fn ast(source: &str) -> Ast {
    parse(source).unwrap_or_else(|error| panic!("parse failed: {error}\nsource:\n{source}"))
}

fn failure(source: &str) -> crate::error::ParseError {
    match parse(source) {
        Ok(_) => panic!("expected a parse error\nsource:\n{source}"),
        Err(error) => error,
    }
}

fn requirement<'a>(ast: &'a Ast, name: &str) -> &'a Requirement {
    ast.requirement(name)
        .unwrap_or_else(|| panic!("missing requirement {name}"))
}

fn element<'a>(ast: &'a Ast, name: &str) -> &'a Element {
    ast.element(name)
        .unwrap_or_else(|| panic!("missing element {name}"))
}

fn class<'a>(ast: &'a Ast, name: &str) -> &'a RequirementClass {
    ast.class(name)
        .unwrap_or_else(|| panic!("missing class {name}"))
}

fn requirement_definition(kind: &str, risk: &str, verify: &str) -> String {
    format!(
        "requirementDiagram\n\n{kind} test_req {{\nid: test_id\ntext: the test text.\nrisk: {risk}\nverifymethod: {verify}\n}}"
    )
}

mod parser_spec {
    use super::*;

    #[test]
    fn accepts_a_full_requirement_definition() {
        let ast = ast(
            "requirementDiagram\n\nrequirement test_req {\nid: test_id\ntext: the test text.\nrisk: High\nverifymethod: Analysis\n}",
        );
        assert_eq!(ast.requirements.len(), 1);
        let found = requirement(&ast, "test_req");
        assert_eq!(found.requirement_id, "test_id");
        assert_eq!(found.text, "the test text.");
        assert!(ast.elements.is_empty());
        assert!(ast.relationships.is_empty());
    }

    #[test]
    fn accepts_a_full_element_definition() {
        let ast =
            ast("requirementDiagram\n\nelement test_el {\ntype: test_type\ndocref: test_ref\n}");
        assert!(ast.requirements.is_empty());
        assert_eq!(ast.elements.len(), 1);
        let found = element(&ast, "test_el");
        assert_eq!(found.type_name, "test_type");
        assert_eq!(found.doc_ref, "test_ref");
        assert!(ast.relationships.is_empty());
    }

    #[test]
    fn uses_an_accessibility_title_and_description() {
        let ast = ast("requirementDiagram
      accTitle: test title
      accDescr: my chart description
      element test_name {
      type: test_type
      docref: test_ref
      }");
        assert_eq!(ast.acc_title, "test title");
        assert_eq!(ast.acc_description, "my chart description");
    }

    #[test]
    fn uses_an_accessibility_title_and_multiline_description() {
        let ast = ast("requirementDiagram
      accTitle: test title
      accDescr {
        my chart description
line 2
      }
      element test_name {
      type: test_type
      docref: test_ref
      }");
        assert_eq!(ast.acc_title, "test title");
        assert_eq!(ast.acc_description, "my chart description\nline 2");
    }

    #[test]
    fn accepts_a_full_relationship_definition() {
        let ast = ast("requirementDiagram\n\na - contains -> b");
        assert!(ast.requirements.is_empty());
        assert!(ast.elements.is_empty());
        assert_eq!(ast.relationships.len(), 1);
        let found = &ast.relationships[0];
        assert_eq!(found.relationship_type, RelationshipType::Contains);
        assert_eq!(found.src, "a");
        assert_eq!(found.dst, "b");
    }

    #[test]
    fn accepts_every_requirement_type() {
        for (keyword, expected) in [
            ("requirement", RequirementType::Requirement),
            (
                "functionalRequirement",
                RequirementType::FunctionalRequirement,
            ),
            (
                "interfaceRequirement",
                RequirementType::InterfaceRequirement,
            ),
            (
                "performanceRequirement",
                RequirementType::PerformanceRequirement,
            ),
            ("physicalRequirement", RequirementType::PhysicalRequirement),
            ("designConstraint", RequirementType::DesignConstraint),
        ] {
            let ast = ast(&requirement_definition(keyword, "High", "Analysis"));
            assert_eq!(
                requirement(&ast, "test_req").requirement_type,
                expected,
                "keyword: {keyword}"
            );
        }
    }

    #[test]
    fn accepts_every_risk_level() {
        for (keyword, expected) in [
            ("Low", RiskLevel::Low),
            ("Medium", RiskLevel::Medium),
            ("High", RiskLevel::High),
        ] {
            let ast = ast(&requirement_definition(
                "designConstraint",
                keyword,
                "Analysis",
            ));
            assert_eq!(
                requirement(&ast, "test_req").risk,
                Some(expected),
                "keyword: {keyword}"
            );
        }
    }

    #[test]
    fn accepts_every_verification_method() {
        for (keyword, expected) in [
            ("Analysis", VerifyType::Analysis),
            ("Inspection", VerifyType::Inspection),
            ("Test", VerifyType::Test),
            ("Demonstration", VerifyType::Demonstration),
        ] {
            let ast = ast(&requirement_definition("designConstraint", "High", keyword));
            assert_eq!(
                requirement(&ast, "test_req").verify_method,
                Some(expected),
                "keyword: {keyword}"
            );
        }
    }

    #[test]
    fn accepts_every_relationship_type() {
        for (keyword, expected) in [
            ("contains", RelationshipType::Contains),
            ("copies", RelationshipType::Copies),
            ("derives", RelationshipType::Derives),
            ("satisfies", RelationshipType::Satisfies),
            ("verifies", RelationshipType::Verifies),
            ("refines", RelationshipType::Refines),
            ("traces", RelationshipType::Traces),
        ] {
            let source = format!("requirementDiagram\n\na - {keyword} -> b");
            let ast = ast(&source);
            let found = &ast.relationships[0];
            assert_eq!(found.relationship_type, expected, "keyword: {keyword}");
            assert_eq!(found.src, "a");
            assert_eq!(found.dst, "b");
        }
    }

    #[test]
    fn accepts_prototype_property_names_as_ids() {
        for name in ["__proto__", "constructor"] {
            let source = format!(
                "requirementDiagram
      requirement {name} {{
        id: 1
        text: the test text.
        risk: high
        verifymethod: test
      }}"
            );
            let ast = ast(&source);
            assert_eq!(ast.requirements.len(), 1, "name: {name}");
            assert!(ast.requirement(name).is_some(), "name: {name}");
        }
    }

    #[test]
    fn accepts_prototype_property_names_as_element_ids() {
        for name in ["__proto__", "constructor"] {
            let source = format!(
                "requirementDiagram
      element {name} {{
        type: simulation
      }}"
            );
            let ast = ast(&source);
            assert_eq!(ast.elements.len(), 1, "name: {name}");
            assert!(ast.element(name).is_some(), "name: {name}");
        }
    }

    #[test]
    fn accepts_styling_a_requirement() {
        let ast = ast(
            "requirementDiagram\n\nrequirement test_req {\n}\nstyle test_req fill:#f9f,stroke:#333,stroke-width:4px\n",
        );
        assert_eq!(
            requirement(&ast, "test_req").css_styles,
            vec!["fill:#f9f", "stroke:#333", "stroke-width:4px"]
        );
    }

    #[test]
    fn accepts_styling_an_element() {
        let ast = ast(
            "requirementDiagram\n\nelement test_element {\n}\nstyle test_element fill:#f9f,stroke:#333,stroke-width:4px\n",
        );
        assert_eq!(
            element(&ast, "test_element").css_styles,
            vec!["fill:#f9f", "stroke:#333", "stroke-width:4px"]
        );
    }

    #[test]
    fn accepts_styling_multiple_things_at_once() {
        let ast = ast(
            "requirementDiagram\n\nrequirement test_requirement {\n}\nelement test_element {\n}\nstyle test_requirement,test_element fill:#f9f,stroke:#333,stroke-width:4px\n",
        );
        assert_eq!(
            requirement(&ast, "test_requirement").css_styles,
            vec!["fill:#f9f", "stroke:#333", "stroke-width:4px"]
        );
        assert_eq!(
            element(&ast, "test_element").css_styles,
            vec!["fill:#f9f", "stroke:#333", "stroke-width:4px"]
        );
    }

    #[test]
    fn accepts_defining_a_class() {
        let ast =
            ast("requirementDiagram\n\nclassDef myClass fill:#f9f,stroke:#333,stroke-width:4px\n");
        assert_eq!(
            class(&ast, "myClass"),
            &RequirementClass {
                id: "myClass".to_string(),
                styles: vec![
                    "fill:#f9f".to_string(),
                    "stroke:#333".to_string(),
                    "stroke-width:4px".to_string(),
                ],
                text_styles: Vec::new(),
            }
        );
    }

    #[test]
    fn accepts_defining_multiple_classes_at_once() {
        let ast = ast(
            "requirementDiagram\n\nclassDef firstClass,secondClass fill:#f9f,stroke:#333,stroke-width:4px\n",
        );
        assert_eq!(
            class(&ast, "firstClass"),
            &RequirementClass {
                id: "firstClass".to_string(),
                styles: vec![
                    "fill:#f9f".to_string(),
                    "stroke:#333".to_string(),
                    "stroke-width:4px".to_string(),
                ],
                text_styles: Vec::new(),
            }
        );
        assert_eq!(
            class(&ast, "secondClass"),
            &RequirementClass {
                id: "secondClass".to_string(),
                styles: vec![
                    "fill:#f9f".to_string(),
                    "stroke:#333".to_string(),
                    "stroke-width:4px".to_string(),
                ],
                text_styles: Vec::new(),
            }
        );
    }

    #[test]
    fn accepts_assigning_a_class_via_the_class_statement() {
        let ast = ast(
            "requirementDiagram\n\nrequirement myReq {\n}\nclassDef myClass fill:#f9f,stroke:#333,stroke-width:4px\nclass myReq myClass\n",
        );
        assert_eq!(
            requirement(&ast, "myReq").classes,
            vec!["default".to_string(), "myClass".to_string()]
        );
    }

    #[test]
    fn accepts_assigning_multiple_classes_to_multiple_things() {
        let ast = ast(
            "requirementDiagram\n\nrequirement req {\n}\nelement elem {\n}\nclassDef class1,class2 fill:#f9f,stroke:#333,stroke-width:4px\nclass req,elem class1,class2\n",
        );
        assert_eq!(
            requirement(&ast, "req").classes,
            vec![
                "default".to_string(),
                "class1".to_string(),
                "class2".to_string()
            ]
        );
        assert_eq!(
            element(&ast, "elem").classes,
            vec![
                "default".to_string(),
                "class1".to_string(),
                "class2".to_string()
            ]
        );
    }

    #[test]
    fn accepts_assigning_a_class_via_the_shorthand_syntax() {
        let ast = ast(
            "requirementDiagram\n\nrequirement myReq {\n}\nclassDef myClass fill:#f9f,stroke:#333,stroke-width:4px\nmyReq:::myClass\n",
        );
        assert_eq!(
            requirement(&ast, "myReq").classes,
            vec!["default".to_string(), "myClass".to_string()]
        );
    }

    #[test]
    fn accepts_assigning_multiple_classes_via_the_shorthand_syntax() {
        let ast = ast(
            "requirementDiagram\n\nrequirement myReq {\n}\nclassDef class1 fill:#f9f,stroke:#333,stroke-width:4px\nclassDef class2 color:blue\nmyReq:::class1,class2\n",
        );
        assert_eq!(
            requirement(&ast, "myReq").classes,
            vec![
                "default".to_string(),
                "class1".to_string(),
                "class2".to_string()
            ]
        );
    }

    #[test]
    fn accepts_shorthand_classes_when_defining_requirements_and_elements() {
        let ast = ast(
            "requirementDiagram\n\nrequirement myReq:::class1 {\n}\nelement myElem:::class1,class2 {\n}\n\nclassDef class1 fill:#f9f,stroke:#333,stroke-width:4px\nclassDef class2 color:blue\n",
        );
        assert_eq!(
            requirement(&ast, "myReq").classes,
            vec!["default".to_string(), "class1".to_string()]
        );
        assert_eq!(
            element(&ast, "myElem").classes,
            vec![
                "default".to_string(),
                "class1".to_string(),
                "class2".to_string()
            ]
        );
    }

    #[test]
    fn accepts_direction_statements() {
        for (keyword, expected) in [
            ("TB", Direction::Tb),
            ("BT", Direction::Bt),
            ("LR", Direction::Lr),
            ("RL", Direction::Rl),
        ] {
            let source = format!("requirementDiagram\n\ndirection {keyword}\n");
            let ast = ast(&source);
            assert_eq!(ast.direction, expected, "direction: {keyword}");
        }
    }
}

mod db_spec {
    use super::*;

    #[test]
    fn adds_a_requirement() {
        let mut db = Db::new();
        db.add_requirement("requirement", RequirementType::Requirement);
        let ast = db.into_ast();
        assert!(ast.requirement("requirement").is_some());
    }

    #[test]
    fn adds_an_element() {
        let mut db = Db::new();
        db.add_element("element");
        let ast = db.into_ast();
        assert!(ast.element("element").is_some());
    }

    #[test]
    fn adds_a_relationship() {
        let mut db = Db::new();
        db.add_relationship(RelationshipType::Contains, "src", "dst");
        let ast = db.into_ast();
        assert_eq!(
            ast.relationships,
            vec![Relation {
                relationship_type: RelationshipType::Contains,
                src: "src".to_string(),
                dst: "dst".to_string(),
            }]
        );
    }

    #[test]
    fn detects_a_single_class() {
        let mut db = Db::new();
        db.define_class(&["a".to_string()], &["stroke-width: 8px".to_string()]);
        let ast = db.into_ast();
        assert_eq!(class(&ast, "a").styles, vec!["stroke-width: 8px"]);
    }

    #[test]
    fn detects_many_classes() {
        let mut db = Db::new();
        db.define_class(
            &["a".to_string(), "b".to_string()],
            &["stroke-width: 8px".to_string()],
        );
        let ast = db.into_ast();
        assert_eq!(class(&ast, "a").styles, vec!["stroke-width: 8px"]);
        assert_eq!(class(&ast, "b").styles, vec!["stroke-width: 8px"]);
    }

    #[test]
    fn detects_the_direction() {
        let mut db = Db::new();
        db.set_direction(Direction::Tb);
        assert_eq!(db.into_ast().direction, Direction::Tb);
    }

    #[test]
    fn adds_styles_to_a_requirement_and_element() {
        let mut db = Db::new();
        db.add_requirement("requirement", RequirementType::Requirement);
        db.set_css_style(&["requirement".to_string()], &["color:red".to_string()]);
        db.add_element("element");
        db.set_css_style(
            &["element".to_string()],
            &["stroke-width:4px".to_string(), "stroke: yellow".to_string()],
        );
        let ast = db.into_ast();
        assert_eq!(requirement(&ast, "requirement").css_styles, ["color:red"]);
        assert_eq!(
            element(&ast, "element").css_styles,
            ["stroke-width:4px", "stroke: yellow"]
        );
    }

    #[test]
    fn adds_classes_to_a_requirement_and_element() {
        let mut db = Db::new();
        db.add_requirement("requirement", RequirementType::Requirement);
        db.add_element("element");
        db.set_class(
            &["requirement".to_string(), "element".to_string()],
            &["myClass".to_string()],
        );
        let ast = db.into_ast();
        assert_eq!(
            requirement(&ast, "requirement").classes,
            ["default", "myClass"]
        );
        assert_eq!(element(&ast, "element").classes, ["default", "myClass"]);
    }

    #[test]
    fn adds_class_styles_to_a_requirement_and_element() {
        let mut db = Db::new();
        db.add_requirement("requirement", RequirementType::Requirement);
        db.add_element("element");
        db.define_class(&["myClass".to_string()], &["color:red".to_string()]);
        db.define_class(
            &["myClass2".to_string()],
            &["stroke-width:4px".to_string(), "stroke: yellow".to_string()],
        );
        db.set_class(&["requirement".to_string()], &["myClass".to_string()]);
        db.set_class(&["element".to_string()], &["myClass2".to_string()]);
        let ast = db.into_ast();
        assert_eq!(requirement(&ast, "requirement").css_styles, ["color:red"]);
        assert_eq!(
            element(&ast, "element").css_styles,
            ["stroke-width:4px", "stroke: yellow"]
        );
    }
}

mod port_specific {
    use super::*;

    #[test]
    fn accepts_the_left_arrow_relationship_form() {
        let ast = ast("requirementDiagram\n\na <- contains - b");
        assert_eq!(
            ast.relationships,
            vec![Relation {
                relationship_type: RelationshipType::Contains,
                src: "b".to_string(),
                dst: "a".to_string(),
            }]
        );
    }

    #[test]
    fn keeps_the_first_definition_of_a_repeated_name() {
        let ast = ast(
            "requirementDiagram\nrequirement r {\nid: one\ntext: first\n}\nrequirement r {\nid: two\ntext: second\nrisk: high\n}",
        );
        let found = requirement(&ast, "r");
        assert_eq!(found.requirement_id, "one");
        assert_eq!(found.text, "first");
        assert_eq!(found.risk, None);
    }

    #[test]
    fn accepts_quoted_names_and_values() {
        let ast = ast(
            "requirementDiagram\nrequirement \"test req\" {\nid: \"the id\"\ntext: \"the text\"\n}",
        );
        let found = requirement(&ast, "test req");
        assert_eq!(found.requirement_id, "the id");
        assert_eq!(found.text, "the text");
    }

    #[test]
    fn skips_percent_and_hash_comments() {
        let ast = ast(
            "requirementDiagram\n%% a comment\n# another comment\nrequirement r {\n%% inside\n}",
        );
        assert!(ast.requirement("r").is_some());
    }

    #[test]
    fn rejects_reserved_words_as_field_values() {
        // `low` is the LOW_RISK token, and `id` only accepts `unqString` or `qString`.
        failure("requirementDiagram\nrequirement r {\nid: low\n}");
    }

    #[test]
    fn rejects_a_style_statement_without_a_trailing_newline() {
        // The style state only returns to INITIAL on `\n`; jison's `EOF` rule exists in
        // INITIAL only, so the parser never receives the terminal the grammar needs.
        failure("requirementDiagram\nstyle r fill:red");
    }

    #[test]
    fn rejects_a_brace_followed_by_spaces_before_its_newline() {
        // `\s+` runs before jison tries `(\r?\n)+` again, swallowing the newline the
        // requirement body needs.
        failure("requirementDiagram\nrequirement r {   \nid: x\n}");
    }

    #[test]
    fn a_line_containing_direction_is_a_direction_statement() {
        let ast = ast("requirementDiagram\nfoo direction TB bar\n");
        assert_eq!(ast.direction, Direction::Tb);
        assert!(ast.requirements.is_empty());
    }

    #[test]
    fn accepts_an_empty_multiline_accessibility_block() {
        // `accDescr {` produces no token when the block closes before any content; the
        // closing `}` is consumed by the lexer.
        let ast = ast("requirementDiagram\naccDescr {}\nrequirement r {\n}\n");
        assert_eq!(ast.acc_description, "");
        assert!(ast.requirement("r").is_some());
    }

    #[test]
    fn drops_spaces_inside_style_values() {
        let ast = ast("requirementDiagram\nrequirement r {\n}\nstyle r fill: red\n");
        assert_eq!(requirement(&ast, "r").css_styles, ["fill:red"]);
    }

    #[test]
    fn rejects_percent_in_a_style_value() {
        // The lexer returns `PERCENT`, which the grammar never declares; `PCT` is a
        // different, unreachable terminal.
        failure("requirementDiagram\nstyle r width:50%\n");
    }

    #[test]
    fn accepts_case_insensitive_keywords() {
        let ast = ast(
            "REQUIREMENTDIAGRAM\nFUNCTIONALREQUIREMENT R {\nID: x\nTEXT: y\nRISK: HIGH\nVERIFYMETHOD: TEST\n}",
        );
        let found = requirement(&ast, "R");
        assert_eq!(
            found.requirement_type,
            RequirementType::FunctionalRequirement
        );
        assert_eq!(found.requirement_id, "x");
        assert_eq!(found.risk, Some(RiskLevel::High));
        assert_eq!(found.verify_method, Some(VerifyType::Test));
    }

    #[test]
    fn a_defined_default_class_styles_the_nodes_that_already_carry_it() {
        let ast = ast("requirementDiagram\nrequirement r {\n}\nclassDef default fill:#f9f\n");
        assert_eq!(requirement(&ast, "r").css_styles, ["fill:#f9f"]);
    }

    #[test]
    fn set_css_style_stops_at_the_first_unknown_id() {
        let mut db = Db::new();
        db.add_requirement("a", RequirementType::Requirement);
        db.set_css_style(
            &["missing".to_string(), "a".to_string()],
            &["fill:red".to_string()],
        );
        let ast = db.into_ast();
        assert!(requirement(&ast, "a").css_styles.is_empty());
    }

    #[test]
    fn define_class_extracts_text_styles() {
        let ast = ast("requirementDiagram\nclassDef c fill:#f9f,color:blue\n");
        let found = class(&ast, "c");
        assert_eq!(found.styles, ["fill:#f9f", "color:blue"]);
        assert_eq!(found.text_styles, ["color:blue"]);
    }

    #[test]
    fn accepts_directives_before_the_header() {
        let ast = ast("accTitle: hello\naccDescr: world\nrequirementDiagram\nrequirement r {\n}\n");
        assert_eq!(ast.acc_title, "hello");
        assert_eq!(ast.acc_description, "world");
        assert!(ast.requirement("r").is_some());
    }

    #[test]
    fn skips_comments_before_the_header() {
        // Upstream deletes comment lines before parsing; this crate's lexer leaves their
        // line breaks, which are skipped like the blank lines preprocess leaves.
        let ast = ast("%% a comment\naccTitle: hello\n%% another\nrequirementDiagram\n");
        assert_eq!(ast.acc_title, "hello");
    }

    #[test]
    fn accepts_a_multiline_description_that_closes_on_its_line() {
        let ast = ast("requirementDiagram\naccDescr { desc }\nrequirement r {\n}\n");
        assert_eq!(ast.acc_description, "desc");
        assert!(ast.requirement("r").is_some());
    }

    #[test]
    fn errors_carry_a_span() {
        let error = failure("requirementDiagram\nrequirement r {\nid: low\n}");
        let span = error.span.expect("a span");
        assert_eq!(span.line, 3);
    }

    #[test]
    fn skips_the_blank_lines_preprocess_leaves_behind() {
        // `preprocess` blanks removed frontmatter and directive lines instead of deleting
        // them, so the header can arrive after empty lines.
        let ast = ast("\n\n\nrequirementDiagram\nrequirement r {\n}\n");
        assert!(ast.requirement("r").is_some());
    }

    #[test]
    fn parses_through_the_crate_entry_point() {
        let body = "---\ntitle: Release\n---\nrequirementDiagram\nrequirement r {\nid: x\n}\n";
        match crate::parse(body).expect("the crate parses the diagram") {
            crate::Diagram::Requirement(ast) => {
                assert_eq!(requirement(&ast, "r").requirement_id, "x");
            }
            _ => panic!("expected a requirement diagram"),
        }
    }
}
