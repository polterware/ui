//! Tests ported from upstream's usecase specs: `usecase.parser.spec.ts`, `usecase.spec.ts`,
//! `usecaseDb.spec.ts`, `parser/usecase.parser.spec.ts`, `parser/usecase.lexer.spec.ts`, and
//! `parser/usecaseJson.spec.ts`.
//!
//! Not ported, and why:
//! - `usecaseRenderer.spec.ts` and `usecaseRoleColors.spec.ts`: rendering and CSS.
//! - `usecase-colorIndex.spec.ts` and `usecase.spec.ts`'s `getData` / `P4 LayoutData
//!   contract` describes: layout projection and configuration.
//! - `usecaseDb.spec.ts`'s commit/clear cases: this crate has no mutable singleton database;
//!   `parse` returns a complete [`super::Ast`] or nothing. The defaults it creates are
//!   checked here instead.
//! - `parser/usecase.parser.spec.ts`'s "defines the stable canonical CST rule names": it
//!   inspects Chevrotain's serialized grammar, which a recursive-descent parser does not
//!   have; every accepted and rejected source in that spec is still ported below.
//! - Config assertions (`getConfig`, `DEFAULT_USECASE_CONFIG`, layout fields) are absent
//!   because this crate owns no configuration.

use serde_json::json;

use super::parser::parse_ordered_json_object;
use super::*;
use crate::error::ParseError;

fn parsed(source: &str) -> Ast {
    parse(source).unwrap_or_else(|error| panic!("expected a parse of {source:?}, got {error}"))
}

fn rejected(source: &str) -> ParseError {
    match parse(source) {
        Ok(_) => panic!("expected {source:?} to be rejected"),
        Err(error) => error,
    }
}

fn accepts(source: &str) {
    super::parser::parse_grammar(source)
        .unwrap_or_else(|error| panic!("expected the grammar to accept {source:?}, got {error}"));
}

fn rejects(source: &str) {
    if super::parser::parse_grammar(source).is_ok() {
        panic!("expected the grammar to reject {source:?}");
    }
}

fn sorted<'a, T>(items: impl Iterator<Item = &'a T>, id: impl Fn(&T) -> &str) -> Vec<&'a T>
where
    T: 'a,
{
    let mut values: Vec<&T> = items.collect();
    values.sort_by(|left, right| id(left).cmp(id(right)));
    values
}

// Upstream's `sourceLocation`: 1-based line, and the column is the byte distance from the
// previous line break.
fn source_location(source: &str, start: usize, end: usize) -> String {
    let mut line = 1;
    let mut line_start: isize = -1;
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < start {
        if bytes[index] == b'\r' {
            line += 1;
            line_start = index as isize;
            index += 1;
            if index < start && bytes[index] == b'\n' {
                line_start = index as isize;
                index += 1;
            }
        } else if bytes[index] == b'\n' {
            line += 1;
            line_start = index as isize;
            index += 1;
        } else {
            index += 1;
        }
    }
    format!(
        "line {line}, column {} [{start},{end})",
        start as isize - line_start
    )
}

fn occurrence_location(
    source: &str,
    text: &str,
    occurrence: usize,
    trim_start: usize,
    trim_end: usize,
) -> String {
    let mut start = 0;
    let mut found = None;
    for _ in 0..occurrence {
        start = source[start..]
            .find(text)
            .map(|index| index + start)
            .unwrap_or_else(|| panic!("{text:?} not found in {source:?}"));
        found = Some(start);
        start += 1;
    }
    let start = found.expect("occurrence is at least 1");
    source_location(source, start + trim_start, start + text.len() - trim_end)
}

// ----- parser/usecase.parser.spec.ts -------------------------------------------------

#[test]
fn accepts_the_header_at_eof() {
    accepts("usecase-beta");
}

#[test]
fn accepts_line_feed_statements() {
    accepts("usecase-beta\nactor User");
}

#[test]
fn accepts_carriage_return_and_line_feed_statements() {
    accepts("usecase-beta\r\nactor User\r\n");
}

#[test]
fn accepts_carriage_return_statements() {
    accepts("usecase-beta\ractor User\r");
}

#[test]
fn accepts_blank_and_comment_lines() {
    accepts("usecase-beta\n\n  %% retained\nLogin");
}

#[test]
fn accepts_canonical_actors_use_cases_metadata_stereotypes_and_class_suffixes() {
    accepts(
        "usecase-beta
actor Admin(\"Main Administrator\")@{ type: hollow, business: true } <<Human>>:::external
actor \"System Administrator\", \"`**Automation**`\"
Login(Sign in)@{ business: false } <<Primary>>:::critical
Report[Generate report]
\"Reset password\"
\"`**Markdown** use case`\"",
    );
}

#[test]
fn accepts_multiline_metadata_with_newline_and_comma_separators_and_a_trailing_comma() {
    accepts(
        "usecase-beta
User@{
  business: true
  type: normal,
}",
    );
}

#[test]
fn accepts_canonical_relations() {
    for relation in [
        "A --> B",
        "A <-- B",
        "A -- B",
        "A --o B",
        "A o-- B",
        "A --x B",
        "A x-- B",
        "A -- \"starts session\" --> B",
        "A <-- \"reverse label\" -- B",
        "A -- label -- B",
        "A -- label --o B",
        "A o-- label -- B",
        "A -- label --x B",
        "A x-- label -- B",
        "A ..> : include B",
        "A ..> : INCLUDE B",
        "A ..> : ExTeNd B",
        "A --|> B",
        "A ---> B",
        "A <---- B",
        "A ---- B",
        "A -- longer ----> B",
        "A login@--> B",
        "A dependency@..> : include B",
    ] {
        accepts(&format!("usecase-beta\n{relation}"));
    }
}

#[test]
fn accepts_restricted_system_boundary_declarations_blanks_and_comments() {
    accepts(
        "usecase-beta
systemBoundary \"Authentication System\":::system
  %% boundary comment
  actor User, Admin(\"Administrator\")

  Login(\"Sign in\"):::critical
  Report[Generate report]
end
User --> Login",
    );
}

#[test]
fn accepts_explicit_boundary_ids_with_labels_inline_metadata_and_class_suffixes() {
    accepts(
        "usecase-beta
systemBoundary sb1[\"Payment service\"]@{ type: package }:::system
  Authorize(\"Authorize payment\")
end
systemBoundary sb2(Billing)@{
  type: rect
}:::billing
  Invoice[Create invoice]
end
systemBoundary sb3(\"`**Support**`\")
  Refund[Issue refund]
end",
    );
}

#[test]
fn accepts_metadata_assignments_notes_json_direction_classes_and_styles() {
    accepts(
        "usecase-beta
direction LR
Auth@{ type: package }
note for Login \"`Requires an **active session**`\"
json Payload@{
  \"fruit\": \"Apple\",
  \"nested\": { \"brace\": \"}\", \"items\": [1, 2] }
}:::data
classDef external,critical fill:#fff,stroke-width:3px
class Login,Payload external,critical
style Login fill:#fee,stroke-dasharray:5\\,5",
    );
}

#[test]
fn accepts_single_line_and_multiline_accessibility_statements() {
    accepts(
        "usecase-beta
accTitle: Authentication use cases
accDescr: Actors and authentication flows",
    );
    accepts(
        "usecase-beta
accDescr {
  Actors authenticate,
  reset credentials, and sign out.
}",
    );
}

#[test]
fn rejects_the_grammar_cases_upstream_rejects() {
    for source in [
        "Usecase\nA",
        "usecase-beta\nactor A actor B",
        "usecase actor A",
        "usecase-beta\nactor A, B --> C",
        "usecase-beta\nunknown command",
        "usecase-beta\n// not a comment",
        "usecase-beta\n# not a comment",
        "usecase-beta\nA; B",
        "usecase-beta\n:User:",
        "usecase-beta\nactor User as U",
        "usecase-beta\nusecase \"Login\" as Login",
        "usecase-beta\nnewpage",
        "usecase-beta\npackage \"Auth\" {",
        "usecase-beta\nrectangle \"Auth\" {",
        "usecase-beta\nskinparam actorStyle awesome",
        "usecase-beta\nallowmixing",
        "usecase-beta\n== Section ==",
        "usecase-beta\nA \\n B",
        "usecase-beta\nA -left-> B",
        "usecase-beta\nnote left of Login \"text\"",
        "usecase-beta\nnote for Login as N \"text\"",
        "usecase-beta\nnote for Login,Report \"text\"",
        "usecase-beta\nsystemBoundary A\nsystemBoundary B\nend\nend",
        "usecase-beta\nsystemBoundary A\nUser --> Login\nend",
        "usecase-beta\nsystemBoundary A\nnote for User \"text\"\nend",
        "usecase-beta\nsystemBoundary A\njson Payload@{}\nend",
        "usecase-beta\nA ---o B",
        "usecase-beta\nA o--- B",
        "usecase-beta\nA ---x B",
        "usecase-beta\nA ---|> B",
        "usecase-beta\nA ...> : include B",
        "usecase-beta\nA <<>>",
        "usecase-beta\nA <<first\nsecond>>",
        "usecase-beta\nA <<primary",
        "usecase-beta\njson Payload@{\"nested\": {",
        "usecase-beta\nA(\"`unfinished\")",
        "usecase-beta\nstyle A fill:red;stroke:blue",
        "usecase-beta\nsystemBoundary A:::system@{ type: package }\nend",
        "usecase-beta\nsystemBoundary A <<S>>\nend",
        "usecase-beta\nsystemBoundary [Payment service]\nend",
    ] {
        rejects(source);
    }
}

// ----- parser/usecase.lexer.spec.ts --------------------------------------------------

fn token_images(input: &str) -> Vec<(String, String)> {
    let tokens = super::lexer::tokenize(input)
        .unwrap_or_else(|error| panic!("expected {input:?} to lex, got {error}"));
    tokens
        .iter()
        .filter(|token| token.kind != super::lexer::TokenKind::Eof)
        .map(|token| (token.kind.name().to_string(), token.image.to_string()))
        .collect()
}

fn token_kinds(input: &str) -> Vec<String> {
    token_images(input)
        .into_iter()
        .map(|(kind, _)| kind)
        .collect()
}

#[test]
fn lexer_uses_longest_operator_and_delimiter_precedence() {
    assert_eq!(
        token_images("--|> ..> <<Human>> ::: ---> <--- --o o-- --x x-- --"),
        vec![
            ("GENERALIZATION".into(), "--|>".into()),
            ("DEPENDENCY_ARROW".into(), "..>".into()),
            ("STEREOTYPE_START".into(), "<<".into()),
            ("STEREOTYPE_TEXT".into(), "Human".into()),
            ("STEREOTYPE_END".into(), ">>".into()),
            ("CLASS_SEPARATOR".into(), ":::".into()),
            ("FORWARD_SOLID".into(), "--->".into()),
            ("BACKWARD_SOLID".into(), "<---".into()),
            ("FORWARD_CIRCLE".into(), "--o".into()),
            ("BACKWARD_CIRCLE".into(), "o--".into()),
            ("FORWARD_CROSS".into(), "--x".into()),
            ("BACKWARD_CROSS".into(), "x--".into()),
            ("MARKERLESS_SOLID".into(), "--".into()),
        ]
    );
}

#[test]
fn lexer_keeps_exact_keywords_out_of_longer_identifiers_and_matches_semantic_words_case_insensitively()
 {
    assert_eq!(
        token_images("actor actorName include INCLUDE Include included extend EXTEND extended"),
        vec![
            ("ACTOR".into(), "actor".into()),
            ("IDENTIFIER".into(), "actorName".into()),
            ("INCLUDE".into(), "include".into()),
            ("INCLUDE".into(), "INCLUDE".into()),
            ("INCLUDE".into(), "Include".into()),
            ("IDENTIFIER".into(), "included".into()),
            ("EXTEND".into(), "extend".into()),
            ("EXTEND".into(), "EXTEND".into()),
            ("IDENTIFIER".into(), "extended".into()),
        ]
    );
}

#[test]
fn lexer_emits_one_token_per_physical_newline() {
    for newline in ["\n", "\r\n", "\r"] {
        let source = format!("{newline}{newline}");
        let tokens = super::lexer::tokenize(&source).expect("lexes");
        assert_eq!(tokens.len(), 3, "two newlines and EOF for {newline:?}");
        assert!(
            tokens[..2]
                .iter()
                .all(|token| token.kind == super::lexer::TokenKind::Newline)
        );
    }
}

#[test]
fn lexer_preserves_multiline_markdown_and_treats_comment_lookalikes_inside_it_as_data() {
    assert_eq!(
        token_images("\"`first\n%% still markdown\nlast`\""),
        vec![(
            "MARKDOWN_STRING".into(),
            "\"`first\n%% still markdown\nlast`\"".into()
        )]
    );
}

#[test]
fn lexer_emits_indented_whole_line_comments_but_not_percent_pairs_after_another_token() {
    assert_eq!(
        token_images("  %% comment\n"),
        vec![
            ("COMMENT".into(), "%% comment".into()),
            ("NEWLINE".into(), "\n".into()),
        ]
    );
    assert_eq!(
        token_kinds("A %% data"),
        vec!["IDENTIFIER", "PERCENT", "PERCENT", "IDENTIFIER"]
    );
}

#[test]
fn lexer_balances_nested_json_while_ignoring_escaped_quotes_and_braces_in_strings() {
    let source =
        "json Payload@{\n  \"text\": \"} \\\" {\",\n  \"nested\": {\"items\": [1, 2]}\n}:::data";
    let tokens = super::lexer::tokenize(source).expect("lexes");
    let kinds: Vec<&str> = tokens
        .iter()
        .map(|token| token.kind.name())
        .filter(|kind| *kind != "EOF")
        .collect();
    assert_eq!(
        kinds,
        vec![
            "JSON_DECLARATION_START",
            "JSON_OBJECT_LITERAL",
            "CLASS_SEPARATOR",
            "IDENTIFIER",
        ]
    );
    assert_eq!(tokens[1].span.line, 1);
    assert_eq!(tokens[1].span.column, 14);
}

#[test]
fn lexer_consumes_an_unclosed_json_object_once_from_its_opening_brace() {
    let source = format!("json Payload@{{{}}}", "{".repeat(32_000));
    let tokens = super::lexer::tokenize(&source).expect("lexes");
    let last = &tokens[tokens.len() - 2];
    assert_eq!(
        last.kind,
        super::lexer::TokenKind::UnclosedJsonObjectLiteral
    );
    assert_eq!(last.span.line, 1);
    assert_eq!(last.span.column, 14);
    assert_eq!(last.span.start, 13);
}

#[test]
fn lexer_consumes_unclosed_markdown_and_stereotypes_once() {
    let markdown = "\"`x".repeat(32_000);
    let tokens = super::lexer::tokenize(&markdown).expect("lexes");
    assert_eq!(tokens.len(), 2);
    assert_eq!(
        tokens[0].kind,
        super::lexer::TokenKind::UnclosedMarkdownString
    );

    let stereotype = format!("<<{}", "x".repeat(128_000));
    assert_eq!(
        token_kinds(&stereotype),
        vec!["STEREOTYPE_START", "UNCLOSED_STEREOTYPE_TEXT"]
    );
}

#[test]
fn lexer_preserves_strict_strings_colors_css_units_and_escaped_commas() {
    assert_eq!(
        token_images("\"%%\" 'plain' #f96 4px 50% red\\,blue"),
        vec![
            ("PLAIN_STRING".into(), "\"%%\"".into()),
            ("PLAIN_STRING".into(), "'plain'".into()),
            ("HASH_COLOR".into(), "#f96".into()),
            ("IDENTIFIER".into(), "4px".into()),
            ("IDENTIFIER".into(), "50".into()),
            ("PERCENT".into(), "%".into()),
            ("IDENTIFIER".into(), "red".into()),
            ("CSS_ESCAPED_COMMA".into(), "\\,".into()),
            ("IDENTIFIER".into(), "blue".into()),
        ]
    );
}

#[test]
fn lexer_lexes_leftover_punctuation_and_non_ascii_characters_as_label_text() {
    assert_eq!(
        token_images("*bold* ; < > ` \\ Ärlig 日本語"),
        vec![
            ("CSS_PUNCTUATION".into(), "*".into()),
            ("IDENTIFIER".into(), "bold".into()),
            ("CSS_PUNCTUATION".into(), "*".into()),
            ("LABEL_PUNCTUATION".into(), ";".into()),
            ("LABEL_PUNCTUATION".into(), "<".into()),
            ("LABEL_PUNCTUATION".into(), ">".into()),
            ("LABEL_PUNCTUATION".into(), "`".into()),
            ("LABEL_PUNCTUATION".into(), "\\".into()),
            ("LABEL_SYMBOL".into(), "Ä".into()),
            ("IDENTIFIER".into(), "rlig".into()),
            ("LABEL_SYMBOL".into(), "日本語".into()),
        ]
    );
    assert_eq!(
        token_images("<-- --> ..> ::: @{ \\,"),
        vec![
            ("BACKWARD_SOLID".into(), "<--".into()),
            ("FORWARD_SOLID".into(), "-->".into()),
            ("DEPENDENCY_ARROW".into(), "..>".into()),
            ("CLASS_SEPARATOR".into(), ":::".into()),
            ("METADATA_START".into(), "@{".into()),
            ("CSS_ESCAPED_COMMA".into(), "\\,".into()),
        ]
    );
}

#[test]
fn lexer_lexes_digit_leading_ids_as_identifiers_while_decimals_stay_numbers() {
    assert_eq!(
        token_images("1 1mg a1 _x"),
        vec![
            ("IDENTIFIER".into(), "1".into()),
            ("IDENTIFIER".into(), "1mg".into()),
            ("IDENTIFIER".into(), "a1".into()),
            ("IDENTIFIER".into(), "_x".into()),
        ]
    );
    assert_eq!(
        token_images("1.5 1.5px .5"),
        vec![
            ("NUMBER".into(), "1.5".into()),
            ("NUMBER".into(), "1.5px".into()),
            ("NUMBER".into(), ".5".into()),
        ]
    );
}

// ----- parser/usecaseJson.spec.ts ----------------------------------------------------

fn ordered_json(
    text: &str,
    line: u32,
    column: u32,
) -> (
    serde_json::Value,
    serde_json::Map<String, serde_json::Value>,
) {
    parse_ordered_json_object(text, line, column, 0)
        .unwrap_or_else(|error| panic!("expected ordered JSON, got {error}"))
}

#[test]
fn json_collects_escaped_json_pointer_order_for_nested_objects_and_objects_inside_arrays() {
    let (value, order) = ordered_json(
        "{
        \"a/b\": {
          \"items\": [
            { \"~name\": \"first\", \"id\": 1 },
            { \"id\": 2, \"details\": { \"enabled\": true } }
          ]
        },
        \"last\": null
      }",
        1,
        1,
    );

    assert_eq!(
        value,
        json!({
            "a/b": {
                "items": [
                    { "~name": "first", "id": 1 },
                    { "id": 2, "details": { "enabled": true } },
                ],
            },
            "last": null,
        })
    );
    assert_eq!(
        order,
        json!({
            "": ["a/b", "last"],
            "/a~1b": ["items"],
            "/a~1b/items/0": ["~name", "id"],
            "/a~1b/items/1": ["id", "details"],
            "/a~1b/items/1/details": ["enabled"],
        })
        .as_object()
        .expect("object")
        .clone()
    );
}

#[test]
fn json_preserves_source_order_for_integer_like_property_names() {
    let (_, order) = ordered_json(
        "{\"10\":\"ten\",\"2\":\"two\",\"1\":\"one\",\"plain\":true}",
        1,
        1,
    );
    assert_eq!(order[""], json!(["10", "2", "1", "plain"]));
}

#[test]
fn json_keeps_a_duplicate_key_first_in_order_while_retaining_its_last_value_and_nested_order() {
    let (value, order) = ordered_json(
        "{
        \"entry\": { \"discarded\": { \"old\": true } },
        \"tail\": 0,
        \"entry\": { \"second\": 2, \"first\": 1, \"\\u0073econd\": 3 }
      }",
        1,
        1,
    );
    assert_eq!(
        value,
        json!({ "entry": { "second": 3, "first": 1 }, "tail": 0 })
    );
    assert_eq!(
        order,
        json!({
            "": ["entry", "tail"],
            "/entry": ["second", "first"],
        })
        .as_object()
        .expect("object")
        .clone()
    );
    assert!(!order.contains_key("/entry/discarded"));
}

#[test]
fn json_records_empty_objects_and_accepts_empty_arrays_and_strings_as_property_values() {
    let (value, order) = ordered_json(
        "{\"emptyObject\":{},\"emptyArray\":[],\"emptyString\":\"\"}",
        4,
        9,
    );
    assert_eq!(
        value,
        json!({ "emptyObject": {}, "emptyArray": [], "emptyString": "" })
    );
    assert_eq!(
        order,
        json!({
            "": ["emptyObject", "emptyArray", "emptyString"],
            "/emptyObject": [],
        })
        .as_object()
        .expect("object")
        .clone()
    );
}

#[test]
fn json_ignores_braces_brackets_and_escaped_quotes_inside_string_values() {
    let (value, order) = ordered_json(
        r#"{"text":"a } and { plus [brackets] and \"quotes\"","nested":{"value":"{\"key\": 1}"}}"#,
        1,
        1,
    );
    assert_eq!(
        value,
        json!({
            "text": "a } and { plus [brackets] and \"quotes\"",
            "nested": { "value": "{\"key\": 1}" },
        })
    );
    assert_eq!(
        order,
        json!({
            "": ["text", "nested"],
            "/nested": ["value"],
        })
        .as_object()
        .expect("object")
        .clone()
    );
}

#[test]
fn json_rejects_scalar_null_and_array_roots_with_the_start_location() {
    for text in ["true", "null", "[{\"nested\":true}]"] {
        let error = parse_ordered_json_object(text, 12, 5, 0).expect_err("rejects");
        assert_eq!(
            error.span.map(|span| (span.line, span.column)),
            Some((12, 5))
        );
        assert!(
            error
                .message
                .contains("JSON value must have an object root")
        );
    }
}

#[test]
fn json_maps_a_malformed_offset_across_physical_lines_into_diagram_coordinates() {
    let error = parse_ordered_json_object(
        "{
  \"valid\": true,
  \"invalid\":
}",
        20,
        7,
        0,
    )
    .expect_err("rejects");
    assert_eq!(
        error.span.map(|span| (span.line, span.column)),
        Some((23, 1))
    );
    assert!(error.message.contains("Invalid JSON"));
}

#[test]
fn json_adds_the_token_start_column_to_malformed_json_on_its_first_line() {
    let error = parse_ordered_json_object("{\"key\":,}", 8, 12, 0).expect_err("rejects");
    assert_eq!(
        error.span.map(|span| (span.line, span.column)),
        Some((8, 19))
    );
}

#[test]
fn json_maps_crlf_as_one_physical_line_break() {
    let error =
        parse_ordered_json_object("{\r\n  \"key\": true,\r\n}", 30, 4, 0).expect_err("rejects");
    assert_eq!(
        error.span.map(|span| (span.line, span.column)),
        Some((32, 1))
    );
}

// ----- usecase.parser.spec.ts (publication and normalized AST) ------------------------

#[derive(Clone, Debug, PartialEq)]
struct Published {
    actors: Vec<Actor>,
    use_cases: Vec<UseCase>,
    boundaries: Vec<SystemBoundary>,
    relationships: Vec<Relationship>,
    notes: Vec<UsecaseNote>,
    json_nodes: Vec<UsecaseJsonNode>,
    class_defs: Vec<ClassDef>,
    direction: Direction,
}

fn published(ast: &Ast) -> Published {
    Published {
        actors: sorted(ast.actors.iter(), |actor| &actor.id)
            .into_iter()
            .cloned()
            .collect(),
        use_cases: sorted(ast.use_cases.iter(), |use_case| &use_case.id)
            .into_iter()
            .cloned()
            .collect(),
        boundaries: sorted(ast.system_boundaries.iter(), |boundary| &boundary.id)
            .into_iter()
            .cloned()
            .collect(),
        relationships: ast.relationships.clone(),
        notes: sorted(ast.notes.iter(), |note| &note.id)
            .into_iter()
            .cloned()
            .collect(),
        json_nodes: sorted(ast.json_nodes.iter(), |node| &node.id)
            .into_iter()
            .cloned()
            .collect(),
        class_defs: sorted(ast.class_defs.iter(), |def| &def.id)
            .into_iter()
            .cloned()
            .collect(),
        direction: ast.direction,
    }
}

fn assert_empty_publication(ast: &Ast) {
    assert!(ast.actors.is_empty());
    assert!(ast.use_cases.is_empty());
    assert!(ast.system_boundaries.is_empty());
    assert!(ast.relationships.is_empty());
    assert!(ast.notes.is_empty());
    assert!(ast.json_nodes.is_empty());
    assert!(ast.class_defs.is_empty());
    assert_eq!(ast.direction, Direction::Lr);
    assert_eq!(ast.relationship_counter, 0);
    assert_eq!(ast.note_counter, 0);
    assert!(ast.acc_title.is_empty());
    assert!(ast.acc_description.is_empty());
    assert_eq!(ast.graph.version, 1);
    assert_eq!(ast.graph.diagram_type, "usecase");
    assert!(ast.graph.nodes.is_empty());
    assert!(ast.graph.edges.is_empty());
    assert!(ast.graph.groups.is_empty());
    assert!(ast.graph.class_defs.is_empty());
    assert!(ast.graph.statements.is_empty());
    assert_eq!(ast.graph.acc_title, None);
    assert_eq!(ast.graph.acc_descr, None);
}

fn assert_exact_error(source: &str, expected_message: String) {
    let error = rejected(source);
    assert_eq!(error.message, expected_message);
}

#[test]
fn accepts_the_header_at_eof_and_publishes_an_empty_ast() {
    let ast = parsed("usecase-beta");
    assert_empty_publication(&ast);
    assert_eq!(ast.graph.source, "usecase-beta");
    assert_eq!(ast.graph.header.keyword, "usecase");
    assert_eq!(ast.graph.header.direction, Direction::Lr);
    assert_eq!(ast.graph.header.span.start, 0);
    assert_eq!(ast.graph.header.span.end, 12);
}

#[test]
fn publishes_accessibility_text_through_the_database_and_normalized_ast_transactionally() {
    let source = "usecase-beta
accTitle: Authentication use cases
accDescr {
  First line
  Second line
}
actor User";
    let ast = parsed(source);

    assert_eq!(ast.acc_title, "Authentication use cases");
    assert_eq!(ast.acc_description, "First line\nSecond line");
    assert_eq!(
        ast.graph.acc_title.as_deref(),
        Some("Authentication use cases")
    );
    assert_eq!(
        ast.graph.acc_descr.as_deref(),
        Some("First line\n  Second line")
    );
    let title_start = source.find("accTitle").expect("accTitle");
    let descr_start = source.find("accDescr").expect("accDescr");
    let statements: Vec<(StatementKind, usize, usize)> = ast
        .graph
        .statements
        .iter()
        .map(|statement| (statement.kind, statement.span.start, statement.span.end))
        .collect();
    assert_eq!(
        statements,
        vec![
            (
                StatementKind::AccTitle,
                title_start,
                title_start + "accTitle: Authentication use cases".len()
            ),
            (
                StatementKind::AccDescr,
                descr_start,
                source.find('}').expect("closing brace") + 1
            ),
            (
                StatementKind::Node,
                source.find("actor User").expect("actor"),
                source.len()
            ),
        ]
    );

    rejected(
        "usecase-beta
accTitle: Draft title
accDescr: Draft description
note for Missing \"invalid\"",
    );
}

#[test]
fn accepts_a_final_statement_without_a_newline_and_keeps_exclusive_source_spans() {
    let source = "usecase-beta\nactor User";
    let ast = parsed(source);
    let actor = ast.actor("User").expect("actor");
    assert_eq!(actor.id, "User");
    assert_eq!(actor.label, "User");
    assert_eq!(ast.graph.source, source);
    let statements = &ast.graph.statements;
    assert_eq!(statements.len(), 1);
    assert_eq!(statements[0].kind, StatementKind::Node);
    assert_eq!((statements[0].span.start, statements[0].span.end), (13, 23));
    let node = &statements[0].nodes[0];
    assert_eq!(node.id, "User");
    assert_eq!((node.span.start, node.span.end), (19, 23));
    assert_eq!((node.id_span.start, node.id_span.end), (19, 23));
    assert_eq!(
        node.label_span.map(|span| (span.start, span.end)),
        Some((19, 23))
    );
    assert!(node.defines);
}

#[test]
fn isolates_success_clear_failure_and_a_second_success_across_reuse() {
    let ast = parsed(
        "usecase-beta
actor First
First --> Login
note for Login \"First note\"",
    );
    assert_eq!(
        ast.actors
            .iter()
            .map(|actor| actor.id.as_str())
            .collect::<Vec<_>>(),
        vec!["First"]
    );
    assert_eq!(
        ast.use_cases
            .iter()
            .map(|use_case| use_case.id.as_str())
            .collect::<Vec<_>>(),
        vec!["Login"]
    );
    assert_eq!(
        ast.relationships
            .iter()
            .map(|relationship| relationship.id.as_str())
            .collect::<Vec<_>>(),
        vec!["edge-0"]
    );
    assert_eq!(
        ast.notes
            .iter()
            .map(|note| note.id.as_str())
            .collect::<Vec<_>>(),
        vec!["note-0"]
    );

    rejected(
        "usecase-beta
actor Draft
Draft --> Pending
note for Missing \"invalid\"",
    );

    let ast = parsed(
        "usecase-beta
actor Second
Second --> Done
note for Done \"Second note\"",
    );
    assert_eq!(
        ast.actors
            .iter()
            .map(|actor| actor.id.as_str())
            .collect::<Vec<_>>(),
        vec!["Second"]
    );
    assert_eq!(
        ast.use_cases
            .iter()
            .map(|use_case| use_case.id.as_str())
            .collect::<Vec<_>>(),
        vec!["Done"]
    );
    assert_eq!(
        ast.relationships
            .iter()
            .map(|relationship| relationship.id.as_str())
            .collect::<Vec<_>>(),
        vec!["edge-0"]
    );
    assert!(ast.graph.source.contains("Second"));
    assert!(!ast.graph.source.contains("First"));
    assert!(!ast.graph.source.contains("Draft"));
}

#[test]
fn resolves_forward_refinements_independently_of_declaration_order() {
    for (declaration, relation, expected_actor, expected_use_case, expected_shape) in [
        (
            "actor User",
            "User --> Login",
            Some("User"),
            "Login",
            UseCaseShape::Ellipse,
        ),
        (
            "Login[Sign in]",
            "User --> Login",
            None,
            "Login",
            UseCaseShape::Rect,
        ),
    ] {
        let declaration_first = parsed(&format!("usecase-beta\n{declaration}\n{relation}"));
        let relation_first = parsed(&format!("usecase-beta\n{relation}\n{declaration}"));
        assert_eq!(published(&relation_first), published(&declaration_first));
        assert_eq!(
            declaration_first
                .actors
                .iter()
                .map(|actor| actor.id.clone())
                .collect::<Vec<_>>(),
            expected_actor
                .map(|actor| vec![actor.to_string()])
                .unwrap_or_default()
        );
        assert_eq!(
            declaration_first
                .use_case(expected_use_case)
                .expect("use case")
                .shape,
            expected_shape
        );
    }
}

#[test]
fn finalizes_unresolved_relation_endpoints_as_ellipse_use_cases_without_actor_inference() {
    let ast = parsed("usecase-beta\nLeft --> Right");
    assert!(ast.actors.is_empty());
    assert_eq!(
        ast.use_cases,
        vec![
            UseCase {
                id: "Left".into(),
                label: "Left".into(),
                label_type: LabelType::Text,
                shape: UseCaseShape::Ellipse,
                business: false,
                stereotype: None,
                parent_id: None,
                classes: vec![],
                styles: vec![],
            },
            UseCase {
                id: "Right".into(),
                label: "Right".into(),
                label_type: LabelType::Text,
                shape: UseCaseShape::Ellipse,
                business: false,
                stereotype: None,
                parent_id: None,
                classes: vec![],
                styles: vec![],
            },
        ]
    );
}

#[test]
fn merges_equivalent_repeated_declarations_including_equivalent_parent_ownership() {
    let ast = parsed(
        "usecase-beta
systemBoundary Auth
actor User(\"Person\")@{ type: hollow } <<Human>>
actor User(\"Person\")@{ type: hollow } <<Human>>
Login[Sign in] <<Primary>>
Login[Sign in] <<Primary>>
end",
    );
    assert_eq!(
        ast.actors,
        vec![Actor {
            id: "User".into(),
            label: "Person".into(),
            label_type: LabelType::Text,
            type_: ActorType::Hollow,
            icon: None,
            business: false,
            stereotype: Some("Human".into()),
            parent_id: Some("Auth".into()),
            classes: vec![],
            styles: vec![],
        }]
    );
    assert_eq!(
        ast.use_cases,
        vec![UseCase {
            id: "Login".into(),
            label: "Sign in".into(),
            label_type: LabelType::Text,
            shape: UseCaseShape::Rect,
            business: false,
            stereotype: Some("Primary".into()),
            parent_id: Some("Auth".into()),
            classes: vec![],
            styles: vec![],
        }]
    );
    assert_eq!(
        ast.system_boundary("Auth").expect("boundary").members,
        vec!["User", "Login"]
    );
}

#[test]
fn rejects_conflicting_kind_shape_label_stereotype_and_parent_declarations() {
    for (body, id) in [
        ("actor Shared\nShared", "Shared"),
        ("Login(Sign in)\nLogin[Sign in]", "Login"),
        ("Login(First label)\nLogin(Second label)", "Login"),
        ("Login <<Primary>>\nLogin <<Secondary>>", "Login"),
        (
            "systemBoundary First\nLogin\nend\nsystemBoundary Second\nLogin\nend",
            "Login",
        ),
    ] {
        let error = rejected(&format!("usecase-beta\n{body}"));
        assert!(
            error.message.contains(id),
            "{:?} should mention {id}: {}",
            body,
            error.message
        );
    }
}

#[test]
fn rejects_generated_and_explicit_id_collisions_in_the_diagram_global_namespace() {
    for (body, id) in [
        ("\"A-B\"\n\"A B\"", "A_B"),
        ("\"A B\"\nA_B(Explicit)", "A_B"),
        ("actor Shared\njson Shared@{}", "Shared"),
        ("actor link\nA link@--> B", "link"),
        ("A link@--> B\nB link@--> C", "link"),
    ] {
        let error = rejected(&format!("usecase-beta\n{body}"));
        assert!(
            error.message.contains(id),
            "{:?} should mention {id}: {}",
            body,
            error.message
        );
    }
}

#[test]
fn rejects_unresolved_note_class_style_and_metadata_targets() {
    for statement in [
        "note for Ghost \"Missing\"",
        "class Ghost important",
        "style Ghost fill:red",
        "Ghost@{ type: package }",
    ] {
        let error = rejected(&format!("usecase-beta\n{statement}"));
        assert!(
            error.message.contains("Ghost"),
            "{statement}: {}",
            error.message
        );
    }
}

#[test]
fn publishes_nothing_when_a_late_semantic_resolution_fails() {
    let _ = parsed(
        "usecase-beta
actor Previous
Previous --> Published",
    );
    rejected(
        "usecase-beta
direction TB
classDef important fill:red
actor User:::important
Login[Sign in]
User link@--> Login
link@{ animation: fast }
json Payload@{\"ok\": true}
note for Missing \"late failure\"",
    );
}

#[test]
fn allocates_anonymous_edge_and_note_ids_deterministically_in_source_order() {
    let source = "usecase-beta
actor User
User --> Login
note for User \"first\"
Login --> Logout
note for Logout \"second\"";
    let first = parsed(source);
    assert_eq!(
        first
            .relationships
            .iter()
            .map(|relationship| relationship.id.as_str())
            .collect::<Vec<_>>(),
        vec!["edge-0", "edge-1"]
    );
    assert_eq!(
        first
            .notes
            .iter()
            .map(|note| note.id.as_str())
            .collect::<Vec<_>>(),
        vec!["note-0", "note-1"]
    );
    let second = parsed(source);
    assert_eq!(published(&second), published(&first));
}

#[test]
fn distinguishes_generated_and_explicit_ids_and_records_their_exact_occurrences() {
    let source = "usecase-beta
\"Reset password\"
Login(\"Reset password\")";
    let ast = parsed(source);
    assert_eq!(
        ast.use_cases
            .iter()
            .map(|use_case| use_case.id.as_str())
            .collect::<Vec<_>>(),
        vec!["Reset_password", "Login"]
    );
    let generated = ast.graph.node("Reset_password").expect("generated node");
    assert_eq!(generated.label.as_deref(), Some("Reset password"));
    assert_eq!(generated.shape, "ellipse");
    assert_eq!(generated.attrs["kind"], json!("usecase"));
    assert_eq!(generated.attrs["labelType"], json!("text"));
    let explicit = ast.graph.node("Login").expect("explicit node");
    assert_eq!(explicit.label.as_deref(), Some("Reset password"));

    let first = source.find("\"Reset password\"").expect("first label");
    let last = source.rfind("\"Reset password\"").expect("last label");
    let statements = &ast.graph.statements;
    assert_eq!(
        statements[0].nodes[0].id_span.start,
        first + 1,
        "generated id span"
    );
    assert_eq!(statements[0].nodes[0].id_span.end, first + 15);
    assert_eq!(
        statements[0].nodes[0]
            .label_span
            .map(|span| (span.start, span.end)),
        Some((first + 1, first + 15))
    );
    assert_eq!(statements[1].nodes[0].id, "Login");
    assert_eq!(
        (
            statements[1].nodes[0].id_span.start,
            statements[1].nodes[0].id_span.end
        ),
        (
            source.find("Login").expect("Login"),
            source.find("Login").expect("Login") + 5
        )
    );
    assert_eq!(
        statements[1].nodes[0]
            .label_span
            .map(|span| (span.start, span.end)),
        Some((last + 1, source.len() - 2))
    );
}

#[test]
fn names_both_exact_locations_and_source_labels_for_generated_and_explicit_id_collisions() {
    let generated = "usecase-beta\n\"A-B\"\n\"A B\"";
    assert_exact_error(
        generated,
        format!(
            "Generated ID 'A_B' collides with another declaration at {} (label \"A B\"); previous declaration at {} (label \"A-B\")",
            occurrence_location(generated, "\"A B\"", 1, 1, 1),
            occurrence_location(generated, "\"A-B\"", 1, 1, 1)
        ),
    );

    let generated_and_explicit = "usecase-beta\n\"A B\"\nA_B(Explicit)";
    assert_exact_error(
        generated_and_explicit,
        format!(
            "Generated ID 'A_B' collides with another declaration at {}; previous declaration at {} (label \"A B\")",
            occurrence_location(generated_and_explicit, "A_B", 1, 0, 0),
            occurrence_location(generated_and_explicit, "\"A B\"", 1, 1, 1)
        ),
    );

    let generated_kinds = "usecase-beta\nactor \"A-B\"\n\"A B\"";
    assert_exact_error(
        generated_kinds,
        format!(
            "ID 'A_B' is declared as both actor and usecase at {} (label \"A B\"); previous declaration at {} (label \"A-B\")",
            occurrence_location(generated_kinds, "\"A B\"", 1, 1, 1),
            occurrence_location(generated_kinds, "\"A-B\"", 1, 1, 1)
        ),
    );

    let generated_boundaries =
        "usecase-beta\nsystemBoundary \"A-B\"\nend\nsystemBoundary \"A B\"\nend";
    assert_exact_error(
        generated_boundaries,
        format!(
            "Generated ID 'A_B' collides with another declaration at {} (label \"A B\"); previous declaration at {} (label \"A-B\")",
            occurrence_location(generated_boundaries, "\"A B\"", 1, 1, 1),
            occurrence_location(generated_boundaries, "\"A-B\"", 1, 1, 1)
        ),
    );

    let explicit_edges = "usecase-beta\nA link@--> B\nB link@--> C";
    assert_exact_error(
        explicit_edges,
        format!(
            "ID 'link' is declared more than once (edge and edge) at {}; previous declaration at {}",
            occurrence_location(explicit_edges, "link", 2, 0, 0),
            occurrence_location(explicit_edges, "link", 1, 0, 0)
        ),
    );
}

#[test]
fn preserves_entity_codes_and_literal_backslash_n_while_accepting_newlines_only_in_markdown_labels()
{
    let source = [
        "usecase-beta",
        "classDef decorated fill:#fee",
        "actor User(\"`**User**",
        "role`\") <<Human>>:::decorated",
        "Login(\"`Sign",
        "in`\") <<Primary>>:::decorated",
        "Escaped(\"Open #40;safe#41; #91;now#93;\")",
        r#"Literal("First\nSecond")"#,
    ]
    .join("\n");
    let ast = parsed(&source);

    let user = ast.actor("User").expect("actor");
    assert_eq!(user.label, "**User**\nrole");
    assert_eq!(user.label_type, LabelType::Markdown);
    assert_eq!(user.stereotype.as_deref(), Some("Human"));
    assert_eq!(user.classes, vec!["decorated"]);
    let login = ast.use_case("Login").expect("use case");
    assert_eq!(login.label, "Sign\nin");
    assert_eq!(login.label_type, LabelType::Markdown);
    assert_eq!(login.stereotype.as_deref(), Some("Primary"));
    assert_eq!(login.classes, vec!["decorated"]);
    assert_eq!(
        ast.use_case("Escaped").expect("escaped").label,
        "Open #40;safe#41; #91;now#93;"
    );
    assert_eq!(
        ast.use_case("Literal").expect("literal").label,
        r"First\nSecond"
    );
    assert_eq!(
        ast.graph.node("User").expect("node").attrs["labelType"],
        json!("markdown")
    );
    assert_eq!(
        ast.graph.node("User").expect("node").attrs["stereotype"],
        json!("Human")
    );
    assert_eq!(
        ast.graph.node("Escaped").expect("node").attrs["labelType"],
        json!("text")
    );

    let error = rejected(&["usecase-beta", "Broken(\"First", "Second\")"].join("\n"));
    assert!(
        error.message.contains("unexpected character '\"'"),
        "{error}"
    );
}

#[test]
fn accepts_markdown_markers_punctuation_and_non_ascii_characters_in_unquoted_labels() {
    let source = [
        "usecase-beta",
        "actor Reviewer(*Reviewer*)",
        "Literal(**Literal markers**)",
        "Priced[Costs 19.95 EUR, incl. VAT!]",
        "Progress(Ärende — 50% klart)",
        "Reviewer -- opens **form** --> Literal",
        "Reviewer --> Priced",
        "Reviewer --> Progress",
        "note for Literal Read the guide; then sign off.",
    ]
    .join("\n");
    let ast = parsed(&source);

    assert_eq!(ast.actor("Reviewer").expect("actor").label, "*Reviewer*");
    assert_eq!(
        ast.use_case("Literal").expect("literal").label,
        "**Literal markers**"
    );
    assert_eq!(
        ast.use_case("Priced").expect("priced").label,
        "Costs 19.95 EUR, incl. VAT!"
    );
    assert_eq!(
        ast.use_case("Progress").expect("progress").label,
        "Ärende — 50% klart"
    );
    assert_eq!(
        ast.relationships[0].label.as_deref(),
        Some("opens **form**")
    );
    assert_eq!(ast.notes[0].label, "Read the guide; then sign off.");
    assert_eq!(
        ast.graph.node("Reviewer").expect("node").attrs["labelType"],
        json!("text")
    );
}

fn assert_grammar_error_at(source: &str, image: &str, occurrence: usize) {
    let error = super::parser::parse_grammar(source).expect_err("rejected");
    assert!(
        error.message.contains(&format!("but found: '{image}'")),
        "{}",
        error.message
    );
    let span = error.span.expect("a span");
    assert_eq!(
        source_location(source, span.start, span.end),
        occurrence_location(source, image, occurrence, 0, 0)
    );
}

#[test]
fn still_rejects_parentheses_brackets_braces_operators_and_suffixes_inside_unquoted_labels() {
    for (statement, image, occurrence) in [
        ("Literal(Literal (nested) markers)", "(", 2),
        ("Literal[Literal [nested] markers]", "[", 2),
        ("Literal(Literal {nested} markers)", "{", 1),
        ("Literal(Literal -- markers)", "--", 1),
        ("Literal(Literal ::: markers)", ":::", 1),
        ("Literal(Literal @{ markers)", "@{", 1),
    ] {
        let source = ["usecase-beta", statement].join("\n");
        assert_grammar_error_at(&source, image, occurrence);
    }
}

#[test]
fn still_rejects_quotes_inside_unquoted_labels_at_the_lexer() {
    for statement in ["Literal(Literal \"markers)", "Literal(Reviewer's markers)"] {
        let source = ["usecase-beta", statement].join("\n");
        let error = super::parser::parse_grammar(&source).expect_err("rejected");
        assert!(
            error.message.contains("unexpected character"),
            "{}",
            error.message
        );
    }
}

#[test]
fn reports_exact_actor_metadata_and_incompatible_icon_locations() {
    let invalid_type = "usecase-beta\nactor User@{ type: giant }";
    assert_exact_error(
        invalid_type,
        format!(
            "Metadata property 'type' is invalid for actor 'User' at {}",
            occurrence_location(invalid_type, "type", 1, 0, 0)
        ),
    );

    let invalid_key = "usecase-beta\nactor User@{ fillColor: red }";
    assert_exact_error(
        invalid_key,
        format!(
            "Metadata property 'fillColor' is invalid for actor 'User' at {}",
            occurrence_location(invalid_key, "fillColor", 1, 0, 0)
        ),
    );

    let icon_and_awesome = "usecase-beta\nactor User@{ icon: \"fa:user\", type: awesome }";
    assert_exact_error(
        icon_and_awesome,
        format!(
            "Actor 'User' cannot combine icon with type 'awesome' at {}",
            occurrence_location(icon_and_awesome, "User", 1, 0, 0)
        ),
    );
}

#[test]
fn rejects_mixed_kind_generalization_and_actor_include_at_the_exact_relation_span() {
    let generalization = "usecase-beta\nactor User\nLogin\nUser --|> Login";
    assert_exact_error(
        generalization,
        format!(
            "Generalization requires actor-to-actor or use-case-to-use-case endpoints at {}",
            occurrence_location(generalization, "--|> Login", 1, 0, 0)
        ),
    );

    let actor_include = "usecase-beta\nactor User\nactor Admin\nUser ..> : include Admin";
    assert_exact_error(
        actor_include,
        format!(
            "include relationship requires use-case endpoints at {}",
            occurrence_location(actor_include, "..> : include Admin", 1, 0, 0)
        ),
    );
}

#[test]
fn rejects_duplicate_and_unknown_edge_class_and_style_targets_with_exact_locations() {
    let duplicate = "usecase-beta\nA link@--> B\nB link@--> C";
    assert_exact_error(
        duplicate,
        format!(
            "ID 'link' is declared more than once (edge and edge) at {}; previous declaration at {}",
            occurrence_location(duplicate, "link", 2, 0, 0),
            occurrence_location(duplicate, "link", 1, 0, 0)
        ),
    );

    let unknown_class = "usecase-beta\nA known@--> B\nclass missingEdge decorated";
    assert_exact_error(
        unknown_class,
        format!(
            "Class/style target 'missingEdge' is unresolved or anonymous at {}",
            occurrence_location(unknown_class, "missingEdge", 1, 0, 0)
        ),
    );

    let unknown_style = "usecase-beta\nA known@--> B\nstyle missingEdge stroke:red";
    assert_exact_error(
        unknown_style,
        format!(
            "Class/style target 'missingEdge' is unresolved or anonymous at {}",
            occurrence_location(unknown_style, "missingEdge", 1, 0, 0)
        ),
    );
}

#[test]
fn publishes_exact_true_fast_slow_and_false_animation_state_and_rejects_invalid_values() {
    let source = "usecase-beta
A trueEdge@--> B
trueEdge@{ animate: true }
A fastEdge@--> B
fastEdge@{ animation: fast }
A slowEdge@--> B
slowEdge@{ animation: slow }
A offEdge@--> B
offEdge@{ animate: false }";
    let ast = parsed(source);
    let states: Vec<(&str, bool, Option<Animation>)> = ast
        .relationships
        .iter()
        .map(|relationship| {
            (
                relationship.id.as_str(),
                relationship.animate,
                relationship.animation,
            )
        })
        .collect();
    assert_eq!(
        states,
        vec![
            ("trueEdge", true, None),
            ("fastEdge", true, Some(Animation::Fast)),
            ("slowEdge", true, Some(Animation::Slow)),
            ("offEdge", false, None),
        ]
    );
    let graph_states: Vec<(&str, bool, Option<Animation>)> = ast
        .graph
        .edges
        .iter()
        .map(|edge| {
            (
                edge.id.as_str(),
                edge.attrs["animate"].as_bool().expect("animate"),
                edge.attrs
                    .get("animation")
                    .and_then(|value| value.as_str())
                    .map(|value| {
                        if value == "fast" {
                            Animation::Fast
                        } else {
                            Animation::Slow
                        }
                    }),
            )
        })
        .collect();
    assert_eq!(
        graph_states,
        vec![
            ("trueEdge", true, None),
            ("fastEdge", true, Some(Animation::Fast)),
            ("slowEdge", true, Some(Animation::Slow)),
            ("offEdge", false, None),
        ]
    );

    let invalid_animation = "usecase-beta\nA link@--> B\nlink@{ animation: medium }";
    assert_exact_error(
        invalid_animation,
        format!(
            "Metadata property 'animation' is invalid for edge 'link' at {}",
            occurrence_location(invalid_animation, "animation", 1, 0, 0)
        ),
    );

    let invalid_animate = "usecase-beta\nA link@--> B\nlink@{ animate: fast }";
    assert_exact_error(
        invalid_animate,
        format!(
            "Metadata property 'animate' is invalid for edge 'link' at {}",
            occurrence_location(invalid_animate, "animate", 1, 0, 0)
        ),
    );
}

#[test]
fn locates_rejected_relation_note_json_and_nested_boundary_content_inside_a_boundary() {
    for (statement, unexpected, occurrence) in [
        ("User --> Login", "-->", 1),
        ("note for User \"inside\"", "note", 1),
        ("json Payload@{}", "json Payload@", 1),
        ("systemBoundary Nested\nend", "systemBoundary", 2),
    ] {
        let source = format!("usecase-beta\nsystemBoundary Auth\n{statement}\nend");
        assert_grammar_error_at(&source, unexpected, occurrence);
    }
}

#[test]
fn rejects_notes_targeting_json_boundary_and_explicit_edge_ids_with_both_locations() {
    let json = "usecase-beta\njson Payload@{}\nnote for Payload \"invalid\"";
    assert_exact_error(
        json,
        format!(
            "Note target 'Payload' must be an actor or use case, not json at {}; previous declaration at {}",
            occurrence_location(json, "Payload", 2, 0, 0),
            occurrence_location(json, "Payload", 1, 0, 0)
        ),
    );

    let boundary = "usecase-beta\nsystemBoundary Auth\nLogin\nend\nnote for Auth \"invalid\"";
    assert_exact_error(
        boundary,
        format!(
            "Note target 'Auth' must be an actor or use case, not boundary at {}; previous declaration at {}",
            occurrence_location(boundary, "Auth", 2, 0, 0),
            occurrence_location(boundary, "Auth", 1, 0, 0)
        ),
    );

    let edge = "usecase-beta\nA link@--> B\nnote for link \"invalid\"";
    assert_exact_error(
        edge,
        format!(
            "Note target 'link' must be an actor or use case, not edge at {}; previous declaration at {}",
            occurrence_location(edge, "link", 2, 0, 0),
            occurrence_location(edge, "link", 1, 0, 0)
        ),
    );
}

#[test]
fn locates_a_stereotype_on_invalid_boundary_note_json_and_edge_syntax() {
    for body in [
        "systemBoundary Auth <<System>>\nLogin\nend",
        "Login\nnote for Login \"text\" <<Memo>>",
        "json Payload@{} <<Data>>",
        "A link@--> B\nlink@{ animate: true } <<Link>>",
    ] {
        let source = format!("usecase-beta\n{body}");
        assert_grammar_error_at(&source, "<<", 1);
    }
}

#[test]
fn locates_an_unterminated_declaration_or_edge_at_the_end_of_input() {
    for body in ["actor", "A -->"] {
        let source = format!("usecase-beta\n{body}");
        let error = super::parser::parse_grammar(&source).expect_err("rejected");
        assert!(error.message.contains("but found: ''"), "{}", error.message);
        let span = error.span.expect("a span");
        assert_eq!(
            source_location(&source, span.start, span.end),
            source_location(&source, source.len(), source.len())
        );
    }
}

#[test]
fn rejects_business_icon_awesome_and_rectangular_declarations_at_exact_locations() {
    let icon = "usecase-beta\nactor Icon@{ icon: \"fa:user\", business: true }";
    assert_exact_error(
        icon,
        format!(
            "Business actor 'Icon' must use normal or hollow geometry at {}",
            occurrence_location(icon, "Icon", 1, 0, 0)
        ),
    );

    let awesome = "usecase-beta\nactor Awesome@{ type: awesome, business: true }";
    assert_exact_error(
        awesome,
        format!(
            "Business actor 'Awesome' must use normal or hollow geometry at {}",
            occurrence_location(awesome, "Awesome", 1, 0, 0)
        ),
    );

    let rectangle = "usecase-beta\nReport[Generate report]@{ business: true }";
    assert_exact_error(
        rectangle,
        format!(
            "Rectangular use case 'Report' cannot be a business use case at {}",
            occurrence_location(rectangle, "Report", 1, 0, 0)
        ),
    );
}

#[test]
fn rejects_json_in_boundaries_and_json_semantic_or_circle_relations_at_exact_locations() {
    let boundary = "usecase-beta\nsystemBoundary Auth\njson Payload@{}\nend";
    assert_grammar_error_at(boundary, "json Payload@", 1);

    let semantic = "usecase-beta\njson Payload@{}\nInspect\nPayload ..> : include Inspect";
    assert_exact_error(
        semantic,
        format!(
            "include relationship requires use-case endpoints at {}",
            occurrence_location(semantic, "..> : include Inspect", 1, 0, 0)
        ),
    );

    let circle = "usecase-beta\njson Payload@{}\nInspect --o Payload";
    assert_exact_error(
        circle,
        format!(
            "JSON relationship 'Inspect' to 'Payload' permits only point, reversed-point, or markerless solid association at {}",
            occurrence_location(circle, "--o Payload", 1, 0, 0)
        ),
    );
}

#[test]
fn declares_a_boundary_with_an_explicit_id_a_label_inline_metadata_and_classes() {
    let ast = parsed(
        "usecase-beta
classDef system fill:#f8f8ff
systemBoundary sb1[\"Payment service\"]@{ type: package }:::system
  actor Clerk(\"Payment clerk\")
  Authorize(\"Authorize payment\")
end
Clerk --> Authorize",
    );
    assert_eq!(
        ast.system_boundary("sb1").expect("boundary"),
        &SystemBoundary {
            id: "sb1".into(),
            label: "Payment service".into(),
            label_type: LabelType::Text,
            type_: BoundaryType::Package,
            members: vec!["Clerk".into(), "Authorize".into()],
            classes: vec!["system".into()],
            styles: vec![],
        }
    );
}

#[test]
fn accepts_each_boundary_label_form() {
    for (declaration, label, label_type) in [
        ("sb1[Payment service]", "Payment service", LabelType::Text),
        (
            "sb1[\"Payment service\"]",
            "Payment service",
            LabelType::Text,
        ),
        (
            "sb1[\"`**Payment** service`\"]",
            "**Payment** service",
            LabelType::Markdown,
        ),
        ("sb1(Payment service)", "Payment service", LabelType::Text),
        (
            "sb1(\"Payment service\")",
            "Payment service",
            LabelType::Text,
        ),
        (
            "sb1(\"`**Payment** service`\")",
            "**Payment** service",
            LabelType::Markdown,
        ),
    ] {
        let ast = parsed(&format!(
            "usecase-beta\nsystemBoundary {declaration}\nAuthorize(\"Authorize payment\")\nend"
        ));
        let boundary = ast.system_boundary("sb1").expect("boundary");
        assert_eq!(boundary.label, label);
        assert_eq!(boundary.label_type, label_type);
    }
}

#[test]
fn keeps_plain_ids_derived_ids_and_separate_metadata_statements_working() {
    let ast = parsed(
        "usecase-beta
classDef system fill:#f8f8ff
systemBoundary \"Payment service\":::system
  Authorize(\"Authorize payment\")
end
systemBoundary Billing
  Invoice[Create invoice]
end
Payment_service@{ type: package }",
    );
    let payment = ast.system_boundary("Payment_service").expect("payment");
    assert_eq!(payment.label, "Payment service");
    assert_eq!(payment.type_, BoundaryType::Package);
    assert_eq!(payment.classes, vec!["system"]);
    assert_eq!(payment.members, vec!["Authorize"]);
    let billing = ast.system_boundary("Billing").expect("billing");
    assert_eq!(billing.label, "Billing");
    assert_eq!(billing.type_, BoundaryType::Rect);
    assert_eq!(billing.members, vec!["Invoice"]);
}

#[test]
fn lets_a_later_metadata_statement_override_inline_boundary_metadata() {
    let ast = parsed(
        "usecase-beta
systemBoundary \"Payment service\"@{ type: package }
  Authorize(\"Authorize payment\")
end
Payment_service@{ type: rect }",
    );
    assert_eq!(
        ast.system_boundary("Payment_service")
            .expect("boundary")
            .type_,
        BoundaryType::Rect
    );
}

#[test]
fn records_the_boundary_id_title_and_inline_metadata_spans_in_the_ast() {
    let source = "usecase-beta
systemBoundary sb1[\"Payment service\"]@{ type: package }
Authorize(\"Authorize payment\")
end";
    let ast = parsed(source);
    let id_start = source.find("sb1").expect("id");
    let title_start = source.find("Payment service").expect("title");
    let type_start = source.find("type").expect("type");
    let statement = &ast.graph.statements[0];
    assert_eq!(statement.kind, StatementKind::Group);
    assert_eq!(statement.group.as_deref(), Some("sb1"));
    assert_eq!(
        statement.id_span.map(|span| (span.start, span.end)),
        Some((id_start, id_start + 3))
    );
    assert_eq!(
        statement.title_span.map(|span| (span.start, span.end)),
        Some((title_start, title_start + "Payment service".len()))
    );
    assert_eq!(statement.metadata.len(), 1);
    let metadata = &statement.metadata[0];
    assert_eq!(metadata.key, "type");
    assert_eq!(
        (metadata.span.start, metadata.span.end),
        (type_start, type_start + "type: package".len())
    );
    assert_eq!(
        (metadata.key_span.start, metadata.key_span.end),
        (type_start, type_start + 4)
    );
    assert_eq!(
        (metadata.value_span.start, metadata.value_span.end),
        (type_start + 6, type_start + "type: package".len())
    );
}

#[test]
fn requires_inline_boundary_metadata_before_the_class_suffix() {
    let source = "usecase-beta
systemBoundary sb1:::system@{ type: package }
Authorize(\"Authorize payment\")
end
classDef system fill:#f8f8ff";
    assert_grammar_error_at(source, "@{", 1);
}

#[test]
fn rejects_inline_boundary_metadata_a_boundary_does_not_accept() {
    let source = "usecase-beta
systemBoundary sb1[\"Payment service\"]@{ animate: true }
Authorize(\"Authorize payment\")
end";
    assert_exact_error(
        source,
        format!(
            "Metadata property 'animate' is invalid for boundary 'sb1' at {}",
            occurrence_location(source, "animate", 1, 0, 0)
        ),
    );
}

#[test]
fn publishes_a_complete_serializable_ast_with_ordered_statements_and_exact_spans() {
    let source = "usecase-beta

%% representative
systemBoundary Auth
actor User@{ type: hollow } <<Human>>
Login[Sign in]
end
Auth@{ type: package }
json Payload@{\"ok\": true}
Login includeEdge@..> : include Login
includeEdge@{ animation: fast }
note for Login \"Remember\"";
    let ast = parsed(source);
    let graph = &ast.graph;
    assert_eq!(graph.version, 1);
    assert_eq!(graph.diagram_type, "usecase");
    assert_eq!(graph.source, source);
    assert_eq!(graph.header.keyword, "usecase");
    assert_eq!(graph.header.direction, Direction::Lr);
    assert_eq!((graph.header.span.start, graph.header.span.end), (0, 12));
    assert!(graph.class_defs.is_empty());

    let user = graph.node("User").expect("user");
    assert_eq!(user.shape, "actor-hollow");
    assert_eq!(user.attrs["kind"], json!("actor"));
    assert_eq!(user.attrs["actorType"], json!("hollow"));
    assert_eq!(user.attrs["business"], json!(false));
    assert_eq!(user.attrs["stereotype"], json!("Human"));
    assert_eq!(user.attrs["labelType"], json!("text"));
    assert_eq!(user.attrs["parentId"], json!("Auth"));
    let login = graph.node("Login").expect("login");
    assert_eq!(login.label.as_deref(), Some("Sign in"));
    assert_eq!(login.shape, "rect");
    assert_eq!(login.attrs["kind"], json!("usecase"));
    assert_eq!(login.attrs["useCaseShape"], json!("rect"));
    assert_eq!(login.attrs["business"], json!(false));
    assert_eq!(login.attrs["labelType"], json!("text"));
    assert_eq!(login.attrs["parentId"], json!("Auth"));
    let payload = graph.node("Payload").expect("payload");
    assert_eq!(payload.label.as_deref(), Some("Payload"));
    assert_eq!(payload.shape, "json-table");
    assert_eq!(payload.attrs["kind"], json!("json"));
    assert_eq!(payload.attrs["value"], json!({ "ok": true }));
    assert_eq!(payload.attrs["propertyOrder"], json!({ "": ["ok"] }));
    let note = graph.node("note-0").expect("note");
    assert_eq!(note.label.as_deref(), Some("Remember"));
    assert_eq!(note.shape, "note");
    assert_eq!(note.attrs["kind"], json!("note"));
    assert_eq!(note.attrs["target"], json!("Login"));
    assert_eq!(note.attrs["labelType"], json!("text"));
    let group = graph.group("Auth").expect("group");
    assert_eq!(group.nodes, vec!["User", "Login"]);
    assert_eq!(group.attrs["kind"], json!("systemBoundary"));
    assert_eq!(group.attrs["boundaryType"], json!("package"));
    assert_eq!(group.attrs["labelType"], json!("text"));

    assert_eq!(
        graph.edges[0],
        GraphEdge {
            id: "includeEdge".into(),
            source: "Login".into(),
            target: "Login".into(),
            label: Some("include".into()),
            classes: vec![],
            styles: vec![],
            attrs: json!({
                "relationshipType": "include",
                "arrowType": 0,
                "minlen": 1,
                "explicitId": true,
                "animate": true,
                "animation": "fast",
                "labelType": "text",
            }),
        }
    );
    assert_eq!(graph.edges[1].source, "note-0");
    assert_eq!(graph.edges[1].target, "Login");
    assert_eq!(graph.edges[1].attrs["relationshipType"], json!("note"));
    assert_eq!(graph.edges[1].attrs["internal"], json!(true));

    let statement_spans: Vec<(StatementKind, usize, usize)> = graph
        .statements
        .iter()
        .map(|statement| (statement.kind, statement.span.start, statement.span.end))
        .collect();
    assert_eq!(
        statement_spans,
        vec![
            (StatementKind::Blank, 13, 14),
            (StatementKind::Comment, 14, 31),
            (StatementKind::Group, 32, 108),
            (StatementKind::Metadata, 109, 131),
            (StatementKind::Json, 132, 157),
            (StatementKind::Edge, 158, 195),
            (StatementKind::EdgeMetadata, 196, 227),
            (StatementKind::Note, 228, 253),
        ]
    );

    let group_statement = &graph.statements[2];
    assert_eq!(group_statement.group.as_deref(), Some("Auth"));
    assert_eq!(
        group_statement.id_span.map(|span| (span.start, span.end)),
        Some((47, 51))
    );
    assert_eq!(
        group_statement
            .title_span
            .map(|span| (span.start, span.end)),
        Some((47, 51))
    );
    assert_eq!(
        group_statement.end_span.map(|span| (span.start, span.end)),
        Some((105, 108))
    );
    let child_spans: Vec<(StatementKind, usize, usize)> = group_statement
        .children
        .iter()
        .map(|statement| (statement.kind, statement.span.start, statement.span.end))
        .collect();
    assert_eq!(
        child_spans,
        vec![
            (StatementKind::Node, 52, 89),
            (StatementKind::Node, 90, 104)
        ]
    );
    let user_occurrence = &group_statement.children[0].nodes[0];
    assert_eq!(user_occurrence.id, "User");
    assert_eq!(
        (user_occurrence.id_span.start, user_occurrence.id_span.end),
        (58, 62)
    );
    assert_eq!(
        user_occurrence
            .stereotype_span
            .map(|span| (span.start, span.end)),
        Some((82, 87))
    );
    let metadata = &user_occurrence.metadata[0];
    assert_eq!(metadata.key, "type");
    assert_eq!((metadata.span.start, metadata.span.end), (65, 77));
    assert_eq!((metadata.key_span.start, metadata.key_span.end), (65, 69));
    assert_eq!(
        (metadata.value_span.start, metadata.value_span.end),
        (71, 77)
    );
    let login_occurrence = &group_statement.children[1].nodes[0];
    assert_eq!(login_occurrence.id, "Login");
    assert_eq!(
        (login_occurrence.id_span.start, login_occurrence.id_span.end),
        (90, 95)
    );
    assert_eq!(
        login_occurrence
            .label_span
            .map(|span| (span.start, span.end)),
        Some((96, 103))
    );
    assert_eq!(graph.statements[3].nodes[0].id, "Auth");
    assert_eq!(
        (
            graph.statements[3].nodes[0].id_span.start,
            graph.statements[3].nodes[0].id_span.end
        ),
        (109, 113)
    );
    assert_eq!(graph.statements[3].metadata[0].key, "type");
    assert_eq!(
        (
            graph.statements[3].metadata[0].span.start,
            graph.statements[3].metadata[0].span.end
        ),
        (116, 129)
    );
    assert_eq!(graph.statements[4].nodes[0].id, "Payload");
    assert_eq!(
        (
            graph.statements[4].nodes[0].id_span.start,
            graph.statements[4].nodes[0].id_span.end
        ),
        (137, 144)
    );
    assert_eq!(graph.statements[5].nodes[0].id, "Login");
    assert_eq!(graph.statements[5].nodes[1].id, "Login");
    assert_eq!(
        (
            graph.statements[5].nodes[0].id_span.start,
            graph.statements[5].nodes[0].id_span.end
        ),
        (158, 163)
    );
    assert_eq!(
        (
            graph.statements[5].nodes[1].id_span.start,
            graph.statements[5].nodes[1].id_span.end
        ),
        (190, 195)
    );
    assert_eq!(graph.statements[5].edges[0].id, "includeEdge");
    assert_eq!(
        (
            graph.statements[5].edges[0].id_span.expect("id span").start,
            graph.statements[5].edges[0].id_span.expect("id span").end
        ),
        (164, 175)
    );
    assert_eq!(
        (
            graph.statements[5].edges[0]
                .label_span
                .expect("label")
                .start,
            graph.statements[5].edges[0].label_span.expect("label").end
        ),
        (182, 189)
    );
    assert_eq!(graph.statements[6].kind, StatementKind::EdgeMetadata);
    assert_eq!(graph.statements[6].edges[0].id, "includeEdge");
    assert_eq!(
        (
            graph.statements[6].edges[0].id_span.expect("id span").start,
            graph.statements[6].edges[0].id_span.expect("id span").end
        ),
        (196, 207)
    );
    assert_eq!(graph.statements[6].metadata[0].key, "animation");
    assert_eq!(
        (
            graph.statements[6].metadata[0].span.start,
            graph.statements[6].metadata[0].span.end
        ),
        (210, 225)
    );
    assert_eq!(graph.statements[7].reference.as_deref(), Some("note-0"));
    assert_eq!(
        (
            graph.statements[7]
                .reference_span
                .expect("reference span")
                .start,
            graph.statements[7]
                .reference_span
                .expect("reference span")
                .end
        ),
        (244, 252)
    );
    assert_eq!(graph.statements[7].nodes[0].id, "Login");
    assert_eq!(
        (
            graph.statements[7].nodes[0].id_span.start,
            graph.statements[7].nodes[0].id_span.end
        ),
        (237, 242)
    );
}

#[test]
fn locates_a_json_id_that_is_a_substring_of_the_json_keyword() {
    for id in ["o", "n", "s", "j", "jo", "so", "on", "son", "json"] {
        let source = format!("usecase-beta\njson {id} @{{\"a\": 1}}");
        let ast = parsed(&source);
        let id_start = source.find(&format!("json {id}")).expect("declaration") + "json ".len();
        let statement = ast.graph.statements.last().expect("statement");
        assert_eq!(statement.kind, StatementKind::Json);
        assert_eq!(statement.nodes[0].id, id);
        assert_eq!(
            (
                statement.nodes[0].id_span.start,
                statement.nodes[0].id_span.end
            ),
            (id_start, id_start + id.len())
        );
    }
}

#[test]
fn preserves_the_separators_inside_multi_word_style_and_class_def_values() {
    let ast = parsed(
        "usecase-beta
Checkout(\"Place order\")
classDef big font-family:Arial Black
style Checkout border:1px solid red,fill:#eee",
    );
    assert_eq!(
        ast.use_case("Checkout").expect("checkout").styles,
        vec!["border:1px solid red", "fill:#eee"]
    );
    assert_eq!(
        ast.class_def("big").expect("classDef").styles,
        vec!["font-family:Arial Black"]
    );
}

#[test]
fn reports_the_source_position_of_a_lexer_failure() {
    let error = rejected("usecase-beta\nactor \"Bob");
    assert!(
        error.message.contains("unexpected character '\"'"),
        "{error}"
    );
    let span = error.span.expect("a span");
    assert_eq!((span.line, span.column), (2, 7));
}

#[test]
fn accepts_ids_that_start_with_a_digit() {
    let ast = parsed(
        "usecase-beta
actor 1(\"Customer\")
actor 1mg(\"Milligram\")
2(\"Place order\")
3rd(\"Third party\")
1 --> 2
1mg --> 3rd
json 4 @{\"a\": 1}
2 --> 4",
    );
    assert_eq!(
        ast.actors
            .iter()
            .map(|actor| actor.id.as_str())
            .collect::<Vec<_>>(),
        vec!["1", "1mg"]
    );
    assert_eq!(
        ast.use_cases
            .iter()
            .map(|use_case| use_case.id.as_str())
            .collect::<Vec<_>>(),
        vec!["2", "3rd"]
    );
    assert_eq!(
        ast.json_nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        vec!["4"]
    );
    assert_eq!(ast.actor("1").expect("actor").label, "Customer");
    assert_eq!(ast.use_case("3rd").expect("use case").label, "Third party");
    assert_eq!(
        ast.relationships
            .iter()
            .map(|relationship| format!("{}->{}", relationship.source, relationship.target))
            .collect::<Vec<_>>(),
        vec!["1->2", "1mg->3rd", "2->4"]
    );
}

#[test]
fn keeps_decimal_style_values_intact_now_that_ids_may_start_with_a_digit() {
    let ast = parsed(
        "usecase-beta
A(\"x\")
style A stroke-width:1.5px,opacity:0.5,margin:2",
    );
    assert_eq!(
        ast.use_case("A").expect("use case").styles,
        vec!["stroke-width:1.5px", "opacity:0.5", "margin:2"]
    );
}

// ----- usecase.spec.ts (parsing and database accessors) ------------------------------

#[test]
fn parses_a_single_actor() {
    let ast = parsed(
        "usecase-beta
        actor User",
    );
    assert_eq!(ast.actors.len(), 1);
    assert_eq!(ast.actor("User").expect("actor").label, "User");
}

#[test]
fn parses_multiple_actors() {
    let ast = parsed(
        "usecase-beta
        actor User
        actor Admin
        actor Guest",
    );
    assert_eq!(ast.actors.len(), 3);
    for id in ["User", "Admin", "Guest"] {
        assert!(ast.actor(id).is_some(), "{id}");
    }
}

#[test]
fn parses_an_actor_with_a_simple_name() {
    let ast = parsed(
        "usecase-beta
        actor SystemUser",
    );
    assert!(ast.actor("SystemUser").is_some());
}

#[test]
fn parses_use_cases_from_relationships() {
    let ast = parsed(
        "usecase-beta
        actor User
        User --> Login",
    );
    assert_eq!(ast.use_cases.len(), 1);
    assert!(ast.use_case("Login").is_some());
}

#[test]
fn parses_multiple_use_cases_from_relationships() {
    let ast = parsed(
        "usecase-beta
        actor User
        User --> Login
        User --> Logout
        User --> Register",
    );
    assert_eq!(ast.use_cases.len(), 3);
    for id in ["Login", "Logout", "Register"] {
        assert!(ast.use_case(id).is_some(), "{id}");
    }
}

#[test]
fn parses_a_use_case_from_a_relationship() {
    let ast = parsed(
        "usecase-beta
        actor User
        User --> UserLoginProcess",
    );
    assert!(ast.use_case("UserLoginProcess").is_some());
}

#[test]
fn parses_use_cases_with_quoted_names() {
    let ast = parsed(
        "usecase-beta
        actor \"Customer Service\"
        actor \"System Administrator\"
        \"Customer Service\" --> \"Handle Tickets\"
        \"System Administrator\" --> \"Manage System\"",
    );
    assert_eq!(ast.actors.len(), 2);
    assert_eq!(
        ast.actor("Customer_Service").expect("actor").label,
        "Customer Service"
    );
    assert_eq!(
        ast.actor("System_Administrator").expect("actor").label,
        "System Administrator"
    );
    assert_eq!(ast.use_cases.len(), 2);
    assert_eq!(
        ast.use_case("Handle_Tickets").expect("use case").label,
        "Handle Tickets"
    );
    assert_eq!(
        ast.use_case("Manage_System").expect("use case").label,
        "Manage System"
    );
}

#[test]
fn parses_actor_to_use_case_relationships() {
    let ast = parsed(
        "usecase-beta
        actor User
        User --> Login",
    );
    assert_eq!(ast.relationships.len(), 1);
    let relationship = &ast.relationships[0];
    assert_eq!(relationship.source, "User");
    assert_eq!(relationship.target, "Login");
    assert_eq!(relationship.type_, RelationshipType::Association);
}

#[test]
fn parses_multiple_relationships() {
    let ast = parsed(
        "usecase-beta
        actor User
        User --> Login
        User --> Logout",
    );
    assert_eq!(ast.relationships.len(), 2);
    assert_eq!(
        (
            ast.relationships[0].source.as_str(),
            ast.relationships[0].target.as_str()
        ),
        ("User", "Login")
    );
    assert_eq!(
        (
            ast.relationships[1].source.as_str(),
            ast.relationships[1].target.as_str()
        ),
        ("User", "Logout")
    );
}

#[test]
fn parses_a_relationship_with_a_label() {
    let ast = parsed(
        "usecase-beta
        actor Developer
        Developer --important--> WriteCode",
    );
    assert_eq!(ast.relationships[0].label.as_deref(), Some("important"));
}

#[test]
fn parses_different_arrow_types() {
    let ast = parsed(
        "usecase-beta
        actor User
        actor Admin
        User --> Login
        Admin <-- Logout
        User -- ViewData",
    );
    assert_eq!(ast.relationships.len(), 3);
    assert_eq!(ast.relationships[0].arrow_type, ArrowType::SolidArrow);
    assert_eq!(ast.relationships[1].arrow_type, ArrowType::BackArrow);
    assert_eq!(ast.relationships[2].arrow_type, ArrowType::LineSolid);
}

#[test]
fn parses_a_system_boundary() {
    let ast = parsed(
        "usecase-beta
        actor Admin, User
        systemBoundary \"Authentication\"
          Login
          Logout
        end
        Admin --> Login
        User --> Login",
    );
    assert!(!ast.system_boundaries.is_empty());
}

#[test]
fn parses_use_cases_within_a_system_boundary() {
    let ast = parsed(
        "usecase-beta
        actor User
        systemBoundary \"Authentication System\"
          Login
          Logout
        end
        User --> Login",
    );
    assert_eq!(ast.use_cases.len(), 2);
    assert!(ast.use_case("Login").is_some());
    assert!(ast.use_case("Logout").is_some());
}

#[test]
fn parses_a_boundary_with_an_explicit_id_a_label_and_inline_metadata() {
    let ast = parsed(
        "usecase-beta
        systemBoundary sb1[\"Payment service\"]@{ type: package }
          actor Clerk(\"Payment clerk\")
          Authorize(\"Authorize payment\")
        end
        Clerk --> Authorize",
    );
    let boundary = ast.system_boundary("sb1").expect("boundary");
    assert_eq!(boundary.label, "Payment service");
    assert_eq!(boundary.type_, BoundaryType::Package);
    assert_eq!(boundary.members, vec!["Clerk", "Authorize"]);
}

#[test]
fn handles_directions_and_normalizes_td_to_tb() {
    for (operand, expected) in [
        ("TB", Direction::Tb),
        ("LR", Direction::Lr),
        ("TD", Direction::Tb),
        ("BT", Direction::Bt),
        ("RL", Direction::Rl),
    ] {
        let ast = parsed(&format!("usecase-beta\ndirection {operand}\nactor User"));
        assert_eq!(ast.direction, expected);
    }
}

#[test]
fn parses_actor_metadata() {
    let ast = parsed(
        "usecase-beta
        actor User@{ \"type\" : \"normal\", \"icon\" : \"user\" }
        User --> Login",
    );
    let user = ast.actor("User").expect("actor");
    assert_eq!(user.type_, ActorType::Icon);
    assert_eq!(user.icon.as_deref(), Some("user"));
    assert!(!user.business);
}

#[test]
fn parses_multiple_actors_with_different_metadata() {
    let ast = parsed(
        "usecase-beta
        actor User@{ \"icon\" : \"user\" }
        actor Admin@{ \"type\" : \"hollow\" }
        User --> Login
        Admin --> ManageUsers",
    );
    assert_eq!(ast.actors.len(), 2);
    assert_eq!(ast.actor("User").expect("user").type_, ActorType::Icon);
    assert_eq!(ast.actor("Admin").expect("admin").type_, ActorType::Hollow);
}

#[test]
fn parses_a_complete_authentication_system() {
    let ast = parsed(
        "usecase-beta
        actor User
        actor Admin

        systemBoundary \"Authentication System\"
          Login
          Logout
          Register
          ResetPassword
        end

        User --> Login
        User --> Register
        User --> Logout
        Admin --> Login",
    );
    assert_eq!(ast.actors.len(), 2);
    assert_eq!(ast.use_cases.len(), 4);
    assert_eq!(ast.relationships.len(), 4);
}

#[test]
fn parses_diagram_with_multiple_arrow_types() {
    let ast = parsed(
        "usecase-beta
        actor User
        actor Admin
        User --> Login
        Admin <-- Logout
        User -- ViewData",
    );
    assert_eq!(ast.relationships.len(), 3);
}

#[test]
fn handles_use_case_creation_from_relationships() {
    let ast = parsed(
        "usecase-beta
        actor Developer
        Developer --> LoginSystem
        Developer --> Authentication",
    );
    assert_eq!(ast.use_cases.len(), 2);
    assert!(ast.use_case("LoginSystem").is_some());
    assert!(ast.use_case("Authentication").is_some());
}

#[test]
fn handles_class_definitions() {
    let ast = parsed(
        "usecase-beta
        actor User
        User --> Login
        classDef important fill:#f96,stroke:#333,stroke-width:4px
        class Login important",
    );
    assert!(!ast.class_defs.is_empty());
    assert_eq!(
        ast.class_def("important").expect("classDef").styles,
        vec!["fill:#f96", "stroke:#333", "stroke-width:4px"]
    );
    assert_eq!(
        ast.use_case("Login").expect("use case").classes,
        vec!["important"]
    );
}

#[test]
fn database_accessors_return_present_and_absent_records() {
    let ast = parsed(
        "usecase-beta
        actor User
        actor Admin
        User --> Login
        User --> Logout",
    );
    assert_eq!(ast.actor("User").expect("actor").id, "User");
    assert_eq!(ast.actor("User").expect("actor").label, "User");
    assert_eq!(ast.use_case("Login").expect("use case").id, "Login");
    assert_eq!(ast.use_case("Login").expect("use case").label, "Login");
    assert!(ast.actor("NonExistent").is_none());
    assert!(ast.use_case("NonExistent").is_none());
    assert!(ast.actor("Unknown").is_none());
}

#[test]
fn projects_actor_and_use_case_geometry_exactly_into_the_ast() {
    let ast = parsed(
        "usecase-beta
actor Normal
actor Hollow@{ type: hollow, business: true }
Business(Business)@{ business: true }
Ellipse
RectCase[Rectangle]",
    );
    assert_eq!(ast.graph.node("Normal").expect("normal").shape, "actor");
    assert_eq!(
        ast.graph.node("Normal").expect("normal").attrs["actorType"],
        json!("normal")
    );
    assert_eq!(
        ast.graph.node("Normal").expect("normal").attrs["business"],
        json!(false)
    );
    assert_eq!(
        ast.graph.node("Hollow").expect("hollow").shape,
        "actor-hollow"
    );
    assert_eq!(
        ast.graph.node("Hollow").expect("hollow").attrs["actorType"],
        json!("hollow")
    );
    assert_eq!(
        ast.graph.node("Hollow").expect("hollow").attrs["business"],
        json!(true)
    );
    let business = ast.graph.node("Business").expect("business");
    assert_eq!(business.shape, "ellipse");
    assert_eq!(business.attrs["kind"], json!("usecase"));
    assert_eq!(business.attrs["useCaseShape"], json!("ellipse"));
    assert_eq!(business.attrs["business"], json!(true));
    let ellipse = ast.graph.node("Ellipse").expect("ellipse");
    assert_eq!(ellipse.shape, "ellipse");
    assert_eq!(ellipse.attrs["business"], json!(false));
    let rect = ast.graph.node("RectCase").expect("rect");
    assert_eq!(rect.shape, "rect");
    assert_eq!(rect.attrs["useCaseShape"], json!("rect"));
    assert_eq!(rect.label.as_deref(), Some("Rectangle"));
}

#[test]
fn projects_base_one_extra_n_extra_and_labelled_minimum_lengths() {
    let ast = parsed(
        "usecase-beta
A base@--> B
B two@---> C
C many@-----> D
D labelled@-- \"long\" ----> E",
    );
    let projected: Vec<(&str, Option<&str>, u32)> = ast
        .graph
        .edges
        .iter()
        .map(|edge| {
            (
                edge.id.as_str(),
                edge.label.as_deref(),
                edge.attrs["minlen"].as_u64().expect("minlen") as u32,
            )
        })
        .collect();
    assert_eq!(
        projected,
        vec![
            ("base", None, 1),
            ("two", None, 2),
            ("many", None, 4),
            ("labelled", Some("long"), 3),
        ]
    );
}

#[test]
fn projects_complex_source_ordered_json_a_forward_edge_classes_and_styles() {
    let ast = parsed(
        "usecase-beta
classDef data fill:#eef,stroke:#369
json Payload@{
  \"2\": \"second\",
  \"1\": \"first\",
  \"colors\": [\"Red\", \"Green\"],
  \"items\": [{ \"name\": \"A\" }, {}],
  \"empty\": {},
  \"duplicate\": \"old\",
  \"duplicate\": \"new\"
}:::data
style Payload stroke-width:4px
Inspect payloadEdge@--> Payload
class payloadEdge data
style payloadEdge stroke:#f00",
    );
    assert_eq!(
        ast.graph.node("Payload").expect("payload"),
        &GraphNode {
            label: Some("Payload".into()),
            shape: "json-table".into(),
            classes: vec!["data".into()],
            styles: vec!["stroke-width:4px".into()],
            attrs: json!({
                "kind": "json",
                "value": {
                    "1": "first",
                    "2": "second",
                    "colors": ["Red", "Green"],
                    "items": [{ "name": "A" }, {}],
                    "empty": {},
                    "duplicate": "new",
                },
                "propertyOrder": {
                    "": ["2", "1", "colors", "items", "empty", "duplicate"],
                    "/items/0": ["name"],
                    "/items/1": [],
                    "/empty": [],
                },
                "labelType": "text",
            }),
        }
    );
    let edge = &ast.graph.edges[0];
    assert_eq!(edge.id, "payloadEdge");
    assert_eq!(edge.source, "Inspect");
    assert_eq!(edge.target, "Payload");
    assert_eq!(edge.classes, vec!["data"]);
    assert_eq!(edge.styles, vec!["stroke:#f00"]);
    assert_eq!(edge.attrs["relationshipType"], json!("association"));
    assert_eq!(edge.attrs["arrowType"], json!(0));
    assert_eq!(edge.attrs["minlen"], json!(1));
    assert_eq!(edge.attrs["explicitId"], json!(true));
}

// ----- usecaseDb.spec.ts (the model defaults this crate creates) ----------------------

#[test]
fn creates_an_empty_model_with_the_documented_defaults() {
    let ast = parsed("usecase-beta");
    assert_empty_publication(&ast);
    assert!(ast.symbols.is_empty());
    assert_eq!(ast.direction, Direction::Lr);
}

#[test]
fn skips_the_blank_and_comment_lines_preprocess_leaves_before_the_header() {
    let ast = parsed("\n\n%% a leading comment\n\nusecase-beta\nactor User");
    assert!(ast.actor("User").is_some());
    assert_eq!(
        ast.graph
            .statements
            .iter()
            .map(|statement| statement.kind)
            .collect::<Vec<_>>(),
        vec![StatementKind::Node]
    );
}

#[test]
fn parses_through_the_crate_entry_point_with_frontmatter_and_a_directive() {
    let text = "---\ntitle: Authentication\n---\n%%{init: {'theme': 'dark'}}%%\nusecase-beta\nactor User\nUser --> Login";
    match crate::parse(text).expect("the frontmatter and directive are removed before parsing") {
        crate::Diagram::Usecase(ast) => {
            assert!(ast.actor("User").is_some());
            assert!(ast.use_case("Login").is_some());
        }
        other => panic!("expected a usecase diagram, got {other:?}"),
    }
}
