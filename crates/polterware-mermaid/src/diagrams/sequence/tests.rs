//! Ports of upstream `sequenceDiagram.spec.js` (mermaid 12.3.0) for the parser and the
//! database: every input is upstream's, and every assertion is upstream's unless it needs
//! a renderer or a configuration value.
//!
//! Skipped upstream tests, and why:
//! - `should not show sequence numbers per default` and `should show sequence numbers when
//!   autonumber is enabled`: they call `renderer.draw`, which is what flips the flag.
//! - The whole `when checking the bounds in a sequenceDiagram` block (six tests): they
//!   exercise `bounds.insert/newLoop/endLoop` in the renderer.
//! - `when rendering a sequenceDiagram APA` except `should handle same actor with different
//!   whitespace properly` and the `wrap` assertion of `... text wrapped (inline)`: bounds,
//!   theme, font, and `%%{init}%%`/`%%{wrap}%%` configuration.
//! - The `textPlacement` bounds cases of `when rendering ... actor mirror activated`; the
//!   `__proto__`/`constructor` actor-name test is kept.
//! - `should have functions used in sequence JISON as own property`: it inspects the JS
//!   `SequenceDB` object, which has no Rust counterpart.
//!
//! Also skipped inside kept tests: `getConfig()` reads, and the `%%{init}%%`/`%%{wrap}%%`
//! directives themselves (they are removed before the parser runs, see `preprocess`).

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

fn actor<'a>(ast: &'a Ast, id: &str) -> &'a Actor {
    ast.actor(id)
        .unwrap_or_else(|| panic!("missing actor {id}"))
}

fn texts(ast: &Ast) -> Vec<&str> {
    ast.messages
        .iter()
        .map(|message| message.message.as_str())
        .collect()
}

fn types(ast: &Ast) -> Vec<LineType> {
    ast.messages
        .iter()
        .map(|message| message.line_type)
        .collect()
}

mod more_than_one_sequence_diagram {
    use super::*;

    #[test]
    fn should_not_have_duplicated_messages() {
        let source = "
        sequenceDiagram
        Alice->Bob:Hello Bob, how are you?
        Bob-->Alice: I am good thanks!";
        let first = ast(source);
        assert_eq!(first.messages.len(), 2);
        assert_eq!(first.messages[0].from.as_deref(), Some("Alice"));
        assert_eq!(first.messages[0].to.as_deref(), Some("Bob"));
        assert_eq!(
            first.messages[0].message.as_str(),
            "Hello Bob, how are you?"
        );
        assert_eq!(first.messages[0].line_type, LineType::SolidOpen);
        assert!(!first.messages[0].wrap);
        assert!(!first.messages[0].activate);
        assert_eq!(first.messages[0].central_connection, 0);
        assert_eq!(first.messages[0].id, "0");
        assert_eq!(first.messages[1].from.as_deref(), Some("Bob"));
        assert_eq!(first.messages[1].to.as_deref(), Some("Alice"));
        assert_eq!(first.messages[1].line_type, LineType::DottedOpen);

        let second = ast(source);
        assert_eq!(second.messages, first.messages);

        let third = ast("
        sequenceDiagram
        Alice->John:Hello John, how are you?
        John-->Alice: I am good thanks!");
        assert_eq!(third.messages.len(), 2);
        assert_eq!(third.messages[0].to.as_deref(), Some("John"));
        assert_eq!(third.messages[1].from.as_deref(), Some("John"));
    }
}

mod central_connection_parsing {
    use super::*;

    fn central_message(ast: &Ast) -> &Message {
        ast.messages
            .iter()
            .find(|message| message.from.is_some() && message.to.is_some())
            .expect("a message between two actors")
    }

    #[test]
    fn should_parse_actor_dual_paren_arrow_paren_actor_as_central_connection_dual() {
        let ast = ast("
        sequenceDiagram
        participant Alice
        participant Bob
        Alice ()->>() Bob: Hello Bob, how are you?
      ");
        assert_eq!(ast.messages.len(), 3);
        let message = central_message(&ast);
        assert_eq!(message.from.as_deref(), Some("Alice"));
        assert_eq!(message.to.as_deref(), Some("Bob"));
        assert_eq!(message.message.as_str(), "Hello Bob, how are you?");
        assert_eq!(message.central_connection, 61);
        assert!(message.activate);
        assert_eq!(message.line_type, LineType::Solid);
    }

    #[test]
    fn should_parse_actor_dual_dotted_arrow_as_central_connection_dual() {
        let ast = ast("
        sequenceDiagram
        participant Alice
        participant Bob
        Alice ()-->>() Bob: Hello Bob, how are you?
      ");
        assert_eq!(ast.messages.len(), 3);
        let message = central_message(&ast);
        assert_eq!(message.central_connection, 61);
        assert!(message.activate);
        assert_eq!(message.line_type, LineType::Dotted);
    }

    #[test]
    fn should_parse_actor_arrow_paren_actor_as_central_connection() {
        let ast = ast("
        sequenceDiagram
        participant Alice
        participant Bob
        Alice ->>() Bob: Hello Bob, how are you?
      ");
        assert_eq!(ast.messages.len(), 2);
        let message = central_message(&ast);
        assert_eq!(message.central_connection, 59);
        assert!(message.activate);
        assert_eq!(message.line_type, LineType::Solid);
    }

    #[test]
    fn should_parse_actor_paren_dotted_arrow_as_central_connection_reverse() {
        let ast = ast("
        sequenceDiagram
        participant Alice
        participant Bob
        Alice ()-->> Bob: Hello Bob, how are you?
      ");
        assert_eq!(ast.messages.len(), 2);
        let message = central_message(&ast);
        assert_eq!(message.central_connection, 60);
        assert!(!message.activate);
        assert_eq!(message.line_type, LineType::Dotted);
    }

    #[test]
    fn should_parse_actor_paren_arrow_as_central_connection_reverse() {
        let ast = ast("
        sequenceDiagram
        participant Alice
        participant Bob
        Alice ()->> Bob: Hello Bob, how are you?
      ");
        assert_eq!(ast.messages.len(), 2);
        let message = central_message(&ast);
        assert_eq!(message.central_connection, 60);
        assert!(!message.activate);
        assert_eq!(message.line_type, LineType::Solid);
    }

    #[test]
    fn should_parse_bidirectional_dotted_dual() {
        let ast = ast("
        sequenceDiagram
        participant Alice
        participant Bob
        Alice ()<<-->>() Bob: Hello Bob, how are you?
      ");
        assert_eq!(ast.messages.len(), 3);
        let message = central_message(&ast);
        assert_eq!(message.central_connection, 61);
        assert_eq!(message.line_type, LineType::BidirectionalDotted);
    }

    #[test]
    fn should_parse_bidirectional_solid_dual() {
        let ast = ast("
        sequenceDiagram
        participant Alice
        participant Bob
        Alice ()<<->>() Bob: Hello Bob, how are you?
      ");
        assert_eq!(ast.messages.len(), 3);
        let message = central_message(&ast);
        assert_eq!(message.central_connection, 61);
        assert_eq!(message.line_type, LineType::BidirectionalSolid);
    }

    #[test]
    fn should_handle_multiple_central_connection_types_in_one_diagram() {
        let ast = ast("
        sequenceDiagram
        participant Alice
        participant Bob
        participant Charlie
        Alice ()->>() Bob: Message 1
        Bob ()-->> Charlie: Message 2
        Charlie ()<<-->>() Alice: Message 3
      ");
        assert_eq!(ast.messages.len(), 8);
        let actual: Vec<&Message> = ast
            .messages
            .iter()
            .filter(|message| message.from.is_some() && message.to.is_some())
            .collect();
        assert_eq!(actual.len(), 3);
        assert_eq!(actual[0].central_connection, 61);
        assert_eq!(actual[1].central_connection, 60);
        assert_eq!(actual[2].central_connection, 61);
    }

    #[test]
    fn should_handle_central_connections_with_different_arrow_types() {
        let ast = ast("
        sequenceDiagram
        participant Alice
        participant Bob
        Alice ()-x() Bob: Cross message
        Alice ()--x() Bob: Dotted cross message
      ");
        assert_eq!(ast.messages.len(), 6);
        let actual: Vec<&Message> = ast
            .messages
            .iter()
            .filter(|message| message.from.is_some() && message.to.is_some())
            .collect();
        assert_eq!(actual.len(), 2);
        assert_eq!(actual[0].central_connection, 61);
        assert_eq!(actual[0].line_type, LineType::SolidCross);
        assert_eq!(actual[1].central_connection, 61);
        assert_eq!(actual[1].line_type, LineType::DottedCross);
    }

    #[test]
    fn should_not_break_existing_parsing_without_central_connections() {
        let ast = ast("
        sequenceDiagram
        participant Alice
        participant Bob
        Alice ->> Bob: Normal message
        Bob -->> Alice: Normal dotted message
        Alice -x Bob: Normal cross message
      ");
        assert_eq!(ast.messages.len(), 3);
        assert!(
            ast.messages
                .iter()
                .all(|message| message.central_connection == 0)
        );
        assert_eq!(ast.messages[0].line_type, LineType::Solid);
        assert_eq!(ast.messages[1].line_type, LineType::Dotted);
        assert_eq!(ast.messages[2].line_type, LineType::SolidCross);
    }
}

mod parsing_a_sequence_diagram {
    use super::*;

    #[test]
    fn should_handle_a_sequence_diagram_definition() {
        let ast = ast("
sequenceDiagram
Alice->Bob:Hello Bob, how are you?
Note right of Bob: Bob thinks
Bob-->Alice: I am good thanks!");
        assert_eq!(actor(&ast, "Alice").description, "Alice");
        assert_eq!(actor(&ast, "Bob").description, "Bob");
        assert_eq!(ast.messages.len(), 3);
        assert_eq!(ast.messages[0].from.as_deref(), Some("Alice"));
        assert_eq!(ast.messages[2].from.as_deref(), Some("Bob"));
    }

    #[test]
    fn should_allow_sequence_numbers_to_have_decimals_up_to_the_hundredths_place() {
        let ast = ast("
        sequenceDiagram
        autonumber 10.1 .01
        Alice->Bob:Hello Bob, how are you?
        Note right of Bob: Bob thinks
        Bob-->Alice: I am good thanks!
      ");
        assert_eq!(ast.messages[0].line_type, LineType::Autonumber);
        assert_eq!(
            ast.messages[0].message,
            MessageText::SequenceIndex {
                start: Some(10.1),
                step: Some(0.01),
                visible: true
            }
        );
    }

    #[test]
    fn should_not_allow_sequence_numbers_to_have_decimals_to_the_thousandths_place() {
        let error = failure(
            "
      sequenceDiagram
      autonumber 10.001
      Alice->Bob:Hello Bob, how are you?
    ",
        );
        assert!(error.span.is_some());
    }

    #[test]
    fn should_handle_a_title_with_a_colon() {
        let ast = ast("
sequenceDiagram
title: Diagram Title
Alice->Bob:Hello Bob, how are you?
Note right of Bob: Bob thinks
Bob-->Alice: I am good thanks!");
        assert_eq!(actor(&ast, "Alice").description, "Alice");
        assert_eq!(ast.acc_description, "");
        assert_eq!(ast.messages.len(), 3);
        assert_eq!(ast.diagram_title, "Diagram Title");
    }

    #[test]
    fn should_handle_a_title_without_a_colon() {
        let ast = ast("
sequenceDiagram
title Diagram Title
Alice->Bob:Hello Bob, how are you?
Note right of Bob: Bob thinks
Bob-->Alice: I am good thanks!");
        assert_eq!(ast.acc_description, "");
        assert_eq!(ast.messages.len(), 3);
        assert_eq!(ast.diagram_title, "Diagram Title");
    }

    #[test]
    fn should_handle_an_accessibility_title_and_description() {
        let ast = ast("
sequenceDiagram
title: Diagram Title
accTitle: This is the title
accDescr: Accessibility Description
Alice->Bob:Hello Bob, how are you?
");
        assert_eq!(ast.diagram_title, "Diagram Title");
        assert_eq!(ast.acc_title, "This is the title");
        assert_eq!(ast.acc_description, "Accessibility Description");
    }

    #[test]
    fn should_handle_an_accessibility_title_and_multiline_description() {
        let ast = ast("
sequenceDiagram
accTitle: This is the title
accDescr {
Accessibility
Description
}
Alice->Bob:Hello Bob, how are you?
");
        assert_eq!(ast.acc_title, "This is the title");
        assert_eq!(ast.acc_description, "Accessibility\nDescription");
    }

    #[test]
    fn should_handle_spaces_in_actor_names() {
        let ast = ast("
sequenceDiagram
Alice->Bob:Hello Bob, how are - you?
Bob-->Alice: I am good thanks!");
        assert_eq!(actor(&ast, "Alice").description, "Alice");
        assert_eq!(ast.messages.len(), 2);
        assert_eq!(ast.messages[0].from.as_deref(), Some("Alice"));
        assert_eq!(ast.messages[1].from.as_deref(), Some("Bob"));
    }

    #[test]
    fn should_handle_dashes_in_actor_names() {
        let ast = ast("
sequenceDiagram
Alice-in-Wonderland->Bob:Hello Bob, how are - you?
Bob-->Alice-in-Wonderland:I am good thanks!");
        assert_eq!(
            actor(&ast, "Alice-in-Wonderland").description,
            "Alice-in-Wonderland"
        );
        assert_eq!(actor(&ast, "Bob").description, "Bob");
        assert_eq!(ast.messages.len(), 2);
        assert_eq!(ast.messages[0].from.as_deref(), Some("Alice-in-Wonderland"));
        assert_eq!(ast.messages[1].from.as_deref(), Some("Bob"));
    }

    #[test]
    fn should_handle_dashes_in_participant_names() {
        let ast = ast("
sequenceDiagram
participant Alice-in-Wonderland
participant Bob
Alice-in-Wonderland->Bob:Hello Bob, how are - you?
Bob-->Alice-in-Wonderland:I am good thanks!");
        assert_eq!(ast.actor_keys(), vec!["Alice-in-Wonderland", "Bob"]);
        assert_eq!(
            actor(&ast, "Alice-in-Wonderland").description,
            "Alice-in-Wonderland"
        );
        assert_eq!(ast.messages.len(), 2);
    }

    #[test]
    fn should_handle_equals_in_participant_names() {
        let ast = ast("
sequenceDiagram
participant Alice=Wonderland
participant Bob
Alice=Wonderland->Bob:Hello Bob, how are - you?
Bob-->Alice=Wonderland:I am good thanks!");
        assert_eq!(ast.actor_keys(), vec!["Alice=Wonderland", "Bob"]);
        assert_eq!(
            actor(&ast, "Alice=Wonderland").description,
            "Alice=Wonderland"
        );
        assert_eq!(ast.messages.len(), 2);
    }

    #[test]
    fn should_alias_participants() {
        let ast = ast("
sequenceDiagram
participant A as Alice
participant B as Bob
A->B:Hello Bob, how are you?
B-->A: I am good thanks!");
        assert_eq!(ast.actor_keys(), vec!["A", "B"]);
        assert_eq!(actor(&ast, "A").description, "Alice");
        assert_eq!(actor(&ast, "B").description, "Bob");
        assert_eq!(ast.messages.len(), 2);
        assert_eq!(ast.messages[0].from.as_deref(), Some("A"));
    }

    #[test]
    fn should_alias_a_mix_of_actors_and_participants() {
        let ast = ast("
sequenceDiagram
  actor Alice as Alice2
  actor Bob
  participant John as John2
  participant Mandy
  Alice->>Bob: Hi Bob
  Bob->>Alice: Hi Alice
  Alice->>John: Hi John
  John->>Mandy: Hi Mandy
  Mandy ->>Joan: Hi Joan");
        assert_eq!(
            ast.actor_keys(),
            vec!["Alice", "Bob", "John", "Mandy", "Joan"]
        );
        assert_eq!(actor(&ast, "Alice").description, "Alice2");
        assert_eq!(actor(&ast, "Alice").actor_type, "actor");
        assert_eq!(actor(&ast, "Bob").description, "Bob");
        assert_eq!(actor(&ast, "John").actor_type, "participant");
        assert_eq!(actor(&ast, "Joan").actor_type, "participant");
        assert_eq!(ast.messages.len(), 5);
        assert_eq!(ast.messages[0].from.as_deref(), Some("Alice"));
        assert_eq!(ast.messages[4].to.as_deref(), Some("Joan"));
    }

    #[test]
    fn should_alias_actors() {
        let ast = ast("
sequenceDiagram
actor A as Alice
actor B as Bob
A->B:Hello Bob, how are you?
B-->A: I am good thanks!");
        assert_eq!(ast.actor_keys(), vec!["A", "B"]);
        assert_eq!(actor(&ast, "A").description, "Alice");
        assert_eq!(actor(&ast, "B").description, "Bob");
        assert_eq!(ast.messages.len(), 2);
    }

    #[test]
    fn should_handle_async_messages() {
        let ast = ast("
sequenceDiagram
Alice-xBob:Hello Bob, how are you?");
        assert_eq!(ast.messages.len(), 1);
        assert_eq!(ast.messages[0].line_type, LineType::SolidCross);
    }

    #[test]
    fn should_handle_async_dotted_messages() {
        let ast = ast("
sequenceDiagram
Alice--xBob:Hello Bob, how are you?");
        assert_eq!(ast.messages.len(), 1);
        assert_eq!(ast.messages[0].line_type, LineType::DottedCross);
    }

    #[test]
    fn should_handle_sync_messages() {
        let ast = ast("
sequenceDiagram
Alice-)Bob:Hello Bob, how are you?");
        assert_eq!(ast.messages.len(), 1);
        assert_eq!(ast.messages[0].line_type, LineType::SolidPoint);
    }

    #[test]
    fn should_handle_sync_dotted_messages() {
        let ast = ast("
sequenceDiagram
Alice--)Bob:Hello Bob, how are you?");
        assert_eq!(ast.messages.len(), 1);
        assert_eq!(ast.messages[0].line_type, LineType::DottedPoint);
    }

    #[test]
    fn should_handle_solid_arrow_messages() {
        let ast = ast("
sequenceDiagram
Alice->>Bob:Hello Bob, how are you?");
        assert_eq!(ast.messages.len(), 1);
        assert_eq!(ast.messages[0].line_type, LineType::Solid);
    }

    #[test]
    fn should_handle_dotted_arrow_messages() {
        let ast = ast("sequenceDiagram\nAlice-->>Bob:Hello Bob, how are you?");
        assert_eq!(ast.messages.len(), 1);
        assert_eq!(ast.messages[0].line_type, LineType::Dotted);
    }

    #[test]
    fn should_handle_bidirectional_arrow_messages() {
        let ast = ast("
sequenceDiagram
Alice<<->>Bob:Hello Bob, how are you?");
        assert_eq!(ast.messages.len(), 1);
        assert_eq!(ast.messages[0].line_type, LineType::BidirectionalSolid);
    }

    #[test]
    fn should_handle_bidirectional_dotted_arrow_messages() {
        let ast = ast("
    sequenceDiagram
    Alice<<-->>Bob:Hello Bob, how are you?");
        assert_eq!(ast.messages.len(), 1);
        assert_eq!(ast.messages[0].line_type, LineType::BidirectionalDotted);
    }

    #[test]
    fn should_handle_actor_activation() {
        let ast = ast("
sequenceDiagram
Alice-->>Bob:Hello Bob, how are you?
activate Bob
Bob-->>Alice:Hello Alice, I'm fine and  you?
deactivate Bob");
        assert_eq!(ast.messages.len(), 4);
        assert_eq!(ast.messages[0].line_type, LineType::Dotted);
        assert_eq!(ast.messages[1].line_type, LineType::ActiveStart);
        assert_eq!(ast.messages[1].from.as_deref(), Some("Bob"));
        assert_eq!(ast.messages[2].line_type, LineType::Dotted);
        assert_eq!(ast.messages[3].line_type, LineType::ActiveEnd);
        assert_eq!(ast.messages[3].from.as_deref(), Some("Bob"));
    }

    #[test]
    fn should_handle_actor_one_line_notation_activation() {
        let ast = ast("
    sequenceDiagram
    Alice-->>+Bob:Hello Bob, how are you?
    Bob-->>- Alice:Hello Alice, I'm fine and  you?");
        assert_eq!(ast.messages.len(), 4);
        assert_eq!(ast.messages[0].line_type, LineType::Dotted);
        assert!(ast.messages[0].activate);
        assert_eq!(ast.messages[1].line_type, LineType::ActiveStart);
        assert_eq!(ast.messages[1].from.as_deref(), Some("Bob"));
        assert_eq!(ast.messages[2].line_type, LineType::Dotted);
        assert_eq!(ast.messages[3].line_type, LineType::ActiveEnd);
        assert_eq!(ast.messages[3].from.as_deref(), Some("Bob"));
    }

    #[test]
    fn should_handle_stacked_activations() {
        let ast = ast("
      sequenceDiagram
      Alice-->>+Bob:Hello Bob, how are you?
      Bob-->>+Carol:Carol, let me introduce Alice?
      Bob-->>- Alice:Hello Alice, please meet Carol?
      Carol->>- Bob:Oh Bob, I'm so happy to be here!");
        assert_eq!(ast.messages.len(), 8);
        assert_eq!(ast.messages[1].line_type, LineType::ActiveStart);
        assert_eq!(ast.messages[1].from.as_deref(), Some("Bob"));
        assert_eq!(ast.messages[3].line_type, LineType::ActiveStart);
        assert_eq!(ast.messages[3].from.as_deref(), Some("Carol"));
        assert_eq!(ast.messages[5].line_type, LineType::ActiveEnd);
        assert_eq!(ast.messages[5].from.as_deref(), Some("Bob"));
        assert_eq!(ast.messages[7].line_type, LineType::ActiveEnd);
        assert_eq!(ast.messages[7].from.as_deref(), Some("Carol"));
    }

    #[test]
    fn should_fail_parsing_when_activating_an_inactive_participant() {
        let error = failure(
            "
      sequenceDiagram
      participant user as End User
      participant Server as Server
      participant System as System
      participant System2 as System2

      user->>+Server: Test
      user->>+Server: Test2
      user->>System: Test
      Server->>-user: Test
      Server->>-user: Test2

      %% The following deactivation of Server will fail
      Server->>-user: Test3",
        );
        assert_eq!(
            error.message,
            "Trying to inactivate an inactive participant (Server)"
        );
        assert_eq!(error.span.unwrap().line, 15);
    }

    #[test]
    fn should_handle_comments() {
        let ast = ast("
      sequenceDiagram
      Alice->Bob: Hello Bob, how are you?
      %% Comment
      Note right of Bob: Bob thinks
      Bob-->Alice: I am good thanks!");
        assert_eq!(ast.messages.len(), 3);
        assert_eq!(ast.messages[0].from.as_deref(), Some("Alice"));
        assert_eq!(ast.messages[2].from.as_deref(), Some("Bob"));
    }

    #[test]
    fn should_handle_new_lines() {
        let ast = ast("
      sequenceDiagram
      Alice->Bob: Hello Bob, how are you?

      %% Comment
      Note right of Bob: Bob thinks
      Bob-->Alice: I am good thanks!
      ");
        assert_eq!(ast.messages.len(), 3);
    }

    #[test]
    fn should_handle_semicolons() {
        let ast = ast(
            "
sequenceDiagram;Alice->Bob: Hello Bob, how are you?;Note right of Bob: Bob thinks;Bob-->Alice: I am good thanks!;",
        );
        assert_eq!(ast.messages.len(), 3);
        assert_eq!(ast.messages[0].from.as_deref(), Some("Alice"));
        assert_eq!(ast.messages[2].from.as_deref(), Some("Bob"));
    }

    #[test]
    fn should_handle_one_leading_space_per_line() {
        let ast = ast("
sequenceDiagram
 Alice->Bob: Hello Bob, how are you?

%% Comment
Note right of Bob: Bob thinks
Bob-->Alice: I am good thanks!");
        assert_eq!(ast.messages.len(), 3);
    }

    #[test]
    fn should_handle_several_leading_spaces_per_line() {
        let ast = ast("
sequenceDiagram
   Alice->Bob: Hello Bob, how are you?

%% Comment
Note right of Bob: Bob thinks
Bob-->Alice: I am good thanks!");
        assert_eq!(ast.messages.len(), 3);
    }

    #[test]
    fn should_handle_indented_blocks_and_notes() {
        let ast = ast("
sequenceDiagram
participant Alice
participant Bob
Alice->John: Hello John, how are you?
    loop Healthcheck
John->John: Fight against hypochondria
 end
Note right of John: Rational thoughts<br/>prevail...
    John-->Alice: Great!
    John->Bob: How about you?
Bob-->John: Jolly good!");
        assert_eq!(actor(&ast, "Alice").description, "Alice");
        assert_eq!(ast.messages.len(), 8);
        assert_eq!(ast.messages[0].from.as_deref(), Some("Alice"));
        assert_eq!(ast.messages[2].from.as_deref(), Some("John"));
    }

    #[test]
    fn should_handle_different_line_breaks() {
        let ast = ast("
sequenceDiagram
participant 1 as multiline<br>text
participant 2 as multiline<br/>text
participant 3 as multiline<br />text
participant 4 as multiline<br \t/>text
1->>2: multiline<br>text
note right of 2: multiline<br>text
2->>3: multiline<br/>text
note right of 3: multiline<br/>text
3->>4: multiline<br />text
note right of 4: multiline<br />text
4->>1: multiline<br \t/>text
note right of 1: multiline<br \t/>text
");
        assert_eq!(actor(&ast, "1").description, "multiline<br>text");
        assert_eq!(actor(&ast, "2").description, "multiline<br/>text");
        assert_eq!(actor(&ast, "3").description, "multiline<br />text");
        assert_eq!(actor(&ast, "4").description, "multiline<br \t/>text");
        let texts = texts(&ast);
        assert_eq!(
            texts,
            vec![
                "multiline<br>text",
                "multiline<br>text",
                "multiline<br/>text",
                "multiline<br/>text",
                "multiline<br />text",
                "multiline<br />text",
                "multiline<br \t/>text",
                "multiline<br \t/>text",
            ]
        );
    }

    #[test]
    fn should_handle_notes_and_messages_without_a_wrap_marker() {
        let ast = ast("
sequenceDiagram
participant 1
participant 2
participant 3
participant 4
1->>2: single-line text
note right of 2: single-line text
2->>3:nowrap: single-line text
note right of 3:nowrap: single-line text
3->>4: multiline<br/>text
note right of 4: multiline<br/>text
4->>1:nowrap: multiline<br/>text
note right of 1:nowrap: multiline<br/>text
");
        assert_eq!(
            texts(&ast),
            vec![
                "single-line text",
                "single-line text",
                "single-line text",
                "single-line text",
                "multiline<br/>text",
                "multiline<br/>text",
                "multiline<br/>text",
                "multiline<br/>text",
            ]
        );
        assert!(ast.messages.iter().all(|message| !message.wrap));
    }

    #[test]
    fn should_handle_notes_and_messages_with_a_wrap_marker() {
        let ast = ast("
sequenceDiagram
participant 1
participant 2
participant 3
participant 4
1->>2:wrap: single-line text
note right of 2:wrap: single-line text
2->>3:wrap: multiline<br/>text
note right of 3:wrap: multiline<br/>text
");
        assert_eq!(
            texts(&ast),
            vec![
                "single-line text",
                "single-line text",
                "multiline<br/>text",
                "multiline<br/>text",
            ]
        );
        assert!(ast.messages.iter().all(|message| message.wrap));
    }

    #[test]
    fn should_handle_notes_and_messages_with_nowrap_or_line_breaks() {
        let ast = ast("
sequenceDiagram
participant 1
participant 2
1->>2: single-line text
note right of 2: single-line text
");
        assert_eq!(texts(&ast), vec!["single-line text", "single-line text"]);
        assert!(ast.messages.iter().all(|message| !message.wrap));
    }

    #[test]
    fn should_handle_notes_over_a_single_actor() {
        let ast = ast("
sequenceDiagram
Alice->Bob: Hello Bob, how are you?
Note over Bob: Bob thinks
");
        assert_eq!(ast.messages[1].from.as_deref(), Some("Bob"));
        assert_eq!(ast.messages[1].to.as_deref(), Some("Bob"));
    }

    #[test]
    fn should_handle_notes_over_multiple_actors() {
        let ast = ast("
sequenceDiagram
Alice->Bob: Hello Bob, how are you?
Note over Alice,Bob: confusion
Note over Bob,Alice: resolution
");
        assert_eq!(ast.messages[1].from.as_deref(), Some("Alice"));
        assert_eq!(ast.messages[1].to.as_deref(), Some("Bob"));
        assert_eq!(ast.messages[2].from.as_deref(), Some("Bob"));
        assert_eq!(ast.messages[2].to.as_deref(), Some("Alice"));
    }

    #[test]
    fn should_handle_loop_statements() {
        let ast = ast("
sequenceDiagram
Alice->Bob: Hello Bob, how are you?

%% Comment
Note right of Bob: Bob thinks
loop Multiple happy responses

Bob-->Alice: I am good thanks!
end");
        assert_eq!(ast.messages.len(), 5);
        assert_eq!(ast.messages[0].from.as_deref(), Some("Alice"));
        assert_eq!(ast.messages[1].from.as_deref(), Some("Bob"));
    }

    #[test]
    fn should_add_a_rect_around_a_sequence() {
        let ast = ast("
      sequenceDiagram
        Alice->Bob: Hello Bob, how are you?
        %% Comment
        rect rgb(200, 255, 200)
        Note right of Bob: Bob thinks
        Bob-->Alice: I am good thanks
        end
    ");
        assert_eq!(ast.messages[1].line_type, LineType::RectStart);
        assert_eq!(ast.messages[1].message.as_str(), "rgb(200, 255, 200)");
        assert_eq!(ast.messages[2].line_type, LineType::Note);
        assert_eq!(ast.messages[3].line_type, LineType::DottedOpen);
        assert_eq!(ast.messages[4].line_type, LineType::RectEnd);
    }

    #[test]
    fn should_allow_nested_rects() {
        let ast = ast("
      sequenceDiagram
        Alice->Bob: Hello Bob, how are you?
        %% Comment
        rect rgb(200, 255, 200)
        rect rgb(0, 0, 0)
        Note right of Bob: Bob thinks
        end
        Bob-->Alice: I am good thanks
        end
    ");
        assert_eq!(ast.messages[1].line_type, LineType::RectStart);
        assert_eq!(ast.messages[1].message.as_str(), "rgb(200, 255, 200)");
        assert_eq!(ast.messages[2].line_type, LineType::RectStart);
        assert_eq!(ast.messages[2].message.as_str(), "rgb(0, 0, 0)");
        assert_eq!(ast.messages[3].line_type, LineType::Note);
        assert_eq!(ast.messages[4].line_type, LineType::RectEnd);
        assert_eq!(ast.messages[5].line_type, LineType::DottedOpen);
        assert_eq!(ast.messages[6].line_type, LineType::RectEnd);
    }

    #[test]
    fn should_handle_opt_statements() {
        let ast = ast("
sequenceDiagram
Alice->Bob: Hello Bob, how are you?

%% Comment
Note right of Bob: Bob thinks
opt Perhaps a happy response

Bob-->Alice: I am good thanks!
end");
        assert_eq!(ast.messages.len(), 5);
        assert_eq!(ast.messages[0].from.as_deref(), Some("Alice"));
        assert_eq!(ast.messages[1].from.as_deref(), Some("Bob"));
    }

    #[test]
    fn should_handle_alt_statements() {
        let ast = ast("
sequenceDiagram
Alice->Bob: Hello Bob, how are you?

%% Comment
Note right of Bob: Bob thinks
alt isWell

Bob-->Alice: I am good thanks!
else isSick
Bob-->Alice: Feel sick...
end");
        assert_eq!(ast.messages.len(), 7);
        assert_eq!(ast.messages[0].from.as_deref(), Some("Alice"));
        assert_eq!(ast.messages[1].from.as_deref(), Some("Bob"));
    }

    #[test]
    fn should_handle_alt_statements_with_multiple_elses() {
        let ast = ast("
sequenceDiagram
Alice->Bob: Hello Bob, how are you?

%% Comment
Note right of Bob: Bob thinks
alt isWell

Bob-->Alice: I am good thanks!
else isSick
Bob-->Alice: Feel sick...
else default
Bob-->Alice: :-)
end");
        assert_eq!(ast.messages.len(), 9);
        assert_eq!(ast.messages[1].from.as_deref(), Some("Bob"));
        assert_eq!(ast.messages[2].line_type, LineType::AltStart);
        assert_eq!(ast.messages[3].from.as_deref(), Some("Bob"));
        assert_eq!(ast.messages[4].line_type, LineType::AltElse);
        assert_eq!(ast.messages[5].from.as_deref(), Some("Bob"));
        assert_eq!(ast.messages[6].line_type, LineType::AltElse);
        assert_eq!(ast.messages[7].from.as_deref(), Some("Bob"));
        assert_eq!(ast.messages[8].line_type, LineType::AltEnd);
    }

    #[test]
    fn should_handle_critical_statements_without_options() {
        let ast = ast("
sequenceDiagram
    critical Establish a connection to the DB
        Service-->DB: connect
    end");
        assert_eq!(ast.messages.len(), 3);
        assert_eq!(ast.messages[0].line_type, LineType::CriticalStart);
        assert_eq!(ast.messages[1].from.as_deref(), Some("Service"));
        assert_eq!(ast.messages[2].line_type, LineType::CriticalEnd);
    }

    #[test]
    fn should_handle_critical_statements_with_options() {
        let ast = ast("
sequenceDiagram
    critical Establish a connection to the DB
        Service-->DB: connect
    option Network timeout
        Service-->Service: Log error
    option Credentials rejected
        Service-->Service: Log different error
    end");
        assert_eq!(ast.messages.len(), 7);
        assert_eq!(ast.messages[0].line_type, LineType::CriticalStart);
        assert_eq!(ast.messages[1].from.as_deref(), Some("Service"));
        assert_eq!(ast.messages[2].line_type, LineType::CriticalOption);
        assert_eq!(ast.messages[3].from.as_deref(), Some("Service"));
        assert_eq!(ast.messages[4].line_type, LineType::CriticalOption);
        assert_eq!(ast.messages[5].from.as_deref(), Some("Service"));
        assert_eq!(ast.messages[6].line_type, LineType::CriticalEnd);
    }

    #[test]
    fn should_handle_break_statements() {
        let ast = ast("
sequenceDiagram
    Consumer-->API: Book something
    API-->BookingService: Start booking process
    break when the booking process fails
        API-->Consumer: show failure
    end
    API-->BillingService: Start billing process");
        assert_eq!(ast.messages.len(), 6);
        assert_eq!(ast.messages[0].from.as_deref(), Some("Consumer"));
        assert_eq!(ast.messages[1].from.as_deref(), Some("API"));
        assert_eq!(ast.messages[2].line_type, LineType::BreakStart);
        assert_eq!(ast.messages[3].from.as_deref(), Some("API"));
        assert_eq!(ast.messages[4].line_type, LineType::BreakEnd);
        assert_eq!(ast.messages[5].from.as_deref(), Some("API"));
    }

    #[test]
    fn should_handle_par_statements() {
        let ast = ast("
sequenceDiagram
par Parallel one
Alice->>Bob: Hello Bob, how are you?
Bob-->>Alice: I am good thanks!
and Parallel two
Alice->>Bob: Are you OK?
Bob-->>Alice: Fine!
and Parallel three
Alice->>Bob: What do you think about it?
Bob-->>Alice: It's good!
end");
        assert_eq!(ast.messages.len(), 10);
        assert_eq!(ast.messages[0].message.as_str(), "Parallel one");
        assert_eq!(ast.messages[1].from.as_deref(), Some("Alice"));
        assert_eq!(ast.messages[2].from.as_deref(), Some("Bob"));
    }

    #[test]
    fn should_handle_par_over_statements() {
        let ast = ast("
sequenceDiagram
par_over Parallel overlap
Alice ->> Bob: Message
Note left of Alice: Alice note
Note right of Bob: Bob note
end");
        assert_eq!(ast.messages.len(), 5);
        assert_eq!(ast.messages[0].message.as_str(), "Parallel overlap");
        assert_eq!(ast.messages[0].line_type, LineType::ParOverStart);
        assert_eq!(ast.messages[1].from.as_deref(), Some("Alice"));
        assert_eq!(ast.messages[2].from.as_deref(), Some("Alice"));
        assert_eq!(ast.messages[3].from.as_deref(), Some("Bob"));
    }

    #[test]
    fn should_handle_special_characters_in_signals() {
        let ast = ast("sequenceDiagram\nAlice->Bob: -:<>,;# comment");
        assert_eq!(ast.messages[0].message.as_str(), "-:<>,");
    }

    #[test]
    fn should_handle_special_characters_in_notes() {
        let ast = ast("
sequenceDiagram
Alice->Bob: Hello Bob, how are you?
Note right of Bob: -:<>,;# comment");
        assert_eq!(ast.messages[1].message.as_str(), "-:<>,");
    }

    #[test]
    fn should_handle_special_characters_in_loops() {
        let ast = ast("
sequenceDiagram
Alice->Bob: Hello Bob, how are you?
loop -:<>,;# comment
Bob-->Alice: I am good thanks!
end");
        assert_eq!(ast.messages[1].message.as_str(), "-:<>,");
    }

    #[test]
    fn should_handle_special_characters_in_opts() {
        let ast = ast("
sequenceDiagram
Alice->Bob: Hello Bob, how are you?
opt -:<>,;# comment
Bob-->Alice: I am good thanks!
end");
        assert_eq!(ast.messages[1].message.as_str(), "-:<>,");
    }

    #[test]
    fn should_handle_special_characters_in_alts() {
        let ast = ast("
sequenceDiagram
Alice->Bob: Hello Bob, how are you?
alt -:<>,;# comment
Bob-->Alice: I am good thanks!
else ,<>:-#; comment
Bob-->Alice: I am good thanks!
end");
        assert_eq!(ast.messages[1].message.as_str(), "-:<>,");
        assert_eq!(ast.messages[3].message.as_str(), ",<>:-");
    }

    #[test]
    fn should_handle_special_characters_in_pars() {
        let ast = ast("
sequenceDiagram
Alice->Bob: Hello Bob, how are you?
par -:<>,;# comment
Bob-->Alice: I am good thanks!
and ,<>:-#; comment
Bob-->Alice: I am good thanks!
end");
        assert_eq!(ast.messages[1].message.as_str(), "-:<>,");
        assert_eq!(ast.messages[3].message.as_str(), ",<>:-");
    }

    #[test]
    fn should_handle_a_no_label_loop() {
        let ast = ast("
sequenceDiagram
Alice->Bob: Hello Bob, how are you?
loop
Bob-->Alice: I am good thanks!
end");
        assert_eq!(ast.messages[1].message.as_str(), "");
        assert_eq!(ast.messages[2].message.as_str(), "I am good thanks!");
    }

    #[test]
    fn should_handle_a_no_label_opt() {
        let ast = ast("
sequenceDiagram
Alice->Bob: Hello Bob, how are you?
opt # comment
Bob-->Alice: I am good thanks!
end");
        assert_eq!(ast.messages[1].message.as_str(), "");
        assert_eq!(ast.messages[2].message.as_str(), "I am good thanks!");
    }

    #[test]
    fn should_handle_a_no_label_alt() {
        let ast = ast("
sequenceDiagram
Alice->Bob: Hello Bob, how are you?
alt;Bob-->Alice: I am good thanks!
else # comment
Bob-->Alice: I am good thanks!
end");
        assert_eq!(ast.messages[1].message.as_str(), "");
        assert_eq!(ast.messages[2].message.as_str(), "I am good thanks!");
        assert_eq!(ast.messages[3].message.as_str(), "");
        assert_eq!(ast.messages[4].message.as_str(), "I am good thanks!");
    }

    #[test]
    fn should_handle_a_no_label_par() {
        let ast = ast("
sequenceDiagram
Alice->Bob: Hello Bob, how are you?
par;Bob-->Alice: I am good thanks!
and # comment
Bob-->Alice: I am good thanks!
end");
        assert_eq!(ast.messages[1].message.as_str(), "");
        assert_eq!(ast.messages[2].message.as_str(), "I am good thanks!");
        assert_eq!(ast.messages[3].message.as_str(), "");
        assert_eq!(ast.messages[4].message.as_str(), "I am good thanks!");
    }

    #[test]
    fn should_use_link_as_a_declared_message_endpoint() {
        for id in ["Link", "link", "LINK"] {
            let source = format!(
                "
sequenceDiagram
participant A
participant {id} as Channel
A->>{id}: message
{id}-->>A: reply
"
            );
            let ast = ast(&source);
            assert_eq!(ast.actor_keys(), vec!["A", id]);
            assert_eq!(actor(&ast, id).description, "Channel");
            assert_eq!(ast.messages.len(), 2);
            assert_eq!(ast.messages[0].to.as_deref(), Some(id));
            assert_eq!(ast.messages[1].from.as_deref(), Some(id));
        }
    }

    #[test]
    fn should_use_link_as_an_implicit_message_endpoint() {
        for id in ["Link", "link", "LINK"] {
            let source = format!(
                "
sequenceDiagram
{id}->>A: message
A-->>{id}: reply
"
            );
            let ast = ast(&source);
            assert_eq!(ast.actor_keys(), vec![id, "A"]);
            assert_eq!(ast.messages[0].from.as_deref(), Some(id));
            assert_eq!(ast.messages[1].to.as_deref(), Some(id));
        }
    }

    #[test]
    fn should_attach_actor_menu_links_to_link() {
        for id in ["Link", "link", "LINK"] {
            let source = format!(
                "
sequenceDiagram
participant A
participant {id} as Channel
link A: Help @ https://example.com/help
link {id}: Help @ https://example.com/channel
links {id}: {{ \"Repo\": \"https://example.com/repo\" }}
"
            );
            let ast = ast(&source);
            assert_eq!(
                actor(&ast, "A")
                    .links
                    .get("Help")
                    .and_then(|value| value.as_str()),
                Some("https://example.com/help")
            );
            assert_eq!(
                actor(&ast, id)
                    .links
                    .get("Help")
                    .and_then(|value| value.as_str()),
                Some("https://example.com/channel")
            );
            assert_eq!(
                actor(&ast, id)
                    .links
                    .get("Repo")
                    .and_then(|value| value.as_str()),
                Some("https://example.com/repo")
            );
            assert_eq!(actor(&ast, id).description, "Channel");
        }
    }

    #[test]
    fn should_handle_links() {
        let ast = ast(
            "
sequenceDiagram
participant a as Alice
participant b as Bob
participant c as Charlie
links a: { \"Repo\": \"https://repo.contoso.com/\", \"Dashboard\": \"https://dashboard.contoso.com/\" }
links b: { \"Dashboard\": \"https://dashboard.contoso.com/\" }
links a: { \"On-Call\": \"https://oncall.contoso.com/?svc=alice\" }
link a: Endpoint @ https://alice.contoso.com
link a: Swagger @ https://swagger.contoso.com
link a: Tests @ https://tests.contoso.com/?svc=alice@contoso.com
",
        );
        let link = |id: &str, key: &str| {
            actor(&ast, id)
                .links
                .get(key)
                .and_then(|value| value.as_str())
                .map(str::to_string)
        };
        assert_eq!(
            link("a", "Repo").as_deref(),
            Some("https://repo.contoso.com/")
        );
        assert_eq!(link("b", "Repo"), None);
        assert_eq!(
            link("a", "Dashboard").as_deref(),
            Some("https://dashboard.contoso.com/")
        );
        assert_eq!(
            link("b", "Dashboard").as_deref(),
            Some("https://dashboard.contoso.com/")
        );
        assert_eq!(
            link("a", "On-Call").as_deref(),
            Some("https://oncall.contoso.com/?svc=alice")
        );
        assert_eq!(link("c", "Dashboard"), None);
        assert_eq!(
            link("a", "Endpoint").as_deref(),
            Some("https://alice.contoso.com")
        );
        assert_eq!(
            link("a", "Swagger").as_deref(),
            Some("https://swagger.contoso.com")
        );
        assert_eq!(
            link("a", "Tests").as_deref(),
            Some("https://tests.contoso.com/?svc=alice@contoso.com")
        );
    }

    #[test]
    fn should_handle_properties() {
        let ast = ast("
sequenceDiagram
participant a as Alice
participant b as Bob
participant c as Charlie
properties a: {\"class\": \"internal-service-actor\", \"icon\": \"@clock\"}
properties b: {\"class\": \"external-service-actor\", \"icon\": \"@computer\"}
");
        let property = |id: &str, key: &str| {
            actor(&ast, id)
                .properties
                .get(key)
                .and_then(|value| value.as_str())
                .map(str::to_string)
        };
        assert_eq!(
            property("a", "class").as_deref(),
            Some("internal-service-actor")
        );
        assert_eq!(
            property("b", "class").as_deref(),
            Some("external-service-actor")
        );
        assert_eq!(property("a", "icon").as_deref(), Some("@clock"));
        assert_eq!(property("b", "icon").as_deref(), Some("@computer"));
        assert_eq!(property("c", "class"), None);
    }

    #[test]
    fn should_handle_a_box() {
        let ast = ast(
            "
sequenceDiagram
box green Group 1
participant a as Alice
participant b as Bob
end
participant c as Charlie
links a: { \"Repo\": \"https://repo.contoso.com/\", \"Dashboard\": \"https://dashboard.contoso.com/\" }
links b: { \"Dashboard\": \"https://dashboard.contoso.com/\" }
links a: { \"On-Call\": \"https://oncall.contoso.com/?svc=alice\" }
link a: Endpoint @ https://alice.contoso.com
link a: Swagger @ https://swagger.contoso.com
link a: Tests @ https://tests.contoso.com/?svc=alice@contoso.com
",
        );
        assert_eq!(ast.boxes.len(), 1);
        assert_eq!(ast.boxes[0].name.as_deref(), Some("Group 1"));
        assert_eq!(ast.boxes[0].actor_keys, vec!["a", "b"]);
        assert_eq!(ast.boxes[0].fill, "green");
        assert_eq!(actor(&ast, "a").box_index, Some(0));
        assert_eq!(actor(&ast, "c").box_index, None);
    }

    #[test]
    fn should_handle_a_box_without_a_color() {
        let ast = ast("
  sequenceDiagram
  box Group 1
  participant a as Alice
  participant b as Bob
  end
  participant c as Charlie
  ");
        assert_eq!(ast.boxes[0].name.as_deref(), Some("Group 1"));
        assert_eq!(ast.boxes[0].actor_keys, vec!["a", "b"]);
        assert_eq!(ast.boxes[0].fill, "transparent");
    }

    #[test]
    fn should_handle_a_box_without_a_description() {
        let ast = ast("
  sequenceDiagram
  box aqua
  participant a as Alice
  participant b as Bob
  end
  participant c as Charlie
  ");
        assert_eq!(ast.boxes[0].name, None);
        assert_eq!(ast.boxes[0].actor_keys, vec!["a", "b"]);
        assert_eq!(ast.boxes[0].fill, "aqua");
    }

    #[test]
    fn should_handle_simple_actor_creation() {
        let ast = ast("
  sequenceDiagram
  participant a as Alice
  a ->>b: Hello Bob?
  create participant c
  b-->>c: Hello c!
  c ->> b: Hello b?
  create actor d as Donald
  a ->> d: Hello Donald?
  ");
        assert_eq!(actor(&ast, "c").name, "c");
        assert_eq!(actor(&ast, "c").description, "c");
        assert_eq!(actor(&ast, "c").actor_type, "participant");
        assert_eq!(ast.created_actor("c"), Some(1));
        assert_eq!(actor(&ast, "d").name, "d");
        assert_eq!(actor(&ast, "d").description, "Donald");
        assert_eq!(actor(&ast, "d").actor_type, "actor");
        assert_eq!(ast.created_actor("d"), Some(3));
    }

    #[test]
    fn should_handle_simple_actor_destruction() {
        let ast = ast("
  sequenceDiagram
  participant a as Alice
  a ->>b: Hello Bob?
  destroy a
  b-->>a: Hello Alice!
  b ->> c: Where is Alice?
  destroy c
  b ->> c: Where are you?
  ");
        assert_eq!(ast.destroyed_actor("a"), Some(1));
        assert_eq!(ast.destroyed_actor("c"), Some(3));
    }

    #[test]
    fn should_handle_the_creation_and_destruction_of_the_same_actor() {
        let ast = ast("
  sequenceDiagram
  a ->>b: Hello Bob?
  create participant c
  b ->>c: Hello c!
  c ->> b: Hello b?
  destroy c
  b ->> c : Bye c !
  ");
        assert_eq!(ast.created_actor("c"), Some(1));
        assert_eq!(ast.destroyed_actor("c"), Some(3));
    }
}

mod sequence_db_class {
    use super::*;

    #[test]
    fn should_parse_a_message_with_a_trailing_colon_but_no_content() {
        let ast = ast("
sequenceDiagram
Alice->>Bob:
Bob->>Alice:Got it!
");
        assert_eq!(ast.messages.len(), 2);
        assert_eq!(ast.messages[0].message.as_str(), "");
        assert_eq!(ast.messages[0].from.as_deref(), Some("Alice"));
        assert_eq!(ast.messages[0].to.as_deref(), Some("Bob"));
    }

    #[test]
    fn should_parse_the_diagonal_arrows() {
        let ast = ast("
    sequenceDiagram
    actor Bob
    actor Alice
        Bob -|\\ Alice: Hello Alice, how are you?
        Bob -|/ Alice: Hello Alice, how are you?
        Bob -// Alice: Hello Alice, how are you?
        Bob -\\\\ Alice: Hello Alice, how are you?
        
        Bob \\|- Alice: Hello Alice, how are you?
        Bob /|- Alice: Hello Alice, how are you?
        Bob //- Alice: Hello Alice, how are you?
        Bob \\\\- Alice: Hello Alice, how are you?        
    ");
        assert_eq!(
            types(&ast),
            vec![
                LineType::SolidTop,
                LineType::SolidBottom,
                LineType::StickBottom,
                LineType::StickTop,
                LineType::SolidBottomReverse,
                LineType::SolidTopReverse,
                LineType::StickTopReverse,
                LineType::StickBottomReverse,
            ]
        );
    }

    #[test]
    fn should_parse_a_central_connection_with_bidirectional_arrows() {
        let ast = ast("
    sequenceDiagram
    actor Bob
    actor Alice
      Alice ()<<->>() Bob: hey?
    ");
        assert_eq!(ast.messages.len(), 3);
    }

    #[test]
    fn should_parse_participants_with_different_quote_styles_and_whitespace() {
        let ast = ast("
  sequenceDiagram
      participant Alice@{ \"type\" : \"database\" }
      participant Bob@{ \"type\" : \"database\" }
      participant Carl@{ type: \"database\" }
      participant David@{ \"type\" : 'database' }
      participant Eve@{ type: 'database' }
      participant Favela@{ \"type\" : \"database\"    }
      Bob->>+Alice: Hi Alice
      Alice->>+Bob: Hi Bob
    ");
        for id in ["Alice", "Bob", "Carl", "David", "Eve", "Favela"] {
            assert_eq!(actor(&ast, id).actor_type, "database", "actor {id}");
            assert_eq!(actor(&ast, id).description, id);
        }
        let messages = &ast.messages;
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].from.as_deref(), Some("Bob"));
        assert_eq!(messages[0].to.as_deref(), Some("Alice"));
        assert_eq!(messages[0].message.as_str(), "Hi Alice");
        assert_eq!(messages[2].from.as_deref(), Some("Alice"));
        assert_eq!(messages[2].to.as_deref(), Some("Bob"));
        assert_eq!(messages[2].message.as_str(), "Hi Bob");
    }

    #[test]
    fn should_parse_a_participant_config_object_separated_by_whitespace() {
        let ast = ast("
  sequenceDiagram
      participant Gina@{ \"type\" : \"database\" }
      participant Harry @{ \"type\" : \"database\" }
    ");
        assert_eq!(actor(&ast, "Gina").actor_type, "database");
        assert_eq!(actor(&ast, "Gina").description, "Gina");
        assert_eq!(actor(&ast, "Harry").actor_type, "database");
        assert_eq!(actor(&ast, "Harry").description, "Harry");
    }

    #[test]
    fn should_parse_mixed_participant_types_with_extended_syntax() {
        let ast = ast("
    sequenceDiagram
        participant lead
        participant dsa@{ \"type\" : \"queue\" }
        API->>+Database: getUserb
        Database-->>-API: userb
        dsa --> Database: hello
");
        assert_eq!(actor(&ast, "lead").actor_type, "participant");
        assert_eq!(actor(&ast, "dsa").actor_type, "queue");
        assert_eq!(actor(&ast, "API").actor_type, "participant");
        assert_eq!(actor(&ast, "Database").actor_type, "participant");
        assert_eq!(ast.messages.len(), 5);
        assert_eq!(ast.messages[0].from.as_deref(), Some("API"));
        assert_eq!(ast.messages[0].to.as_deref(), Some("Database"));
        assert_eq!(ast.messages[0].message.as_str(), "getUserb");
        assert!(ast.messages[0].activate);
        assert_eq!(ast.messages[2].from.as_deref(), Some("Database"));
        assert_eq!(ast.messages[2].to.as_deref(), Some("API"));
        assert_eq!(ast.messages[2].message.as_str(), "userb");
        assert_eq!(ast.messages[4].from.as_deref(), Some("dsa"));
        assert_eq!(ast.messages[4].to.as_deref(), Some("Database"));
        assert_eq!(ast.messages[4].message.as_str(), "hello");
    }

    #[test]
    fn should_parse_hyphenated_names_with_an_attached_config_object() {
        let ast = ast("
    sequenceDiagram
        actor lead-actor@{ \"type\" : \"database\" }
        participant order-svc@{ \"type\" : \"queue\" }
        actor kebab-case@{ \"type\" : \"control\" } as Kebab-Cased
        participant web-ui@{ \"type\" : \"entity\" } as Web UI
    ");
        assert_eq!(actor(&ast, "lead-actor").actor_type, "database");
        assert_eq!(actor(&ast, "lead-actor").description, "lead-actor");
        assert_eq!(actor(&ast, "order-svc").actor_type, "queue");
        assert_eq!(actor(&ast, "kebab-case").actor_type, "control");
        assert_eq!(actor(&ast, "kebab-case").description, "Kebab-Cased");
        assert_eq!(actor(&ast, "web-ui").actor_type, "entity");
        assert_eq!(actor(&ast, "web-ui").description, "Web UI");
    }

    #[test]
    fn should_fail_for_malformed_json_in_a_participant_definition() {
        failure(
            "
    sequenceDiagram
      participant D@{ \"type: \"entity\" }
      participant E@{ \"type\": \"dat
      abase }
  ",
        );
    }

    #[test]
    fn should_fail_for_a_missing_colon_separator() {
        failure(
            "
    sequenceDiagram
      participant C@{ \"type\" \"control\" }
      C ->> C: action
  ",
        );
    }

    #[test]
    fn should_fail_for_a_missing_closing_brace() {
        failure(
            "
    sequenceDiagram
      participant E@{ \"type\": \"entity\"
      E ->> E: process
  ",
        );
    }

    #[test]
    fn should_parse_boundary_participant() {
        let ast = ast("
          sequenceDiagram
          participant boundary@{ \"type\" : \"boundary\" }
          boundary->boundary: test
      ");
        assert_eq!(actor(&ast, "boundary").actor_type, "boundary");
        assert_eq!(actor(&ast, "boundary").description, "boundary");
    }

    #[test]
    fn should_parse_control_participant() {
        let ast = ast("
        sequenceDiagram
         participant C@{ \"type\" : \"control\" }
        C->C: test
        ");
        assert_eq!(actor(&ast, "C").actor_type, "control");
        assert_eq!(actor(&ast, "C").description, "C");
    }

    #[test]
    fn should_parse_entity_participant() {
        let ast = ast("
      sequenceDiagram
      participant E@{ \"type\" : \"entity\" }
      E->E: test
      ");
        assert_eq!(actor(&ast, "E").actor_type, "entity");
        assert_eq!(actor(&ast, "E").description, "E");
    }

    #[test]
    fn should_parse_database_participant() {
        let ast = ast("
      sequenceDiagram
      participant D@{ \"type\" : \"database\" }
      D->D: test
      ");
        assert_eq!(actor(&ast, "D").actor_type, "database");
        assert_eq!(actor(&ast, "D").description, "D");
    }

    #[test]
    fn should_parse_collections_participant() {
        let ast = ast("
      sequenceDiagram
      participant L@{ \"type\" : \"collections\" }
      L->L: test
      ");
        assert_eq!(actor(&ast, "L").actor_type, "collections");
        assert_eq!(actor(&ast, "L").description, "L");
    }

    #[test]
    fn should_parse_queue_participant() {
        let ast = ast("
      sequenceDiagram
      participant Q@{ \"type\" : \"queue\" }
      Q->Q: test 
      ");
        assert_eq!(actor(&ast, "Q").actor_type, "queue");
        assert_eq!(actor(&ast, "Q").description, "Q");
    }

    #[test]
    fn should_parse_actor_participant() {
        let ast = ast("
      sequenceDiagram
      participant A@{ \"type\" : \"queue\" }
      A->A: test
      ");
        assert_eq!(actor(&ast, "A").actor_type, "queue");
        assert_eq!(actor(&ast, "A").description, "A");
    }

    #[test]
    fn should_parse_participant_participant() {
        let ast = ast("
      sequenceDiagram
      participant P@{ \"type\" : \"database\" }
      P->P: test
      ");
        assert_eq!(actor(&ast, "P").actor_type, "database");
        assert_eq!(actor(&ast, "P").description, "P");
    }

    #[test]
    fn should_parse_boundary_using_the_actor_keyword() {
        let ast = ast("
      sequenceDiagram
        participant Alice@{ \"type\" : \"collections\" }
        participant Bob@{ \"type\" : \"control\" }
        Alice->>Bob: Hello Bob, how are you?
      ");
        assert_eq!(actor(&ast, "Alice").actor_type, "collections");
        assert_eq!(actor(&ast, "Bob").actor_type, "control");
        assert_eq!(actor(&ast, "Bob").description, "Bob");
    }

    #[test]
    fn should_parse_control_using_the_participant_keyword() {
        let ast = ast("
      sequenceDiagram
      participant C@{ \"type\" : \"control\" }
      C->C: test
      ");
        assert_eq!(actor(&ast, "C").actor_type, "control");
        assert_eq!(actor(&ast, "C").description, "C");
    }

    #[test]
    fn should_parse_entity_using_the_actor_keyword() {
        let ast = ast("
      sequenceDiagram
      participant E@{ \"type\" : \"entity\" }
      E->E: test
      ");
        assert_eq!(actor(&ast, "E").actor_type, "entity");
        assert_eq!(actor(&ast, "E").description, "E");
    }

    #[test]
    fn should_fail_when_the_alias_token_conflicts_in_a_participant_definition() {
        failure(
            "
        sequenceDiagram
        participant SAS MyServiceWithMoreThan20Chars <br> service decription
       ",
        );
    }

    #[test]
    fn should_not_hang_when_as_is_used_without_a_space_before_the_alias() {
        let error = failure(
            "
          sequenceDiagram
          participant X_AutoPublishable asAAAAAAAAAAAAA:AAAAAAAAAAAAA
        ",
        );
        assert!(!error.message.contains("Lexical error"));
    }

    #[test]
    fn should_parse_a_participant_with_a_stereotype_and_alias() {
        let ast = ast("
      sequenceDiagram
      participant Alice@{ \"type\" : \"boundary\" } as Public API
      participant Bob@{ \"type\" : \"control\" } as Controller
      Alice->>Bob: Request
      Bob-->>Alice: Response
      ");
        assert_eq!(actor(&ast, "Alice").actor_type, "boundary");
        assert_eq!(actor(&ast, "Alice").description, "Public API");
        assert_eq!(actor(&ast, "Bob").actor_type, "control");
        assert_eq!(actor(&ast, "Bob").description, "Controller");
    }

    #[test]
    fn should_parse_an_actor_with_a_stereotype_and_alias() {
        let ast = ast("
      sequenceDiagram
      actor A@{ \"type\" : \"database\" } AS Database Server
      actor B@{ \"type\" : \"queue\" } as Message Queue
      A->>B: Send message
      ");
        assert_eq!(actor(&ast, "A").actor_type, "database");
        assert_eq!(actor(&ast, "A").description, "Database Server");
        assert_eq!(actor(&ast, "B").actor_type, "queue");
        assert_eq!(actor(&ast, "B").description, "Message Queue");
    }

    #[test]
    fn should_parse_a_participant_with_a_stereotype_and_simple_alias() {
        let ast = ast("
      sequenceDiagram
      participant API@{ \"type\" : \"boundary\" } AS Public API
      API->>API: test
      ");
        assert_eq!(actor(&ast, "API").actor_type, "boundary");
        assert_eq!(actor(&ast, "API").description, "Public API");
    }

    #[test]
    fn should_parse_a_participant_with_an_inline_alias() {
        let ast = ast("
      sequenceDiagram
      participant API@{ \"type\" : \"boundary\", \"alias\": \"Public API\" }
      participant Auth@{ \"type\" : \"control\", \"alias\": \"Auth Controller\" }
      API->>Auth: Request
      Auth-->>API: Response
      ");
        assert_eq!(actor(&ast, "API").actor_type, "boundary");
        assert_eq!(actor(&ast, "API").description, "Public API");
        assert_eq!(actor(&ast, "Auth").actor_type, "control");
        assert_eq!(actor(&ast, "Auth").description, "Auth Controller");
    }

    #[test]
    fn should_parse_an_actor_with_an_inline_alias() {
        let ast = ast("
      sequenceDiagram
      actor U@{ \"type\" : \"actor\", \"alias\": \"End User\" }
      actor DB@{ \"type\" : \"database\", \"alias\": \"User Database\" }
      U->>DB: Query
      DB-->>U: Result
      ");
        assert_eq!(actor(&ast, "U").actor_type, "actor");
        assert_eq!(actor(&ast, "U").description, "End User");
        assert_eq!(actor(&ast, "DB").actor_type, "database");
        assert_eq!(actor(&ast, "DB").description, "User Database");
    }

    #[test]
    fn should_prioritize_the_external_alias_over_the_inline_alias() {
        let ast = ast("
      sequenceDiagram
      participant API@{ \"type\" : \"boundary\", \"alias\": \"Internal Name\" } as External Name
      API->>API: test
      ");
        assert_eq!(actor(&ast, "API").actor_type, "boundary");
        assert_eq!(actor(&ast, "API").description, "External Name");
    }

    #[test]
    fn should_handle_a_participant_with_only_an_inline_alias() {
        let ast = ast("
      sequenceDiagram
      participant API@{ \"alias\": \"Public API\" }
      API->>API: test
      ");
        assert_eq!(actor(&ast, "API").description, "Public API");
    }

    #[test]
    fn should_handle_mixed_inline_and_external_alias_syntax() {
        let ast = ast("
      sequenceDiagram
      participant A@{ \"type\" : \"boundary\", \"alias\": \"Service A\" }
      participant B@{ \"type\" : \"control\" } as Service B
      participant C@{ \"type\" : \"database\" }
      A->>B: Request
      B->>C: Query
      ");
        assert_eq!(actor(&ast, "A").actor_type, "boundary");
        assert_eq!(actor(&ast, "A").description, "Service A");
        assert_eq!(actor(&ast, "B").actor_type, "control");
        assert_eq!(actor(&ast, "B").description, "Service B");
        assert_eq!(actor(&ast, "C").actor_type, "database");
        assert_eq!(actor(&ast, "C").description, "C");
    }
}

mod rendering_but_database_only {
    use super::*;

    #[test]
    fn should_handle_the_same_actor_with_different_whitespace() {
        let ast = ast("
sequenceDiagram
participant Alice
participant Alice
participant Alice
");
        assert_eq!(ast.actor_keys(), vec!["Alice"]);
    }

    #[test]
    fn should_allow_proto_and_constructor_as_actor_names() {
        for name in ["__proto__", "constructor"] {
            let source = format!(
                "
sequenceDiagram
{name}-->>A: Hello, how are you?"
            );
            parse(&source).unwrap_or_else(|error| panic!("{name}: {error}"));
        }
    }

    #[test]
    fn should_wrap_an_inline_marker_without_rendering() {
        let ast = ast(
            "
sequenceDiagram
Alice->>Bob:wrap: Hello Bob, how are you? If you are not available right now, I can leave you a message. Please get back to me as soon as you can!
Note left of Alice: Bob thinks
Bob->>Alice: Fine!",
        );
        assert!(ast.messages[0].wrap);
    }
}

mod grammar_details {
    use super::*;

    #[test]
    fn line_type_codes_match_upstream() {
        assert_eq!(LineType::Solid as u8, 0);
        assert_eq!(LineType::Dotted as u8, 1);
        assert_eq!(LineType::Note as u8, 2);
        assert_eq!(LineType::ActiveStart as u8, 17);
        assert_eq!(LineType::ActiveEnd as u8, 18);
        assert_eq!(LineType::Autonumber as u8, 26);
        assert_eq!(LineType::ParOverStart as u8, 32);
        assert_eq!(LineType::SolidTop as u8, 41);
        assert_eq!(LineType::StickBottomReverseDotted as u8, 58);
        assert_eq!(LineType::CentralConnectionDual as u8, 61);
    }

    #[test]
    fn autonumber_alone_and_off_keep_their_counter_instructions() {
        let ast = ast("sequenceDiagram
autonumber
Alice->>Bob: one
autonumber off
Alice->>Bob: two");
        assert_eq!(
            ast.messages[0].message,
            MessageText::SequenceIndex {
                start: None,
                step: None,
                visible: true
            }
        );
        assert_eq!(
            ast.messages[2].message,
            MessageText::SequenceIndex {
                start: None,
                step: None,
                visible: false
            }
        );
    }

    #[test]
    fn the_body_must_start_with_the_header() {
        let error = failure(
            "
graph TD
Alice->>Bob: hi",
        );
        assert!(error.message.contains("'sequenceDiagram'"));
        assert_eq!(error.span.unwrap().line, 2);
    }

    #[test]
    fn parse_error_spans_point_at_the_line_and_column() {
        let error = failure(
            "sequenceDiagram
Alice->>Bob: hi
this is not a signal",
        );
        assert_eq!(error.span.unwrap().line, 3);

        let error = failure("sequenceDiagram\nparticipant");
        let span = error.span.unwrap();
        assert_eq!(span.line, 2);
        assert_eq!(span.column, 12);
    }

    #[test]
    fn actors_link_to_their_neighbours() {
        let ast = ast("sequenceDiagram\nparticipant A\nparticipant B\nparticipant C");
        assert_eq!(actor(&ast, "A").next_actor.as_deref(), Some("B"));
        assert_eq!(actor(&ast, "B").prev_actor.as_deref(), Some("A"));
        assert_eq!(actor(&ast, "B").next_actor.as_deref(), Some("C"));
        assert_eq!(actor(&ast, "C").prev_actor.as_deref(), Some("B"));
        assert_eq!(actor(&ast, "C").next_actor, None);
    }

    #[test]
    fn participant_names_may_contain_spaces() {
        let ast = ast("sequenceDiagram
participant Alice in Wonderland
Alice in Wonderland->>Bob: hi");
        assert_eq!(
            actor(&ast, "Alice in Wonderland").description,
            "Alice in Wonderland"
        );
        assert_eq!(ast.messages[0].from.as_deref(), Some("Alice in Wonderland"));
    }

    #[test]
    fn a_participant_cannot_change_boxes() {
        let error = failure(
            "sequenceDiagram
box one
participant a
end
box two
participant a
end",
        );
        assert!(error.message.contains("only be defined in one Box"));
    }

    #[test]
    fn a_created_actor_needs_its_creating_message() {
        let error = failure(
            "sequenceDiagram
create participant c
a->>b: hello",
        );
        assert!(
            error
                .message
                .contains("does not have an associated creating message")
        );
    }

    #[test]
    fn a_destroyed_actor_needs_its_destroying_message() {
        let error = failure(
            "sequenceDiagram
destroy a
b->>c: hello",
        );
        assert!(
            error
                .message
                .contains("does not have an associated destroying message")
        );
    }

    #[test]
    fn an_actor_cannot_be_created_twice() {
        let error = failure(
            "sequenceDiagram
create participant c
b->>c: hello
create participant c",
        );
        assert!(error.message.contains("same id"));
    }
}
