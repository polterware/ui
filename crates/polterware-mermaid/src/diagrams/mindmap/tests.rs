//! Ports of upstream `mindmap.spec.ts`, `mindmapDb.getData.test.ts`, and
//! `mindmapColors.spec.ts` (mermaid 12.3.0): every input is upstream's, and every assertion
//! is upstream's unless it needs a renderer or a configuration value.
//!
//! Skipped upstream tests and assertions, and why:
//! - `mindmapLayout.spec.ts` is configuration only (`config.layout` resolving to
//!   `cose-bilkent`); the parser reads no configuration.
//! - Every `configApi.setSiteConfig`/`getConfig` assertion: `colorBy` is passed to
//!   [`Ast::data_with`] instead of read from config, so the color tests keep their inputs
//!   and expected sections with the mode as an argument. `getData().config` itself is not
//!   modelled. The `it.each([undefined, 'branch'])` case of `mindmapColors.spec.ts` runs
//!   once, since both values select the default mode.
//! - The theme-dependent shape assertion of `flattenNodes` (`redux` turns the default shape
//!   into `rounded`), `look`, and `cssStyles`: renderer and configuration values.
//! - `node.x`/`node.y` being `undefined` in `getData`: layout writes them after the call, so
//!   the port has no such fields.
//! - The `sanitizeText` (DOMPurify) effect on descriptions, which the db applies and the
//!   port leaves to the painter.

use super::db::{Db, node_type};
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

fn mindmap(ast: &Ast) -> &Node {
    ast.root.as_ref().expect("no mindmap")
}

fn add(db: &mut Db, level: i32, id: &str, descr: &str, node_type: NodeType) {
    db.add_node(level, id, descr, node_type)
        .unwrap_or_else(|message| panic!("add_node failed: {message}"));
}

fn sections(data: &Data) -> Vec<Option<usize>> {
    data.nodes.iter().map(|node| node.section).collect()
}

mod hierarchy {
    use super::*;

    #[test]
    fn mmp_1_should_handle_a_simple_root_definition_abc122() {
        let ast = ast("mindmap\n    root");
        assert_eq!(mindmap(&ast).descr, "root");
    }

    #[test]
    fn mmp_2_should_handle_a_hierarchical_mindmap_definition() {
        let ast = ast("mindmap\n    root\n      child1\n      child2\n ");
        let mm = mindmap(&ast);
        assert_eq!(mm.descr, "root");
        assert_eq!(mm.children.len(), 2);
        assert_eq!(mm.children[0].descr, "child1");
        assert_eq!(mm.children[1].descr, "child2");
    }

    #[test]
    fn mmp_3_should_handle_a_simple_root_definition_with_a_shape_and_without_an_id_abc123() {
        let ast = ast("mindmap\n    (root)");
        assert_eq!(mindmap(&ast).descr, "root");
    }

    #[test]
    fn mmp_4_should_handle_a_deeper_hierarchical_mindmap_definition() {
        let ast = ast("mindmap\n    root\n      child1\n        leaf1\n      child2");
        let mm = mindmap(&ast);
        assert_eq!(mm.descr, "root");
        assert_eq!(mm.children.len(), 2);
        assert_eq!(mm.children[0].descr, "child1");
        assert_eq!(mm.children[0].children[0].descr, "leaf1");
        assert_eq!(mm.children[1].descr, "child2");
    }

    #[test]
    fn mmp_5_multiple_roots_are_illegal() {
        let error = failure("mindmap\n    root\n    fakeRoot");
        assert_eq!(
            error.message,
            "There can be only one root. No parent could be found for (\"fakeRoot\")"
        );
        assert_eq!(
            error.span.map(|span| (span.line, span.column)),
            Some((3, 5))
        );
    }

    #[test]
    fn mmp_6_real_root_in_wrong_place() {
        let error = failure("mindmap\n          root\n        fakeRoot\n    realRootWrongPlace");
        assert_eq!(
            error.message,
            "There can be only one root. No parent could be found for (\"fakeRoot\")"
        );
    }
}

mod nodes {
    use super::*;

    #[test]
    fn mmp_7_should_handle_an_id_and_type_for_a_node_definition() {
        let ast = ast("mindmap\n    root[The root]\n      ");
        let mm = mindmap(&ast);
        assert_eq!(mm.node_id, "root");
        assert_eq!(mm.descr, "The root");
        assert_eq!(mm.node_type, NodeType::Rect);
    }

    #[test]
    fn mmp_8_should_handle_an_id_and_type_for_a_node_definition() {
        let ast = ast("mindmap\n    root\n      theId(child1)");
        let mm = mindmap(&ast);
        assert_eq!(mm.descr, "root");
        assert_eq!(mm.children.len(), 1);
        let child = &mm.children[0];
        assert_eq!(child.descr, "child1");
        assert_eq!(child.node_id, "theId");
        assert_eq!(child.node_type, NodeType::RoundedRect);
    }

    #[test]
    fn mmp_9_should_handle_an_id_and_type_for_a_node_definition() {
        let ast = ast("mindmap\nroot\n      theId(child1)");
        let mm = mindmap(&ast);
        assert_eq!(mm.descr, "root");
        assert_eq!(mm.children.len(), 1);
        let child = &mm.children[0];
        assert_eq!(child.descr, "child1");
        assert_eq!(child.node_id, "theId");
        assert_eq!(child.node_type, NodeType::RoundedRect);
    }

    #[test]
    fn mmp_10_multiple_types_circle() {
        let ast = ast("mindmap\n root((the root))\n  ");
        let mm = mindmap(&ast);
        assert_eq!(mm.descr, "the root");
        assert_eq!(mm.children.len(), 0);
        assert_eq!(mm.node_type, NodeType::Circle);
    }

    #[test]
    fn mmp_11_multiple_types_cloud() {
        let ast = ast("mindmap\n root)the root(\n");
        let mm = mindmap(&ast);
        assert_eq!(mm.descr, "the root");
        assert_eq!(mm.children.len(), 0);
        assert_eq!(mm.node_type, NodeType::Cloud);
    }

    #[test]
    fn mmp_12_multiple_types_bang() {
        let ast = ast("mindmap\n root))the root((\n");
        let mm = mindmap(&ast);
        assert_eq!(mm.descr, "the root");
        assert_eq!(mm.children.len(), 0);
        assert_eq!(mm.node_type, NodeType::Bang);
    }

    #[test]
    fn mmp_12_a_multiple_types_hexagon() {
        let ast = ast("mindmap\n root{{the root}}\n");
        let mm = mindmap(&ast);
        assert_eq!(mm.node_type, NodeType::Hexagon);
        assert_eq!(mm.descr, "the root");
        assert_eq!(mm.children.len(), 0);
    }
}

mod decorations {
    use super::*;

    #[test]
    fn mmp_13_should_be_possible_to_set_an_icon_for_the_node() {
        let ast = ast("mindmap\n    root[The root]\n    ::icon(bomb)\n    ");
        let mm = mindmap(&ast);
        assert_eq!(mm.node_id, "root");
        assert_eq!(mm.descr, "The root");
        assert_eq!(mm.node_type, NodeType::Rect);
        assert_eq!(mm.icon.as_deref(), Some("bomb"));
    }

    #[test]
    fn mmp_14_should_be_possible_to_set_classes_for_the_node() {
        let ast = ast("mindmap\n    root[The root]\n    :::m-4 p-8\n    ");
        let mm = mindmap(&ast);
        assert_eq!(mm.node_id, "root");
        assert_eq!(mm.descr, "The root");
        assert_eq!(mm.node_type, NodeType::Rect);
        assert_eq!(mm.class.as_deref(), Some("m-4 p-8"));
    }

    #[test]
    fn mmp_15_should_be_possible_to_set_both_classes_and_icon_for_the_node() {
        let ast = ast("mindmap\n    root[The root]\n    :::m-4 p-8\n    ::icon(bomb)\n    ");
        let mm = mindmap(&ast);
        assert_eq!(mm.node_id, "root");
        assert_eq!(mm.descr, "The root");
        assert_eq!(mm.node_type, NodeType::Rect);
        assert_eq!(mm.class.as_deref(), Some("m-4 p-8"));
        assert_eq!(mm.icon.as_deref(), Some("bomb"));
    }

    #[test]
    fn mmp_16_should_be_possible_to_set_both_classes_and_icon_for_the_node() {
        let ast = ast("mindmap\n    root[The root]\n    ::icon(bomb)\n    :::m-4 p-8\n    ");
        let mm = mindmap(&ast);
        assert_eq!(mm.node_id, "root");
        assert_eq!(mm.descr, "The root");
        assert_eq!(mm.node_type, NodeType::Rect);
        assert_eq!(mm.class.as_deref(), Some("m-4 p-8"));
        assert_eq!(mm.icon.as_deref(), Some("bomb"));
    }
}

mod descriptions {
    use super::*;

    #[test]
    fn mmp_17_should_be_possible_to_use_node_syntax_in_the_descriptions() {
        let ast = ast("mindmap\n    root[\"String containing []\"]\n");
        let mm = mindmap(&ast);
        assert_eq!(mm.node_id, "root");
        assert_eq!(mm.descr, "String containing []");
    }

    #[test]
    fn mmp_18_should_be_possible_to_use_node_syntax_in_the_descriptions_in_children() {
        let ast = ast(
            "mindmap\n    root[\"String containing []\"]\n      child1[\"String containing ()\"]\n",
        );
        let mm = mindmap(&ast);
        assert_eq!(mm.node_id, "root");
        assert_eq!(mm.descr, "String containing []");
        assert_eq!(mm.children.len(), 1);
        assert_eq!(mm.children[0].descr, "String containing ()");
    }

    #[test]
    fn mmp_19_should_be_possible_to_have_a_child_after_a_class_assignment() {
        let ast = ast(
            "mindmap\n  root(Root)\n    Child(Child)\n    :::hot\n      a(a)\n      b[New Stuff]",
        );
        let mm = mindmap(&ast);
        assert_eq!(mm.node_id, "root");
        assert_eq!(mm.descr, "Root");
        assert_eq!(mm.children.len(), 1);

        let child = &mm.children[0];
        assert_eq!(child.node_id, "Child");
        assert_eq!(child.class.as_deref(), Some("hot"));
        assert_eq!(child.children[0].node_id, "a");
        assert_eq!(child.children.len(), 2);
        assert_eq!(child.children[1].node_id, "b");
    }
}

#[test]
fn mmp_20_should_be_possible_to_have_meaningless_empty_rows_in_a_mindmap_abc124() {
    let ast = ast("mindmap\n  root(Root)\n    Child(Child)\n      a(a)\n\n      b[New Stuff]");
    let mm = mindmap(&ast);
    assert_eq!(mm.node_id, "root");
    assert_eq!(mm.descr, "Root");
    assert_eq!(mm.children.len(), 1);

    let child = &mm.children[0];
    assert_eq!(child.node_id, "Child");
    assert_eq!(child.children[0].node_id, "a");
    assert_eq!(child.children.len(), 2);
    assert_eq!(child.children[1].node_id, "b");
}

#[test]
fn mmp_21_should_be_possible_to_have_comments_in_a_mindmap() {
    let ast = ast(
        "mindmap\n  root(Root)\n    Child(Child)\n      a(a)\n\n      %% This is a comment\n      b[New Stuff]",
    );
    let mm = mindmap(&ast);
    assert_eq!(mm.node_id, "root");
    assert_eq!(mm.descr, "Root");
    assert_eq!(mm.children.len(), 1);

    let child = &mm.children[0];
    assert_eq!(child.node_id, "Child");
    assert_eq!(child.children[0].node_id, "a");
    assert_eq!(child.children.len(), 2);
    assert_eq!(child.children[1].node_id, "b");
}

#[test]
fn mmp_22_should_be_possible_to_have_comments_at_the_end_of_a_line() {
    let ast = ast(
        "mindmap\n  root(Root)\n    Child(Child)\n      a(a) %% This is a comment\n      b[New Stuff]",
    );
    let mm = mindmap(&ast);
    assert_eq!(mm.node_id, "root");
    assert_eq!(mm.descr, "Root");
    assert_eq!(mm.children.len(), 1);

    let child = &mm.children[0];
    assert_eq!(child.node_id, "Child");
    assert_eq!(child.children[0].node_id, "a");
    assert_eq!(child.children.len(), 2);
    assert_eq!(child.children[1].node_id, "b");
}

#[test]
fn mmp_23_rows_with_only_spaces_should_not_interfere() {
    let ast = ast("mindmap\nroot\n A\n \n\n B");
    let mm = mindmap(&ast);
    assert_eq!(mm.node_id, "root");
    assert_eq!(mm.children.len(), 2);
    assert_eq!(mm.children[0].node_id, "A");
    assert_eq!(mm.children[1].node_id, "B");
}

#[test]
fn mmp_24_handle_rows_above_the_mindmap_declarations() {
    let ast = ast("\n \nmindmap\nroot\n A\n \n\n B");
    let mm = mindmap(&ast);
    assert_eq!(mm.node_id, "root");
    assert_eq!(mm.children.len(), 2);
    assert_eq!(mm.children[0].node_id, "A");
    assert_eq!(mm.children[1].node_id, "B");
}

#[test]
fn mmp_25_handle_rows_above_the_mindmap_declarations_no_space() {
    let ast = ast("\n\n\nmindmap\nroot\n A\n \n\n B");
    let mm = mindmap(&ast);
    assert_eq!(mm.node_id, "root");
    assert_eq!(mm.children.len(), 2);
    assert_eq!(mm.children[0].node_id, "A");
    assert_eq!(mm.children[1].node_id, "B");
}

mod get_data {
    use super::*;

    #[test]
    fn should_return_empty_data_when_no_mindmap_is_set() {
        let ast = Ast::default();
        let data = ast.data();

        assert_eq!(data.nodes, Vec::new());
        assert_eq!(data.edges, Vec::new());
        // Upstream's `config` is the caller's; its `rootNode` is `getMindmap`.
        assert!(ast.root.is_none());
        // The fields `getData` sets for every diagram.
        assert_eq!(data.markers, vec!["point".to_string()]);
        assert_eq!(data.direction, "TB");
        assert_eq!(data.node_spacing, 50.0);
        assert_eq!(data.rank_spacing, 50.0);
        assert_eq!(data.type_name, "mindmap");
    }

    #[test]
    fn should_return_structured_data_for_simple_mindmap() {
        let mut db = Db::new();
        add(&mut db, 0, "root", "Root Node", NodeType::NoBorder);
        add(&mut db, 1, "child1", "Child 1", NodeType::NoBorder);
        add(&mut db, 1, "child2", "Child 2", NodeType::NoBorder);
        let ast = db.into_ast();

        let data = ast.data();

        assert_eq!(data.nodes.len(), 3);
        assert_eq!(data.edges.len(), 2);
        assert!(ast.root.is_some());

        let root = data.nodes.iter().find(|node| node.id == "0").unwrap();
        assert_eq!(root.label, "Root Node");
        assert_eq!(root.level, 0);

        let child1 = data.nodes.iter().find(|node| node.id == "1").unwrap();
        assert_eq!(child1.label, "Child 1");
        assert_eq!(child1.level, 1);

        let edge = data
            .edges
            .iter()
            .find(|edge| edge.start == "0" && edge.end == "1")
            .expect("no edge 0 -> 1");
        assert_eq!(edge.depth, 0);
    }

    #[test]
    fn should_return_structured_data_for_hierarchical_mindmap() {
        let mut db = Db::new();
        add(&mut db, 0, "root", "Root Node", NodeType::NoBorder);
        add(&mut db, 1, "child1", "Child 1", NodeType::NoBorder);
        add(
            &mut db,
            2,
            "grandchild1",
            "Grandchild 1",
            NodeType::NoBorder,
        );
        add(
            &mut db,
            2,
            "grandchild2",
            "Grandchild 2",
            NodeType::NoBorder,
        );
        add(&mut db, 1, "child2", "Child 2", NodeType::NoBorder);
        let ast = db.into_ast();

        let data = ast.data();

        assert_eq!(data.nodes.len(), 5);
        assert_eq!(data.edges.len(), 4);

        let levels: Vec<i32> = data.nodes.iter().map(|node| node.level).collect();
        assert!(levels.contains(&0));
        assert!(levels.contains(&1));
        assert!(levels.contains(&2));

        let edge_relations: Vec<String> = data
            .edges
            .iter()
            .map(|edge| format!("{}->{}", edge.start, edge.end))
            .collect();
        assert!(edge_relations.contains(&"0->1".to_string()));
        assert!(edge_relations.contains(&"1->2".to_string()));
        assert!(edge_relations.contains(&"1->3".to_string()));
        assert!(edge_relations.contains(&"0->4".to_string()));
    }

    #[test]
    fn should_preserve_node_properties_in_processed_data() {
        let mut db = Db::new();
        add(&mut db, 0, "root", "Root Node", NodeType::Rect);
        let mut ast = db.into_ast();
        let mindmap = ast.root.as_mut().expect("no mindmap");
        mindmap.width = 150.0;
        mindmap.height = Some(75.0);
        mindmap.padding = 15.0;
        mindmap.class = Some("custom-class".to_string());
        mindmap.icon = Some("star".to_string());

        let data = ast.data();

        assert_eq!(data.nodes.len(), 1);
        let node = &data.nodes[0];
        assert_eq!(node.node_type, NodeType::Rect);
        assert_eq!(node.width, 150.0);
        assert_eq!(node.height, 75.0);
        assert_eq!(node.padding, 15.0);
        // The root's section is undefined whatever a caller set before `getData`.
        assert_eq!(node.section, None);
        assert_eq!(
            node.css_classes,
            "mindmap-node section-root section--1 custom-class"
        );
        assert_eq!(node.icon.as_deref(), Some("star"));
    }

    #[test]
    fn should_generate_unique_edge_ids() {
        let mut db = Db::new();
        add(&mut db, 0, "root", "Root Node", NodeType::NoBorder);
        add(&mut db, 1, "child1", "Child 1", NodeType::NoBorder);
        add(&mut db, 1, "child2", "Child 2", NodeType::NoBorder);
        add(&mut db, 1, "child3", "Child 3", NodeType::NoBorder);
        let ast = db.into_ast();

        let data = ast.data();

        let edge_ids: Vec<&String> = data.edges.iter().map(|edge| &edge.id).collect();
        assert_eq!(edge_ids.len(), 3);
        let mut unique = edge_ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 3);
    }

    #[test]
    fn should_handle_nodes_with_missing_optional_properties() {
        let mut db = Db::new();
        add(&mut db, 0, "root", "Root Node", NodeType::NoBorder);
        let ast = db.into_ast();

        let data = ast.data();
        let node = &data.nodes[0];

        assert_eq!(node.section, None);
        assert_eq!(node.css_classes, "mindmap-node section-root section--1");
        assert_eq!(node.icon, None);
    }

    #[test]
    fn should_centre_labels_of_every_node_shape_issue_8387() {
        let mut db = Db::new();
        add(&mut db, 0, "root", "Cell", NodeType::Circle);
        add(&mut db, 1, "a", "Square", NodeType::Rect);
        add(&mut db, 1, "b", "Rounded", NodeType::RoundedRect);
        add(&mut db, 1, "c", "Hexagon", NodeType::Hexagon);
        add(&mut db, 1, "d", "Cloud", NodeType::Cloud);
        add(&mut db, 1, "e", "Bang", NodeType::Bang);
        add(&mut db, 1, "f", "Default", NodeType::NoBorder);
        let ast = db.into_ast();

        let data = ast.data();

        assert_eq!(data.nodes.len(), 7);
        for node in &data.nodes {
            assert!(node.center_label, "label of {} not centered", node.label);
        }
    }

    #[test]
    fn should_assign_correct_section_classes_based_on_sibling_position() {
        let mut db = Db::new();
        add(&mut db, 0, "A", "A", NodeType::NoBorder);
        add(&mut db, 1, "a0", "a0", NodeType::NoBorder);
        add(&mut db, 2, "aa0", "aa0", NodeType::NoBorder);
        add(&mut db, 1, "a1", "a1", NodeType::NoBorder);
        add(&mut db, 2, "aaa", "aaa", NodeType::NoBorder);
        add(&mut db, 1, "a2", "a2", NodeType::NoBorder);
        let ast = db.into_ast();

        let data = ast.data();

        let node = |label: &str| {
            data.nodes
                .iter()
                .find(|node| node.label == label)
                .unwrap_or_else(|| panic!("missing node {label}"))
        };

        assert_eq!(
            sections(&data),
            [None, Some(0), Some(0), Some(1), Some(1), Some(2)]
        );

        assert_eq!(
            node("A").css_classes,
            "mindmap-node section-root section--1"
        );
        assert_eq!(node("a0").css_classes, "mindmap-node section-0");
        assert_eq!(node("aa0").css_classes, "mindmap-node section-0");
        assert_eq!(node("a1").css_classes, "mindmap-node section-1");
        assert_eq!(node("aaa").css_classes, "mindmap-node section-1");
        assert_eq!(node("a2").css_classes, "mindmap-node section-2");
    }

    #[test]
    fn should_preserve_custom_classes_while_adding_section_classes() {
        let mut db = Db::new();
        add(&mut db, 0, "root", "Root Node", NodeType::NoBorder);
        add(&mut db, 1, "child", "Child Node", NodeType::NoBorder);
        let mut ast = db.into_ast();
        let mindmap = ast.root.as_mut().expect("no mindmap");
        mindmap.class = Some("custom-root-class".to_string());
        mindmap.children[0].class = Some("custom-child-class".to_string());

        let data = ast.data();
        let node = |label: &str| {
            data.nodes
                .iter()
                .find(|node| node.label == label)
                .unwrap_or_else(|| panic!("missing node {label}"))
        };

        assert_eq!(
            node("Root Node").css_classes,
            "mindmap-node section-root section--1 custom-root-class"
        );
        assert_eq!(
            node("Child Node").css_classes,
            "mindmap-node section-0 custom-child-class"
        );
    }

    #[test]
    fn should_not_create_any_fake_root_nodes() {
        let mut db = Db::new();
        add(&mut db, 0, "A", "A", NodeType::NoBorder);
        add(&mut db, 1, "a0", "a0", NodeType::NoBorder);
        add(&mut db, 1, "a1", "a1", NodeType::NoBorder);
        let ast = db.into_ast();

        let data = ast.data();

        assert_eq!(data.nodes.len(), 3);
        let labels: Vec<&str> = data.nodes.iter().map(|node| node.label.as_str()).collect();
        assert_eq!(labels, ["A", "a0", "a1"]);

        assert!(data.nodes.iter().all(|node| node.label != "mindmap"));

        let root = data.nodes.iter().find(|node| node.label == "A").unwrap();
        assert_eq!(root.css_classes, "mindmap-node section-root section--1");
        assert_eq!(root.level, 0);
    }

    #[test]
    fn should_assign_correct_section_classes_to_edges() {
        let mut db = Db::new();
        add(&mut db, 0, "A", "A", NodeType::NoBorder);
        add(&mut db, 1, "a0", "a0", NodeType::NoBorder);
        add(&mut db, 2, "aa0", "aa0", NodeType::NoBorder);
        add(&mut db, 1, "a1", "a1", NodeType::NoBorder);
        add(&mut db, 2, "aaa", "aaa", NodeType::NoBorder);
        add(&mut db, 1, "a2", "a2", NodeType::NoBorder);
        let ast = db.into_ast();

        let data = ast.data();

        assert_eq!(data.edges.len(), 5);

        let edge = |start: &str, end: &str| {
            data.edges
                .iter()
                .find(|edge| edge.start == start && edge.end == end)
                .unwrap_or_else(|| panic!("missing edge {start} -> {end}"))
        };

        assert_eq!(edge("0", "1").classes, "edge section-edge-0 edge-depth-1");
        assert_eq!(edge("1", "2").classes, "edge section-edge-0 edge-depth-2");
        assert_eq!(edge("0", "3").classes, "edge section-edge-1 edge-depth-1");
        assert_eq!(edge("3", "4").classes, "edge section-edge-1 edge-depth-2");
        assert_eq!(edge("0", "5").classes, "edge section-edge-2 edge-depth-1");

        assert_eq!(edge("0", "1").section, Some(0));
        assert_eq!(edge("1", "2").section, Some(0));
        assert_eq!(edge("0", "3").section, Some(1));
        assert_eq!(edge("3", "4").section, Some(1));
        assert_eq!(edge("0", "5").section, Some(2));
    }

    #[test]
    fn should_wrap_section_numbers_when_there_are_more_than_11_level_2_nodes() {
        let mut db = Db::new();
        add(&mut db, 0, "root", "Example", NodeType::NoBorder);
        for index in 1..=15 {
            let label = index.to_string();
            add(
                &mut db,
                1,
                &format!("child{index}"),
                &label,
                NodeType::NoBorder,
            );
        }
        let ast = db.into_ast();

        let data = ast.data();

        assert_eq!(data.nodes.len(), 16);

        let node = |label: &str| {
            data.nodes
                .iter()
                .find(|node| node.label == label)
                .unwrap_or_else(|| panic!("missing node {label}"))
        };

        assert_eq!(node("1").section, Some(0));
        assert_eq!(node("11").section, Some(10));
        assert_eq!(node("12").section, Some(0));
        assert_eq!(node("13").section, Some(1));
        assert_eq!(node("14").section, Some(2));
        assert_eq!(node("15").section, Some(3));

        assert_eq!(node("12").css_classes, "mindmap-node section-0");
        assert_eq!(node("13").css_classes, "mindmap-node section-1");
        assert_eq!(node("14").css_classes, "mindmap-node section-2");
        assert_eq!(node("15").css_classes, "mindmap-node section-3");
    }
}

mod colors {
    use super::*;

    /// An indented root, different indentation widths, and unclear indentation: Leaf A and
    /// Leaf B are siblings despite starting in different columns.
    const DIAGRAM: &str = "mindmap\n    root((Root))\n          ownerA{{Owner A}}\n            repoA[Repo A]\n                leafA[Leaf A]\n              leafB[Leaf B]\n          ownerB{{Owner B}}\n              repoB[Repo B]\n                leafC[Leaf C]\n";

    #[test]
    fn preserves_branch_coloring() {
        // Upstream runs this for `colorBy` undefined and `branch`, the same mode.
        let data = ast(DIAGRAM).data();
        assert_eq!(
            sections(&data),
            [
                None,
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some(1),
                Some(1),
                Some(1)
            ]
        );
    }

    #[test]
    fn colors_by_tree_depth_independently_of_indentation_and_branch() {
        let data = ast(DIAGRAM).data_with(ColorBy::Depth);
        assert_eq!(
            sections(&data),
            [
                None,
                Some(0),
                Some(1),
                Some(2),
                Some(2),
                Some(0),
                Some(1),
                Some(2)
            ]
        );
        assert!(data.nodes[0].css_classes.contains("section-root"));
        for edge in &data.edges {
            let child = data
                .nodes
                .iter()
                .find(|node| node.id == edge.end)
                .expect("edge to a missing node");
            assert_eq!(edge.section, child.section);
            match child.section {
                Some(section) => assert!(edge.classes.contains(&format!("section-edge-{section}"))),
                None => assert!(!edge.classes.contains("section-edge-")),
            }
        }
    }

    #[test]
    fn keeps_existing_depth_colors_when_a_preceding_branch_is_inserted() {
        let original = ast(DIAGRAM).data_with(ColorBy::Depth);
        let edited = ast(&DIAGRAM.replace(
            "          ownerA",
            "          added[Added]\n          ownerA",
        ))
        .data_with(ColorBy::Depth);
        for node in &original.nodes {
            let candidate = edited
                .nodes
                .iter()
                .find(|candidate| candidate.node_id == node.node_id)
                .unwrap_or_else(|| panic!("missing node {}", node.node_id));
            assert_eq!(candidate.section, node.section, "node {}", node.node_id);
        }
    }

    #[test]
    fn cycles_through_the_non_root_palette_for_deep_trees() {
        let mut text = vec!["mindmap".to_string(), "Root".to_string()];
        for depth in 1..=13 {
            text.push(format!("{}Node {depth}", "  ".repeat(depth)));
        }
        let data = ast(&format!("{}\n", text.join("\n"))).data_with(ColorBy::Depth);
        assert_eq!(
            sections(&data),
            [
                None,
                Some(0),
                Some(1),
                Some(2),
                Some(3),
                Some(4),
                Some(5),
                Some(6),
                Some(7),
                Some(8),
                Some(9),
                Some(10),
                Some(0),
                Some(1)
            ]
        );
    }

    #[test]
    fn preserves_custom_classes_and_icons() {
        let data = ast(
            "mindmap\nRoot\n  :::custom-root\n  Child[Child]\n    :::custom-child\n    ::icon(mdi:fire)\n",
        )
        .data_with(ColorBy::Depth);
        assert!(data.nodes[0].css_classes.contains("custom-root"));
        assert!(data.nodes[1].css_classes.contains("custom-child"));
        assert_eq!(data.nodes[1].icon.as_deref(), Some("mdi:fire"));
    }

    #[test]
    fn reassigns_colors_when_the_mode_changes_on_the_same_parsed_tree() {
        let ast = ast(DIAGRAM);
        let depth = sections(&ast.data_with(ColorBy::Depth));
        let branch = sections(&ast.data_with(ColorBy::Branch));
        assert_eq!(
            depth,
            [
                None,
                Some(0),
                Some(1),
                Some(2),
                Some(2),
                Some(0),
                Some(1),
                Some(2)
            ]
        );
        assert_eq!(
            branch,
            [
                None,
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                Some(1),
                Some(1),
                Some(1)
            ]
        );
    }

    #[test]
    fn handles_empty_and_root_only_mindmaps() {
        assert_eq!(Ast::default().data_with(ColorBy::Depth).nodes, Vec::new());
        let data = ast("mindmap\nRoot\n").data_with(ColorBy::Depth);
        assert_eq!(data.nodes.len(), 1);
        assert_eq!(data.nodes[0].section, None);
        assert_eq!(data.edges, Vec::new());
    }
}

/// What the port pins that upstream's specs do not spell out: the lexer's start conditions,
/// the grammar's `stop` rule, and the shape of an error.
mod grammar {
    use super::*;

    #[test]
    fn the_header_is_case_insensitive() {
        assert_eq!(mindmap(&ast("MINDMAP\nroot")).descr, "root");
        assert_eq!(mindmap(&ast("MindMap\nroot")).descr, "root");
    }

    #[test]
    fn an_indented_header_is_a_syntax_error() {
        let error = failure("  mindmap\nroot");
        assert_eq!(
            error.span.map(|span| (span.line, span.column)),
            Some((1, 1))
        );
    }

    #[test]
    fn a_blank_line_after_the_header_is_a_syntax_error() {
        // `[\s]+[\n]` makes a SPACELINE of a whitespace run that holds a line break and one
        // more whitespace character before it, so the blank line is a statement that still
        // owes a `stop` before the first node.
        for source in ["mindmap\n\nroot", "mindmap\n\n\nroot", "mindmap\n \nroot"] {
            let error = failure(source);
            assert_eq!(
                error.message, "Expected a line break, found 'root'",
                "source: {source:?}"
            );
        }
    }

    #[test]
    fn a_header_without_a_document_is_a_syntax_error() {
        assert!(parse("mindmap").is_err());
        assert!(parse("mindmap\n").is_err());
    }

    #[test]
    fn comment_lines_before_the_header_are_skipped() {
        let ast = ast("%% a comment\nmindmap\nroot");
        assert_eq!(mindmap(&ast).descr, "root");
    }

    #[test]
    fn an_unclosed_node_is_an_error() {
        assert!(parse("mindmap\nroot(a").is_err());
        assert!(parse("mindmap\nroot[a").is_err());
    }

    #[test]
    fn an_empty_quoted_description_is_an_error() {
        assert!(parse("mindmap\nroot[\"\"]").is_err());
    }

    #[test]
    fn a_stray_opening_brace_is_a_lexical_error() {
        let error = failure("mindmap\nroot{x");
        assert!(
            error.message.starts_with("Lexical error on line 2"),
            "{}",
            error.message
        );
    }

    #[test]
    fn two_statements_may_not_share_a_line() {
        let error = failure("mindmap\nroot(a)b");
        assert_eq!(error.message, "Expected a line break, found 'b'");
    }

    #[test]
    fn a_tab_counts_as_one_indentation_column() {
        let ast = ast("mindmap\nroot\n\ta");
        let mm = mindmap(&ast);
        assert_eq!(mm.children.len(), 1);
        assert_eq!(mm.children[0].descr, "a");
    }

    #[test]
    fn a_description_may_span_a_line_break() {
        // The NODE condition's `[^\)\]\(\}]+` does not exclude `\n`.
        let ast = ast("mindmap\nroot(a\nb)");
        assert_eq!(mindmap(&ast).descr, "a\nb");
    }

    #[test]
    fn get_type_has_no_case_for_the_exploding_delimiters() {
        assert_eq!(node_type("-)", "(-"), NodeType::NoBorder);
        assert_eq!(node_type("(-", "-)"), NodeType::NoBorder);
        assert_eq!(node_type("(", "]"), NodeType::Cloud);
        assert_eq!(node_type("{{", "("), NodeType::Hexagon);
    }

    #[test]
    fn a_decoration_before_any_node_is_an_error() {
        let error = failure("mindmap\n::icon(bomb)");
        assert_eq!(error.message, super::super::db::NO_NODE);
        assert_eq!(
            error.span.map(|span| (span.line, span.column)),
            Some((2, 8))
        );
    }

    #[test]
    fn the_indentation_of_a_leaf_settles_its_parent() {
        let ast = ast("mindmap\nroot\n A\n   B\n  C");
        let data = ast.data();
        let relations: Vec<String> = data
            .edges
            .iter()
            .map(|edge| format!("{}->{}", edge.start, edge.end))
            .collect();
        assert_eq!(relations, ["0->1", "1->2", "1->3"]);
    }
}
