//! Tests ported from the upstream treeView specs:
//!
//! * `packages/parser/tests/treeView.test.ts` (the Langium grammar),
//! * `packages/parser/tests/treeViewValueConverter.test.ts` (the value converter),
//! * `packages/mermaid/src/diagrams/treeView/db.spec.ts` (the database),
//! * `packages/mermaid/src/diagrams/treeView/parser.spec.ts` (the parser pipeline),
//! * `packages/mermaid/src/diagrams/treeView/boxDrawingPreprocessor.spec.ts`.
//!
//! Inputs and expectations are upstream's. The grammar tests assert on the Langium AST's flat
//! `nodes`; here the same document is read through the database the parser populates, so a
//! grammar `name` is [`Node::name`] (with a trailing `/` moved to [`NodeType`]) and a grammar
//! `indent` is [`Node::level`].
//!
//! Skipped, and why:
//!
//! * `expectNoErrorsOrAlternatives` checks Langium's lexer and parser error arrays and that
//!   no `console.log` ran; `parse` returning `Ok` is the error-free assertion here.
//! * `result.value.$type` is Langium's generated AST type marker, not database data.
//! * `treeViewValueConverter.test.ts`'s "unknown rules" case asks the converter dispatcher
//!   for a rule it does not know; this port has one function per terminal instead.
//! * `db.spec.ts`'s `getConfig` describe compares against `DEFAULT_CONFIG.treeView`; this
//!   crate owns no configuration.
//! * `remapErrorLines` rewrites `line N` inside a message; errors here carry a
//!   [`Span`](crate::error::Span), so the equivalent case is under "box-drawing errors".
//! * `icons.spec.ts` tests `icons.ts`, which resolves a node's icon at render time from
//!   configuration.
//! * `renderer.ts` and `styles.ts` render; nothing here renders.

use super::Ast;
use super::Node;
use super::NodeType;
use super::box_drawing;
use super::parse;
use super::parser::{
    class_value, desc_value, icon_value, indentation_value, strip_quotes, trim_bare_name,
};
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

/// `parseAndGetNode`: the database child at `index` of `treeView-beta\n{line}`.
fn node(line: &str, index: usize) -> Node {
    let body = format!("treeView-beta\n{line}");
    let ast = ast(&body);
    ast.root
        .children
        .get(index)
        .cloned()
        .unwrap_or_else(|| panic!("{line:?} produced no node at {index}"))
}

fn names(node: &Node) -> Vec<&str> {
    node.children
        .iter()
        .map(|child| child.name.as_str())
        .collect()
}

// packages/parser/tests/treeView.test.ts

#[test]
fn parses_an_empty_tree_view() {
    let ast = ast("treeView-beta");
    assert!(ast.root.children.is_empty());
    assert_eq!(ast.count, 1);
}

#[test]
fn parses_a_tree_view_with_a_quoted_root_node() {
    let ast = ast("treeView-beta\n\"Root\"");
    assert_eq!(ast.root.children.len(), 1);
    assert_eq!(ast.root.children[0].name, "Root");
    assert_eq!(ast.root.children[0].level, 0);
}

#[test]
fn parses_a_tree_view_with_multiple_words_in_a_quoted_node() {
    let ast = ast("treeView-beta\n\"Multi Word Root\"");
    assert_eq!(ast.root.children.len(), 1);
    assert_eq!(ast.root.children[0].name, "Multi Word Root");
    assert_eq!(ast.root.children[0].level, 0);
}

#[test]
fn parses_a_tree_view_with_child_nodes() {
    let ast = ast("treeView-beta\n\"Root\"\n    \"Child1\"\n    \"Child2\"\n        \"Child3\"");
    assert_eq!(ast.count, 5);
    let root = &ast.root.children[0];
    assert_eq!(root.name, "Root");
    assert_eq!(root.level, 0);
    assert_eq!(names(root), ["Child1", "Child2"]);
    assert_eq!(root.children[0].level, 4);
    assert_eq!(root.children[1].level, 4);
    assert_eq!(root.children[1].children[0].name, "Child3");
    assert_eq!(root.children[1].children[0].level, 8);
}

#[test]
fn parses_bare_labels() {
    let ast = ast("treeView-beta\nindex.js");
    assert_eq!(ast.root.children.len(), 1);
    assert_eq!(ast.root.children[0].name, "index.js");
}

#[test]
fn parses_bare_labels_with_a_directory_trailing_slash() {
    let ast = ast("treeView-beta\nsrc/\n    index.js");
    assert_eq!(ast.root.children.len(), 1);
    let src = &ast.root.children[0];
    assert_eq!(src.name, "src");
    assert_eq!(src.node_type, NodeType::Directory);
    assert_eq!(src.children.len(), 1);
    assert_eq!(src.children[0].name, "index.js");
    assert_eq!(src.children[0].level, 4);
}

#[test]
fn parses_bare_labels_with_annotations() {
    let node = node("index.js :::highlight icon(javascript) ## entry point", 0);
    assert_eq!(node.name, "index.js");
    assert_eq!(node.css_class.as_deref(), Some("highlight"));
    assert_eq!(node.icon.as_deref(), Some("javascript"));
    assert_eq!(node.description.as_deref(), Some("entry point"));
}

#[test]
fn parses_quoted_labels_with_annotations() {
    let node = node(
        "\"my file.js\" :::highlight icon(javascript) ## entry point",
        0,
    );
    assert_eq!(node.name, "my file.js");
    assert_eq!(node.css_class.as_deref(), Some("highlight"));
    assert_eq!(node.icon.as_deref(), Some("javascript"));
    assert_eq!(node.description.as_deref(), Some("entry point"));
}

#[test]
fn skips_comment_lines() {
    let ast = ast("treeView-beta\n%% this is a comment\nindex.js");
    assert_eq!(ast.root.children.len(), 1);
    assert_eq!(ast.root.children[0].name, "index.js");
}

#[test]
fn strips_quotes_from_quoted_names() {
    assert_eq!(node("\"my file.js\"", 0).name, "my file.js");
    assert_eq!(node("'my folder/'", 0).name, "my folder");
}

#[test]
fn extracts_the_class_name_from_the_class_annotation() {
    assert_eq!(
        node("index.js :::highlight", 0).css_class.as_deref(),
        Some("highlight")
    );
    assert_eq!(
        node("file.ts :::my-class", 0).css_class.as_deref(),
        Some("my-class")
    );
}

#[test]
fn preserves_consecutive_spaces_in_names() {
    assert_eq!(node("\"But  _  _ton💓.tsx\"", 0).name, "But  _  _ton💓.tsx");
    assert_eq!(node("But  _  _ton💓.tsx", 0).name, "But  _  _ton💓.tsx");
}

#[test]
fn trims_trailing_whitespace_from_bare_names() {
    assert_eq!(node("index.js  ", 0).name, "index.js");
}

#[test]
fn extracts_the_icon_name_from_the_icon_annotation() {
    assert_eq!(
        node("data.bin icon(folder)", 0).icon.as_deref(),
        Some("folder")
    );
    assert_eq!(
        node("App.tsx icon(logos:react)", 0).icon.as_deref(),
        Some("logos:react")
    );
}

#[test]
fn handles_an_empty_icon() {
    assert_eq!(node("index.js icon()", 0).icon.as_deref(), Some("none"));
}

#[test]
fn extracts_the_description_from_the_description_annotation() {
    assert_eq!(
        node("index.js ## entry point", 0).description.as_deref(),
        Some("entry point")
    );
}

#[test]
fn handles_all_annotations_in_any_order() {
    let node = node("app.ts icon(none) :::highlight ## entry point", 0);
    assert_eq!(node.name, "app.ts");
    assert_eq!(node.icon.as_deref(), Some("none"));
    assert_eq!(node.css_class.as_deref(), Some("highlight"));
    assert_eq!(node.description.as_deref(), Some("entry point"));
}

#[test]
fn parses_bare_names_with_spaces_before_annotations() {
    let node = node("My Documents/ :::highlight", 0);
    assert_eq!(node.name, "My Documents");
    assert_eq!(node.node_type, NodeType::Directory);
    assert_eq!(node.css_class.as_deref(), Some("highlight"));
}

#[test]
fn parses_dotfiles() {
    assert_eq!(node(".gitignore", 0).name, ".gitignore");
}

#[test]
fn parses_filenames_with_hyphens() {
    assert_eq!(node("docker-compose.yml", 0).name, "docker-compose.yml");
}

#[test]
fn parses_a_tree_view_with_a_title() {
    let ast = ast("treeView-beta\ntitle My TreeView Diagram\n\"Root\"\n  \"Child\"");
    assert_eq!(ast.diagram_title, "My TreeView Diagram");
    assert_eq!(ast.root.children.len(), 1);
    assert_eq!(ast.root.children[0].children.len(), 1);
}

#[test]
fn parses_a_tree_view_with_an_acc_title() {
    let ast = ast("treeView-beta\naccTitle: Accessible Title\n\"Root\"\n  \"Child\"");
    assert_eq!(ast.acc_title, "Accessible Title");
    assert_eq!(ast.root.children.len(), 1);
    assert_eq!(ast.root.children[0].children.len(), 1);
}

#[test]
fn parses_a_tree_view_with_an_acc_descr() {
    let ast =
        ast("treeView-beta\naccDescr: This is an accessible description\n\"Root\"\n  \"Child\"");
    assert_eq!(ast.acc_description, "This is an accessible description");
    assert_eq!(ast.root.children.len(), 1);
    assert_eq!(ast.root.children[0].children.len(), 1);
}

#[test]
fn parses_a_tree_view_with_multiple_accessibility_attributes() {
    let ast = ast(
        "treeView-beta\ntitle My TreeView Diagram\naccTitle: Accessible Title\naccDescr: This is an accessible description\n\"Root\"\n  \"Child\"",
    );
    assert_eq!(ast.diagram_title, "My TreeView Diagram");
    assert_eq!(ast.acc_title, "Accessible Title");
    assert_eq!(ast.acc_description, "This is an accessible description");
    assert_eq!(ast.root.children.len(), 1);
    assert_eq!(ast.root.children[0].children.len(), 1);
}

// packages/parser/tests/treeViewValueConverter.test.ts

#[test]
fn indentation_converts_to_its_length() {
    assert_eq!(indentation_value("    "), 4);
    assert_eq!(indentation_value(""), 0);
    assert_eq!(indentation_value("        "), 8);
}

#[test]
fn quoted_names_lose_their_quotes() {
    assert_eq!(strip_quotes("\"my file.js\""), "my file.js");
    assert_eq!(strip_quotes("'my folder/'"), "my folder/");
    assert_eq!(strip_quotes("\"\""), "");
}

#[test]
fn class_annotations_lose_their_marker_and_whitespace() {
    assert_eq!(class_value(" :::highlight"), "highlight");
    assert_eq!(class_value(" :::my-class"), "my-class");
    assert_eq!(class_value("  :::highlight  "), "highlight");
}

#[test]
fn bare_names_keep_internal_spaces_and_lose_trailing_ones() {
    assert_eq!(trim_bare_name("But  _  _ton💓.tsx"), "But  _  _ton💓.tsx");
    assert_eq!(trim_bare_name("index.js  "), "index.js");
}

#[test]
fn icon_annotations_lose_their_marker() {
    assert_eq!(icon_value(" icon(folder)"), "folder");
    assert_eq!(icon_value(" icon(logos:react)"), "logos:react");
    assert_eq!(icon_value(" icon()"), "");
    assert_eq!(icon_value(" icon(none)"), "none");
}

#[test]
fn description_annotations_lose_their_marker_and_whitespace() {
    assert_eq!(desc_value(" ## entry point"), "entry point");
    assert_eq!(desc_value("  ## my description  "), "my description");
    assert_eq!(desc_value(" ##"), "");
}

// packages/mermaid/src/diagrams/treeView/db.spec.ts

#[test]
fn a_fresh_database_has_only_the_root() {
    let ast = ast("treeView-beta");
    assert_eq!(ast.root.name, "/");
    assert_eq!(ast.count, 1);
    assert_eq!(ast.root.level, -1);
    assert_eq!(ast.root.node_type, NodeType::Directory);
    assert!(ast.root.children.is_empty());
}

#[test]
fn clearing_resets_to_the_initial_root() {
    // Upstream calls `db.clear()`; a parse always starts from a fresh database.
    let before = ast("treeView-beta\nfile.ts");
    assert_eq!(before.root.children.len(), 1);
    let cleared = ast("treeView-beta");
    assert!(cleared.root.children.is_empty());
    assert_eq!(cleared.root.name, "/");
    assert_eq!(cleared.root.level, -1);
}

#[test]
fn the_count_starts_at_one_and_increments_with_each_node() {
    assert_eq!(ast("treeView-beta").count, 1);
    let ast = ast("treeView-beta\nfile1.ts\nfile2.ts");
    assert_eq!(ast.count, 3);
}

#[test]
fn adds_a_child_to_the_root_at_level_zero() {
    let ast = ast("treeView-beta\nindex.ts");
    assert_eq!(ast.root.children.len(), 1);
    let node = &ast.root.children[0];
    assert_eq!(node.name, "index.ts");
    assert_eq!(node.node_type, NodeType::File);
    assert_eq!(node.level, 0);
}

#[test]
fn adds_nested_children_using_increasing_levels() {
    let ast = ast("treeView-beta\nsrc/\n    index.ts");
    assert_eq!(ast.root.children.len(), 1);
    assert_eq!(ast.root.children[0].name, "src");
    assert_eq!(ast.root.children[0].children.len(), 1);
    assert_eq!(ast.root.children[0].children[0].name, "index.ts");
}

#[test]
fn pops_the_stack_when_the_level_decreases() {
    let ast = ast("treeView-beta\nsrc/\n    index.ts\nREADME.md");
    assert_eq!(names(&ast.root), ["src", "README.md"]);
}

#[test]
fn adds_siblings_at_the_same_level() {
    let ast = ast("treeView-beta\nfile1.ts\nfile2.ts\nfile3.ts");
    assert_eq!(ast.root.children.len(), 3);
}

#[test]
fn stores_the_css_class() {
    let ast = ast("treeView-beta\napp.ts :::highlight");
    assert_eq!(ast.root.children[0].css_class.as_deref(), Some("highlight"));
}

#[test]
fn stores_the_icon() {
    let ast = ast("treeView-beta\ndata.bin icon(logos:mysql)");
    assert_eq!(ast.root.children[0].icon.as_deref(), Some("logos:mysql"));
}

#[test]
fn stores_the_description() {
    let ast = ast("treeView-beta\nindex.ts ## entry point");
    assert_eq!(
        ast.root.children[0].description.as_deref(),
        Some("entry point")
    );
}

#[test]
fn stores_all_optional_fields_together() {
    let ast = ast("treeView-beta\nApp.tsx :::highlight icon(logos:react) ## main component");
    let node = &ast.root.children[0];
    assert_eq!(node.name, "App.tsx");
    assert_eq!(node.css_class.as_deref(), Some("highlight"));
    assert_eq!(node.icon.as_deref(), Some("logos:react"));
    assert_eq!(node.description.as_deref(), Some("main component"));
}

#[test]
fn builds_a_deep_tree() {
    let ast = ast(
        "treeView-beta\nsrc/\n    components/\n        Button.tsx\n        Header.tsx\n    utils/\n        helpers.ts",
    );
    let src = &ast.root.children[0];
    assert_eq!(src.children[0].children.len(), 2);
    assert_eq!(src.children[1].name, "utils");
    assert_eq!(src.children[1].children[0].name, "helpers.ts");
}

#[test]
fn assigns_unique_incrementing_ids() {
    let ast = ast("treeView-beta\na\nb\nc");
    let ids: Vec<usize> = ast.root.children.iter().map(|node| node.id).collect();
    assert_eq!(ids, [1, 2, 3]);
    assert_eq!(ast.root.id, 0);
}

#[test]
fn stores_the_acc_title() {
    let ast = ast("treeView-beta\naccTitle: My Tree");
    assert_eq!(ast.acc_title, "My Tree");
}

#[test]
fn stores_the_acc_description() {
    let ast = ast("treeView-beta\naccDescr: A file tree");
    assert_eq!(ast.acc_description, "A file tree");
}

#[test]
fn stores_the_diagram_title() {
    let ast = ast("treeView-beta\ntitle File Structure");
    assert_eq!(ast.diagram_title, "File Structure");
}

// packages/mermaid/src/diagrams/treeView/parser.spec.ts

#[test]
fn parses_a_simple_filename() {
    let node = node("index.js", 0);
    assert_eq!(node.name, "index.js");
    assert_eq!(node.node_type, NodeType::File);
}

#[test]
fn detects_a_directory_from_a_trailing_slash() {
    let node = node("src/", 0);
    assert_eq!(node.name, "src");
    assert_eq!(node.node_type, NodeType::Directory);
    assert_eq!(node.icon, None);
}

#[test]
fn parses_dotfiles_without_an_icon() {
    let node = node(".gitignore", 0);
    assert_eq!(node.name, ".gitignore");
    assert_eq!(node.node_type, NodeType::File);
    assert_eq!(node.icon, None);
}

#[test]
fn parses_hyphenated_filenames_without_an_icon() {
    let node = node("docker-compose.yml", 0);
    assert_eq!(node.name, "docker-compose.yml");
    assert_eq!(node.icon, None);
}

#[test]
fn strips_double_quotes() {
    let node = node("\"my file.js\"", 0);
    assert_eq!(node.name, "my file.js");
    assert_eq!(node.node_type, NodeType::File);
}

#[test]
fn strips_single_quotes() {
    let node = node("'my folder/'", 0);
    assert_eq!(node.name, "my folder");
    assert_eq!(node.node_type, NodeType::Directory);
}

#[test]
fn handles_an_empty_quoted_string() {
    assert_eq!(node("\"\"", 0).name, "");
}

#[test]
fn extracts_the_class_from_a_bare_label() {
    let node = node("index.js :::highlight", 0);
    assert_eq!(node.name, "index.js");
    assert_eq!(node.css_class.as_deref(), Some("highlight"));
}

#[test]
fn extracts_the_class_from_a_quoted_label() {
    let node = node("\"my file.js\" :::important", 0);
    assert_eq!(node.name, "my file.js");
    assert_eq!(node.css_class.as_deref(), Some("important"));
}

#[test]
fn handles_a_class_with_hyphens() {
    assert_eq!(
        node("file.ts :::my-class", 0).css_class.as_deref(),
        Some("my-class")
    );
}

#[test]
fn extracts_an_icon_override() {
    let node = node("data.bin icon(folder)", 0);
    assert_eq!(node.name, "data.bin");
    assert_eq!(node.icon.as_deref(), Some("folder"));
}

#[test]
fn accepts_prefixed_iconify_names() {
    let node = node("App.tsx icon(logos:react)", 0);
    assert_eq!(node.name, "App.tsx");
    assert_eq!(node.icon.as_deref(), Some("logos:react"));
}

#[test]
fn extracts_the_description_from_a_bare_label() {
    let node = node("index.js ## entry point", 0);
    assert_eq!(node.name, "index.js");
    assert_eq!(node.description.as_deref(), Some("entry point"));
}

#[test]
fn extracts_the_description_from_a_quoted_label() {
    let node = node("\"my file.js\" ## the main file", 0);
    assert_eq!(node.name, "my file.js");
    assert_eq!(node.description.as_deref(), Some("the main file"));
}

#[test]
fn a_description_marker_without_text_stays_unset() {
    let node = node("file.txt ##", 0);
    assert_eq!(node.name, "file.txt");
    assert_eq!(node.description, None);
}

#[test]
fn handles_all_annotations_together() {
    let node = node(
        "App.tsx :::highlight icon(logos:react) ## main component",
        0,
    );
    assert_eq!(node.name, "App.tsx");
    assert_eq!(node.css_class.as_deref(), Some("highlight"));
    assert_eq!(node.icon.as_deref(), Some("logos:react"));
    assert_eq!(node.description.as_deref(), Some("main component"));
}

#[test]
fn handles_a_quoted_label_with_all_annotations() {
    let node = node(
        "\"my app.tsx\" :::highlight icon(logos:react) ## main component",
        0,
    );
    assert_eq!(node.name, "my app.tsx");
    assert_eq!(node.css_class.as_deref(), Some("highlight"));
    assert_eq!(node.icon.as_deref(), Some("logos:react"));
    assert_eq!(node.description.as_deref(), Some("main component"));
}

#[test]
fn handles_a_directory_with_a_class() {
    let node = node("src/ :::highlight", 0);
    assert_eq!(node.name, "src");
    assert_eq!(node.node_type, NodeType::Directory);
    assert_eq!(node.css_class.as_deref(), Some("highlight"));
    assert_eq!(node.icon, None);
}

#[test]
fn handles_folder_names_with_spaces() {
    let node = node("My Documents/", 0);
    assert_eq!(node.name, "My Documents");
    assert_eq!(node.node_type, NodeType::Directory);
    assert_eq!(node.icon, None);
}

#[test]
fn handles_file_names_with_spaces() {
    let node = node("my file.ts ## some description", 0);
    assert_eq!(node.name, "my file.ts");
    assert_eq!(node.node_type, NodeType::File);
    assert_eq!(node.icon, None);
    assert_eq!(node.description.as_deref(), Some("some description"));
}

#[test]
fn preserves_consecutive_spaces_and_unicode_in_bare_names() {
    let node = node("But  _  _ton💓.tsx", 0);
    assert_eq!(node.name, "But  _  _ton💓.tsx");
    assert_eq!(node.node_type, NodeType::File);
}

#[test]
fn preserves_consecutive_spaces_and_unicode_in_quoted_names() {
    assert_eq!(node("\"But  _  _ton💓.tsx\"", 0).name, "But  _  _ton💓.tsx");
}

#[test]
fn trims_trailing_whitespace_from_a_bare_name() {
    assert_eq!(node("index.js  ", 0).name, "index.js");
}

#[test]
fn assigns_no_default_icon_without_an_annotation() {
    assert_eq!(node("utils.ts", 0).icon, None);
    assert_eq!(node("components/", 0).icon, None);
}

#[test]
fn sets_the_icon_to_none_for_icon_none() {
    let node = node("index.js icon(none)", 0);
    assert_eq!(node.name, "index.js");
    assert_eq!(node.icon.as_deref(), Some("none"));
}

#[test]
fn sets_the_icon_to_none_for_an_empty_icon() {
    let node = node("index.js icon()", 0);
    assert_eq!(node.name, "index.js");
    assert_eq!(node.icon.as_deref(), Some("none"));
}

#[test]
fn suppresses_the_icon_on_a_directory() {
    let node = node("src/ icon(none)", 0);
    assert_eq!(node.name, "src");
    assert_eq!(node.node_type, NodeType::Directory);
    assert_eq!(node.icon.as_deref(), Some("none"));
}

#[test]
fn combines_icon_none_with_other_annotations() {
    let node = node("app.ts icon(none) :::highlight ## entry point", 0);
    assert_eq!(node.name, "app.ts");
    assert_eq!(node.icon.as_deref(), Some("none"));
    assert_eq!(node.css_class.as_deref(), Some("highlight"));
    assert_eq!(node.description.as_deref(), Some("entry point"));
}

/// The box-drawing input of each case must produce the same tree as its indent-based form.
#[test]
fn box_drawing_flat_list_matches_indent_input() {
    let indent = "treeView-beta\n    src/\n        index.js\n        App.tsx\n    package.json";
    let box_drawing =
        "treeView-beta\n├── src/\n│   ├── index.js\n│   └── App.tsx\n└── package.json";
    assert_eq!(ast(box_drawing), ast(indent));
}

#[test]
fn box_drawing_nested_directories_match_indent_input() {
    let indent = "treeView-beta\n    src/\n        components/\n            Button.tsx\n            Modal.tsx\n        utils/\n            helpers.ts\n    README.md";
    let box_drawing = "treeView-beta\n├── src/\n│   ├── components/\n│   │   ├── Button.tsx\n│   │   └── Modal.tsx\n│   └── utils/\n│       └── helpers.ts\n└── README.md";
    assert_eq!(ast(box_drawing), ast(indent));
}

#[test]
fn box_drawing_with_annotations_matches_indent_input() {
    let indent = "treeView-beta\n    src/\n        App.tsx :::highlight ## main component\n        index.js ## entry point\n    package.json";
    let box_drawing = "treeView-beta\n├── src/\n│   ├── App.tsx :::highlight ## main component\n│   └── index.js ## entry point\n└── package.json";
    assert_eq!(ast(box_drawing), ast(indent));
}

#[test]
fn box_drawing_deeply_nested_matches_indent_input() {
    let indent = "treeView-beta\n    a/\n        b/\n            c/\n                d.txt\n            e.txt\n        f.txt";
    let box_drawing = "treeView-beta\n└── a/\n    ├── b/\n    │   ├── c/\n    │   │   └── d.txt\n    │   └── e.txt\n    └── f.txt";
    assert_eq!(ast(box_drawing), ast(indent));
}

// packages/mermaid/src/diagrams/treeView/boxDrawingPreprocessor.spec.ts

#[test]
fn detects_box_drawing_characters() {
    assert!(box_drawing::is_box_drawing_format(&[
        "├── file.txt",
        "└── other.txt"
    ]));
    assert!(box_drawing::is_box_drawing_format(&[
        "┣━━ file.txt",
        "┗━━ other.txt"
    ]));
    assert!(box_drawing::is_box_drawing_format(&["│   └── file.txt"]));
}

#[test]
fn does_not_detect_indentation_or_plain_text() {
    assert!(!box_drawing::is_box_drawing_format(&[
        "    file.txt",
        "        nested.txt"
    ]));
    assert!(!box_drawing::is_box_drawing_format(&["", "   "]));
    assert!(!box_drawing::is_box_drawing_format(&["root/", "file.txt"]));
}

fn preprocess(input: &str) -> box_drawing::Preprocessed {
    box_drawing::preprocess(input)
        .unwrap_or_else(|error| panic!("expected {input:?} to preprocess, got {error}"))
}

fn lines_of(preprocessed: &box_drawing::Preprocessed) -> Vec<&str> {
    preprocessed.text.split('\n').collect()
}

#[test]
fn returns_indent_based_input_unchanged() {
    let input = "treeView-beta\nroot/\n    src/\n        index.js";
    let preprocessed = preprocess(input);
    assert_eq!(preprocessed.text, input);
    assert_eq!(preprocessed.original_line(1), None);
}

#[test]
fn returns_input_without_the_keyword_unchanged() {
    let input = "not-a-keyword\nsome text";
    let preprocessed = preprocess(input);
    assert_eq!(preprocessed.text, input);
    assert_eq!(preprocessed.original_line(1), None);
}

#[test]
fn converts_a_simple_tree() {
    let input = "treeView-beta\nroot/\n├── src/\n│   └── index.js\n└── README.md";
    let preprocessed = preprocess(input);
    assert_eq!(
        lines_of(&preprocessed),
        [
            "treeView-beta",
            "root/",
            "    src/",
            "        index.js",
            "    README.md"
        ]
    );
}

#[test]
fn converts_a_deeply_nested_tree() {
    let input = "treeView-beta\nroot/\n├── a/\n│   ├── b/\n│   │   ├── c/\n│   │   │   └── deep.txt\n│   │   └── d.txt\n│   └── e.txt\n└── f.txt";
    let preprocessed = preprocess(input);
    assert_eq!(
        lines_of(&preprocessed),
        [
            "treeView-beta",
            "root/",
            "    a/",
            "        b/",
            "            c/",
            "                deep.txt",
            "            d.txt",
            "        e.txt",
            "    f.txt"
        ]
    );
}

#[test]
fn converts_a_flat_list() {
    let input = "treeView-beta\nroot/\n├── file1.txt\n├── file2.txt\n└── file3.txt";
    let preprocessed = preprocess(input);
    assert_eq!(
        lines_of(&preprocessed),
        [
            "treeView-beta",
            "root/",
            "    file1.txt",
            "    file2.txt",
            "    file3.txt"
        ]
    );
}

#[test]
fn converts_a_tree_without_a_root_line() {
    let input = "treeView-beta\n├── file1.txt\n└── file2.txt";
    let preprocessed = preprocess(input);
    assert_eq!(
        lines_of(&preprocessed),
        ["treeView-beta", "    file1.txt", "    file2.txt"]
    );
}

#[test]
fn handles_space_continuation_after_the_last_child_marker() {
    let input = "treeView-beta\nroot/\n├── a/\n│   └── b.txt\n└── c/\n    └── d.txt";
    let preprocessed = preprocess(input);
    assert_eq!(
        lines_of(&preprocessed),
        [
            "treeView-beta",
            "root/",
            "    a/",
            "        b.txt",
            "    c/",
            "        d.txt"
        ]
    );
}

#[test]
fn handles_flexible_prefix_widths() {
    for (line, expected) in [
        ("├─── file.txt", "    file.txt"),
        ("├──file.txt", "    file.txt"),
        ("├───── file.txt", "    file.txt"),
        ("└─ file.txt", "    file.txt"),
    ] {
        let input = format!("treeView-beta\nroot/\n{line}");
        let preprocessed = preprocess(&input);
        assert_eq!(lines_of(&preprocessed)[2], expected, "line: {line:?}");
    }
}

#[test]
fn handles_heavy_unicode_variants() {
    let input = "treeView-beta\nroot/\n┣━━ src/\n┃   ┗━━ index.js\n┗━━ README.md";
    let preprocessed = preprocess(input);
    assert_eq!(
        lines_of(&preprocessed),
        [
            "treeView-beta",
            "root/",
            "    src/",
            "        index.js",
            "    README.md"
        ]
    );
    let extra_dashes = preprocess("treeView-beta\nroot/\n┣━━━ file.txt");
    assert_eq!(lines_of(&extra_dashes)[2], "    file.txt");
}

#[test]
fn preserves_annotations() {
    for (line, expected) in [
        ("├── file.txt :::highlight", "    file.txt :::highlight"),
        ("├── data.bin icon(database)", "    data.bin icon(database)"),
        ("├── index.js ## entry point", "    index.js ## entry point"),
        (
            "├── App.tsx :::highlight icon(react) ## main component",
            "    App.tsx :::highlight icon(react) ## main component",
        ),
    ] {
        let input = format!("treeView-beta\n{line}");
        let preprocessed = preprocess(&input);
        assert_eq!(lines_of(&preprocessed)[1], expected, "line: {line:?}");
    }
}

#[test]
fn preserves_comments_and_blank_lines() {
    let input = "treeView-beta\n%% a comment\n├── file.txt\n%% another comment\n└── other.txt";
    let preprocessed = preprocess(input);
    assert_eq!(
        lines_of(&preprocessed),
        [
            "treeView-beta",
            "%% a comment",
            "    file.txt",
            "%% another comment",
            "    other.txt"
        ]
    );

    let blank = "treeView-beta\n\n├── file.txt\n\n└── other.txt";
    let preprocessed = preprocess(blank);
    assert_eq!(
        lines_of(&preprocessed),
        ["treeView-beta", "", "    file.txt", "", "    other.txt"]
    );
}

#[test]
fn passes_metadata_lines_through() {
    for (line, expected) in [
        ("title My Tree", "title My Tree"),
        ("accTitle: Accessible Title", "accTitle: Accessible Title"),
        ("accDescr: A description", "accDescr: A description"),
    ] {
        let input = format!("treeView-beta\n{line}\n├── file.txt\n└── other.txt");
        let preprocessed = preprocess(&input);
        assert_eq!(lines_of(&preprocessed)[1], expected, "line: {line:?}");
        assert_eq!(lines_of(&preprocessed)[2], "    file.txt", "line: {line:?}");
    }
}

#[test]
fn skips_decoration_only_lines() {
    let input = "treeView-beta\nroot/\n├── a.txt\n│\n└── b.txt";
    let preprocessed = preprocess(input);
    assert_eq!(
        lines_of(&preprocessed),
        ["treeView-beta", "root/", "    a.txt", "    b.txt"]
    );
}

#[test]
fn maps_output_lines_to_original_lines() {
    let input = "treeView-beta\nroot/\n├── src/\n│   └── index.js\n└── README.md";
    let preprocessed = preprocess(input);
    for line in 1..=5 {
        assert_eq!(preprocessed.original_line(line), Some(line));
    }
}

#[test]
fn maps_skipped_decoration_lines() {
    let input = "treeView-beta\n├── a.txt\n│\n└── b.txt";
    let preprocessed = preprocess(input);
    assert_eq!(preprocessed.original_line(1), Some(1));
    assert_eq!(preprocessed.original_line(2), Some(2));
    assert_eq!(preprocessed.original_line(3), Some(4));
}

#[test]
fn rejects_empty_node_content() {
    let error = error("treeView-beta\n├── ");
    assert_eq!(
        error.message,
        "Line 2: Empty node — expected a filename or directory name after the box-drawing prefix"
    );
    assert_eq!(error.span.map(|span| span.line), Some(2));
}

#[test]
fn includes_the_line_number_in_the_empty_content_error() {
    let error = error("treeView-beta\nroot/\n├── src/\n│   └── ");
    assert!(error.message.starts_with("Line 4:"), "{}", error.message);
}

#[test]
fn normalizes_tabs_to_spaces() {
    let preprocessed = preprocess("treeView-beta\n├──\tfile.txt");
    assert_eq!(lines_of(&preprocessed)[1], "    file.txt");
}

#[test]
fn handles_tab_indented_box_drawing_lines() {
    let input = "treeView-beta\n├── src/\n\t├── index.ts\n\t└── utils.ts";
    let preprocessed = preprocess(input);
    assert_eq!(
        lines_of(&preprocessed),
        [
            "treeView-beta",
            "    src/",
            "        index.ts",
            "        utils.ts"
        ]
    );
}

#[test]
fn rejects_indentation_without_box_drawing_characters() {
    let error = error("treeView-beta\n├── src/\n    index.js\n└── README.md");
    assert_eq!(
        error.message,
        "Line 3: Unexpected indentation without box-drawing characters. In box-drawing format, use ├── or └── prefixes for indented nodes."
    );
    assert_eq!(error.span.map(|span| span.line), Some(3));
}

#[test]
fn converts_typical_tree_command_output() {
    let input = "treeView-beta\nmy-project/\n├── src/\n│   ├── components/\n│   │   └── MyComponent.js\n│   ├── index.css\n│   └── index.js\n├── package.json\n└── README.md";
    let preprocessed = preprocess(input);
    assert_eq!(
        lines_of(&preprocessed),
        [
            "treeView-beta",
            "my-project/",
            "    src/",
            "        components/",
            "            MyComponent.js",
            "        index.css",
            "        index.js",
            "    package.json",
            "    README.md"
        ]
    );
}

#[test]
fn converts_tree_output_with_annotations() {
    let input = "treeView-beta\nmy-project/\n├── src/ :::highlight\n│   ├── App.tsx :::highlight icon(react) ## main component\n│   └── index.ts ## entry point\n├── package.json\n└── README.md ## project docs";
    let preprocessed = preprocess(input);
    assert_eq!(
        lines_of(&preprocessed),
        [
            "treeView-beta",
            "my-project/",
            "    src/ :::highlight",
            "        App.tsx :::highlight icon(react) ## main component",
            "        index.ts ## entry point",
            "    package.json",
            "    README.md ## project docs"
        ]
    );
}

// `remapErrorLines`: an error the grammar raises on a converted line points at the original
// line and column instead of the rewritten one.

#[test]
fn remaps_an_error_on_a_converted_line() {
    // Output line 3 is original line 4 because the `│` line was dropped.
    let error = error("treeView-beta\n├── a.txt\n│\n├── \"oops");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (4, 5));
}

#[test]
fn remaps_the_column_past_a_shorter_prefix() {
    // `└─ ` is three characters, so the content starts one column earlier than the four-space
    // indentation the conversion writes.
    let error = error("treeView-beta\n└─ \"oops");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (2, 4));
}

// Behavior the specs above do not pin down.

#[test]
fn a_later_title_replaces_an_earlier_one() {
    let ast = ast("treeView-beta\ntitle first\ntitle second");
    assert_eq!(ast.diagram_title, "second");
}

#[test]
fn a_title_line_after_the_first_node_is_an_error() {
    let error = error("treeView-beta\nindex.js\ntitle My Tree");
    assert!(error.message.contains("TITLE"), "{}", error.message);
}

#[test]
fn a_stray_quote_reports_its_span() {
    let error = error("treeView-beta\nindex.js\n\"");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (3, 1));
}

#[test]
fn a_header_without_its_keyword_is_an_error() {
    let error = error("not-a-keyword\nsome text");
    assert!(
        error.message.contains("expected treeView-beta"),
        "{}",
        error.message
    );
}

#[test]
fn a_longer_name_wins_over_the_keyword() {
    // Langium gives the keyword the `BARE_NAME` terminal as its `LONGER_ALT`, so a line that
    // continues past the keyword is a name, not a keyword.
    let ast = ast("treeView-beta\ntreeView-beta wall");
    assert_eq!(ast.root.children[0].name, "treeView-beta wall");

    let error = error("treeView-beta wall\nindex.js");
    assert!(
        error.message.contains("expected treeView-beta"),
        "{}",
        error.message
    );
}

#[test]
fn parses_crlf_line_endings() {
    let ast = ast("treeView-beta\r\nsrc/\r\n    index.js\r\n");
    assert_eq!(ast.root.children[0].name, "src");
    assert_eq!(ast.root.children[0].children[0].name, "index.js");
    assert_eq!(ast.root.children[0].children[0].level, 4);
}

#[test]
fn sanitizes_a_description() {
    for (line, expected) in [
        ("a.ts ## <script>alert(1)</script>ok", "ok"),
        ("a.ts ## a < b", "a &lt; b"),
    ] {
        assert_eq!(
            node(line, 0).description.as_deref(),
            Some(expected),
            "line: {line:?}"
        );
    }
}

#[test]
fn parses_a_multi_line_acc_description() {
    let ast = ast("treeView-beta\naccDescr {\n    first line\n    second line\n}\na.ts");
    assert_eq!(ast.acc_description, "first line\nsecond line");
    assert_eq!(ast.root.children[0].name, "a.ts");
}

#[test]
fn a_child_deeper_than_its_parent_nests_under_it() {
    let ast = ast("treeView-beta\na/\n        b/\n            c.txt");
    let a = &ast.root.children[0];
    assert_eq!(a.children[0].name, "b");
    assert_eq!(a.children[0].level, 8);
    assert_eq!(a.children[0].children[0].name, "c.txt");
    assert_eq!(a.children[0].children[0].level, 12);
}

#[test]
fn the_crate_parses_the_diagram_through_its_entry_point() {
    let text = "---\ntitle: Tree\n---\ntreeView-beta\n%% a comment\n├── src/\n│   └── index.js";
    match crate::parse(text) {
        Ok(crate::Diagram::TreeView(ast)) => {
            assert_eq!(ast.root.children[0].name, "src");
            assert_eq!(ast.root.children[0].children[0].name, "index.js");
        }
        other => panic!("expected a treeView diagram, got {other:?}"),
    }
}
