//! Tests ported from the upstream packet specs: `packages/parser/tests/packet.test.ts` (the
//! Langium grammar) and `packages/mermaid/src/diagrams/packet/packet.spec.ts` (the database).
//! Inputs and expectations are upstream's.
//!
//! Skipped, and why:
//! - `expectNoErrorsOrAlternatives` checks Langium's lexer/parser error arrays and that no
//!   `console.log` ran; here `parse` returning `Ok` is the error-free assertion, and there is no
//!   console.
//! - `result.value.$type` is Langium's generated AST type marker, not database data.
//! - `packetRenderer.spec.ts` renders: it reads the drawn SVG back (each block's `x` and
//!   `width`, each bit number's position and text, `bitOrder`, and `showBits`). The parser and
//!   database tests here stay on the AST; `render.rs` ports the renderer, and its fixture test
//!   covers `bitOrder` and `showBits`. A configured `bitsPerRow` is still not read by the
//!   parser, which this crate documents in `mod.rs`.

use pretty_assertions::assert_eq;

use super::{Ast, Block, parse};
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

fn block(start: i64, end: i64, bits: i64, label: &str) -> Block {
    Block {
        start,
        end,
        bits,
        label: label.to_string(),
    }
}

// packages/parser/tests/packet.test.ts

#[test]
fn parses_the_header_with_or_without_surrounding_whitespace() {
    for body in [
        "packet-beta",
        "  packet-beta  ",
        "\tpacket-beta\t",
        "\n    \tpacket-beta\n    ",
        "packet",
        "  packet  ",
        "\tpacket\t",
        "\n    \tpacket\n    ",
    ] {
        assert!(ast(body).packet.is_empty(), "{body:?}");
    }
}

// packages/mermaid/src/diagrams/packet/packet.spec.ts

#[test]
fn handles_a_packet_beta_definition() {
    let ast = ast("packet-beta");
    assert!(ast.packet.is_empty());
}

#[test]
fn handles_a_packet_definition() {
    let ast = ast("packet");
    assert!(ast.packet.is_empty());
}

#[test]
fn handles_a_diagram_with_data_and_title() {
    let ast = ast("packet
    title Packet diagram
    accTitle: Packet accTitle
    accDescr: Packet accDescription
    0-10: \"test\"
    ");
    assert_eq!(ast.diagram_title, "Packet diagram");
    assert_eq!(ast.acc_title, "Packet accTitle");
    assert_eq!(ast.acc_description, "Packet accDescription");
    assert_eq!(ast.packet, vec![vec![block(0, 10, 11, "test")]]);
}

#[test]
fn handles_single_bits() {
    let ast = ast("packet
    0-10: \"test\"
    11: \"single\"
    ");
    assert_eq!(
        ast.packet,
        vec![vec![block(0, 10, 11, "test"), block(11, 11, 1, "single")]]
    );
}

#[test]
fn handles_bit_counts() {
    let ast = ast("packet
    +8: \"byte\"
    +16: \"word\"
    ");
    assert_eq!(
        ast.packet,
        vec![vec![block(0, 7, 8, "byte"), block(8, 23, 16, "word")]]
    );
}

#[test]
fn handles_bit_counts_with_bit_or_bits() {
    let ast = ast("packet
    +8: \"byte\"
    +16: \"word\"
    ");
    assert_eq!(
        ast.packet,
        vec![vec![block(0, 7, 8, "byte"), block(8, 23, 16, "word")]]
    );
}

#[test]
fn splits_into_multiple_rows() {
    let ast = ast("packet
    0-10: \"test\"
    11-90: \"multiple\"
    ");
    assert_eq!(
        ast.packet,
        vec![
            vec![block(0, 10, 11, "test"), block(11, 31, 20, "multiple")],
            vec![block(32, 63, 31, "multiple")],
            vec![block(64, 90, 26, "multiple")],
        ]
    );
}

#[test]
fn splits_into_multiple_rows_when_cut_at_exact_length() {
    let ast = ast("packet
    0-16: \"test\"
    17-63: \"multiple\"
    ");
    assert_eq!(
        ast.packet,
        vec![
            vec![block(0, 16, 17, "test"), block(17, 31, 14, "multiple")],
            vec![block(32, 63, 31, "multiple")],
        ]
    );
}

#[test]
fn throws_if_numbers_are_not_continuous() {
    let error = error(
        "packet
    0-16: \"test\"
    18-20: \"error\"
    ",
    );
    assert_eq!(
        error.message,
        "Packet block 18 - 20 is not contiguous. It should start from 17."
    );
}

#[test]
fn throws_if_numbers_are_not_continuous_with_bit_counts() {
    let error = error(
        "packet
    +16: \"test\"
    18-20: \"error\"
    ",
    );
    assert_eq!(
        error.message,
        "Packet block 18 - 20 is not contiguous. It should start from 16."
    );
}

#[test]
fn throws_if_numbers_are_not_continuous_for_single_packets() {
    let error = error(
        "packet
    0-16: \"test\"
    18: \"error\"
    ",
    );
    assert_eq!(
        error.message,
        "Packet block 18 - 18 is not contiguous. It should start from 17."
    );
}

#[test]
fn throws_if_numbers_are_not_continuous_for_single_packets_with_bit_counts() {
    let error = error(
        "packet
    +16: \"test\"
    18: \"error\"
    ",
    );
    assert_eq!(
        error.message,
        "Packet block 18 - 18 is not contiguous. It should start from 16."
    );
}

#[test]
fn throws_if_numbers_are_not_continuous_for_single_packets_2() {
    let error = error(
        "packet
    0-16: \"test\"
    17: \"good\"
    19: \"error\"
    ",
    );
    assert_eq!(
        error.message,
        "Packet block 19 - 19 is not contiguous. It should start from 18."
    );
}

#[test]
fn throws_if_end_is_less_than_start() {
    let error = error(
        "packet
    0-16: \"test\"
    25-20: \"error\"
    ",
    );
    assert_eq!(
        error.message,
        "Packet block 25 - 20 is invalid. End must be greater than start."
    );
}

#[test]
fn throws_if_bit_count_is_zero() {
    let error = error(
        "packet
    +0: \"test\"
    ",
    );
    assert_eq!(
        error.message,
        "Packet block 0 is invalid. Cannot have a zero bit field."
    );
}

// Behavior of the grammar, the lexer, and the database the specs above do not pin down.

#[test]
fn a_contiguity_error_reports_the_block_number_span() {
    let error = error(
        "packet
    0-16: \"test\"
    18-20: \"error\"
",
    );
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (3, 5));
}

#[test]
fn a_zero_bit_count_reports_its_number_span() {
    let error = error("packet\n  +0: \"test\"");
    let span = error.span.expect("the error carries a span");
    assert_eq!((span.line, span.column), (2, 4));
}

#[test]
fn a_later_title_replaces_an_earlier_one() {
    let ast = ast("packet\n  title first\n  title second");
    assert_eq!(ast.diagram_title, "second");
}

#[test]
fn a_title_without_text_stays_empty() {
    let ast = ast("packet title");
    assert_eq!(ast.diagram_title, "");
}

#[test]
fn comments_are_skipped() {
    let ast = ast("packet\n%% a comment\n0-7: \"x\" %% trailing\n%% another");
    assert_eq!(ast.packet, vec![vec![block(0, 7, 8, "x")]]);
}

#[test]
fn parses_crlf_line_endings() {
    let ast = ast("packet\r\n  0-7: \"x\"\r\n");
    assert_eq!(ast.packet, vec![vec![block(0, 7, 8, "x")]]);
}

#[test]
fn a_label_interprets_backslash_escapes() {
    let ast = ast("packet\n  0: \"a \\\"quoted\\\" name\"");
    assert_eq!(ast.packet[0][0].label, "a \"quoted\" name");
}

#[test]
fn an_empty_label_is_kept() {
    let ast = ast("packet\n  0: \"\"");
    assert_eq!(ast.packet, vec![vec![block(0, 0, 1, "")]]);
}

#[test]
fn whitespace_around_a_blocks_punctuation_is_ignored() {
    let ast = ast("packet\n  0 - 10 :  \"x\"  ");
    assert_eq!(ast.packet, vec![vec![block(0, 10, 11, "x")]]);
}

#[test]
fn a_block_may_end_the_input_without_a_newline() {
    let ast = ast("packet\n  0-7: \"x\"");
    assert_eq!(ast.packet, vec![vec![block(0, 7, 8, "x")]]);
}

#[test]
fn a_block_without_a_label_is_rejected() {
    let error = error("packet\n  0-7:");
    assert!(error.message.contains("STRING"), "{}", error.message);
}

#[test]
fn a_missing_header_is_rejected() {
    assert!(parse("0-7: \"x\"").is_err());
}

#[test]
fn a_header_keyword_must_stand_alone() {
    assert!(parse("packetx").is_err());
    assert!(parse("packet-betaX").is_err());
}

#[test]
fn a_multi_line_acc_description_is_normalized() {
    let ast = ast("packet
accDescr {
            a neat description
            on two lines
        }
0-7: \"x\"
");
    assert_eq!(ast.acc_description, "a neat description\non two lines");
}

#[test]
fn accessibility_strings_are_sanitized() {
    let sanitized = ast("packet\n  accTitle: <script>alert(1)</script>name");
    assert_eq!(sanitized.acc_title, "name");
    let escaped = ast("packet\n  title a < b");
    assert_eq!(escaped.diagram_title, "a &lt; b");
}

#[test]
fn caps_the_packet_at_ten_thousand_rows() {
    // `maxPacketSize`: a million bits need 31 250 rows, and `populate` stops at 10 000.
    let ast = ast("packet\n+1000000: \"huge\"");
    assert_eq!(ast.packet.len(), 10_000);
    assert_eq!(ast.packet[0], vec![block(0, 31, 31, "huge")]);
}

#[test]
fn the_crate_parses_a_packet_diagram() {
    match crate::parse("packet\n0-7: \"x\"") {
        Ok(crate::Diagram::Packet(ast)) => {
            assert_eq!(ast.packet, vec![vec![block(0, 7, 8, "x")]]);
        }
        other => panic!("expected a packet diagram, got {other:?}"),
    }
}
