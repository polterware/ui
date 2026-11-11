//! The lexer rules the grammar specs exercise indirectly.

use super::super::lexer::{Lexer, TokenKind};

fn kinds(source: &str) -> Vec<TokenKind> {
    let mut lexer = Lexer::new(source);
    let mut kinds = Vec::new();
    loop {
        let token = lexer.next_token().expect("lexes");
        let done = token.kind == TokenKind::Eof;
        kinds.push(token.kind);
        if done {
            break;
        }
    }
    kinds
}

#[test]
fn lexes_a_header_and_two_nodes() {
    assert_eq!(
        kinds("graph TD;A-->B;"),
        vec![
            TokenKind::Graph,
            TokenKind::Dir,
            TokenKind::Semi,
            TokenKind::NodeString,
            TokenKind::Link,
            TokenKind::NodeString,
            TokenKind::Semi,
            TokenKind::Eof,
        ]
    );
}
