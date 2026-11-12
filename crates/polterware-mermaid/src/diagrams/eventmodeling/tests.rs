//! Tests ported from the upstream event modeling specs:
//! `packages/mermaid/src/diagrams/eventmodeling/eventmodeling.spec.ts` (the database and the
//! parser glue) and `packages/parser/tests/eventmodeling.test.ts` (the Langium grammar), plus
//! the in-source case of `eventmodeling/parser.ts`. Inputs and expectations are upstream's.
//!
//! Skipped, and why:
//! - `eventmodeling.spec.ts`'s "should keep explicit sources on a reset frame without inferring
//!   the previous frame" asserts `db.getState().relations`, the layout projection this port
//!   does not own (`getState` needs text measurement). The parse half is covered by
//!   [`reset_frames_keep_explicit_sources_without_parse_side_inference`].
//! - In the same test, `expect(() => db.getState()).toThrow('No data for EventModel')` checks
//!   that a failed `setAst` leaves the database empty; there is no state to inspect here.
//! - `eventmodeling.test.ts`'s `Validation` describe exercises `EventModelingValidator`, the
//!   Langium validation phase, which `mermaidAPI.parse` does not run and this port does not
//!   include.
//! - `expectNoErrorsOrAlternatives` (in `test-util.ts`) checks Langium's error arrays and that
//!   no `console.log` ran; `parse` returning `Ok` is the error-free assertion here, and there
//!   is no console.
//! - `frame.$type` is Langium's generated AST type marker; [`FrameKind`] carries the same
//!   distinction.
//! - The `getBBox` setup in `eventmodeling.spec.ts` is there for `getState`'s text measurement,
//!   which is not ported.

use super::{Ast, Frame, FrameKind, ModelEntityType, parse};
use crate::error::ParseError;

fn model(body: &str) -> Ast {
    parse(body).unwrap_or_else(|error| panic!("expected {body:?} to parse, got {error}"))
}

fn error(body: &str) -> ParseError {
    match parse(body) {
        Ok(ast) => panic!("expected {body:?} to fail, parsed {ast:?}"),
        Err(error) => error,
    }
}

fn frame<'a>(ast: &'a Ast, name: &str) -> &'a Frame {
    ast.frames
        .iter()
        .find(|frame| frame.name == name)
        .unwrap_or_else(|| panic!("frame {name:?} not found in {:?}", ast.frames))
}

// packages/mermaid/src/diagrams/eventmodeling/eventmodeling.spec.ts

#[test]
fn handles_a_simple_eventmodeling_definition() {
    let ast = model(
        "eventmodeling\n    tf 01 ui UI\n    tf 02 cmd RunAction\n    tf 03 evt ActionExecuted",
    );
    assert_eq!(ast.frames.len(), 3);
    assert_eq!(frame(&ast, "01").model_entity_type, ModelEntityType::Ui);
    assert_eq!(frame(&ast, "02").model_entity_type, ModelEntityType::Cmd);
    assert_eq!(frame(&ast, "03").model_entity_type, ModelEntityType::Evt);
    for name in ["01", "02", "03"] {
        assert_eq!(frame(&ast, name).kind, FrameKind::Time);
    }
}

#[test]
fn handles_data_inline_in_frames() {
    let ast = model(
        "eventmodeling\n    tf 01 cmd AddItem { productId: 7 }\n    tf 02 evt ItemAdded { productId: 7 }",
    );
    assert_eq!(
        frame(&ast, "01").data_inline_value.as_deref(),
        Some("{ productId: 7 }")
    );
    assert_eq!(
        frame(&ast, "02").data_inline_value.as_deref(),
        Some("{ productId: 7 }")
    );
    assert_eq!(frame(&ast, "01").data_type, None);
}

#[test]
fn handles_data_block_references() {
    let ast = model(
        "eventmodeling\n    tf 01 cmd AddItem\n    tf 02 evt ItemAdded [[ItemAddedData]]\n\ndata ItemAddedData\n{\n  productId: 7\n}",
    );
    assert_eq!(
        frame(&ast, "02").data_reference.as_deref(),
        Some("ItemAddedData")
    );
    assert_eq!(ast.data_entities.len(), 1);
    assert_eq!(ast.data_entities[0].name, "ItemAddedData");
    assert_eq!(ast.data_entities[0].data_type, None);
    assert_eq!(
        ast.data_entities[0].data_block_value,
        "{\n  productId: 7\n}"
    );
}

#[test]
fn handles_qualified_names() {
    let ast = model(
        "eventmodeling\n    tf 01 ui CartUI\n    tf 02 cmd Inventory.AddItem\n    tf 03 evt Inventory.ItemAdded",
    );
    assert_eq!(frame(&ast, "01").entity_identifier, "CartUI");
    assert_eq!(frame(&ast, "02").entity_identifier, "Inventory.AddItem");
    assert_eq!(frame(&ast, "03").entity_identifier, "Inventory.ItemAdded");
}

#[test]
fn handles_multiple_source_frames() {
    let ast = model(
        "eventmodeling\n    tf 01 ui CartUI\n    tf 02 cmd AddItem\n    tf 03 cmd RemoveItem\n    tf 04 evt ItemChanged ->> 02 ->> 03",
    );
    assert_eq!(frame(&ast, "04").source_frames, ["02", "03"]);
    assert!(frame(&ast, "01").source_frames.is_empty());
}

#[test]
fn handles_reset_frames() {
    let ast =
        model("eventmodeling\n    rf 01 ui CartUI\n    rf 02 cmd AddItem\n    rf 03 evt ItemAdded");
    assert_eq!(ast.frames.len(), 3);
    for name in ["01", "02", "03"] {
        assert_eq!(frame(&ast, name).kind, FrameKind::Reset);
    }
}

#[test]
fn rejects_a_frame_id_reused_by_a_reset_frame() {
    let error = error(
        "eventmodeling\n        tf 01 ui UI\n        tf 02 cmd Command\n        rf 01 evt Event",
    );
    assert_eq!(
        error.message,
        "Duplicate event modeling frame ID \"01\" on line 4"
    );
    assert_eq!(error.span.expect("the error carries a span").line, 4);
}

#[test]
fn reports_the_original_line_number_when_comments_precede_a_duplicate_frame() {
    let source = crate::preprocess::preprocess(
        "eventmodeling\n        tf 01 ui UI\n        %% This comment occupies a source line.\n        rf 01 evt Event",
    );
    // The event modeling database sets `preserveCommentsWhenParsing`, so the pipeline hands
    // its parser `withComments`, not the comment-stripped `body`.
    let error = error(&source.with_comments);
    assert_eq!(
        error.message,
        "Duplicate event modeling frame ID \"01\" on line 4"
    );
}

#[test]
fn handles_all_entity_types() {
    let ast = model(
        "eventmodeling\n    tf 01 ui UI\n    tf 02 ui UI2\n    tf 03 cmd Command\n    tf 04 command Command2\n    tf 05 evt Event\n    tf 06 event Event2\n    tf 07 pcr Processor\n    tf 08 processor Processor2\n    tf 09 rmo ReadModel\n    tf 10 readmodel ReadModel2",
    );
    let types: Vec<&str> = ast
        .frames
        .iter()
        .map(|frame| frame.model_entity_type.as_str())
        .collect();
    assert_eq!(
        types,
        [
            "ui",
            "ui",
            "cmd",
            "command",
            "evt",
            "event",
            "pcr",
            "processor",
            "rmo",
            "readmodel"
        ]
    );
}

#[test]
fn reset_frames_keep_explicit_sources_without_parse_side_inference() {
    let ast = model(
        "eventmodeling\n      tf 01 ui UI\n      tf 02 cmd Command\n      tf 03 evt Event\n      rf 04 rmo ReadModel ->> 03\n      rf 05 ui OtherUI",
    );
    assert_eq!(frame(&ast, "04").kind, FrameKind::Reset);
    assert_eq!(frame(&ast, "04").source_frames, ["03"]);
    assert_eq!(frame(&ast, "05").kind, FrameKind::Reset);
    assert!(frame(&ast, "05").source_frames.is_empty());
}

// packages/parser/tests/eventmodeling.test.ts

#[test]
fn parses_a_complex_model() {
    let ast = model(
        "eventmodeling
tf 01 cmd UpdateCartCommand
tf 02 evt CartUpdatedEvent ->> 01 `jsobj`{ a: b }
tf 03 rmo CartItemsReadModel ->> 02 [[CartItemsReadModel03]]
tf 04 evt ProductDescriptionUpdatedEvent ->> 01 `jsobj`{ a: { c: d } }
tf 05 evt ProductTitleUpdatedEvent ->> 01 { \"a\": { \"c\": true } }
tf 06 evt ProductCountIncrementedEvent ->> 01 `json`\" { \"a\": { \"c\": true } } \"

data CartItemsReadModel03 {
  { a: b }
}

data NotAssignedData02 `jsobj` {
  { a: {
    d: true
  }}
}

data AnotherNotAssignedData06 {
  a: 'abc'
}

note 02 `md` {
    # head 1
    this is markdown note
}

note 05 {
  This is whatever <b>you</b> want
  On multiple lines
}

gwt 01
  given
    evt CartUpdatedEvent
    evt CartUpdatedEvent
  when
    evt ProductDescriptionUpdatedEvent
    evt ProductTitleUpdatedEvent
  then
    evt ProductTitleUpdatedEvent


gwt 03
  given
    evt CartUpdatedEvent
    evt ProductTitleUpdatedEvent
  then
    evt ProductTitleUpdatedEvent
    evt CartUpdatedEvent
",
    );
    assert_eq!(ast.frames.len(), 6);
    assert_eq!(ast.data_entities.len(), 3);
    assert_eq!(ast.note_entities.len(), 2);
    assert_eq!(ast.gwt_entities.len(), 2);

    let updated = frame(&ast, "02");
    assert_eq!(updated.source_frames, ["01"]);
    assert_eq!(updated.data_type.as_deref(), Some("jsobj"));
    assert_eq!(updated.data_inline_value.as_deref(), Some("{ a: b }"));

    let read_model = frame(&ast, "03");
    assert_eq!(
        read_model.data_reference.as_deref(),
        Some("CartItemsReadModel03")
    );
    assert_eq!(ast.data_entities[1].name, "NotAssignedData02");
    assert_eq!(ast.data_entities[1].data_type.as_deref(), Some("jsobj"));

    assert_eq!(ast.note_entities[0].source_frame, "02");
    assert_eq!(ast.note_entities[0].data_type.as_deref(), Some("md"));
    assert_eq!(ast.note_entities[1].source_frame, "05");
    assert_eq!(ast.note_entities[1].data_type, None);

    assert_eq!(ast.gwt_entities[0].source_frame, "01");
    assert_eq!(ast.gwt_entities[0].given.len(), 2);
    assert_eq!(ast.gwt_entities[0].when.len(), 2);
    assert_eq!(ast.gwt_entities[0].then.len(), 1);
    assert_eq!(ast.gwt_entities[1].when, []);
}

#[test]
fn parses_a_simple_model_with_full_syntax() {
    let ast = model("eventmodeling\ntimeframe 01 event Start\n\n  ");
    assert_eq!(ast.frames.len(), 1);
    let frame = &ast.frames[0];
    assert_eq!(frame.name, "01");
    assert_eq!(frame.model_entity_type.as_str(), "event");
    assert_eq!(frame.entity_identifier, "Start");
    assert_eq!(frame.kind, FrameKind::Time);
}

#[test]
fn parses_qualified_names_in_the_model() {
    let ast = model(
        "eventmodeling\n\ntimeframe 02 ui UI\ntf 01 evt Product.PriceChanged\ntf 03 evt Cart.ItemAdded\n\n  ",
    );
    assert_eq!(ast.frames.len(), 3);
    let frame = &ast.frames[1];
    assert_eq!(frame.name, "01");
    assert_eq!(frame.model_entity_type.as_str(), "evt");
    assert_eq!(frame.entity_identifier, "Product.PriceChanged");
}

#[test]
fn parses_both_types_of_frames_in_the_model() {
    let ast = model(
        "eventmodeling\n\ntf 02 ui UI\nresetframe 01 evt Product.PriceChanged\ntf 03 evt Cart.ItemAdded\n\n  ",
    );
    assert_eq!(ast.frames.len(), 3);
    let frame = &ast.frames[1];
    assert_eq!(frame.kind, FrameKind::Reset);
    assert_eq!(frame.name, "01");
    assert_eq!(frame.model_entity_type.as_str(), "evt");
    assert_eq!(frame.entity_identifier, "Product.PriceChanged");
}

#[test]
fn parses_a_model_with_multiple_source_frames() {
    let ast = model(
        "eventmodeling\ntf 01 evt Start\ntf 02 evt End\nrf 03 readmodel ReadModel01 ->> 01 ->> 02 { a: true }\nrf 04 rmo ReadModel02 ->> 01 ->> 02\n  ",
    );
    assert_eq!(ast.frames.len(), 4);

    let first = &ast.frames[2];
    assert_eq!(first.name, "03");
    assert_eq!(first.model_entity_type, ModelEntityType::Readmodel);
    assert_eq!(first.source_frames.len(), 2);
    assert_eq!(first.data_inline_value.as_deref(), Some("{ a: true }"));

    let second = &ast.frames[3];
    assert_eq!(second.name, "04");
    assert_eq!(second.model_entity_type, ModelEntityType::Rmo);
    assert_eq!(second.source_frames.len(), 2);
}

// packages/parser/src/language/eventmodeling/parser.ts (in-source test)

#[test]
fn parses_a_simple_model() {
    assert!(parse("eventmodeling\n  tf 01 evt Start\n\n    ").is_ok());
}

// Behavior of the grammar and the database these specs do not pin down.

#[test]
fn title_and_accessibility_fields_match_on_the_header_line() {
    let ast = model("eventmodeling title Sample\ntf 01 ui UI");
    assert_eq!(ast.diagram_title, " title Sample");

    let ast = model("eventmodeling accTitle: Sample\ntf 01 ui UI");
    assert_eq!(ast.acc_title, "accTitle: Sample");

    let ast = model("eventmodeling accDescr: Some text\ntf 01 ui UI");
    assert_eq!(ast.acc_description, " accDescr: Some text");
}

#[test]
fn title_and_accessibility_fields_do_not_match_on_their_own_line() {
    for body in [
        "eventmodeling\ntitle Sample\ntf 01 ui UI",
        "eventmodeling\naccTitle: Sample\ntf 01 ui UI",
        "eventmodeling\naccDescr: Some text\ntf 01 ui UI",
    ] {
        assert!(parse(body).is_err(), "{body:?}");
    }
}

#[test]
fn keywords_shadowed_by_a_longer_identifier_are_not_keywords() {
    assert!(parse("eventmodeling\ntf01 ui UI").is_err());
    assert!(parse("eventmodeling\ntfa 01 ui UI").is_err());
    assert!(parse("eventmodelingX\ntf 01 ui UI").is_err());
}

#[test]
fn a_data_block_closes_at_the_first_line_starting_with_a_brace() {
    let ast = model("eventmodeling\ntf 01 ui UI\ndata D {\n}\nother\n}");
    assert_eq!(ast.data_entities[0].data_block_value, "{\n}\nother\n}");
}

#[test]
fn a_data_block_must_be_followed_by_whitespace() {
    assert!(parse("eventmodeling\ntf 01 ui UI\ndata D {\n}X").is_err());
}

#[test]
fn a_closing_brace_must_start_its_line() {
    assert!(parse("eventmodeling\ntf 01 ui UI\ndata D {\n  a\n  }\n").is_err());
}

#[test]
fn braces_on_one_line_are_an_inline_value_not_a_block() {
    assert!(parse("eventmodeling\ntf 01 ui UI\ndata D {}\n").is_err());
}

#[test]
fn a_data_inline_value_is_greedy_on_its_line() {
    let ast = model("eventmodeling\ntf 01 ui UI {a} {b}");
    assert_eq!(
        frame(&ast, "01").data_inline_value.as_deref(),
        Some("{a} {b}")
    );

    let ast = model("eventmodeling\ntf 01 ui UI \"a\" \"b\"");
    assert_eq!(
        frame(&ast, "01").data_inline_value.as_deref(),
        Some("\"a\" \"b\"")
    );

    let ast = model("eventmodeling\ntf 01 ui UI 'a' 'b'");
    assert_eq!(
        frame(&ast, "01").data_inline_value.as_deref(),
        Some("'a' 'b'")
    );
}

#[test]
fn a_qualified_name_ignores_whitespace_around_the_dots() {
    let ast = model("eventmodeling\ntf 01 ui A . B\nentity C .\nD");
    assert_eq!(frame(&ast, "01").entity_identifier, "A.B");
    assert_eq!(ast.model_entities[0].name, "C.D");
}

#[test]
fn a_frame_id_is_at_most_three_digits() {
    assert_eq!(frame(&model("eventmodeling\ntf 1 ui UI"), "1").name, "1");
    assert_eq!(
        frame(&model("eventmodeling\ntf 123 ui UI"), "123").name,
        "123"
    );
    assert!(parse("eventmodeling\ntf 1234 ui UI").is_err());
}

#[test]
fn comments_of_every_kind_are_skipped() {
    let ast = model(
        "eventmodeling\ntf 01 ui UI\n%% a line comment\n/* a block\ncomment */\n// a slash comment\ntf 02 cmd C",
    );
    assert_eq!(ast.frames.len(), 2);
}

#[test]
fn parses_crlf_line_endings() {
    let ast = model("eventmodeling\r\ntf 01 ui UI\r\ndata D {\r\n  a: b\r\n}\r\n");
    assert_eq!(frame(&ast, "01").name, "01");
    assert_eq!(
        ast.data_entities[0].data_block_value,
        "{\r\n  a: b\r\n}\r\n"
    );
}

#[test]
fn a_gwt_block_may_omit_the_when_section() {
    let ast = model("eventmodeling\ntf 01 ui UI\ngwt 01\ngiven\nevt A\nthen\nevt B");
    let gwt = &ast.gwt_entities[0];
    assert_eq!(gwt.source_frame, "01");
    assert_eq!(gwt.given.len(), 1);
    assert_eq!(gwt.given[0].entity_identifier, "A");
    assert!(gwt.when.is_empty());
    assert_eq!(gwt.then[0].entity_identifier, "B");
}

#[test]
fn a_gwt_block_requires_statements_after_given_and_then() {
    assert!(parse("eventmodeling\ntf 01 ui UI\ngwt 01\ngiven\nthen\nevt B").is_err());
    assert!(parse("eventmodeling\ntf 01 ui UI\ngwt 01\ngiven\nevt A\nthen").is_err());
}

#[test]
fn a_stray_character_reports_its_span() {
    let error = error("eventmodeling\ntf 01 ui UI\n-->");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (3, 1));
    assert!(error.message.contains('-'));
}

#[test]
fn leading_whitespace_before_the_header_is_skipped() {
    assert!(parse("\n  \t eventmodeling\n  tf 01 ui UI").is_ok());
}

#[test]
fn a_reused_frame_id_reports_the_second_frame() {
    let error = error("eventmodeling\ntf 07 ui A\ntf 07 cmd B");
    assert_eq!(
        error.message,
        "Duplicate event modeling frame ID \"07\" on line 3"
    );
}
