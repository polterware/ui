//! Tests ported from the upstream state diagram specs: `stateDb.spec.js`,
//! `stateDiagram.spec.js`, `stateDiagram-v2.spec.js`, `parser/state-parser.spec.js`,
//! `parser/state-style.spec.js`, `stateNoteEdge.spec.ts`, `stateNoteNode.spec.js`, and
//! `stateDiagram-colorIndex.spec.ts`.
//!
//! Kept as upstream wrote them: inputs and expectations. Skipped, and why:
//! - `stateDb.spec.js` "should have functions used in flow JISON as own property": asserts a
//!   JavaScript binding detail (methods bound as own properties), not database behavior.
//! - `stateDiagram.spec.js` "should handle if statements": upstream itself marks it `it.skip`
//!   (an unimplemented grammar), so it is ported as an ignored test.
//! - `stateRenderer.spec.js`, `stateRenderer-v3-unified.spec.js`: rendering.
//! - Assertions about configured values that `StateDB.getData` only passes through
//!   (`look`, `wrappingWidth`, `minNodeWidth`) are absent because this crate owns no config.

use super::ast::{
    Ast, Direction, EdgePattern, Node, NotePosition, Relation, State, StateKind, Statement,
};
use super::db::Db;
use super::parse;

fn ast(body: &str) -> Ast {
    parse(body).unwrap_or_else(|error| panic!("expected a parse, got {error}"))
}

fn state<'a>(ast: &'a Ast, id: &str) -> &'a State {
    ast.states
        .iter()
        .find(|state| state.id == id)
        .unwrap_or_else(|| panic!("state {id} not found"))
}

fn node<'a>(ast: &'a Ast, id: &str) -> &'a Node {
    ast.nodes
        .iter()
        .find(|node| node.id == id)
        .unwrap_or_else(|| panic!("node {id} not found"))
}

fn relation<'a>(ast: &'a Ast, id1: &str, id2: &str) -> &'a Relation {
    ast.relations
        .iter()
        .find(|relation| relation.id1 == id1 && relation.id2 == id2)
        .unwrap_or_else(|| panic!("relation {id1} --> {id2} not found"))
}

fn doc_state(state: &State, index: usize) -> &super::ast::StateStatement {
    match &state.doc.as_ref().expect("state has a document")[index] {
        Statement::State(state) => state,
        other => panic!("expected a state statement, got {other:?}"),
    }
}

// stateDb.spec.js

#[test]
fn add_style_class_is_added_to_the_list_of_style_classes() {
    let mut db = Db::new();
    db.add_style_class("newStyleClass", "font-weight:bold, border:blue;");
    let class = db
        .classes()
        .iter()
        .find(|class| class.id == "newStyleClass")
        .expect("style class");
    assert_eq!(class.id, "newStyleClass");
    assert_eq!(class.styles.len(), 2);
    assert_eq!(class.styles[0], "font-weight:bold");
    assert_eq!(class.styles[1], "border:blue");
}

#[test]
fn add_description_removes_only_the_first_leading_colon() {
    let mut db = Db::new();
    db.add_state("state1", StateKind::Default, None, &[], None, &[], &[], &[]);
    let id = "state1";
    let rest = "rest of the description";
    let one_leading_colon = format!(":{rest}");
    let two_leading_colons = format!("::{rest}");

    db.add_description(id, rest);
    assert_eq!(db.state(id).unwrap().descriptions[0], rest);

    db.add_description(id, &one_leading_colon);
    assert_eq!(db.state(id).unwrap().descriptions[1], rest);

    db.add_description(id, &two_leading_colons);
    assert_eq!(db.state(id).unwrap().descriptions[2], format!(":{rest}"));
}

#[test]
fn add_description_adds_each_description_to_the_array_of_descriptions() {
    let mut db = Db::new();
    db.add_state("state1", StateKind::Default, None, &[], None, &[], &[], &[]);
    db.add_description("state1", "description 0");
    db.add_description("state1", "description 1");
    db.add_description("state1", "description 2");

    let descriptions = &db.state("state1").unwrap().descriptions;
    assert_eq!(descriptions.len(), 3);
    assert_eq!(descriptions[0], "description 0");
    assert_eq!(descriptions[1], "description 1");
    assert_eq!(descriptions[2], "description 2");
}

#[test]
fn add_description_sanitizes_the_description() {
    let mut db = Db::new();
    db.add_state("state1", StateKind::Default, None, &[], None, &[], &[], &[]);
    db.add_description(
        "state1",
        "desc outside the script <script>the description</script>",
    );
    assert_eq!(
        db.state("state1").unwrap().descriptions[0],
        "desc outside the script "
    );
}

#[test]
fn add_description_adds_the_description_to_the_state_with_the_given_id() {
    let mut db = Db::new();
    db.add_state("state1", StateKind::Default, None, &[], None, &[], &[], &[]);
    db.add_description("state1", "the description");
    assert_eq!(
        db.state("state1").unwrap().descriptions[0],
        "the description"
    );
}

// stateDiagram.spec.js

#[test]
fn v1_super_simple() {
    ast("\n      stateDiagram\n        [*] --> State1\n        State1 --> [*]\n      ");
}

#[test]
fn v1_simple() {
    let parsed = ast(
        "stateDiagram\n\n          State1 : this is another string\n          [*] --> State1\n          State1 --> [*]\n      ",
    );
    assert_eq!(parsed.acc_description, "");
}

#[test]
fn v1_simple_with_acc_description() {
    let parsed = ast(
        "stateDiagram\n\n          accDescr: a simple description of the diagram\n          State1 : this is another string\n          [*] --> State1\n          State1 --> [*]\n      ",
    );
    assert_eq!(
        parsed.acc_description,
        "a simple description of the diagram"
    );
}

#[test]
fn v1_simple_with_multiline_acc_description() {
    let parsed = ast(
        "stateDiagram\n\n          accDescr {\n             a simple description of the diagram\n            using multiple lines\n          }\n          State1 : this is another string\n          [*] --> State1\n          State1 --> [*]\n      ",
    );
    assert_eq!(
        parsed.acc_description,
        "a simple description of the diagram\nusing multiple lines"
    );
}

#[test]
fn v1_simple_with_title() {
    let parsed = ast(
        "stateDiagram\n\n          accTitle: a simple title of the diagram\n          State1 : this is another string\n          [*] --> State1\n          State1 --> [*]\n      ",
    );
    assert_eq!(parsed.acc_title, "a simple title of the diagram");
}

#[test]
fn v1_should_handle_relation_definitions() {
    ast(
        "stateDiagram\n\n        [*] --> State1\n        State1 --> [*]\n        State1 : this is a string\n        State1 : this is another string\n\n        State1 --> State2\n        State2 --> [*]\n      ",
    );
}

#[test]
fn v1_hide_empty_description() {
    ast(
        "stateDiagram\n\n        hide empty description\n        [*] --> State1\n        State1 --> [*]\n        State1 : this is a string\n        State1 : this is another string\n\n        State1 --> State2\n        State2 --> [*]\n      ",
    );
}

#[test]
fn v1_handle_as_in_state_names() {
    ast("stateDiagram\n      assemble\n      state assemble\n      ");
}

#[test]
fn v1_handle_as_in_state_names_1() {
    ast("stateDiagram\n      assemble\n      state assemble\n      ");
}

#[test]
fn v1_handle_as_in_state_names_2() {
    ast("stateDiagram\n      assemblies\n      state assemblies\n      ");
}

#[test]
fn v1_handle_as_in_state_names_3() {
    ast("stateDiagram\n      state \"as\" as as\n      ");
}

#[test]
fn v1_scale() {
    ast(
        "stateDiagram\n\n        scale 350 width\n        [*] --> State1\n        State1 --> [*]\n        State1 : this is a string with - in it\n        State1 : this is another string\n\n        State1 --> State2\n        State2 --> [*]\n      ",
    );
}

#[test]
fn v1_description_after_second_state() {
    ast(
        "stateDiagram\n\n        scale 350 width\n        [*] --> State1 : This is the description with - in it\n        State1 --> [*]\n      ",
    );
}

#[test]
fn v1_shall_handle_descriptions_including_minus_signs() {
    ast(
        "stateDiagram\n\n        scale 350 width\n        [*] --> State1 : This is the description +-!\n        State1 --> [*]\n      ",
    );
}

#[test]
fn v1_should_handle_state_statements() {
    ast(
        "stateDiagram\n\n        state Configuring {\n          [*] --> NewValueSelection\n          NewValueSelection --> NewValuePreview : EvNewValue\n          NewValuePreview --> NewValueSelection : EvNewValueRejected\n          NewValuePreview --> NewValueSelection : EvNewValueSaved1\n        }\n      ",
    );
}

#[test]
fn v1_should_handle_recursive_state_definitions() {
    ast(
        "stateDiagram\n\n        state Configuring {\n          [*] --> NewValueSelection\n          NewValueSelection --> NewValuePreview : EvNewValue\n          NewValuePreview --> NewValueSelection : EvNewValueRejected\n          NewValuePreview --> NewValueSelection : EvNewValueSaved\n\n          state NewValuePreview {\n          State1 --> State2\n          }\n        }\n      ",
    );
}

#[test]
fn v1_should_handle_multiple_recursive_state_definitions() {
    ast(
        "stateDiagram\n\n        scale 350 width\n        [*] --> NotShooting\n\n        state NotShooting {\n          [*] --> Idle\n          Idle --> Configuring : EvConfig\n          Configuring --> Idle : EvConfig\n        }\n\n        state Configuring {\n          [*] --> NewValueSelection\n          NewValueSelection --> NewValuePreview : EvNewValue\n          NewValuePreview --> NewValueSelection : EvNewValueRejected\n          NewValuePreview --> NewValueSelection : EvNewValueSaved\n\n          state NewValuePreview {\n          State1 --> State2\n          }\n        }\n      ",
    );
}

#[test]
fn v1_should_handle_state_definitions_with_separation_of_id() {
    ast("stateDiagram\n\n        state \"Long state description\" as state1\n        ");
}

#[test]
fn v1_should_handle_state_definitions_with_separation_of_id_and_a_document() {
    ast(
        "stateDiagram\n      state \"Not Shooting State\" as NotShooting {\n        state \"Idle mode\" as Idle\n        state \"Configuring mode\" as Configuring\n        [*] --> Idle\n        Idle --> Configuring : EvConfig\n        Configuring --> Idle : EvConfig\n      }\n        ",
    );
}

#[test]
fn v1_should_handle_state_definition_with_quotes() {
    ast(
        "stateDiagram\n\n        scale 600 width\n\n        [*] --> State1\n        State1 --> State2 : Succeeded\n        State1 --> [*] : Aborted\n        State2 --> State3 : Succeeded\n        State2 --> [*] : Aborted\n        state State3 {\n          state \"Accumulate Enough Data\nLong State Name\" as long1\n          long1 : Just a test\n          [*] --> long1\n          long1 --> long1 : New Data\n          long1 --> ProcessData : Enough Data\n        }\n        State3 --> State3 : Failed\n        State3 --> [*] : Succeeded / Save Result\n        State3 --> [*] : Aborted\n      ",
    );
}

#[test]
fn v1_should_handle_fork_statements() {
    ast(
        "stateDiagram\n\n        state fork_state <<fork>>\n        [*] --> fork_state\n        fork_state --> State2\n        fork_state --> State3\n\n        state join_state <<join>>\n        State2 --> join_state\n        State3 --> join_state\n        join_state --> State4\n        State4 --> [*]\n      ",
    );
}

#[test]
fn v1_should_handle_concurrent_state() {
    ast(
        "stateDiagram\n\n        [*] --> Active\n\n        state Active {\n          [*] --> NumLockOff\n          NumLockOff --> NumLockOn : EvNumLockPressed\n          NumLockOn --> NumLockOff : EvNumLockPressed\n          --\n          [*] --> CapsLockOff\n          CapsLockOff --> CapsLockOn : EvCapsLockPressed\n          CapsLockOn --> CapsLockOff : EvCapsLockPressed\n          --\n          [*] --> ScrollLockOff\n          ScrollLockOff --> ScrollLockOn : EvCapsLockPressed\n          ScrollLockOn --> ScrollLockOff : EvCapsLockPressed\n        }\n      ",
    );
}

#[test]
fn v1_should_handle_concurrent_state_without_relations() {
    ast(
        "stateDiagram\n\n        [*] --> Active\n\n        state Active {\n          [*] --> NumLockOff\n          --\n          [*] --> CapsLockOff\n          --\n          [*] --> ScrollLockOff\n        }\n      ",
    );
}

#[test]
fn v1_should_handle_note_statements() {
    ast(
        "stateDiagram\n\n        [*] --> Active\n        Active --> Inactive\n\n        note left of Active : this is a short<br/>note\n\n        note right of Inactive\n          A note can also\n          be defined on\n          several lines\n        end note\n      ",
    );
}

#[test]
fn v1_should_handle_multiline_notes_with_different_line_breaks() {
    ast(
        "stateDiagram\n        State1\n        note right of State1\n          Line1<br>Line2<br/>Line3<br />Line4<br\t/>Line5\n        end note\n      ",
    );
}

#[test]
fn v1_should_handle_floating_notes() {
    ast("stateDiagram\n        foo: bar\n        note \"This is a floating note\" as N1\n      ");
}

#[test]
fn v1_should_handle_floating_notes_after_a_state() {
    ast(
        "stateDiagram\n\n        state foo\n        note \"This is a floating note\" as N1\n      ",
    );
}

#[test]
fn v1_should_handle_notes_for_composite_states() {
    ast(
        "stateDiagram\n\n        [*] --> NotShooting\n\n        state \"Not Shooting State\" as NotShooting {\n          state \"Idle mode\" as Idle\n          state \"Configuring mode\" as Configuring\n          [*] --> Idle\n          Idle --> Configuring : EvConfig\n          Configuring --> Idle : EvConfig\n        }\n\n        note right of NotShooting : This is a note on a composite state\n      ",
    );
}

#[test]
#[ignore = "upstream marks this grammar as unimplemented (it.skip)"]
fn v1_should_handle_if_statements() {
    ast(
        "stateDiagram\n\n      [*] --> \"Order Submitted\"\n      if \"Payment Accepted\" then\n        -->[yes] \"Pack products\"\n        --> \"Send parcel\"\n        -right-> (*)\n      else\n        ->[no] \"Send error message\"\n        -->[Cancel Order] [*]\n      endif\n        }\n\n        note right of NotShooting : This is a note on a composite state\n      ",
    );
}

#[test]
fn click_directive_stores_links_from_click_statements_passed_to_extract() {
    let mut db = Db::new();
    db.add_state("S1", StateKind::Default, None, &[], None, &[], &[], &[]);
    let mut statements = vec![Statement::Click {
        id: "S1".to_string(),
        url: "https://example.com".to_string(),
        tooltip: "Go to Example".to_string(),
    }];
    db.extract(&mut statements).expect("extract");

    let link = db.links().iter().find(|link| link.id == "S1").unwrap();
    assert_eq!(link.url, "https://example.com");
    assert_eq!(link.tooltip, "Go to Example");
}

#[test]
fn click_directive_is_parsed_from_the_grammar() {
    let parsed = ast("stateDiagram-v2\n      click S1 \"https://example.com\" \"Go to Example\"\n");
    let link = &parsed.links[0];
    assert_eq!(link.id, "S1");
    assert_eq!(link.url, "\"https://example.com\"");
    assert_eq!(link.tooltip, "\"Go to Example\"");
}

// stateDiagram-v2.spec.js

#[test]
fn v2_super_simple() {
    ast("\n      stateDiagram-v2\n      [*] --> State1\n      State1 --> [*]\n      ");
}

#[test]
fn v2_simple() {
    let parsed = ast(
        "stateDiagram-v2\n\n      State1 : this is another string\n      [*] --> State1\n      State1 --> [*]\n      ",
    );
    assert_eq!(parsed.acc_description, "");
}

#[test]
fn v2_simple_with_acc_description() {
    let parsed = ast(
        "stateDiagram-v2\n\n      accDescr: a simple description of the diagram\n      State1 : this is another string\n      [*] --> State1\n      State1 --> [*]\n      ",
    );
    assert_eq!(
        parsed.acc_description,
        "a simple description of the diagram"
    );
}

#[test]
fn v2_simple_with_title() {
    let parsed = ast(
        "stateDiagram-v2\n\n      accTitle: a simple title of the diagram\n      State1 : this is another string\n      [*] --> State1\n      State1 --> [*]\n      ",
    );
    assert_eq!(parsed.acc_title, "a simple title of the diagram");
}

#[test]
fn v2_should_handle_relation_definitions() {
    ast(
        "stateDiagram-v2\n\n      [*] --> State1\n      State1 --> [*]\n      State1 : this is a string\n      State1 : this is another string\n\n      State1 --> State2\n      State2 --> [*]\n      ",
    );
}

#[test]
fn v2_hide_empty_description() {
    ast(
        "stateDiagram-v2\n\n      hide empty description\n      [*] --> State1\n      State1 --> [*]\n      State1 : this is a string\n      State1 : this is another string\n\n      State1 --> State2\n      State2 --> [*]\n      ",
    );
}

#[test]
fn v2_handle_as_in_state_names() {
    ast("stateDiagram-v2\n      assemble\n      state assemble\n      ");
}

#[test]
fn v2_handle_as_in_state_names_1() {
    ast("stateDiagram-v2\n      assemble\n      state assemble\n      ");
}

#[test]
fn v2_handle_as_in_state_names_2() {
    ast("stateDiagram-v2\n      assemblies\n      state assemblies\n      ");
}

#[test]
fn v2_handle_as_in_state_names_3() {
    ast("stateDiagram-v2\n      state \"as\" as as\n      ");
}

#[test]
fn v2_simple_states_with_colon_labels() {
    let parsed = ast(
        "\n          stateDiagram-v2\n          [*] --> State1\n          State1 --> State2 : Transition 1\n          State1 --> State3 : Transition 2\n          State1 --> State4 : Transition 3\n          State1 --> [*]\n        ",
    );
    assert_eq!(
        relation(&parsed, "State1", "State2").relation_title,
        "Transition 1"
    );
    assert_eq!(
        relation(&parsed, "State1", "State3").relation_title,
        "Transition 2"
    );
    assert_eq!(
        relation(&parsed, "State1", "State4").relation_title,
        "Transition 3"
    );
}

#[test]
fn v2_scale() {
    ast(
        "stateDiagram-v2\n\n      scale 350 width\n      [*] --> State1\n      State1 --> [*]\n      State1 : this is a string with - in it\n      State1 : this is another string\n\n      State1 --> State2\n      State2 --> [*]\n      ",
    );
}

#[test]
fn v2_description_after_second_state() {
    ast(
        "stateDiagram-v2\n\n      scale 350 width\n      [*] --> State1 : This is the description with - in it\n      State1 --> [*]\n      ",
    );
}

#[test]
fn v2_shall_handle_descriptions_including_minus_signs() {
    ast(
        "stateDiagram-v2\n\n      scale 350 width\n      [*] --> State1 : This is the description +-!\n      State1 --> [*]\n      ",
    );
}

#[test]
fn v2_should_handle_state_statements() {
    ast(
        "stateDiagram-v2\n\n      state Configuring {\n        [*] --> NewValueSelection\n        NewValueSelection --> NewValuePreview : EvNewValue\n        NewValuePreview --> NewValueSelection : EvNewValueRejected\n        NewValuePreview --> NewValueSelection : EvNewValueSaved1\n      }\n      ",
    );
}

#[test]
fn v2_should_handle_recursive_state_definitions() {
    ast(
        "stateDiagram-v2\n\n      state Configuring {\n        [*] --> NewValueSelection\n        NewValueSelection --> NewValuePreview : EvNewValue\n        NewValuePreview --> NewValueSelection : EvNewValueRejected\n        NewValuePreview --> NewValueSelection : EvNewValueSaved\n\n        state NewValuePreview {\n          State1 --> State2\n        }\n      }\n      ",
    );
}

#[test]
fn v2_should_handle_multiple_recursive_state_definitions() {
    ast(
        "stateDiagram-v2\n\n      scale 350 width\n      [*] --> NotShooting\n\n      state NotShooting {\n        [*] --> Idle\n        Idle --> Configuring : EvConfig\n        Configuring --> Idle : EvConfig\n      }\n\n      state Configuring {\n        [*] --> NewValueSelection\n        NewValueSelection --> NewValuePreview : EvNewValue\n        NewValuePreview --> NewValueSelection : EvNewValueRejected\n        NewValuePreview --> NewValueSelection : EvNewValueSaved\n\n        state NewValuePreview {\n          State1 --> State2\n        }\n      }\n      ",
    );
}

#[test]
fn v2_should_handle_state_definitions_with_separation_of_id() {
    ast("stateDiagram-v2\n\n      state \"Long state description\" as state1\n      ");
}

#[test]
fn v2_should_handle_state_definitions_with_separation_of_id_and_a_document() {
    ast(
        "stateDiagram-v2\n      state \"Not Shooting State\" as NotShooting {\n        state \"Idle mode\" as Idle\n        state \"Configuring mode\" as Configuring\n        [*] --> Idle\n        Idle --> Configuring : EvConfig\n        Configuring --> Idle : EvConfig\n      }\n      ",
    );
}

#[test]
fn v2_should_handle_state_definition_with_quotes() {
    ast(
        "stateDiagram-v2\n\n      scale 600 width\n\n      [*] --> State1\n      State1 --> State2 : Succeeded\n      State1 --> [*] : Aborted\n      State2 --> State3 : Succeeded\n      State2 --> [*] : Aborted\n      state State3 {\n        state \"Accumulate Enough Data\nLong State Name\" as long1\n        long1 : Just a test\n        [*] --> long1\n        long1 --> long1 : New Data\n        long1 --> ProcessData : Enough Data\n      }\n      State3 --> State3 : Failed\n      State3 --> [*] : Succeeded / Save Result\n      State3 --> [*] : Aborted\n      ",
    );
}

#[test]
fn v2_should_handle_fork_statements() {
    ast(
        "stateDiagram-v2\n\n      state fork_state <<fork>>\n      [*] --> fork_state\n      fork_state --> State2\n      fork_state --> State3\n\n      state join_state <<join>>\n      State2 --> join_state\n      State3 --> join_state\n      join_state --> State4\n      State4 --> [*]\n      ",
    );
}

#[test]
fn v2_should_handle_concurrent_state() {
    ast(
        "stateDiagram-v2\n\n      [*] --> Active\n\n      state Active {\n        [*] --> NumLockOff\n        NumLockOff --> NumLockOn : EvNumLockPressed\n        NumLockOn --> NumLockOff : EvNumLockPressed\n        --\n        [*] --> CapsLockOff\n        CapsLockOff --> CapsLockOn : EvCapsLockPressed\n        CapsLockOn --> CapsLockOff : EvCapsLockPressed\n        --\n        [*] --> ScrollLockOff\n        ScrollLockOff --> ScrollLockOn : EvCapsLockPressed\n        ScrollLockOn --> ScrollLockOff : EvCapsLockPressed\n      }\n      ",
    );
}

#[test]
fn v2_should_handle_concurrent_state_without_relations() {
    ast(
        "stateDiagram-v2\n\n      [*] --> Active\n\n      state Active {\n        [*] --> NumLockOff\n        --\n        [*] --> CapsLockOff\n        --\n        [*] --> ScrollLockOff\n      }\n      ",
    );
}

#[test]
fn v2_should_handle_note_statements() {
    let parsed = ast(
        "stateDiagram-v2\n\n      [*] --> Active\n      Active --> Inactive\n\n      note left of Active : this is a short<br/>note\n\n      note right of Inactive\n      A note can also\n      be defined on\n      several lines\n      end note\n      ",
    );
    let active = state(&parsed, "Active").note.as_ref().expect("note");
    assert_eq!(active.position, Some(NotePosition::LeftOf));
    assert_eq!(active.text, "this is a short<br>note");
    let inactive = state(&parsed, "Inactive").note.as_ref().expect("note");
    assert_eq!(inactive.position, Some(NotePosition::RightOf));
    assert_eq!(
        inactive.text,
        "A note can also\n      be defined on\n      several lines"
    );
}

#[test]
fn v2_should_handle_multiline_notes_with_different_line_breaks() {
    let parsed = ast(
        "stateDiagram-v2\n      State1\n      note right of State1\n      Line1<br>Line2<br/>Line3<br />Line4<br\t/>Line5\n      end note\n      ",
    );
    let note = state(&parsed, "State1").note.as_ref().expect("note");
    assert_eq!(note.position, Some(NotePosition::RightOf));
    assert_eq!(note.text, "Line1<br>Line2<br>Line3<br>Line4<br>Line5");
}

#[test]
fn v2_should_handle_floating_notes() {
    ast("stateDiagram-v2\n      foo: bar\n      note \"This is a floating note\" as N1\n      ");
}

#[test]
fn v2_should_handle_floating_notes_after_a_state() {
    ast("stateDiagram-v2\n      state foo\n      note \"This is a floating note\" as N1\n      ");
}

#[test]
fn v2_should_handle_notes_for_composite_states() {
    let parsed = ast(
        "stateDiagram-v2\n      [*] --> NotShooting\n\n      state \"Not Shooting State\" as NotShooting {\n        state \"Idle mode\" as Idle\n        state \"Configuring mode\" as Configuring\n        [*] --> Idle\n        Idle --> Configuring : EvConfig\n        Configuring --> Idle : EvConfig\n      }\n\n      note right of NotShooting : This is a note on a composite state\n      ",
    );
    let note = state(&parsed, "NotShooting").note.as_ref().expect("note");
    assert_eq!(note.position, Some(NotePosition::RightOf));
    assert_eq!(note.text, "This is a note on a composite state");
}

#[test]
fn v2_a_composite_state_should_be_able_to_link_to_itself() {
    let parsed = ast(
        "\n          stateDiagram-v2\n            state Active {\n              Idle\n            }\n            Inactive --> Idle: ACT\n            Active --> Active: LOG\n        ",
    );

    assert_eq!(doc_state(state(&parsed, "Active"), 0).id, "Idle");
    assert_eq!(relation(&parsed, "Inactive", "Idle").relation_title, "ACT");
    assert_eq!(relation(&parsed, "Active", "Active").relation_title, "LOG");
}

#[test]
fn v2_should_check_default_diagram_direction() {
    let parsed =
        ast("\n        stateDiagram\n          [*] --> Still\n          Still --> [*]\n        ");
    assert_eq!(parsed.direction, Direction::Tb);
}

#[test]
fn v2_retrieve_the_diagram_direction_correctly() {
    let parsed = ast(
        "\n        stateDiagram\n          direction LR\n          [*] --> Still\n          Still --> [*]\n        ",
    );
    assert_eq!(parsed.direction, Direction::Lr);
    assert_eq!(parsed.layout_direction, Direction::Lr);
}

// parser/state-parser.spec.js

#[test]
fn parser_should_only_accept_one_word_between_state_and_curly_bracket() {
    let parsed = ast("stateDiagram-v2\n        state valid { X }");
    assert!(parsed.states.iter().any(|state| state.id == "valid"));
}

#[test]
fn parser_should_throw_with_two_words() {
    let error = parse("stateDiagram-v2\n        state invalid syntax { Y }").unwrap_err();
    assert!(
        error
            .message
            .contains("Error: State name must be a single word."),
        "{}",
        error.message
    );
}

#[test]
fn parser_should_throw_with_more_than_two_words() {
    let error = parse("stateDiagram-v2\n        state invalid syntax with more than 2 words { Z }")
        .unwrap_err();
    assert!(
        error
            .message
            .contains("Error: State name must be a single word."),
        "{}",
        error.message
    );
}

#[test]
fn parser_state_id_as_quoted_name() {
    let parsed = ast("stateDiagram-v2\n        state \"Small State 1\" as namedState1");
    assert_eq!(
        state(&parsed, "namedState1").descriptions,
        ["Small State 1"]
    );
}

#[test]
fn parser_colon_after_the_id_with_spaces() {
    let parsed = ast("stateDiagram-v2\n        namedState1 : Small State 1");
    assert_eq!(
        state(&parsed, "namedState1").descriptions,
        ["Small State 1"]
    );
}

#[test]
fn parser_colon_after_the_id_without_spaces() {
    let parsed = ast("stateDiagram-v2\n        namedState1:Small State 1");
    assert_eq!(
        state(&parsed, "namedState1").descriptions,
        ["Small State 1"]
    );
}

#[test]
fn parser_handles_assemble_and_assemblies() {
    let parsed = ast(
        "stateDiagram-v2\n      assemble\n      assemblies\n      state assemble\n      state assemblies\n      ",
    );
    assert!(parsed.states.iter().any(|state| state.id == "assemble"));
    assert!(parsed.states.iter().any(|state| state.id == "assemblies"));
}

#[test]
fn parser_state_as_as_as() {
    let parsed = ast("stateDiagram-v2\n      state \"as\" as as\n      ");
    assert_eq!(state(&parsed, "as").descriptions, ["as"]);
}

#[test]
fn parser_groups_with_as_and_without() {
    let parsed = ast(
        "stateDiagram-v2\n        state \"Small State 1\" as namedState1\n        %% Notice that this is named \"Big State 1\" with an \"as\"\n        state \"Big State 1\" as bigState1 {\n            bigState1InternalState\n        }\n        namedState1 --> bigState1: should point to \\nBig State 1 container\n\n        state \"Small State 2\" as namedState2\n        %% Notice that bigState2 does not have a name; no \"as\"\n        state bigState2 {\n            bigState2InternalState\n        }\n        namedState2 --> bigState2: should point to \\nbigState2 container",
    );
    assert!(parsed.states.iter().any(|state| state.id == "namedState1"));
    assert!(parsed.states.iter().any(|state| state.id == "bigState1"));
    assert_eq!(
        doc_state(state(&parsed, "bigState1"), 0).id,
        "bigState1InternalState"
    );
    assert!(parsed.states.iter().any(|state| state.id == "namedState2"));
    assert!(parsed.states.iter().any(|state| state.id == "bigState2"));
    assert_eq!(
        doc_state(state(&parsed, "bigState2"), 0).id,
        "bigState2InternalState"
    );
    assert_eq!(parsed.relations[0].id1, "namedState1");
    assert_eq!(parsed.relations[0].id2, "bigState1");
    assert_eq!(parsed.relations[1].id1, "namedState2");
    assert_eq!(parsed.relations[1].id2, "bigState2");
}

#[test]
fn parser_group_with_state_id_as_and_state_id_colon_name() {
    let parsed = ast(
        "stateDiagram-v2\n        state \"Big State 1\" as bigState1 {\n            state \"inner state 1\" as inner1\n            inner2: inner state 2\n            inner1 --> inner2\n        }",
    );
    let group = state(&parsed, "bigState1");
    let first = doc_state(group, 0);
    assert_eq!(first.id, "inner1");
    assert_eq!(first.description, ["inner state 1"]);
    let second = doc_state(group, 1);
    assert_eq!(second.id, "inner2");
    assert_eq!(second.description, ["inner state 2"]);
}

#[test]
fn parser_allows_a_colon_inside_transition_text() {
    let parsed =
        ast("stateDiagram-v2\n        locked --> pending : recoverable error (ex: timeout)");
    assert_eq!(parsed.relations.len(), 1);
    assert_eq!(parsed.relations[0].id1, "locked");
    assert_eq!(parsed.relations[0].id2, "pending");
    assert_eq!(
        parsed.relations[0].relation_title,
        "recoverable error (ex: timeout)"
    );
}

#[test]
fn parser_allows_multiple_colons_inside_transition_text() {
    let parsed = ast("stateDiagram-v2\n        A --> B : info: key: value");
    assert_eq!(parsed.relations.len(), 1);
    assert_eq!(parsed.relations[0].relation_title, "info: key: value");
}

#[test]
fn parser_allows_a_colon_inside_state_description_text() {
    let parsed = ast("stateDiagram-v2\n        myState : status: active");
    assert_eq!(state(&parsed, "myState").descriptions, ["status: active"]);
}

#[test]
fn parser_allows_proto_as_a_state_name() {
    let parsed = ast("\nstateDiagram-v2\n[*] --> __proto__\n__proto__ --> [*]");
    assert!(parsed.states.iter().any(|state| state.id == "__proto__"));
}

#[test]
fn parser_allows_constructor_as_a_state_name() {
    let parsed = ast("\nstateDiagram-v2\n[*] --> constructor\nconstructor --> [*]");
    assert!(parsed.states.iter().any(|state| state.id == "constructor"));
}

#[test]
fn parser_does_not_terminate_a_note_on_send_note_inside_it() {
    let parsed = ast(
        "stateDiagram-v2\n      State1\n      note right of State1\n        this sentence contains send note inside the note text\n      end note\n      State1 --> State2",
    );
    assert_eq!(parsed.relations.len(), 1);
    assert_eq!(parsed.relations[0].id1, "State1");
    assert_eq!(parsed.relations[0].id2, "State2");
    let note = state(&parsed, "State1").note.as_ref().expect("note");
    assert!(note.text.contains("send note inside the note text"));
}

#[test]
fn parser_does_not_treat_end_note_inside_the_text_as_the_closing_keyword() {
    let parsed = ast(
        "stateDiagram-v2\n      State1\n      note right of State1\n        this sentence contains end note as part of the note text\n      end note\n      State1 --> State2",
    );
    assert_eq!(parsed.relations.len(), 1);
    assert_eq!(parsed.relations[0].id1, "State1");
    assert_eq!(parsed.relations[0].id2, "State2");
    let note = state(&parsed, "State1").note.as_ref().expect("note");
    assert!(note.text.contains("end note as part of the note text"));
}

// parser/state-style.spec.js

#[test]
fn style_class_def_has_an_id_and_one_attribute() {
    let parsed = ast("stateDiagram-v2\n classDef exampleClass background:#bbb;");
    let class = parsed
        .classes
        .iter()
        .find(|class| class.id == "exampleClass")
        .expect("class");
    assert_eq!(class.styles, ["background:#bbb"]);
}

#[test]
fn style_class_def_can_define_multiple_attributes() {
    let parsed = ast(
        "stateDiagram-v2\n classDef exampleClass background:#bbb, font-weight:bold, font-style:italic;",
    );
    let class = parsed
        .classes
        .iter()
        .find(|class| class.id == "exampleClass")
        .expect("class");
    assert_eq!(class.styles.len(), 3);
    assert_eq!(class.styles[0], "background:#bbb");
    assert_eq!(class.styles[1], "font-weight:bold");
    assert_eq!(class.styles[2], "font-style:italic");
}

#[test]
fn style_class_def_attribute_can_have_a_dot() {
    let parsed =
        ast("stateDiagram-v2\n classDef exampleStyleClass background:#bbb,border:1.5px solid red;");
    let class = parsed
        .classes
        .iter()
        .find(|class| class.id == "exampleStyleClass")
        .expect("class");
    assert_eq!(class.styles.len(), 2);
    assert_eq!(class.styles[0], "background:#bbb");
    assert_eq!(class.styles[1], "border:1.5px solid red");
}

#[test]
fn style_class_def_attribute_can_have_a_space() {
    let parsed = ast(
        "stateDiagram-v2\n classDef exampleStyleClass background:  #bbb,border:1.5px solid red;",
    );
    let class = parsed
        .classes
        .iter()
        .find(|class| class.id == "exampleStyleClass")
        .expect("class");
    assert_eq!(class.styles.len(), 2);
    assert_eq!(class.styles[0], "background:  #bbb");
    assert_eq!(class.styles[1], "border:1.5px solid red");
}

#[test]
fn style_class_def_can_have_proto_and_constructor_names() {
    let parsed = ast(
        "stateDiagram-v2\n classDef __proto__ background:#bbb,border:1.5px solid red;\n classDef constructor background:#bbb,border:1.5px solid red;",
    );
    assert_eq!(
        parsed
            .classes
            .iter()
            .find(|class| class.id == "__proto__")
            .expect("class")
            .styles
            .len(),
        2
    );
    assert_eq!(
        parsed
            .classes
            .iter()
            .find(|class| class.id == "constructor")
            .expect("class")
            .styles
            .len(),
        2
    );
}

#[test]
fn style_can_apply_a_class_to_a_state() {
    let parsed = ast(
        "stateDiagram-v2\n\nclassDef exampleStyleClass background:#bbb,border:1px solid red;\na --> b class a exampleStyleClass",
    );
    let class = parsed
        .classes
        .iter()
        .find(|class| class.id == "exampleStyleClass")
        .expect("class");
    assert_eq!(class.styles.len(), 2);
    assert_eq!(class.styles[0], "background:#bbb");
    assert_eq!(class.styles[1], "border:1px solid red");
    assert_eq!(state(&parsed, "a").classes, ["exampleStyleClass"]);
}

#[test]
fn style_can_apply_a_class_to_a_state_with_an_underscore() {
    let parsed = ast(
        "stateDiagram-v2\n\nclassDef exampleStyleClass background:#bbb,border:1px solid red;\na_a --> b_b\nclass a_a exampleStyleClass",
    );
    let class = parsed
        .classes
        .iter()
        .find(|class| class.id == "exampleStyleClass")
        .expect("class");
    assert_eq!(class.styles.len(), 2);
    assert_eq!(class.styles[0], "background:#bbb");
    assert_eq!(class.styles[1], "border:1px solid red");
    assert_eq!(state(&parsed, "a_a").classes, ["exampleStyleClass"]);
}

#[test]
fn style_separator_applies_a_class_to_a_state() {
    let parsed = ast(
        "stateDiagram-v2\n\nclassDef exampleStyleClass background:#bbb,border:1px solid red;\na --> b:::exampleStyleClass\n",
    );
    let class = parsed
        .classes
        .iter()
        .find(|class| class.id == "exampleStyleClass")
        .expect("class");
    assert_eq!(class.styles.len(), 2);
    assert_eq!(class.styles[0], "background:#bbb");
    assert_eq!(class.styles[1], "border:1px solid red");
    assert_eq!(state(&parsed, "b").classes, ["exampleStyleClass"]);
}

#[test]
fn style_separator_applies_a_class_to_a_start_state() {
    let parsed = ast(
        "stateDiagram-v2\n\nclassDef exampleStyleClass background:#bbb,border:1px solid red;\n[*]:::exampleStyleClass --> b\n",
    );
    let class = parsed
        .classes
        .iter()
        .find(|class| class.id == "exampleStyleClass")
        .expect("class");
    assert_eq!(class.styles.len(), 2);
    assert_eq!(class.styles[0], "background:#bbb");
    assert_eq!(class.styles[1], "border:1px solid red");
    assert_eq!(state(&parsed, "root_start").classes, ["exampleStyleClass"]);
}

#[test]
fn style_separator_applies_to_a_comma_separated_list_of_states() {
    let parsed = ast(
        "stateDiagram-v2\n\nclassDef exampleStyleClass background:#bbb,border:1px solid red;\na-->b\nclass a,b exampleStyleClass",
    );
    let class = parsed
        .classes
        .iter()
        .find(|class| class.id == "exampleStyleClass")
        .expect("class");
    assert_eq!(class.styles.len(), 2);
    assert_eq!(class.styles[0], "background:#bbb");
    assert_eq!(class.styles[1], "border:1px solid red");
    assert_eq!(state(&parsed, "a").classes, ["exampleStyleClass"]);
    assert_eq!(state(&parsed, "b").classes, ["exampleStyleClass"]);
}

#[test]
fn style_comma_separated_ids_may_or_may_not_have_spaces_after_the_commas() {
    let parsed = ast(
        "stateDiagram-v2\n\nclassDef exampleStyleClass background:#bbb,border:1px solid red;\na-->b\nclass a,b,c, d, e exampleStyleClass",
    );
    let class = parsed
        .classes
        .iter()
        .find(|class| class.id == "exampleStyleClass")
        .expect("class");
    assert_eq!(class.styles.len(), 2);
    assert_eq!(class.styles[0], "background:#bbb");
    assert_eq!(class.styles[1], "border:1px solid red");
    for id in ["a", "b", "c", "d", "e"] {
        assert_eq!(state(&parsed, id).classes, ["exampleStyleClass"]);
    }
}

#[test]
fn style_separator_applies_to_a_state_inside_a_composite_state() {
    let parsed = ast(
        "stateDiagram-v2\n\nclassDef exampleStyleClass background:#bbb,border:1px solid red;\nstate A {\n  a:::exampleStyleClass --> b\n}\n",
    );
    let a = state(&parsed, "A");
    let Statement::Relation(relation) = &a.doc.as_ref().unwrap()[0] else {
        panic!("expected a relation");
    };
    assert_eq!(relation.state1.classes, ["exampleStyleClass"]);
}

#[test]
fn style_separator_applies_to_a_start_state_inside_a_composite_state() {
    let parsed = ast(
        "stateDiagram-v2\n\nclassDef exampleStyleClass background:#bbb,border:1px solid red;\nstate A {\n  [*]:::exampleStyleClass --> a\n}\n",
    );
    let a = state(&parsed, "A");
    let Statement::Relation(relation) = &a.doc.as_ref().unwrap()[0] else {
        panic!("expected a relation");
    };
    assert_eq!(relation.state1.classes, ["exampleStyleClass"]);
}

#[test]
fn style_comments_work_inside_states() {
    let parsed = ast(
        "stateDiagram-v2\n\n[*] --> Moving\nMoving --> Still\nMoving --> Crash\nstate Moving {\n%% comment inside state\nslow  --> fast\n}\n",
    );
    assert_eq!(state(&parsed, "Moving").doc.as_ref().unwrap().len(), 1);
}

#[test]
fn style_should_handle_comments_correctly() {
    let parsed = ast(
        "stateDiagram-v2\n%%initial_comment\n[*] --> Moving %%inline_comment\n%%final_comment\n",
    );
    assert_eq!(parsed.states.len(), 2);
    for id in ["%%initial_comment", "%%inline_comment", "%%final_comment"] {
        assert!(parsed.states.iter().all(|state| state.id != id));
    }
}

#[test]
fn style_should_handle_comments_correctly_inside_states() {
    let parsed = ast(
        "stateDiagram-v2\nstate Moving {\n%%comment_inside_state\nslow  --> fast %%inline_comment\n}\n",
    );
    let doc = state(&parsed, "Moving").doc.as_ref().unwrap();
    assert!(doc.iter().all(|statement| match statement {
        Statement::State(state) => state.id != "%%comment_inside_state",
        _ => true,
    }));
    assert!(doc.iter().all(|statement| match statement {
        Statement::Relation(relation) => {
            relation.state1.id != "%%inline_comment" && relation.state2.id != "%%inline_comment"
        }
        _ => true,
    }));
}

#[test]
fn style_should_handle_comments_correctly_after_a_blank_line() {
    let parsed =
        ast("stateDiagram-v2\n[*] --> Moving\n\n%% comment after a blank line\nMoving --> Still\n");
    assert_eq!(parsed.states.len(), 3);
}

#[test]
fn style_should_parse_single_percent_as_normal_syntax_not_a_comment() {
    let parsed = ast("stateDiagram-v2\n% not a comment\nMoving --> Still %inline\n");
    assert_eq!(parsed.states.len(), 7);
    assert!(parsed.states.iter().any(|state| state.id == "%inline"));
}

#[test]
fn style_should_skip_multiple_consecutive_comment_lines() {
    let parsed = ast(
        "stateDiagram-v2\n%% first comment\n%% second comment\n%% third comment\n[*] --> Moving\n",
    );
    assert_eq!(parsed.states.len(), 2);
}

#[test]
fn style_transitions_should_be_preserved_when_separated_by_a_comment() {
    let parsed = ast("stateDiagram-v2\nMoving --> Crash\n%% comment between\nMoving --> Still\n");
    assert_eq!(parsed.states.len(), 3);
    assert_eq!(parsed.relations.len(), 2);
    assert_eq!(parsed.relations[0].id1, "Moving");
    assert_eq!(parsed.relations[0].id2, "Crash");
    assert_eq!(parsed.relations[1].id1, "Moving");
    assert_eq!(parsed.relations[1].id2, "Still");
}

#[test]
fn style_should_handle_inline_comments_in_state_syntax() {
    let parsed =
        ast("stateDiagram-v2\n[*] --> Moving%% this is a comment\nMoving --> Still%Active\n");
    assert!(parsed.states.iter().any(|state| state.id == "Moving"));
    assert!(parsed.states.iter().any(|state| state.id == "Still%Active"));
}

#[test]
fn style_statement_sets_a_css_style_on_a_state() {
    let parsed = ast("stateDiagram-v2\n        id1\n        style id1 background:#bbb");
    assert_eq!(node(&parsed, "id1").css_styles, ["background:#bbb"]);
}

#[test]
fn style_statement_sets_a_css_style_on_multiple_states() {
    let parsed =
        ast("stateDiagram-v2\n        id1\n        id2\n        style id1,id2 background:#bbb");
    assert_eq!(node(&parsed, "id1").css_styles, ["background:#bbb"]);
    assert_eq!(node(&parsed, "id2").css_styles, ["background:#bbb"]);
}

#[test]
fn style_statement_can_define_multiple_attributes() {
    let parsed = ast(
        "stateDiagram-v2\n        id1\n        id2\n        style id1,id2 background:#bbb, font-weight:bold, font-style:italic;",
    );
    assert_eq!(
        node(&parsed, "id1").css_styles,
        ["background:#bbb", "font-weight:bold", "font-style:italic"]
    );
    assert_eq!(
        node(&parsed, "id2").css_styles,
        ["background:#bbb", "font-weight:bold", "font-style:italic"]
    );
}

// stateNoteEdge.spec.ts

const NOTE_EDGE_DIAGRAM: &str = "stateDiagram-v2\n    [*] --> Active\n    Active --> Idle\n    note right of Active\n      A note\n    end note\n  ";

#[test]
fn note_edge_declares_the_dashed_pattern_on_the_edge() {
    let parsed = ast(NOTE_EDGE_DIAGRAM);
    let note_edge = parsed
        .edges
        .iter()
        .find(|edge| edge.classes.contains("note-edge"))
        .expect("note edge");
    assert_eq!(note_edge.pattern, Some(EdgePattern::Dashed));
}

#[test]
fn note_edge_leaves_ordinary_transitions_solid() {
    let parsed = ast(NOTE_EDGE_DIAGRAM);
    let transitions: Vec<_> = parsed
        .edges
        .iter()
        .filter(|edge| !edge.classes.contains("note-edge"))
        .collect();
    assert!(!transitions.is_empty());
    assert!(transitions.iter().all(|edge| edge.pattern.is_none()));
}

#[test]
fn note_edge_keeps_the_note_edge_class() {
    let parsed = ast(NOTE_EDGE_DIAGRAM);
    let note_edge = parsed
        .edges
        .iter()
        .find(|edge| edge.classes.contains("note-edge"))
        .expect("note edge");
    assert!(note_edge.classes.contains("note-edge"));
}

// stateNoteNode.spec.js

#[test]
fn note_on_a_composite_state_is_not_a_group() {
    let parsed = ast(
        "\n  stateDiagram-v2\n  state Composite {\n    A --> B\n  }\n  note right of Composite : hello\n  ",
    );
    let note = parsed
        .nodes
        .iter()
        .find(|node| node.shape.as_deref() == Some("note"))
        .expect("a note node should be produced");
    assert!(!note.is_group);
}

#[test]
fn note_on_a_composite_state_still_produces_the_note_group() {
    let parsed = ast(
        "\n  stateDiagram-v2\n  state Composite {\n    A --> B\n  }\n  note right of Composite : hello\n  ",
    );
    let note_group = parsed
        .nodes
        .iter()
        .find(|node| node.shape.as_deref() == Some("noteGroup"))
        .expect("a noteGroup should still wrap the note");
    assert!(note_group.is_group);
}

// stateDiagram-colorIndex.spec.ts

fn color_slots(body: &str) -> Vec<(String, Option<u32>)> {
    ast(body)
        .nodes
        .into_iter()
        .map(|node| (node.id, node.color_index))
        .collect()
}

fn slot(slots: &[(String, Option<u32>)], id: &str) -> Option<u32> {
    slots
        .iter()
        .find(|(slot_id, _)| slot_id == id)
        .unwrap_or_else(|| panic!("node {id} not found"))
        .1
}

fn region_slots(ast: &Ast) -> Vec<(String, Option<String>, Option<u32>)> {
    ast.nodes
        .iter()
        .filter(|node| node.shape.as_deref() == Some("divider"))
        .map(|node| (node.id.clone(), node.parent_id.clone(), node.color_index))
        .collect()
}

#[test]
fn color_each_composite_gets_its_own_slot_in_containment_order() {
    let slots = color_slots(
        "stateDiagram-v2\n      [*] --> Boot\n      state Boot {\n        [*] --> Firmware\n        state Kernel {\n          [*] --> Scheduler\n          state Drivers {\n            [*] --> Probe\n          }\n        }\n      }\n      Boot --> Running\n      state Running {\n        [*] --> Serving\n      }\n    ",
    );
    assert_eq!(slot(&slots, "Boot"), Some(0));
    assert_eq!(slot(&slots, "Kernel"), Some(1));
    assert_eq!(slot(&slots, "Drivers"), Some(2));
    assert_eq!(slot(&slots, "Running"), Some(3));
}

#[test]
fn color_plain_states_are_unslotted() {
    let slots = color_slots(
        "stateDiagram-v2\n      state Outer {\n        [*] --> Inner\n        Inner --> Done\n      }\n    ",
    );
    assert_eq!(slot(&slots, "Outer"), Some(0));
    assert_eq!(slot(&slots, "Inner"), None);
    assert_eq!(slot(&slots, "Done"), None);
}

#[test]
fn color_every_concurrency_region_of_one_composite_gets_the_same_slot() {
    let parsed = ast(
        "stateDiagram-v2\n      state Active {\n        [*] --> NumLockOff\n        --\n        [*] --> CapsLockOff\n        --\n        [*] --> ScrollLockOff\n      }\n    ",
    );
    let slots = color_slots(
        "stateDiagram-v2\n      state Active {\n        [*] --> NumLockOff\n        --\n        [*] --> CapsLockOff\n        --\n        [*] --> ScrollLockOff\n      }\n    ",
    );
    let regions: Vec<_> = region_slots(&parsed)
        .into_iter()
        .map(|(_, _, slot)| slot)
        .collect();
    assert_eq!(regions.len(), 3);
    assert_eq!(
        regions
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        1
    );
    assert_eq!(slot(&slots, "Active"), Some(0));
    assert_eq!(regions[0], Some(0));
}

#[test]
fn color_a_divider_does_not_shift_later_composites() {
    let without_divider = color_slots(
        "stateDiagram-v2\n      state First {\n        [*] --> A\n      }\n      state Second {\n        [*] --> B\n      }\n    ",
    );
    let with_divider = color_slots(
        "stateDiagram-v2\n      state First {\n        [*] --> A\n        --\n        [*] --> C\n      }\n      state Second {\n        [*] --> B\n      }\n    ",
    );
    assert_eq!(
        slot(&without_divider, "First"),
        slot(&with_divider, "First")
    );
    assert_eq!(
        slot(&without_divider, "Second"),
        slot(&with_divider, "Second")
    );
    assert_eq!(slot(&with_divider, "Second"), Some(1));
}

#[test]
fn color_a_styled_composite_carries_its_opt_out_into_its_concurrency_regions() {
    let parsed = ast(
        "stateDiagram-v2\n      classDef pinned fill:#111827,stroke:#F59E0B\n      state Active {\n        [*] --> A\n        --\n        [*] --> B\n      }\n      class Active pinned\n    ",
    );
    let slots = color_slots(
        "stateDiagram-v2\n      classDef pinned fill:#111827,stroke:#F59E0B\n      state Active {\n        [*] --> A\n        --\n        [*] --> B\n      }\n      class Active pinned\n    ",
    );
    assert_eq!(slot(&slots, "Active"), None);
    let regions = region_slots(&parsed);
    assert_eq!(regions.len(), 2);
    assert!(regions.iter().all(|region| region.2.is_none()));
}

#[test]
fn color_two_divided_composites_do_not_share_one_slot() {
    let parsed = ast(
        "stateDiagram-v2\n      state First {\n        [*] --> A\n        --\n        [*] --> B\n      }\n      state Second {\n        [*] --> C\n        --\n        [*] --> D\n      }\n    ",
    );
    let slots = color_slots(
        "stateDiagram-v2\n      state First {\n        [*] --> A\n        --\n        [*] --> B\n      }\n      state Second {\n        [*] --> C\n        --\n        [*] --> D\n      }\n    ",
    );
    let regions = region_slots(&parsed);
    assert_ne!(slot(&slots, "First"), slot(&slots, "Second"));
    assert_eq!(regions.len(), 4);
    let distinct: std::collections::HashSet<_> = regions.iter().map(|region| region.2).collect();
    assert_eq!(distinct.len(), 2);
    let mut by_parent: Vec<(String, std::collections::HashSet<Option<u32>>)> = Vec::new();
    for (_, parent, slot) in &regions {
        let key = parent.clone().unwrap_or_else(|| "root".to_string());
        match by_parent.iter_mut().find(|(id, _)| *id == key) {
            Some((_, slots)) => {
                slots.insert(*slot);
            }
            None => {
                by_parent.push((key, [*slot].into_iter().collect()));
            }
        }
    }
    assert_eq!(
        by_parent
            .iter()
            .map(|(_, slots)| slots.len())
            .collect::<Vec<_>>(),
        vec![1, 1]
    );
    let parent_set = |id: &str| {
        by_parent
            .iter()
            .find(|(parent, _)| parent == id)
            .map(|(_, slots)| slots.clone())
            .unwrap()
    };
    assert_eq!(
        parent_set("First"),
        [slot(&slots, "First")].into_iter().collect()
    );
    assert_eq!(
        parent_set("Second"),
        [slot(&slots, "Second")].into_iter().collect()
    );
}

#[test]
fn color_a_composite_with_its_own_class_def_is_unstamped_but_spends_its_slot() {
    let slots = color_slots(
        "stateDiagram-v2\n      classDef pinned fill:#111827,stroke:#F59E0B\n      state First {\n        [*] --> A\n      }\n      state Second {\n        [*] --> B\n      }\n      state Third {\n        [*] --> C\n      }\n      class Second pinned\n    ",
    );
    assert_eq!(slot(&slots, "First"), Some(0));
    assert_eq!(slot(&slots, "Second"), None);
    assert_eq!(slot(&slots, "Third"), Some(2));
}

#[test]
fn color_a_composite_with_its_own_style_statement_is_unstamped() {
    let slots = color_slots(
        "stateDiagram-v2\n      state First {\n        [*] --> A\n      }\n      state Second {\n        [*] --> B\n      }\n      style Second fill:#111827\n    ",
    );
    assert_eq!(slot(&slots, "First"), Some(0));
    assert_eq!(slot(&slots, "Second"), None);
}

#[test]
fn color_a_composite_named_twice_spends_one_slot() {
    let slots = color_slots(
        "stateDiagram-v2\n      [*] --> Loop\n      state Loop {\n        [*] --> Spin\n      }\n      Loop --> Loop\n      Loop --> Exit\n      state Exit {\n        [*] --> Bye\n      }\n    ",
    );
    assert_eq!(slot(&slots, "Loop"), Some(0));
    assert_eq!(slot(&slots, "Exit"), Some(1));
}

#[test]
fn parse_through_the_crate_takes_both_headers() {
    for header in ["stateDiagram", "stateDiagram-v2"] {
        let text = format!("{header}\n[*] --> Still\nStill --> [*]\n");
        let parsed = crate::parse(&text).expect("crate parse");
        assert!(matches!(parsed, crate::Diagram::State(_)));
    }
}
