//! Tests ported from the upstream git graph specs: `packages/parser/tests/gitGraph.test.ts`
//! (the Langium grammar) and `packages/mermaid/src/diagrams/git/gitGraph.spec.ts` (the
//! database). Inputs and expectations are upstream's; the grammar tests inspect
//! [`parser::parse_tree`], the Langium AST, because several of them describe statements the
//! database would reject.
//!
//! Skipped, and why:
//! - `gitGraph.spec.ts`'s four `it.skip` cases: reset a branch, reset with an argument, fast
//!   forwardable merges, and a fast-forward merge whose history walk has two parents. They are
//!   skipped upstream too (no `reset` statement exists in the grammar).
//! - `gitGraph.spec.ts`'s `config` describe (five cases): `getConfig` belongs to configuration,
//!   which this crate does not own.
//! - The `log.warn` spy in "should log a warning when two commits have the same ID": there is
//!   no logger here; the state the warning accompanies is asserted instead.
//! - `expect(...parents).not.toBeNull()`: a `Vec` is never null.
//! - "should ignore malformed properties and not break parsing" (commit and merge): the raw
//!   `LangiumParser` recovers and the test inspects the recovered tree, while the diagram
//!   pipeline throws when the parser reports an error. Those inputs are errors here.
//! - The one-line `commit ... commit ... cherry-pick ...` input of "should parse cherry-picking
//!   with a commit id": only error recovery accepts it (it inserts the missing `EOL`). The test
//!   keeps the expectations on the newline-separated equivalent and asserts the one-line form
//!   is an error.
//! - `$type` markers, `hasOwnProperty`, and "property is undefined": Langium AST bookkeeping.
//!   The typed trees here have no unknown properties and use `Option` for absent ones.
//! - `createGitGraphTestServices`: Langium service wiring. `parse` returning `Ok` is the
//!   error-free assertion here.

use super::ast::{Ast, Branch, Commit, CommitType, Direction};
use super::parse;
use super::parser::{
    self, BranchStatement, CheckoutStatement, CherryPickStatement, CommitStatement, MergeStatement,
    ParsedGitGraph, Statement,
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

fn commit<'a>(ast: &'a Ast, id: &str) -> &'a Commit {
    ast.commit(id)
        .unwrap_or_else(|| panic!("commit {id:?} not found in {:?}", ast.commits))
}

fn branch<'a>(ast: &'a Ast, name: &str) -> &'a Branch {
    ast.branch(name)
        .unwrap_or_else(|| panic!("branch {name:?} not found in {:?}", ast.branches))
}

fn branch_names(ast: &Ast) -> Vec<&str> {
    ast.branches
        .iter()
        .map(|branch| branch.name.as_str())
        .collect()
}

// packages/parser/tests/gitGraph.test.ts

fn tree(body: &str) -> ParsedGitGraph {
    parser::parse_tree(body)
        .unwrap_or_else(|error| panic!("expected {body:?} to parse, got {error}"))
}

fn tree_error(body: &str) -> ParseError {
    match parser::parse_tree(body) {
        Ok(parsed) => panic!("expected {body:?} to fail, parsed {parsed:?}"),
        Err(error) => error,
    }
}

fn commit_statement(parsed: &ParsedGitGraph, index: usize) -> &CommitStatement {
    match &parsed.statements[index] {
        Statement::Commit(statement) => statement,
        other => panic!("expected a Commit at {index}, got {other:?}"),
    }
}

fn branch_statement(parsed: &ParsedGitGraph, index: usize) -> &BranchStatement {
    match &parsed.statements[index] {
        Statement::Branch(statement) => statement,
        other => panic!("expected a Branch at {index}, got {other:?}"),
    }
}

fn merge_statement(parsed: &ParsedGitGraph, index: usize) -> &MergeStatement {
    match &parsed.statements[index] {
        Statement::Merge(statement) => statement,
        other => panic!("expected a Merge at {index}, got {other:?}"),
    }
}

fn checkout_statement(parsed: &ParsedGitGraph, index: usize) -> &CheckoutStatement {
    match &parsed.statements[index] {
        Statement::Checkout(statement) => statement,
        other => panic!("expected a Checkout at {index}, got {other:?}"),
    }
}

fn cherry_pick_statement(parsed: &ParsedGitGraph, index: usize) -> &CherryPickStatement {
    match &parsed.statements[index] {
        Statement::CherryPick(statement) => statement,
        other => panic!("expected a CherryPick at {index}, got {other:?}"),
    }
}

// Parsing Commit Statements

#[test]
fn parses_a_simple_commit() {
    let parsed = tree("gitGraph\n  commit\n");
    assert!(matches!(parsed.statements[0], Statement::Commit(_)));
}

#[test]
fn parses_multiple_commits() {
    let parsed = tree("gitGraph\n  commit\n  commit\n  commit\n");
    assert_eq!(parsed.statements.len(), 3);
}

#[test]
fn parses_commits_with_all_properties() {
    let parsed = tree("gitGraph\n  commit id:\"1\" msg:\"Fix bug\" tag:\"v1.2\" type:NORMAL\n");
    let commit = commit_statement(&parsed, 0);
    assert_eq!(commit.id.as_deref(), Some("1"));
    assert_eq!(commit.message.as_deref(), Some("Fix bug"));
    assert_eq!(commit.tags, ["v1.2"]);
    assert_eq!(commit.kind, Some(CommitType::Normal));
}

#[test]
fn handles_commit_messages_with_special_characters() {
    let parsed = tree("gitGraph\n  commit msg:\"Fix issue #123: Handle errors\"\n");
    assert_eq!(
        commit_statement(&parsed, 0).message.as_deref(),
        Some("Fix issue #123: Handle errors")
    );
}

#[test]
fn parses_commits_with_only_a_message_and_no_other_properties() {
    let parsed = tree("gitGraph\n  commit msg:\"Initial release\"\n");
    let commit = commit_statement(&parsed, 0);
    assert_eq!(commit.message.as_deref(), Some("Initial release"));
    assert_eq!(commit.id, None);
    assert_eq!(commit.kind, None);
}

#[test]
fn parses_multiple_commits_with_different_types() {
    let parsed = tree("gitGraph\n  commit type:NORMAL\n  commit type:REVERSE\n");
    assert_eq!(commit_statement(&parsed, 0).kind, Some(CommitType::Normal));
    assert_eq!(commit_statement(&parsed, 1).kind, Some(CommitType::Reverse));
}

// Parsing Branch Statements

#[test]
fn parses_a_branch_with_a_simple_name() {
    let parsed = tree("gitGraph\n commit\n commit\n branch master\n");
    assert_eq!(branch_statement(&parsed, 2).name, "master");
}

#[test]
fn parses_a_branch_name_starting_with_numbers() {
    let parsed = tree("gitGraph\n commit\n branch 1.0.1\n");
    assert_eq!(branch_statement(&parsed, 1).name, "1.0.1");
}

#[test]
fn parses_a_branch_with_an_order_property() {
    let parsed = tree("gitGraph\n commit\n  branch feature order:1\n");
    let branch = branch_statement(&parsed, 1);
    assert_eq!(branch.name, "feature");
    assert_eq!(branch.order, Some(1));
}

#[test]
fn handles_branch_names_with_special_characters() {
    let parsed = tree("gitGraph\n  branch feature/test-branch\n");
    assert_eq!(branch_statement(&parsed, 0).name, "feature/test-branch");
}

#[test]
fn parses_branches_with_hyphens_and_underscores() {
    let parsed = tree("gitGraph\n  branch my-feature_branch\n");
    assert_eq!(branch_statement(&parsed, 0).name, "my-feature_branch");
}

#[test]
fn correctly_handles_a_branch_without_an_order_property() {
    let parsed = tree("gitGraph\n  branch feature\n");
    let branch = branch_statement(&parsed, 0);
    assert_eq!(branch.name, "feature");
    assert_eq!(branch.order, None);
}

// Parsing Merge Statements

#[test]
fn parses_a_merge_with_a_branch_name() {
    let parsed = tree("gitGraph\n merge master\n");
    assert_eq!(merge_statement(&parsed, 0).branch, "master");
}

#[test]
fn handles_merges_with_additional_properties() {
    let parsed = tree("gitGraph\n  merge feature id:\"m1\" tag:\"release\" type:HIGHLIGHT\n");
    let merge = merge_statement(&parsed, 0);
    assert_eq!(merge.branch, "feature");
    assert_eq!(merge.id.as_deref(), Some("m1"));
    assert_eq!(merge.tags, ["release"]);
    assert_eq!(merge.kind, Some(CommitType::Highlight));
}

#[test]
fn parses_a_merge_without_any_properties() {
    let parsed = tree("gitGraph\n  merge feature\n");
    assert_eq!(merge_statement(&parsed, 0).branch, "feature");
}

// Parsing Checkout Statements

#[test]
fn parses_a_checkout_to_a_named_branch() {
    let parsed =
        tree("gitGraph\n commit id:\"1\"\n branch develop\n branch fun\n checkout develop\n");
    assert_eq!(checkout_statement(&parsed, 3).branch, "develop");
}

#[test]
fn parses_checkout_to_branches_with_complex_names() {
    let parsed = tree("gitGraph\n  checkout hotfix-123\n");
    assert_eq!(checkout_statement(&parsed, 0).branch, "hotfix-123");
}

#[test]
fn parses_checkouts_with_hyphens_and_numbers() {
    let parsed = tree("gitGraph\n  checkout release-2021\n");
    assert_eq!(checkout_statement(&parsed, 0).branch, "release-2021");
}

// Parsing CherryPicking Statements

#[test]
fn parses_cherry_picking_with_a_commit_id() {
    // Upstream's input puts all three statements on one line, which only Langium's error
    // recovery accepts (it inserts the missing `EOL`); the grammar needs the newlines, and
    // the diagram parser throws on the one-line form. The statement tree is the same.
    let parsed =
        tree("gitGraph\n  commit id:\"123\"\n  commit id:\"321\"\n  cherry-pick id:\"123\"\n");
    assert_eq!(cherry_pick_statement(&parsed, 2).id.as_deref(), Some("123"));
    assert!(
        parser::parse_tree(
            "gitGraph\n commit id:\"123\" commit id:\"321\" cherry-pick id:\"123\"\n"
        )
        .is_err()
    );
}

#[test]
fn parses_cherry_picking_with_multiple_properties() {
    let parsed = tree("gitGraph\n  cherry-pick id:\"123\" tag:\"urgent\" parent:\"100\"\n");
    let cherry_pick = cherry_pick_statement(&parsed, 0);
    assert_eq!(cherry_pick.id.as_deref(), Some("123"));
    assert_eq!(cherry_pick.tags, Some(vec!["urgent".to_string()]));
    assert_eq!(cherry_pick.parent.as_deref(), Some("100"));
}

// Parsing with Accessibility Titles and Descriptions

#[test]
fn parses_accessibility_titles() {
    let parsed = tree("gitGraph\n  accTitle: Accessible Graph\n  commit\n");
    assert_eq!(parsed.acc_title.as_deref(), Some("Accessible Graph"));
}

#[test]
fn parses_multiline_accessibility_descriptions() {
    let parsed = tree(
        "gitGraph\n  accDescr {\n    Detailed description\n    across multiple lines\n  }\n  commit\n",
    );
    assert_eq!(
        parsed.acc_description.as_deref(),
        Some("Detailed description\nacross multiple lines")
    );
}

// Integration Tests

#[test]
fn correctly_parses_a_complex_graph_with_various_elements() {
    let parsed = tree(
        "
        gitGraph TB:
        accTitle: Complex Example
        commit id:\"init\" type:NORMAL
        branch feature
        commit id:\"feat1\" msg:\"Add feature\"
        checkout main
        merge feature tag:\"v1.0\"
        cherry-pick id:\"feat1\" tag:\"critical fix\"
      ",
    );
    assert_eq!(parsed.acc_title.as_deref(), Some("Complex Example"));
    assert_eq!(parsed.direction, Some(Direction::Tb));
    assert!(matches!(parsed.statements[0], Statement::Commit(_)));
    assert!(matches!(parsed.statements[1], Statement::Branch(_)));
    assert!(matches!(parsed.statements[2], Statement::Commit(_)));
    assert!(matches!(parsed.statements[3], Statement::Checkout(_)));
    assert!(matches!(parsed.statements[4], Statement::Merge(_)));
    assert!(matches!(parsed.statements[5], Statement::CherryPick(_)));
}

// Error Handling for Invalid Syntax

#[test]
fn reports_errors_for_unknown_properties_in_commit() {
    // The raw LangiumParser recovers from this; the diagram parser throws.
    let error = tree_error("gitGraph\n  commit unknown:\"oops\"\n");
    assert!(error.message.contains("expected a newline or end of input"));
}

#[test]
fn reports_errors_for_an_invalid_branch_order() {
    let error = tree_error("gitGraph\n  branch feature order:xyz\n");
    assert!(error.message.contains("expected INT"));
}

#[test]
fn ignores_malformed_properties_by_failing_the_statement() {
    // Upstream's recovery keeps `id` and `message` and drops `oops`, but the diagram pipeline
    // reports a parse error for the same text.
    assert!(
        parser::parse_tree(
            "gitGraph\n  commit id:\"2\" msg:\"Malformed commit\" oops:\"ignored\"\n"
        )
        .is_err()
    );
    assert!(parser::parse_tree("gitGraph\n  merge feature random:\"ignored\"\n").is_err());
}

// packages/mermaid/src/diagrams/git/gitGraph.spec.ts

#[test]
fn handles_a_git_graph_definition() {
    let ast = ast("gitGraph:\n commit\n");
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.current_branch, "main");
    assert_eq!(ast.direction, Direction::Lr);
    assert_eq!(ast.branches.len(), 1);
}

#[test]
fn handles_setting_the_direction_top_to_bottom() {
    let ast = ast("gitGraph TB:\ncommit\n");
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.current_branch, "main");
    assert_eq!(ast.direction, Direction::Tb);
    assert_eq!(ast.branches.len(), 1);
}

#[test]
fn handles_setting_the_direction_bottom_to_top() {
    let ast = ast("gitGraph BT:\ncommit\n");
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.current_branch, "main");
    assert_eq!(ast.direction, Direction::Bt);
    assert_eq!(ast.branches.len(), 1);
}

#[test]
fn checks_out_a_branch() {
    let ast = ast("gitGraph:\nbranch new\ncheckout new\n");
    assert_eq!(ast.commits.len(), 0);
    assert_eq!(ast.current_branch, "new");
}

#[test]
fn switches_a_branch() {
    let ast = ast("gitGraph:\nbranch new\nswitch new\n");
    assert_eq!(ast.commits.len(), 0);
    assert_eq!(ast.current_branch, "new");
}

#[test]
fn adds_commits_to_the_checked_out_branch() {
    let ast = ast("gitGraph:\nbranch new\ncheckout new\ncommit\ncommit\n");
    assert_eq!(ast.commits.len(), 2);
    assert_eq!(ast.current_branch, "new");
    let head = branch(&ast, "new").head.as_deref().expect("a head");
    assert!(ast.commit(head).is_some());
}

#[test]
fn handles_a_commit_with_args() {
    let ast = ast("gitGraph:\ncommit \"a commit\"\n");
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.commits[0].message, "a commit");
    assert_eq!(ast.current_branch, "main");
}

#[test]
fn handles_cases_when_merge_is_a_noop() {
    let ast = ast(
        "gitGraph:\ncommit\nbranch newbranch\ncheckout newbranch\ncommit\ncommit\nmerge main\n",
    );
    assert_eq!(ast.commits.len(), 4);
    assert_eq!(ast.current_branch, "newbranch");
    assert_ne!(branch(&ast, "newbranch").head, branch(&ast, "main").head);
    assert_eq!(ast.head, branch(&ast, "newbranch").head);
}

#[test]
fn handles_a_merge_with_2_parents() {
    let ast = ast(
        "gitGraph:\ncommit\nbranch newbranch\ncheckout newbranch\ncommit\ncommit\ncheckout main\ncommit\nmerge newbranch\n",
    );
    assert_eq!(ast.commits.len(), 5);
    assert_eq!(ast.current_branch, "main");
    assert_ne!(branch(&ast, "newbranch").head, branch(&ast, "main").head);
    assert_eq!(ast.head, branch(&ast, "main").head);
}

#[test]
fn generates_an_array_of_known_branches() {
    let ast = ast("gitGraph:\ncommit\nbranch b1\ncheckout b1\ncommit\ncommit\nbranch b2\n");
    assert_eq!(ast.branches_as_obj_array().len(), 3);
    assert_eq!(ast.branches_as_obj_array(), ["main", "b1", "b2"]);
}

#[test]
fn handles_a_commit_with_no_params_and_an_auto_generated_id() {
    let ast = ast("gitGraph:\n        commit\n        ");
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.current_branch, "main");
    assert_eq!(ast.direction, Direction::Lr);
    assert_eq!(ast.branches.len(), 1);
    let commit = &ast.commits[0];
    assert_eq!(commit.message, "");
    assert!(!commit.id.is_empty());
    assert_eq!(commit.tags, Vec::<String>::new());
    assert_eq!(commit.kind, CommitType::Normal);
}

#[test]
fn handles_a_commit_with_a_custom_id_only() {
    let ast = ast("gitGraph:\n        commit id:\"1111\"\n        ");
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.current_branch, "main");
    assert_eq!(ast.direction, Direction::Lr);
    assert_eq!(ast.branches.len(), 1);
    assert_eq!(ast.commits[0].message, "");
    assert_eq!(ast.commits[0].id, "1111");
    assert_eq!(ast.commits[0].tags, Vec::<String>::new());
    assert_eq!(ast.commits[0].kind, CommitType::Normal);
}

#[test]
fn handles_a_commit_with_a_custom_tag_only() {
    let ast = ast("gitGraph:\n        commit tag:\"test\"\n        ");
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.current_branch, "main");
    assert_eq!(ast.direction, Direction::Lr);
    assert_eq!(ast.branches.len(), 1);
    assert_eq!(ast.commits[0].message, "");
    assert!(!ast.commits[0].id.is_empty());
    assert_eq!(ast.commits[0].tags, ["test"]);
    assert_eq!(ast.commits[0].kind, CommitType::Normal);
}

#[test]
fn handles_a_commit_with_the_highlight_type_only() {
    let ast = ast("gitGraph:\n        commit type: HIGHLIGHT\n        ");
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.current_branch, "main");
    assert_eq!(ast.direction, Direction::Lr);
    assert_eq!(ast.branches.len(), 1);
    assert_eq!(ast.commits[0].kind.as_number(), 2);
}

#[test]
fn handles_a_commit_with_the_reverse_type_only() {
    let ast = ast("gitGraph:\n        commit type: REVERSE\n        ");
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.commits[0].kind.as_number(), 1);
}

#[test]
fn handles_a_commit_with_the_normal_type_only() {
    let ast = ast("gitGraph:\n        commit type: NORMAL\n        ");
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.commits[0].kind.as_number(), 0);
}

#[test]
fn handles_a_commit_with_a_custom_message_only() {
    let ast = ast("gitGraph:\n        commit \"test commit\"\n        ");
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.commits[0].message, "test commit");
    assert!(!ast.commits[0].id.is_empty());
    assert_eq!(ast.commits[0].tags, Vec::<String>::new());
    assert_eq!(ast.commits[0].kind, CommitType::Normal);
}

#[test]
fn handles_a_commit_with_a_custom_message_key_only() {
    let ast = ast("gitGraph:\n        commit msg: \"test commit\"\n        ");
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.commits[0].message, "test commit");
}

#[test]
fn handles_a_commit_with_a_custom_id_and_tag() {
    let ast = ast("gitGraph:\n        commit id:\"1111\" tag: \"test tag\"\n        ");
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.commits[0].id, "1111");
    assert_eq!(ast.commits[0].tags, ["test tag"]);
    assert_eq!(ast.commits[0].kind, CommitType::Normal);
}

#[test]
fn handles_a_commit_with_a_custom_type_and_tag() {
    let ast = ast("gitGraph:\n        commit type:HIGHLIGHT tag: \"test tag\"\n        ");
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.commits[0].tags, ["test tag"]);
    assert_eq!(ast.commits[0].kind.as_number(), 2);
}

#[test]
fn handles_a_commit_with_a_custom_tag_and_type() {
    let ast = ast("gitGraph:\n        commit tag: \"test tag\" type:HIGHLIGHT\n        ");
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.commits[0].tags, ["test tag"]);
    assert_eq!(ast.commits[0].kind.as_number(), 2);
}

#[test]
fn handles_a_commit_with_a_custom_id_type_and_tag() {
    let ast = ast("gitGraph:\n        commit id:\"1111\" type:REVERSE tag: \"test tag\"\n        ");
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.commits[0].id, "1111");
    assert_eq!(ast.commits[0].tags, ["test tag"]);
    assert_eq!(ast.commits[0].kind.as_number(), 1);
}

#[test]
fn handles_a_commit_with_a_custom_id_type_tag_and_message() {
    let ast = ast(
        "gitGraph:\n        commit id:\"1111\" type:REVERSE tag: \"test tag\" msg:\"test msg\"\n        ",
    );
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.commits[0].message, "test msg");
    assert_eq!(ast.commits[0].id, "1111");
    assert_eq!(ast.commits[0].tags, ["test tag"]);
    assert_eq!(ast.commits[0].kind.as_number(), 1);
}

#[test]
fn handles_a_commit_with_custom_type_tag_message_and_id() {
    let ast = ast(
        "gitGraph:\n        commit type:REVERSE tag: \"test tag\" msg: \"test msg\" id: \"1111\"\n\n        ",
    );
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.commits[0].message, "test msg");
    assert_eq!(ast.commits[0].id, "1111");
    assert_eq!(ast.commits[0].tags, ["test tag"]);
    assert_eq!(ast.commits[0].kind.as_number(), 1);
}

#[test]
fn handles_a_commit_with_custom_tag_message_id_and_type() {
    let ast = ast(
        "gitGraph:\n        commit  tag: \"test tag\" msg:\"test msg\" id:\"1111\" type:REVERSE\n        ",
    );
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.commits[0].message, "test msg");
    assert_eq!(ast.commits[0].id, "1111");
    assert_eq!(ast.commits[0].tags, ["test tag"]);
    assert_eq!(ast.commits[0].kind.as_number(), 1);
}

#[test]
fn handles_a_commit_with_custom_message_id_type_and_tag() {
    let ast = ast(
        "gitGraph:\n        commit msg:\"test msg\" id:\"1111\" type:REVERSE tag: \"test tag\"\n        ",
    );
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.commits[0].message, "test msg");
    assert_eq!(ast.commits[0].id, "1111");
    assert_eq!(ast.commits[0].tags, ["test tag"]);
    assert_eq!(ast.commits[0].kind.as_number(), 1);
}

#[test]
fn handles_3_straight_commits() {
    let ast = ast("gitGraph:\n        commit\n        commit\n        commit\n        ");
    assert_eq!(ast.commits.len(), 3);
    assert_eq!(ast.current_branch, "main");
    assert_eq!(ast.direction, Direction::Lr);
    assert_eq!(ast.branches.len(), 1);
}

#[test]
fn handles_new_branch_creation() {
    let ast = ast("gitGraph:\n        commit\n        branch testBranch\n        ");
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.current_branch, "testBranch");
    assert_eq!(ast.direction, Direction::Lr);
    assert_eq!(ast.branches.len(), 2);
}

#[test]
fn allows_quoted_branch_names() {
    let ast = ast(
        "gitGraph:\n        commit\n        branch \"branch\"\n        checkout \"branch\"\n        commit\n        checkout main\n        merge \"branch\"\n        ",
    );
    assert_eq!(ast.commits.len(), 3);
    assert_eq!(ast.current_branch, "main");
    assert_eq!(ast.direction, Direction::Lr);
    assert_eq!(ast.branches.len(), 2);
    assert_eq!(ast.commits[0].branch, "main");
    assert_eq!(ast.commits[1].branch, "branch");
    assert_eq!(ast.commits[2].branch, "main");
    assert_eq!(ast.branches_as_obj_array(), ["main", "branch"]);
}

#[test]
fn allows_underscores_dashes_dots_and_slashes_in_branch_names() {
    let ast = ast("gitGraph:\n        commit\n        branch azAZ_-./test\n        ");
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.current_branch, "azAZ_-./test");
    assert_eq!(ast.direction, Direction::Lr);
    assert_eq!(ast.branches.len(), 2);
}

#[test]
fn allows_branch_names_starting_with_numbers() {
    let ast = ast(
        "gitGraph:\n        commit\n        %% branch names starting with numbers are not recommended, but are supported by git\n        branch 1.0.1\n        ",
    );
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.current_branch, "1.0.1");
    assert_eq!(ast.direction, Direction::Lr);
    assert_eq!(ast.branches.len(), 2);
}

#[test]
fn allows_branch_names_starting_with_unusual_prefixes() {
    let ast = ast(
        "gitGraph:\n        commit\n        %% branch names starting with numbers are not recommended, but are supported by git\n        branch branch01\n        branch checkout02\n        branch cherry-pick03\n        branch branch/example-branch\n        branch merge/test_merge\n        %% single character branch name\n        branch A\n        ",
    );
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.current_branch, "A");
    assert_eq!(ast.direction, Direction::Lr);
    assert_eq!(ast.branches.len(), 7);
    for name in [
        "branch01",
        "checkout02",
        "cherry-pick03",
        "branch/example-branch",
        "merge/test_merge",
        "A",
    ] {
        assert!(ast.branch(name).is_some(), "missing {name}");
    }
}

#[test]
fn handles_new_branch_checkout() {
    let ast = ast(
        "gitGraph:\n        commit\n        branch testBranch\n        checkout testBranch\n        ",
    );
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.current_branch, "testBranch");
    assert_eq!(ast.direction, Direction::Lr);
    assert_eq!(ast.branches.len(), 2);
}

#[test]
fn handles_new_branch_checkout_with_order() {
    let ast = ast(
        "gitGraph:\n        commit\n        branch test1 order: 3\n        branch test2 order: 2\n        branch test3 order: 1\n        ",
    );
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.current_branch, "test3");
    assert_eq!(ast.branches.len(), 4);
    assert_eq!(
        ast.branches_as_obj_array(),
        ["main", "test3", "test2", "test1"]
    );
}

#[test]
fn handles_new_branch_checkout_with_and_without_order() {
    let ast = ast(
        "gitGraph:\n        commit\n        branch test1 order: 1\n        branch test2\n        branch test3\n        ",
    );
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.current_branch, "test3");
    assert_eq!(ast.branches.len(), 4);
    assert_eq!(
        ast.branches_as_obj_array(),
        ["main", "test2", "test3", "test1"]
    );
}

#[test]
fn handles_new_branch_checkout_and_commit() {
    let ast = ast(
        "gitGraph:\n        commit\n        branch testBranch\n        checkout testBranch\n        commit\n        ",
    );
    assert_eq!(ast.commits.len(), 2);
    assert_eq!(ast.current_branch, "testBranch");
    assert_eq!(ast.direction, Direction::Lr);
    assert_eq!(ast.branches.len(), 2);
    assert_eq!(ast.commits[0].branch, "main");
    assert_eq!(ast.commits[0].parents, Vec::<String>::new());
    assert_eq!(ast.commits[1].branch, "testBranch");
    assert_eq!(ast.commits[1].parents, vec![ast.commits[0].id.clone()]);
}

#[test]
fn handles_new_branch_checkout_and_commit_and_merge() {
    let ast = ast(
        "gitGraph:\n        commit\n        branch testBranch\n        checkout testBranch\n        commit\n        commit\n        checkout main\n        merge testBranch\n        ",
    );
    assert_eq!(ast.commits.len(), 4);
    assert_eq!(ast.current_branch, "main");
    assert_eq!(ast.direction, Direction::Lr);
    assert_eq!(ast.branches.len(), 2);
    assert_eq!(ast.commits[0].branch, "main");
    assert_eq!(ast.commits[0].parents, Vec::<String>::new());
    assert_eq!(ast.commits[1].branch, "testBranch");
    assert_eq!(ast.commits[1].parents, vec![ast.commits[0].id.clone()]);
    assert_eq!(ast.commits[2].branch, "testBranch");
    assert_eq!(ast.commits[2].parents, vec![ast.commits[1].id.clone()]);
    assert_eq!(ast.commits[3].branch, "main");
    assert_eq!(
        ast.commits[3].parents,
        [ast.commits[0].id.clone(), ast.commits[2].id.clone()]
    );
    assert_eq!(ast.branches_as_obj_array(), ["main", "testBranch"]);
}

#[test]
fn handles_new_branch_switch() {
    let ast = ast(
        "gitGraph:\n        commit\n        branch testBranch\n        switch testBranch\n        ",
    );
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.current_branch, "testBranch");
    assert_eq!(ast.direction, Direction::Lr);
    assert_eq!(ast.branches.len(), 2);
}

#[test]
fn handles_new_branch_switch_and_commit() {
    let ast = ast(
        "gitGraph:\n        commit\n        branch testBranch\n        switch testBranch\n        commit\n        ",
    );
    assert_eq!(ast.commits.len(), 2);
    assert_eq!(ast.current_branch, "testBranch");
    assert_eq!(ast.commits[0].branch, "main");
    assert_eq!(ast.commits[0].parents, Vec::<String>::new());
    assert_eq!(ast.commits[1].branch, "testBranch");
    assert_eq!(ast.commits[1].parents, vec![ast.commits[0].id.clone()]);
}

#[test]
fn handles_new_branch_switch_and_commit_and_merge() {
    let ast = ast(
        "gitGraph:\n        commit\n        branch testBranch\n        switch testBranch\n        commit\n        commit\n        switch main\n        merge testBranch\n        ",
    );
    assert_eq!(ast.commits.len(), 4);
    assert_eq!(ast.current_branch, "main");
    assert_eq!(ast.commits[2].parents, vec![ast.commits[1].id.clone()]);
    assert_eq!(
        ast.commits[3].parents,
        [ast.commits[0].id.clone(), ast.commits[2].id.clone()]
    );
    assert_eq!(ast.branches_as_obj_array(), ["main", "testBranch"]);
}

#[test]
fn handles_merge_tags() {
    let ast = ast(
        "gitGraph:\n        commit\n        branch testBranch\n        checkout testBranch\n        commit\n        checkout main\n        merge testBranch tag: \"merge-tag\"\n        ",
    );
    assert_eq!(ast.commits.len(), 3);
    assert_eq!(ast.current_branch, "main");
    assert_eq!(ast.direction, Direction::Lr);
    assert_eq!(ast.branches.len(), 2);
    assert_eq!(ast.commits[0].parents, Vec::<String>::new());
    assert_eq!(ast.commits[1].parents, vec![ast.commits[0].id.clone()]);
    assert_eq!(
        ast.commits[2].parents,
        [ast.commits[0].id.clone(), ast.commits[1].id.clone()]
    );
    assert_eq!(ast.commits[2].tags, ["merge-tag"]);
    assert_eq!(ast.branches_as_obj_array(), ["main", "testBranch"]);
}

#[test]
fn handles_a_merge_with_custom_ids_tags_and_type() {
    let ast = ast(
        "gitGraph:\n          commit\n          branch testBranch\n          checkout testBranch\n          commit\n          checkout main\n          %% Merge Tag and ID\n          merge testBranch tag: \"merge-tag\" id: \"2-222\"\n          branch testBranch2\n          checkout testBranch2\n          commit\n          checkout main\n          %% Merge ID and Tag (reverse order)\n          merge testBranch2 id: \"4-444\" tag: \"merge-tag2\" type:HIGHLIGHT\n          branch testBranch3\n          checkout testBranch3\n          commit\n          checkout main\n          %% just Merge ID\n          merge testBranch3 id: \"6-666\"\n        ",
    );
    assert_eq!(ast.commits.len(), 7);
    assert_eq!(ast.current_branch, "main");
    assert_eq!(ast.direction, Direction::Lr);

    let commits: Vec<&Commit> = ast.commits.iter().collect();
    let [
        main_commit,
        test_branch_commit,
        test_branch_merge,
        test_branch2_commit,
        test_branch2_merge,
        test_branch3_commit,
        test_branch3_merge,
    ] = commits[..]
    else {
        panic!("expected seven commits, got {}", commits.len());
    };

    assert_eq!(main_commit.branch, "main");
    assert_eq!(main_commit.parents, Vec::<String>::new());

    assert_eq!(test_branch_commit.branch, "testBranch");
    assert_eq!(test_branch_commit.parents, vec![main_commit.id.clone()]);

    assert_eq!(test_branch_merge.branch, "main");
    assert_eq!(
        test_branch_merge.parents,
        [main_commit.id.clone(), test_branch_commit.id.clone()]
    );
    assert_eq!(test_branch_merge.tags, ["merge-tag"]);
    assert_eq!(test_branch_merge.id, "2-222");

    assert_eq!(test_branch2_merge.branch, "main");
    assert_eq!(
        test_branch2_merge.parents,
        [test_branch_merge.id.clone(), test_branch2_commit.id.clone()]
    );
    assert_eq!(test_branch2_merge.tags, ["merge-tag2"]);
    assert_eq!(test_branch2_merge.id, "4-444");
    assert_eq!(
        test_branch2_merge.custom_type.map(CommitType::as_number),
        Some(2)
    );
    assert!(test_branch2_merge.custom_id);

    assert_eq!(test_branch3_merge.branch, "main");
    assert_eq!(
        test_branch3_merge.parents,
        [
            test_branch2_merge.id.clone(),
            test_branch3_commit.id.clone()
        ]
    );
    assert_eq!(test_branch3_merge.id, "6-666");

    assert_eq!(
        ast.branches_as_obj_array(),
        ["main", "testBranch", "testBranch2", "testBranch3"]
    );
    assert_eq!(ast.head.as_deref(), Some("6-666"));
}

#[test]
fn supports_cherry_picking_commits() {
    let ast = ast(
        "gitGraph\n        commit id: \"ZERO\"\n        branch develop\n        commit id:\"A\"\n        checkout main\n        cherry-pick id:\"A\"\n        ",
    );
    assert_eq!(ast.commits[2].tags, ["cherry-pick:A"]);
    assert_eq!(ast.commits[2].branch, "main");
}

#[test]
fn supports_cherry_picking_commits_with_a_custom_tag() {
    let ast = ast(
        "gitGraph\n        commit id: \"ZERO\"\n        branch develop\n        commit id:\"A\"\n        checkout main\n        cherry-pick id:\"A\" tag:\"MyTag\"\n        ",
    );
    assert_eq!(ast.commits[2].tags, ["MyTag"]);
    assert_eq!(ast.commits[2].branch, "main");
}

#[test]
fn supports_cherry_picking_commits_with_no_tag() {
    let ast = ast(
        "gitGraph\n        commit id: \"ZERO\"\n        branch develop\n        commit id:\"A\"\n        checkout main\n        cherry-pick id:\"A\" tag:\"\"\n        ",
    );
    assert_eq!(ast.commits[2].tags, Vec::<String>::new());
    assert_eq!(ast.commits[2].branch, "main");
}

#[test]
fn supports_cherry_picking_merge_commits() {
    let ast = ast(
        "gitGraph\n        commit id: \"ZERO\"\n        branch feature\n        branch release\n        checkout feature\n        commit id: \"A\"\n        commit id: \"B\"\n        checkout main\n        merge feature id: \"M\"\n        checkout release\n        cherry-pick id: \"M\" parent:\"B\"\n        ",
    );
    assert_eq!(ast.commits[4].tags, ["cherry-pick:M|parent:B"]);
    assert_eq!(ast.commits[4].branch, "release");
}

#[test]
fn supports_cherry_picking_merge_commits_with_a_tag() {
    let ast = ast(
        "gitGraph\n        commit id: \"ZERO\"\n        branch feature\n        branch release\n        checkout feature\n        commit id: \"A\"\n        commit id: \"B\"\n        checkout main\n        merge feature id: \"M\"\n        checkout release\n        cherry-pick id: \"M\" parent:\"ZERO\" tag: \"v1.0\"\n        ",
    );
    assert_eq!(ast.commits[4].tags, ["v1.0"]);
    assert_eq!(ast.commits[4].branch, "release");
}

#[test]
fn supports_cherry_picking_merge_commits_with_an_additional_commit() {
    let ast = ast(
        "gitGraph\n        commit id: \"ZERO\"\n        branch feature\n        branch release\n        checkout feature\n        commit id: \"A\"\n        commit id: \"B\"\n        checkout main\n        merge feature id: \"M\"\n        checkout release\n        commit id: \"C\"\n        cherry-pick id: \"M\" tag: \"v2.1:ZERO\" parent:\"ZERO\"\n        commit id: \"D\"\n        ",
    );
    assert_eq!(ast.commits[5].tags, ["v2.1:ZERO"]);
    assert_eq!(ast.commits[5].branch, "release");
}

#[test]
fn supports_cherry_picking_merge_commits_with_an_empty_tag() {
    let ast = ast(
        "gitGraph\n        commit id: \"ZERO\"\n        branch feature\n        branch release\n        checkout feature\n        commit id: \"A\"\n        commit id: \"B\"\n        checkout main\n        merge feature id: \"M\"\n        checkout release\n        commit id: \"C\"\n        cherry-pick id:\"M\" parent: \"ZERO\" tag:\"\"\n        commit id: \"D\"\n        cherry-pick id:\"M\" tag:\"\" parent: \"B\"\n        ",
    );
    assert_eq!(ast.commits[5].tags, Vec::<String>::new());
    assert_eq!(ast.commits[7].tags, Vec::<String>::new());
    assert_eq!(ast.commits[5].branch, "release");
}

#[test]
fn fails_cherry_picking_merge_commits_without_the_merge_parent() {
    let error = error(
        "gitGraph\n        commit id: \"ZERO\"\n        branch feature\n        branch release\n        checkout feature\n        commit id: \"A\"\n        commit id: \"B\"\n        checkout main\n        merge feature id: \"M\"\n        checkout release\n        commit id: \"C\"\n        cherry-pick id:\"M\"\n        ",
    );
    assert_eq!(
        error.message,
        "Incorrect usage of cherry-pick: If the source commit is a merge commit, an immediate parent commit must be specified."
    );
}

#[test]
fn fails_cherry_picking_when_the_parent_is_not_an_immediate_parent() {
    let error = error(
        "gitGraph\n        commit id: \"ZERO\"\n        branch feature\n        branch release\n        checkout feature\n        commit id: \"A\"\n        commit id: \"B\"\n        checkout main\n        merge feature id: \"M\"\n        checkout release\n        commit id: \"C\"\n        cherry-pick id:\"M\" parent: \"A\"\n        ",
    );
    assert_eq!(
        error.message,
        "Invalid operation: The specified parent commit is not an immediate parent of the cherry-picked commit."
    );
}

#[test]
fn throws_an_error_when_branching_the_existing_main_branch() {
    let error = error(
        "gitGraph\n        commit\n        branch testBranch\n        commit\n        branch main\n        commit\n        checkout main\n        merge testBranch\n        ",
    );
    assert_eq!(
        error.message,
        "Trying to create an existing branch. (Help: Either use a new name if you want create a new branch or try using \"checkout main\")"
    );
}

#[test]
fn throws_an_error_when_branching_an_existing_branch() {
    let error = error(
        "gitGraph\n        commit\n        branch testBranch\n        commit\n        branch testBranch\n        commit\n        checkout main\n        merge testBranch\n        ",
    );
    assert_eq!(
        error.message,
        "Trying to create an existing branch. (Help: Either use a new name if you want create a new branch or try using \"checkout testBranch\")"
    );
}

#[test]
fn throws_an_error_when_checking_out_an_unknown_branch() {
    let error = error(
        "gitGraph\n        commit\n        checkout testBranch\n        commit\n        branch testBranch\n        commit\n        checkout main\n        merge testBranch\n        ",
    );
    assert_eq!(
        error.message,
        "Trying to checkout branch which is not yet created. (Help try using \"branch testBranch\")"
    );
}

#[test]
fn throws_an_error_when_merging_and_the_current_branch_has_no_commits() {
    let error = error(
        "gitGraph\n        merge testBranch\n        commit\n        checkout testBranch\n        commit\n        branch testBranch\n        commit\n        checkout main\n        merge testBranch\n        ",
    );
    assert_eq!(
        error.message,
        "Incorrect usage of \"merge\". Current branch (main)has no commits"
    );
}

#[test]
fn throws_an_error_when_merging_an_unknown_branch() {
    let error = error(
        "gitGraph\n        commit\n        merge testBranch\n        commit\n        checkout testBranch\n        commit\n        branch testBranch\n        commit\n        checkout main\n        merge testBranch\n        ",
    );
    assert_eq!(
        error.message,
        "Incorrect usage of \"merge\". Branch to be merged (testBranch) does not exist"
    );
}

#[test]
fn throws_an_error_when_trying_to_merge_a_branch_to_itself() {
    let error = error(
        "gitGraph\n        commit\n        branch testBranch\n        merge testBranch\n        ",
    );
    assert_eq!(
        error.message,
        "Incorrect usage of \"merge\". Cannot merge a branch to itself"
    );
}

#[test]
fn throws_an_error_when_using_an_existing_id_as_a_merge_id() {
    let error = error(
        "gitGraph\n        commit id: \"1-111\"\n        branch testBranch\n        commit id: \"2-222\"\n        commit id: \"3-333\"\n        checkout main\n        merge testBranch id: \"1-111\"\n        ",
    );
    assert_eq!(
        error.message,
        "Incorrect usage of \"merge\". Commit with id:1-111 already exists, use different custom id"
    );
}

#[test]
fn throws_an_error_when_merging_branches_with_the_same_head() {
    let error = error(
        "gitGraph\n        commit\n        branch testBranch\n        checkout main\n        merge testBranch\n        ",
    );
    assert_eq!(
        error.message,
        "Incorrect usage of \"merge\". Both branches have same head"
    );
}

#[test]
fn throws_an_error_when_merging_a_branch_with_no_commits() {
    let error = error(
        "gitGraph\n        branch test1\n\n        checkout main\n        commit\n        merge test1\n        ",
    );
    assert_eq!(
        error.message,
        "Incorrect usage of \"merge\". Branch to be merged (test1) has no commits"
    );
}

#[test]
fn handles_a_title_and_a_description() {
    let ast = ast(
        "gitGraph:\n          accTitle: This is a title\n          accDescr: This is a description\n        commit\n        ",
    );
    assert_eq!(ast.acc_title, "This is a title");
    assert_eq!(ast.acc_description, "This is a description");
}

#[test]
fn handles_a_title_and_a_multiline_description() {
    let ast = ast(
        "gitGraph:\n          accTitle: This is a title\n          accDescr {\n            This is a description\n            using multiple lines\n          }\n        commit\n        ",
    );
    assert_eq!(ast.acc_title, "This is a title");
    assert_eq!(
        ast.acc_description,
        "This is a description\nusing multiple lines"
    );
}

#[test]
fn works_with_custom_commit_ids_and_branch_names_named_like_object_properties() {
    for prop in ["__proto__", "constructor"] {
        let ast = ast(&format!(
            "gitGraph\n        commit id:\"{prop}\"\n        branch {prop}\n        checkout {prop}\n        commit\n        checkout main\n        merge {prop}\n        "
        ));
        assert_eq!(ast.commits.len(), 3, "{prop}");
        assert_eq!(ast.commits[0].id, prop);
        assert_eq!(ast.current_branch, "main");
        assert_eq!(ast.branches.len(), 2);
        assert_eq!(ast.branches_as_obj_array()[1], prop);
    }
}

#[test]
fn keeps_the_first_position_and_the_last_value_of_a_reused_commit_id() {
    // Upstream also logs "Commit ID working on MDR already exists".
    let ast = ast(
        "gitGraph\n     commit id:\"initial commit\"\n     commit id:\"work on first release\"\n     commit id:\"design freeze from here\"\n     branch v1-rc\n     checkout v1-rc\n     commit id:\"bugfix 1\"\n     commit id:\"bigfix 2\" tag:\"v1.0.1\"\n     branch FORK-v1.0-MDR\n     checkout FORK-v1.0-MDR\n     commit id:\"working on MDR\"\n     checkout v1-rc\n     commit id:\"minor design changes for MDR\" tag:\"v1.0.2\"\n     checkout FORK-v1.0-MDR\n     merge v1-rc\n     checkout main\n     commit id:\"new feature for v1.1…\"\n     checkout FORK-v1.0-MDR\n     commit id:\"working on MDR\"\n     commit id:\"finishing MDR\"\n     branch v1.0-MDR\n     checkout v1.0-MDR\n     commit id:\"brush up release\" tag:\"v1.0.2-MDR\"\n     checkout v1-rc\n     commit id:\"bugfix without MDR\"\n     checkout main\n     commit id:\"work on v1.1\"\n  ",
    );
    assert_eq!(ast.commits.len(), 13);
    let duplicate = commit(&ast, "working on MDR");
    assert_eq!(ast.commits[5].id, "working on MDR");
    assert_eq!(duplicate.branch, "FORK-v1.0-MDR");
    assert_eq!(
        ast.commits
            .iter()
            .filter(|commit| commit.id == "working on MDR")
            .count(),
        1
    );
    // getCommitsArray sorts by seq, so the reused id lands on its later sequence.
    assert_eq!(ast.commits_by_seq().len(), 13);
    assert_eq!(ast.commits_by_seq()[8].id, "working on MDR");
    assert_eq!(ast.commits_by_seq()[12].id, "work on v1.1");
}

// Behavior of the database and lexer the specs above do not pin down.

#[test]
fn an_auto_generated_id_has_the_upstream_shape() {
    let ast = ast("gitGraph\n  commit\n");
    assert_eq!(ast.commits[0].id, "0-0000000");
}

#[test]
fn the_header_accepts_all_four_forms() {
    for body in ["gitGraph", "gitGraph:", "gitGraph :", "gitGraph TB:"] {
        let ast = ast(body);
        assert_eq!(ast.commits.len(), 0, "{body:?}");
    }
    assert_eq!(ast("gitGraph TB:").direction, Direction::Tb);
    assert_eq!(ast("gitGraph BT:").direction, Direction::Bt);
}

#[test]
fn a_branch_name_may_start_with_a_statement_keyword() {
    // A keyword only matches when no `REFERENCE` matches a longer run at the same offset.
    let ast = ast("gitGraph\n  commit\n  branch checkout02\n  branch merge-test\n");
    assert_eq!(ast.current_branch, "merge-test");
    assert!(ast.branch("checkout02").is_some());
}

#[test]
fn a_stray_character_reports_its_span() {
    let error = error("gitGraph\n  commit\n  @");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (3, 3));
    assert!(error.message.contains('@'));
}

#[test]
fn a_database_error_reports_the_statement_line() {
    let error = error("gitGraph\n  commit\n  merge nope\n");
    let span = error.span.expect("the error carries a span");
    assert_eq!(span.line, 3);
    assert_eq!(span.column, 3);
}

#[test]
fn the_header_and_a_statement_may_share_a_line() {
    // The grammar has no `EOL` after the header.
    let parsed = ast("gitGraph: commit id:\"one\"\n");
    assert_eq!(parsed.commits.len(), 1);
    assert_eq!(parsed.commits[0].id, "one");
    assert_eq!(ast("gitGraph TB: commit\n").commits.len(), 1);
}

#[test]
fn a_syntax_error_is_reported_before_a_database_error() {
    // The whole document is parsed before the database is populated, as `gitGraphParser.ts` does.
    let error = error("gitGraph\n  merge nope\n  @\n");
    assert_eq!(error.message, "unexpected character '@'");
    assert_eq!(error.span.expect("a span").line, 3);
}

#[test]
fn a_single_space_or_tab_in_a_title_is_kept() {
    // `common/valueConverter.ts` collapses runs of two or more spaces or tabs only.
    assert_eq!(
        ast("gitGraph\n  title a\tb\n  commit\n").diagram_title,
        "a\tb"
    );
    assert_eq!(
        ast("gitGraph\n  title a\t b\n  commit\n").diagram_title,
        "a b"
    );
}

#[test]
fn parses_crlf_line_endings() {
    let ast = ast("gitGraph\r\n  commit id:\"a\"\r\n  branch b\r\n");
    assert_eq!(ast.commits.len(), 1);
    assert_eq!(ast.current_branch, "b");
    assert_eq!(branch_names(&ast), ["main", "b"]);
}
