//! Tests ported from the upstream block specs: `packages/mermaid/src/diagrams/block/`
//! `parser/block.spec.ts` (grammar and database), `blockDB.spec.ts` (the edge helpers), and
//! `blockColorIndex.spec.ts` (the colour slots). Inputs and expectations are upstream's.
//!
//! Skipped, and why:
//! - `block.spec.ts`'s "sanitizes labels with the current dompurify config": it sets a
//!   `dompurifyConfig` and expects DOMPurify to drop `<b>`. This crate owns no configuration
//!   and has no DOM; [`super::sanitize`] runs at the default security level.
//! - `blockDB.spec.ts`'s "block db runtime config": it spies on `getConfig` to prove the
//!   sanitizer reads it at call time. There is no `getConfig` here.
//! - `blockColorIndex.spec.ts`'s four stylesheet cases ("emits one composite rule per palette
//!   entry", "leaves the simple shapes", "emits nothing for a theme without a palette",
//!   "rejects a look that would break out of the selector"): they call `styles.ts`, which is
//!   rendering, not database data.
//! - `layout.spec.ts` (both cases): it tests `layout.ts` and its `padding` config.

use super::db;
use super::{Ast, Block, BlockType, parse};
use crate::error::ParseError;

fn ast(body: &str) -> Ast {
    parse(body).unwrap_or_else(|error| panic!("expected {body:?} to parse, got {error}"))
}

fn block<'a>(ast: &'a Ast, id: &str) -> &'a Block {
    ast.block(id).unwrap_or_else(|| {
        let ids: Vec<&str> = ast.flat.iter().map(|block| block.id.as_str()).collect();
        panic!("block {id:?} not found among {ids:?}")
    })
}

fn error(body: &str) -> ParseError {
    match parse(body) {
        Ok(ast) => panic!("expected {body:?} to fail, parsed {ast:?}"),
        Err(error) => error,
    }
}

fn ids(ast: &Ast) -> Vec<&str> {
    ast.blocks.iter().map(|block| block.id.as_str()).collect()
}

// packages/mermaid/src/diagrams/block/parser/block.spec.ts

#[test]
fn parses_a_diagram_with_a_node() {
    let ast = ast("block-beta\n          id\n      ");
    assert_eq!(ast.blocks.len(), 1);
    assert_eq!(ast.blocks[0].id, "id");
    assert_eq!(ast.blocks[0].label, "id");
}

#[test]
fn parses_a_node_with_a_square_shape_and_a_label() {
    let ast = ast("block\n          id[\"A label\"]\n          ");
    assert_eq!(ast.blocks.len(), 1);
    assert_eq!(ast.blocks[0].id, "id");
    assert_eq!(ast.blocks[0].label, "A label");
    assert_eq!(ast.blocks[0].type_name, BlockType::Square);
}

#[test]
fn parses_multiple_nodes() {
    for body in [
        "block\n          id1\n          id2\n      ",
        "block\n          id1\n          id2\n          id3\n      ",
    ] {
        let ast = ast(body);
        let expected: Vec<String> = (1..=ast.blocks.len())
            .map(|index| format!("id{index}"))
            .collect();
        assert_eq!(ast.blocks.len(), expected.len(), "{body:?}");
        for (block, id) in ast.blocks.iter().zip(&expected) {
            assert_eq!(&block.id, id, "{body:?}");
            assert_eq!(&block.label, id, "{body:?}");
            assert_eq!(block.type_name, BlockType::Na, "{body:?}");
        }
    }
}

#[test]
fn parses_a_shaped_node_next_to_a_plain_one() {
    let ast = ast("block\n          id[\"A label\"]\n          id2");
    assert_eq!(ids(&ast), ["id", "id2"]);
    assert_eq!(ast.blocks[0].label, "A label");
    assert_eq!(ast.blocks[0].type_name, BlockType::Square);
    assert_eq!(ast.blocks[1].label, "id2");
    assert_eq!(ast.blocks[1].type_name, BlockType::Na);
}

#[test]
fn parses_nodes_joined_by_an_edge() {
    let ast = ast("block\n          id1[\"first\"]  -->   id2[\"second\"]\n      ");
    assert_eq!(ast.blocks.len(), 2);
    assert_eq!(ast.edges.len(), 1);
    assert_eq!(ast.edges[0].start, "id1");
    assert_eq!(ast.edges[0].end, "id2");
    assert_eq!(ast.edges[0].arrow_type_end, "arrow_point");
}

#[test]
fn parses_an_edge_with_a_label() {
    let ast = ast("block\n          id1[\"first\"]  -- \"a label\" -->   id2[\"second\"]\n      ");
    assert_eq!(ast.blocks.len(), 2);
    assert_eq!(ast.edges.len(), 1);
    assert_eq!(ast.edges[0].start, "id1");
    assert_eq!(ast.edges[0].end, "id2");
    assert_eq!(ast.edges[0].arrow_type_end, "arrow_point");
    assert_eq!(ast.edges[0].label, "a label");
}

#[test]
fn parses_column_statements() {
    let ast = ast("block\n          columns 2\n          block1[\"Block 1\"]\n      ");
    assert_eq!(ast.columns("root"), 2);
    assert_eq!(ast.blocks.len(), 1);
}

#[test]
fn defaults_the_root_columns_to_one_less_than_none() {
    let ast = ast("block\n          block1[\"Block 1\"]\n      ");
    assert_eq!(ast.columns("root"), -1);
    assert_eq!(ast.blocks.len(), 1);
}

#[test]
fn parses_auto_columns() {
    let ast = ast("block\n          columns auto\n          block1[\"Block 1\"]\n      ");
    assert_eq!(ast.columns("root"), -1);
    assert_eq!(ast.blocks.len(), 1);
}

#[test]
fn parses_blocks_next_to_each_other() {
    let ast = ast(
        "block\n          columns 2\n          block1[\"Block 1\"]\n          block2[\"Block 2\"]\n        ",
    );
    assert_eq!(ast.columns("root"), 2);
    assert_eq!(ast.blocks.len(), 2);
}

#[test]
fn parses_blocks_on_top_of_each_other() {
    let ast = ast(
        "block\n          columns 1\n          block1[\"Block 1\"]\n          block2[\"Block 2\"]\n        ",
    );
    assert_eq!(ast.columns("root"), 1);
    assert_eq!(ast.blocks.len(), 2);
}

#[test]
fn parses_a_compound_block() {
    let ast = ast(
        "block\n          block\n            aBlock[\"ABlock\"]\n            bBlock[\"BBlock\"]\n          end\n        ",
    );
    assert_eq!(ast.blocks.len(), 1);
    let composite = &ast.blocks[0];
    let children = composite.children.as_deref().expect("children");
    assert_eq!(children.len(), 2);
    assert!(!composite.id.is_empty());
    assert_eq!(composite.label, "");
    assert_eq!(composite.type_name, BlockType::Composite);

    assert!(!children[0].id.is_empty());
    assert_eq!(children[0].label, "ABlock");
    assert_eq!(children[0].type_name, BlockType::Square);
    assert!(!children[1].id.is_empty());
    assert_eq!(children[1].label, "BBlock");
    assert_eq!(children[1].type_name, BlockType::Square);
}

#[test]
fn parses_compound_blocks_of_compound_blocks() {
    let ast = ast(
        "block\n          block\n            aBlock[\"ABlock\"]\n            block\n              bBlock[\"BBlock\"]\n            end\n          end\n        ",
    );
    assert_eq!(ast.blocks.len(), 1);
    let outer = &ast.blocks[0];
    let outer_children = outer.children.as_deref().expect("children");
    assert_eq!(outer_children.len(), 2);
    assert!(!outer.id.is_empty());
    assert_eq!(outer.label, "");
    assert_eq!(outer.type_name, BlockType::Composite);

    let nested = &outer_children[1];
    let nested_children = nested.children.as_deref().expect("children");
    assert_eq!(nested_children.len(), 1);
    assert!(!nested.id.is_empty());
    assert_eq!(nested.label, "");
    assert_eq!(nested.type_name, BlockType::Composite);

    assert!(!outer_children[0].id.is_empty());
    assert_eq!(outer_children[0].label, "ABlock");
    assert_eq!(outer_children[0].type_name, BlockType::Square);
    assert!(!nested_children[0].id.is_empty());
    assert_eq!(nested_children[0].label, "BBlock");
    assert_eq!(nested_children[0].type_name, BlockType::Square);
}

#[test]
fn parses_a_compound_block_with_a_title() {
    let ast = ast(
        "block\n          block:compoundBlock[\"Compound block\"]\n            columns 1\n            block2[\"Block 2\"]\n          end\n        ",
    );
    assert_eq!(ast.blocks.len(), 1);
    let compound = &ast.blocks[0];
    let children = compound.children.as_deref().expect("children");
    assert_eq!(children.len(), 1);
    assert_eq!(compound.id, "compoundBlock");
    assert_eq!(compound.label, "Compound block");
    assert_eq!(compound.type_name, BlockType::Composite);
    assert_eq!(children[0].id, "block2");
    assert_eq!(children[0].label, "Block 2");
    assert_eq!(children[0].type_name, BlockType::Square);
}

#[test]
fn parses_blocks_mixed_with_compound_blocks() {
    let ast = ast(
        "block\n          columns 1\n          block1[\"Block 1\"]\n\n          block\n            columns 2\n            block2[\"Block 2\"]\n            block3[\"Block 3\"]\n          end\n        ",
    );
    assert_eq!(ast.blocks.len(), 2);
    let compound = &ast.blocks[1];
    let children = compound.children.as_deref().expect("children");
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].id, "block2");
    assert_eq!(children[0].label, "Block 2");
    assert_eq!(children[0].type_name, BlockType::Square);
}

#[test]
fn parses_block_arrows() {
    let ast = ast(
        "block\n        columns 3\n        block1[\"Block 1\"]\n        blockArrow<[\"&nbsp;&nbsp;&nbsp;\"]>(right)\n        block2[\"Block 2\"]",
    );
    assert_eq!(ast.blocks.len(), 3);
    let arrow = &ast.blocks[1];
    assert_eq!(ast.blocks[0].id, "block1");
    assert_eq!(arrow.id, "blockArrow");
    assert_eq!(ast.blocks[2].id, "block2");
    assert_eq!(ast.blocks[2].label, "Block 2");
    assert_eq!(ast.blocks[2].type_name, BlockType::Square);
    assert_eq!(arrow.type_name, BlockType::BlockArrow);
    assert!(arrow.directions.contains(&"right".to_string()));
}

#[test]
fn parses_block_arrows_with_several_points() {
    let ast = ast(
        "block\n        columns 1\n        A\n        blockArrow<[\"&nbsp;&nbsp;&nbsp;\"]>(up, down)\n        block\n          columns 3\n            B\n            C\n            D\n        end",
    );
    assert_eq!(ast.blocks.len(), 3);
    let arrow = &ast.blocks[1];
    assert_eq!(arrow.type_name, BlockType::BlockArrow);
    assert!(arrow.directions.contains(&"up".to_string()));
    assert!(arrow.directions.contains(&"down".to_string()));
    assert!(!arrow.directions.contains(&"right".to_string()));
}

#[test]
fn parses_blocks_with_different_widths() {
    let ast = ast(
        "block\n        columns 3\n        one[\"One Slot\"]\n        two[\"Two slots\"]:2\n        ",
    );
    assert_eq!(ast.blocks.len(), 2);
    assert_eq!(ast.blocks[0].width_in_columns, Some(1));
    assert_eq!(ast.blocks[1].width_in_columns, Some(2));
}

#[test]
fn parses_empty_blocks() {
    let ast = ast(
        "block\n        columns 3\n        space\n        middle[\"In the middle\"]\n        space\n        ",
    );
    assert_eq!(ast.blocks.len(), 3);
    assert_eq!(ast.blocks[0].type_name, BlockType::Space);
    assert_eq!(ast.blocks[2].type_name, BlockType::Space);
    assert_eq!(ast.blocks[1].label, "In the middle");
}

#[test]
fn applies_class_definitions_and_classes() {
    let ast = ast(
        "block\n        classDef black color:#ffffff, fill:#000000;\n\n        mc[\"Memcache\"]\n        class mc black\n        ",
    );
    assert_eq!(ast.blocks.len(), 1);
    assert_eq!(ast.blocks[0].classes, ["black"]);
    let black = ast.class("black").expect("class black");
    assert_eq!(black.id, "black");
    assert_eq!(black.styles[0], "color:#ffffff");
}

#[test]
fn applies_style_statements() {
    let ast = ast(
        "block\ncolumns 1\n    B[\"A wide one in the middle\"]\n  style B fill:#f9F,stroke:#333,stroke-width:4px\n        ",
    );
    assert_eq!(ast.blocks.len(), 1);
    assert!(
        ast.blocks[0].styles.contains(&"fill:#f9F".to_string()),
        "{:?}",
        ast.blocks[0].styles
    );
}

#[test]
fn warns_when_a_block_is_wider_than_its_columns() {
    let ast = ast("block-beta\n  columns 1\n  A:1\n  B:2\n  C:3\n  D:4\n  E:3\n  F:2\n  G:1");
    assert!(
        ast.warnings
            .contains(&"Block B width 2 exceeds configured column width 1".to_string()),
        "{:?}",
        ast.warnings
    );
}

#[test]
fn parses_ids_that_are_javascript_object_prototype_keys() {
    for name in ["__proto__", "constructor"] {
        ast(&format!("block\n{name}"));
        ast(&format!(
            "block\nA; classDef {name} color:#ffffff,fill:#000000; class A {name}"
        ));
    }
}

// packages/mermaid/src/diagrams/block/blockDB.spec.ts

#[test]
fn identifies_edge_thickness() {
    assert_eq!(db::edge_str_to_thickness("-->"), "normal");
    assert_eq!(db::edge_str_to_thickness("-.->"), "normal");
    assert_eq!(db::edge_str_to_thickness("==>"), "thick");
}

#[test]
fn identifies_edge_patterns() {
    assert_eq!(db::edge_str_to_pattern("-->"), "solid");
    assert_eq!(db::edge_str_to_pattern("==>"), "solid");
    assert_eq!(db::edge_str_to_pattern("-.->"), "dotted");
}

#[test]
fn identifies_start_and_end_arrows() {
    assert_eq!(db::edge_str_to_edge_start_data("<-->"), "arrow_point");
    assert_eq!(db::edge_str_to_edge_data("<-->"), "arrow_point");
    assert_eq!(db::edge_str_to_edge_start_data("<==>"), "arrow_point");
    assert_eq!(db::edge_str_to_edge_data("<==>"), "arrow_point");
    assert_eq!(db::edge_str_to_edge_start_data("-->"), "arrow_open");
    assert_eq!(db::edge_str_to_edge_data("-->"), "arrow_point");
    assert_eq!(db::edge_str_to_edge_data("--x"), "arrow_cross");
    assert_eq!(db::edge_str_to_edge_data("--o"), "arrow_circle");
}

// packages/mermaid/src/diagrams/block/blockColorIndex.spec.ts

#[test]
fn numbers_composites_in_declaration_order() {
    let ast = ast(
        "block-beta\n      block:first\n        a\n      end\n      block:second\n        b\n      end\n      block:third\n        c\n      end\n    ",
    );
    let indices: Vec<Option<usize>> = ["first", "second", "third"]
        .iter()
        .map(|id| block(&ast, id).color_index)
        .collect();
    assert_eq!(indices, [Some(0), Some(1), Some(2)]);
}

#[test]
fn gives_no_color_slot_to_a_simple_block() {
    let ast = ast("block-beta\n      a\n      b\n      block:group\n        c\n      end\n    ");
    assert_eq!(block(&ast, "a").color_index, None);
    assert_eq!(block(&ast, "b").color_index, None);
    assert_eq!(block(&ast, "c").color_index, None);
    assert_eq!(block(&ast, "group").color_index, Some(0));
}

#[test]
fn numbers_an_outer_composite_before_one_nested_inside_it() {
    let ast = ast(
        "block-beta\n      block:outer\n        block:inner\n          a\n        end\n      end\n      block:sibling\n        b\n      end\n    ",
    );
    assert_eq!(block(&ast, "outer").color_index, Some(0));
    assert_eq!(block(&ast, "inner").color_index, Some(1));
    assert_eq!(block(&ast, "sibling").color_index, Some(2));
}

#[test]
fn runs_one_color_counter_across_the_whole_diagram() {
    let ast = ast(
        "block-beta\n      block:a1\n        block:a2\n          x\n        end\n      end\n      block:b1\n        block:b2\n          y\n        end\n      end\n    ",
    );
    let indices: Vec<Option<usize>> = ["a1", "a2", "b1", "b2"]
        .iter()
        .map(|id| block(&ast, id).color_index)
        .collect();
    assert_eq!(indices, [Some(0), Some(1), Some(2), Some(3)]);
}

// Behaviour the jison lexer and database have, pinned here because a hand port can drift.

#[test]
fn maps_every_node_shape() {
    let cases = [
        ("id[\"x\"]", BlockType::Square),
        ("id(\"x\")", BlockType::Round),
        ("id((\"x\"))", BlockType::Circle),
        ("id>\"x\"]", BlockType::RectLeftInvArrow),
        ("id{\"x\"}", BlockType::Diamond),
        ("id{{\"x\"}}", BlockType::Hexagon),
        ("id([\"x\"])", BlockType::Stadium),
        ("id[[\"x\"]]", BlockType::Subroutine),
        ("id[(\"x\")]", BlockType::Cylinder),
        ("id(((\"x\")))", BlockType::DoubleCircle),
        ("id[/\"x\"/]", BlockType::LeanRight),
        ("id[\\\"x\"\\]", BlockType::LeanLeft),
        ("id[/\"x\"\\]", BlockType::Trapezoid),
        ("id[\\\"x\"/]", BlockType::InvTrapezoid),
    ];
    for (statement, expected) in cases {
        let ast = ast(&format!("block\n{statement}"));
        assert_eq!(ast.blocks[0].type_name, expected, "{statement:?}");
    }
}

#[test]
fn reads_only_whole_keywords_as_statements() {
    let plain = ast("block\nspacer ender blocky defaulted classy");
    assert_eq!(
        ids(&plain),
        ["spacer", "ender", "blocky", "defaulted", "classy"]
    );
    assert!(
        plain
            .blocks
            .iter()
            .all(|block| block.type_name == BlockType::Na)
    );

    // `space` alone is a space statement, and `block` / `block:` open a composite.
    let spaced = ast("block\nspace");
    assert_eq!(spaced.blocks[0].type_name, BlockType::Space);
}

#[test]
fn creates_one_block_per_space_column() {
    let ast = ast("block\nspace:2\nspace");
    assert_eq!(ast.blocks.len(), 3);
    assert!(
        ast.blocks
            .iter()
            .all(|block| block.type_name == BlockType::Space)
    );
    assert_eq!(ast.blocks[0].width, Some(2));
    assert_eq!(ast.blocks[2].width, Some(1));
    // Upstream generates `id-<random>-<n>`; this port numbers the calls instead.
    assert_eq!(ast.blocks[0].label, "id-1");
    assert_eq!(ast.blocks[0].id, "id-1-0");
    assert_eq!(ast.blocks[2].id, "id-2-0");
    assert_eq!(block(&ast, "id-1").width, Some(2));
}

#[test]
fn numbers_repeated_edges_between_the_same_blocks() {
    let ast = ast("block\nA-->B\nA-->B");
    assert_eq!(ast.edges.len(), 2);
    assert_eq!(ast.edges[0].id, "1-A-B");
    assert_eq!(ast.edges[1].id, "2-A-B");
}

#[test]
fn keeps_a_chained_statement_as_upstreams_array_spread_does() {
    // `nodeStatement link node` builds an array, and the next link reads `.id` off it, so the
    // earlier statement is thrown away and the edge start is `undefined`.
    let ast = ast("block\nA-->B-->C");
    assert_eq!(ast.blocks.len(), 2);
    assert_eq!(ast.blocks[0].id, "");
    assert_eq!(ast.blocks[1].id, "C");
    assert_eq!(ast.edges.len(), 1);
    assert_eq!(ast.edges[0].id, "1-undefined-C");
    assert_eq!(ast.edges[0].start, "");
    assert_eq!(ast.edges[0].end, "C");
}

#[test]
fn blanks_comment_lines_without_moving_the_lines_after_them() {
    let ast = ast("block\n%% a comment\nA\n%% another\nB");
    assert_eq!(ids(&ast), ["A", "B"]);
    let error = error("block\n%% a comment\nA[ \"x\"]");
    let span = error.span.expect("a span");
    assert_eq!(span.line, 3);
    assert_eq!(span.column, 3);
}

#[test]
fn rejects_a_style_for_a_block_it_has_not_seen() {
    let error = error("block\nstyle A fill:#fff");
    assert_eq!(error.message, "cannot set styles on unknown block 'A'");
    assert_eq!(error.span.expect("a span").line, 2);
}

#[test]
fn rejects_a_document_without_a_statement() {
    for body in ["block", "block\n", "block-beta", "block\nblock\nend"] {
        assert!(parse(body).is_err(), "{body:?}");
    }
}

#[test]
fn rejects_terminals_the_grammar_never_accepts() {
    for body in [
        "block\naccTitle: hello",
        "block\naccDescr: hello",
        "block\nlinkStyle 0 stroke:#000",
        "block\ninterpolate cardinal",
        "block\nclassDef DEFAULT fill:#fff",
        "block\nA[ \"x\"]",
        "block\nA[\"x\" ]",
        "block\nA -- B",
        "block\nA o-- B",
        "block\nA[\"\"]",
    ] {
        assert!(parse(body).is_err(), "{body:?}");
    }
}

#[test]
fn sanitizes_labels_at_the_default_security_level() {
    let bold = ast("block\nid[\"<b>Bold</b>\"]");
    assert_eq!(bold.blocks[0].label, "<b>Bold</b>");
    let scripted = ast("block\nid[\"a<script>x</script>b\"]");
    assert_eq!(scripted.blocks[0].label, "ab");
}

#[test]
fn answers_get_columns_for_any_block() {
    let ast = ast("block\ncolumns 2\nblock:g\n a\n b\nend\nspace\nplain");
    // A block with a `columns` statement answers it; a composite without one answers the
    // number of children it has a `children` property for, and a plain block answers -1.
    assert_eq!(ast.columns("root"), 2);
    assert_eq!(ast.columns("g"), 2);
    assert_eq!(ast.columns("plain"), -1);
    assert_eq!(ast.columns("missing"), -1);
    let space = ast
        .flat
        .iter()
        .find(|block| block.width == Some(1))
        .expect("space");
    assert_eq!(ast.columns(&space.id), 0);
}

#[test]
fn lists_the_database_map_in_insertion_order() {
    let ast = ast("block\nspace\nmc[\"Memcache\"]\nclass ghost blue");
    let ids: Vec<&str> = ast.flat.iter().map(|block| block.id.as_str()).collect();
    // `root`, the space's original and its copy, the block, then the placeholder `class`
    // made for an id it had not seen. A block with children carries copies of them.
    assert_eq!(ids, ["root", "id-1", "id-1-0", "mc", "ghost"]);
    assert_eq!(block(&ast, "ghost").classes, ["blue"]);
    assert!(block(&ast, "ghost").children.as_deref() == Some(&[]));
}

#[test]
fn numbers_composites_in_the_order_upstream_generates_their_ids() {
    // A `space` takes its id when its statement is read; an anonymous `block` takes its own
    // when its `end` is read, so the inner block is numbered after the space it contains.
    let ast = ast("block\nblock\n space\nend\nspace");
    let composites: Vec<&Block> = ast
        .flat
        .iter()
        .filter(|block| block.type_name == BlockType::Composite && block.id != "root")
        .collect();
    assert_eq!(composites.len(), 1);
    assert_eq!(composites[0].id, "id-2");
    assert_eq!(ast.blocks[1].label, "id-3");
}
