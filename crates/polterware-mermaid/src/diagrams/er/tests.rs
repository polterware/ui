//! Ports of upstream `erDiagram.spec.js`, `parser/subgraph.spec.js`, and `erDb.spec.js`
//! (mermaid 12.3.0): every input is upstream's, and every assertion is upstream's unless it
//! needs a renderer or a configuration value.
//!
//! Skipped upstream tests, and why:
//! - `[skipped test] can contain àáâäæãåā`: skipped upstream too (jison cannot handle it).
//! - The `cannot start with the character` case of the entity-name character loop is a
//!   tautology upstream: it parses the *repeated* quoted name and asserts it is absent, so it
//!   passes only because that inner assertion throws. There is no expectation to keep.
//! - The `fails if not surrounded by quotes` case is likewise vacuous for the characters an
//!   unquoted name may contain (`-`, `*`, `.`, and non-ASCII). The port keeps the cases that
//!   genuinely fail.
//! - `should limit the number of relationships...` and `...unless the roles are different`
//!   have empty bodies (`/* TODO */`) upstream.
//! - `look`, `getConfig()`, and the `config` field of `getData` are configuration, which the
//!   parser does not receive.
//! - `getData().other` and the renderer-only state have no getter and are not modelled.

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

fn entity<'a>(ast: &'a Ast, name: &str) -> &'a Entity {
    ast.entity(name)
        .unwrap_or_else(|| panic!("missing entity {name}"))
}

fn relationship(ast: &Ast, index: usize) -> &Relationship {
    ast.relationships
        .get(index)
        .unwrap_or_else(|| panic!("missing relationship {index}"))
}

/// The characters upstream's `has non A-Za-z0-9_- chars` test feeds into and around an
/// entity name. The source separates them with spaces; empty pieces (upstream's double
/// space) are dropped.
const ENTITY_NAME_CHARS: &str = "~ ` ! @ # $ ^ & * ( ) - = + [ ] { } | / ; : ' . ? ¡ ⁄ ™ € £ ‹ ¢ › ∞ ﬁ § ‡ • ° ª · º ‚ ≠ ± œ Œ ∑ „ ® † ˇ ¥ Á ¨ ˆ ˆ Ø π ∏ “ « » å Å ß Í ∂ Î ƒ Ï © ˙ Ó ∆ Ô ˚ ¬ Ò … Ú æ Æ Ω ¸ ≈ π ˛ ç Ç √ ◊ ∫ ı ˜ µ Â ≤ ¯ ≥ ˘ ÷ ¿";

fn entity_name_chars() -> Vec<&'static str> {
    ENTITY_NAME_CHARS
        .split(' ')
        .filter(|piece| !piece.is_empty())
        .collect()
}

mod entity_names {
    use super::*;

    #[test]
    fn allows_stand_alone_entities_with_no_relationships() {
        let ast = ast("erDiagram\nISLAND\nMAINLAND");
        assert_eq!(ast.entities.len(), 2);
        assert!(ast.entity("ISLAND").is_some());
        assert!(ast.entity("MAINLAND").is_some());
        assert!(ast.relationships.is_empty());
    }

    #[test]
    fn cannot_be_empty_quotes() {
        let error = failure("erDiagram\n \"\"\n");
        assert_eq!(
            error.span.map(|span| (span.line, span.column)),
            Some((2, 2))
        );
    }

    #[test]
    fn cannot_contain_percent() {
        assert!(parse("erDiagram\n \"Blo%rf\"\n").is_err());
    }

    #[test]
    fn cannot_contain_backslash() {
        assert!(parse("erDiagram\n \"Blo\\rf\"\n").is_err());
    }

    #[test]
    fn cannot_contain_newline_backspace_or_vertical_tab() {
        for bad in ['\n', '\r', '\u{0008}', '\u{000b}'] {
            let source = format!("erDiagram\n \"Blo{bad}rf\"\n");
            assert!(parse(&source).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn unquoted_names_reject_ascii_punctuation() {
        for ch in entity_name_chars() {
            let Some(character) = ch.chars().next() else {
                continue;
            };
            let allowed = character.is_ascii_alphanumeric()
                || matches!(character, '-' | '*' | '.')
                || !character.is_ascii();
            if allowed {
                continue;
            }
            let source = format!("erDiagram\n Blo{ch}rf\n");
            assert!(parse(&source).is_err(), "accepted {ch:?}");
        }
    }

    #[test]
    fn quoted_names_allow_any_single_occurrence() {
        for ch in entity_name_chars() {
            let name = format!("Blo{ch}rf");
            let source = format!("erDiagram\n \"{name}\"\n");
            let ast = ast(&source);
            assert!(ast.entity(&name).is_some(), "missing {name:?}");
        }
    }

    #[test]
    fn quoted_names_allow_a_repeated_occurrence() {
        for ch in entity_name_chars() {
            let name = format!("Blo{ch}{ch}rf");
            let source = format!("erDiagram\n \"{name}\"\n");
            let ast = ast(&source);
            assert!(ast.entity(&name).is_some(), "missing {name:?}");
        }
    }

    #[test]
    fn quoted_names_allow_a_trailing_character() {
        for ch in entity_name_chars() {
            let name = format!("Blorf{ch}");
            let source = format!("erDiagram\n \"{name}\"\n");
            let ast = ast(&source);
            assert!(ast.entity(&name).is_some(), "missing {name:?}");
        }
    }

    #[test]
    fn quoted_names_allow_all_the_characters_at_once() {
        let combined: String = entity_name_chars().concat();
        let name = format!("a{combined}");
        let source = format!("erDiagram\n \"{name}\"\n");
        let ast = ast(&source);
        assert!(ast.entity(&name).is_some());
    }

    #[test]
    fn can_contain_hyphen_and_underscore_without_quotes() {
        let ast = ast("erDiagram\nDUCK-BILLED_PLATYPUS");
        assert!(ast.entity("DUCK-BILLED_PLATYPUS").is_some());
    }

    #[test]
    fn can_have_an_alias() {
        let ast = ast("erDiagram\nfoo[\"bar\"]");
        assert!(ast.entity("foo").is_some());
        assert_eq!(entity(&ast, "foo").alias, "bar");
    }

    #[test]
    fn can_have_an_alias_when_the_relationship_comes_first() {
        let source = "erDiagram\nfoo ||--o| bar : rel\nbuzz foo[\"batman\"]\n";
        let ast = ast(source);
        assert_eq!(entity(&ast, "foo").alias, "batman");
        assert_eq!(entity(&ast, "bar").alias, "");
    }

    #[test]
    fn can_have_an_alias_when_the_relationship_comes_last() {
        let source = "erDiagram\nbuzz foo[\"batman\"]\nfoo ||--o| bar : rel\n";
        let ast = ast(source);
        assert_eq!(entity(&ast, "foo").alias, "batman");
        assert_eq!(entity(&ast, "bar").alias, "");
    }

    #[test]
    fn can_start_with_an_underscore() {
        let ast = ast("erDiagram\n_foo");
        assert!(ast.entity("_foo").is_some());
    }
}

mod attributes {
    use super::*;

    #[test]
    fn allow_alphanumerics_dashes_underscores_and_brackets_not_leading() {
        let source = "erDiagram\nBOOK {\nstring myBookTitle\nstring MYBOOKSUBTITLE_1\nstring author-ref[name](1)\n}";
        let ast = ast(source);
        assert_eq!(ast.entities.len(), 1);
        let attributes = &entity(&ast, "BOOK").attributes;
        assert_eq!(attributes.len(), 3);
        assert_eq!(attributes[0].name, "myBookTitle");
        assert_eq!(attributes[1].name, "MYBOOKSUBTITLE_1");
        assert_eq!(attributes[2].name, "author-ref[name](1)");
    }

    #[test]
    fn allow_an_asterisk_at_the_start_of_a_name() {
        let ast = ast("erDiagram\nBOOK{\nstring *title}");
        assert_eq!(ast.entities.len(), 1);
        assert_eq!(entity(&ast, "BOOK").attributes.len(), 1);
    }

    #[test]
    fn allow_asterisks_at_the_start_of_a_declared_name() {
        let ast = ast("erDiagram\nBOOK {\nid *the_Primary_Key}");
        assert_eq!(entity(&ast, "BOOK").attributes.len(), 1);
    }

    #[test]
    fn allow_commas_within_a_type() {
        let ast = ast("erDiagram\nBUILDING {\npublic.geometry(point,4326) location}");
        assert_eq!(ast.entities.len(), 1);
        let attribute = &entity(&ast, "BUILDING").attributes[0];
        assert_eq!(attribute.type_name, "public.geometry(point,4326)");
        assert_eq!(attribute.name, "location");
    }

    #[test]
    fn reject_leading_numbers_dashes_and_brackets() {
        for leading in ['0', '-', '[', ']', '(', ')'] {
            let source = format!("erDiagram\nBOOK {{\nstring {leading}author\n}}");
            assert!(parse(&source).is_err(), "accepted {leading:?}");
        }
    }

    #[test]
    fn allow_a_backtick_escaped_name() {
        let ast = ast("erDiagram\nHOTEL {\nstring `geo.accuracy`\n}");
        assert_eq!(ast.entities.len(), 1);
        assert_eq!(entity(&ast, "HOTEL").attributes[0].type_name, "string");
        assert_eq!(entity(&ast, "HOTEL").attributes[0].name, "geo.accuracy");
    }

    #[test]
    fn allow_a_backtick_escaped_name_with_a_key_and_comment() {
        let ast = ast("erDiagram\nHOTEL {\nstring `geo.accuracy` PK \"the comment\"\n}");
        let attribute = &entity(&ast, "HOTEL").attributes[0];
        assert_eq!(attribute.name, "geo.accuracy");
        assert_eq!(attribute.keys, ["PK"]);
        assert_eq!(attribute.comment, "the comment");
    }

    #[test]
    fn allow_a_backtick_escaped_type() {
        let ast = ast("erDiagram\nHOTEL {\n`my.type` address\n}");
        let attribute = &entity(&ast, "HOTEL").attributes[0];
        assert_eq!(attribute.type_name, "my.type");
        assert_eq!(attribute.name, "address");
    }

    #[test]
    fn allow_a_question_mark_suffix_on_a_type() {
        let ast = ast("erDiagram\nPERSON {\nstring? alias\n}");
        assert_eq!(entity(&ast, "PERSON").attributes[0].type_name, "string?");
        assert_eq!(entity(&ast, "PERSON").attributes[0].name, "alias");
    }

    #[test]
    fn allow_a_question_mark_suffix_with_a_key_and_comment() {
        let source = "erDiagram\nPERSON {\nstring? alias UK \"optional display name\"\n}";
        let ast = ast(source);
        let attribute = &entity(&ast, "PERSON").attributes[0];
        assert_eq!(attribute.type_name, "string?");
        assert_eq!(attribute.keys, ["UK"]);
        assert_eq!(attribute.comment, "optional display name");
    }

    #[test]
    fn allow_a_single_attribute() {
        let ast = ast("erDiagram\nBOOK {\nstring title\n}");
        assert_eq!(ast.entities.len(), 1);
        assert_eq!(entity(&ast, "BOOK").attributes.len(), 1);
    }

    #[test]
    fn allow_a_single_attribute_with_a_key() {
        let ast = ast("erDiagram\nBOOK {\nstring title PK\n}");
        assert_eq!(entity(&ast, "BOOK").attributes.len(), 1);
    }

    #[test]
    fn allow_a_single_attribute_with_a_comment() {
        let ast = ast("erDiagram\nBOOK {\nstring title \"comment\"\n}");
        assert_eq!(entity(&ast, "BOOK").attributes[0].comment, "comment");
    }

    #[test]
    fn allow_a_single_attribute_with_a_key_and_a_comment() {
        let ast = ast("erDiagram\nBOOK {\nstring title PK \"comment\"\n}");
        assert_eq!(entity(&ast, "BOOK").attributes.len(), 1);
    }

    #[test]
    fn allow_names_starting_with_fk_pk_or_uk() {
        let source = "erDiagram\nBOOK {\nint fk_title FK\nstring pk_author PK\nstring uk_address UK\nfloat pk_price PK \"comment\"\n}";
        let ast = ast(source);
        assert_eq!(entity(&ast, "BOOK").attributes.len(), 4);
    }

    #[test]
    fn allow_many_constraints_and_comments() {
        let source = "erDiagram\nCUSTOMER {\nint customer_number PK, FK \"comment1\"\ndatetime customer_status_start_datetime PK,UK, FK\ndatetime customer_status_end_datetime PK , UK \"comment3\"\nstring customer_firstname\nstring customer_lastname \"comment5\"\n}";
        let ast = ast(source);
        let attributes = &entity(&ast, "CUSTOMER").attributes;
        assert_eq!(attributes[0].keys, ["PK", "FK"]);
        assert_eq!(attributes[0].comment, "comment1");
        assert_eq!(attributes[1].keys, ["PK", "UK", "FK"]);
        assert_eq!(attributes[2].keys, ["PK", "UK"]);
        assert_eq!(attributes[2].comment, "comment3");
        assert!(attributes[3].keys.is_empty());
        assert!(attributes[4].keys.is_empty());
        assert_eq!(attributes[4].comment, "comment5");
    }

    #[test]
    fn allow_a_generic_type() {
        let source =
            "erDiagram\nBOOK {\ntype~T~ type\noption~T~ readable \"comment\"\nstring id PK\n}";
        let ast = ast(source);
        assert_eq!(ast.entities.len(), 1);
        assert_eq!(entity(&ast, "BOOK").attributes.len(), 3);
        assert_eq!(entity(&ast, "BOOK").attributes[0].type_name, "type~T~");
        assert_eq!(entity(&ast, "BOOK").attributes[0].name, "type");
    }

    #[test]
    fn allow_an_array_type() {
        let source = "erDiagram\nBOOK {\nstring[] readers FK \"comment\"\nstring[] authors FK\n}";
        let ast = ast(source);
        assert_eq!(ast.entities.len(), 1);
        assert_eq!(entity(&ast, "BOOK").attributes.len(), 2);
    }

    #[test]
    fn allow_a_limited_length_string() {
        let source =
            "erDiagram\nBOOK {\ncharacter(10) isbn FK\nvarchar(5) postal_code \"Five digits\"\n}";
        let ast = ast(source);
        let attributes = &entity(&ast, "BOOK").attributes;
        assert_eq!(attributes.len(), 2);
        assert_eq!(attributes[0].type_name, "character(10)");
        assert_eq!(attributes[1].type_name, "varchar(5)");
    }

    #[test]
    fn allow_multiple_attributes() {
        let source = "erDiagram\nBOOK {\nstring title\nstring author\nfloat price\n}";
        let ast = ast(source);
        assert_eq!(entity(&ast, "BOOK").attributes.len(), 3);
    }

    #[test]
    fn allow_attribute_blocks_to_be_split() {
        let source = "erDiagram\nBOOK {\nstring title\n}\nBOOK {\nstring author\nfloat price\n}";
        let ast = ast(source);
        assert_eq!(entity(&ast, "BOOK").attributes.len(), 3);
    }

    #[test]
    fn allow_an_empty_attribute_block() {
        let ast = ast("erDiagram\nBOOK {}");
        assert!(ast.entity("BOOK").is_some());
        assert!(entity(&ast, "BOOK").attributes.is_empty());
    }

    #[test]
    fn allow_a_block_immediately_after_the_entity_name() {
        let ast = ast("erDiagram\nBOOK{}");
        assert!(ast.entity("BOOK").is_some());
        assert!(entity(&ast, "BOOK").attributes.is_empty());
    }

    #[test]
    fn allow_spaces_between_the_entity_name_and_the_block() {
        let ast = ast("erDiagram\nBOOK         {}");
        assert!(ast.entity("BOOK").is_some());
        assert!(entity(&ast, "BOOK").attributes.is_empty());
    }

    #[test]
    fn allow_whitespace_around_attributes() {
        let ast = ast("erDiagram\nBOOK {\n  \n\n  string title\n\n  \n}");
        assert_eq!(ast.entities.len(), 1);
        assert_eq!(entity(&ast, "BOOK").attributes.len(), 1);
    }

    #[test]
    fn allow_no_whitespace_around_attributes() {
        let ast = ast("erDiagram\nBOOK{string title}");
        assert_eq!(ast.entities.len(), 1);
        assert_eq!(entity(&ast, "BOOK").attributes.len(), 1);
    }
}

mod relationships {
    use super::*;

    #[test]
    fn associates_two_entities() {
        let ast = ast("erDiagram\nCAR ||--o{ DRIVER : \"insured for\"");
        assert!(ast.entity("CAR").is_some());
        assert!(ast.entity("DRIVER").is_some());
        assert_eq!(ast.relationships.len(), 1);
        assert_eq!(
            relationship(&ast, 0).rel_spec.card_a,
            Cardinality::ZeroOrMore
        );
        assert_eq!(relationship(&ast, 0).rel_spec.card_b, Cardinality::OnlyOne);
        assert_eq!(
            relationship(&ast, 0).rel_spec.rel_type,
            Identification::Identifying
        );
    }

    #[test]
    fn does_not_create_duplicate_entities() {
        let source = "erDiagram\nCAR ||--o{ DRIVER : \"insured for\"\nDRIVER ||--|| LICENSE : has";
        let ast = ast(source);
        assert_eq!(ast.entities.len(), 3);
    }

    #[test]
    fn creates_the_declared_role() {
        let ast = ast("erDiagram\nTEACHER }o--o{ STUDENT : \"is teacher of\"");
        assert_eq!(relationship(&ast, 0).role_a, "is teacher of");
    }

    #[test]
    fn allows_recursive_relationships() {
        let ast = ast("erDiagram\nNODE ||--o{ NODE : \"leads to\"");
        assert_eq!(ast.entities.len(), 1);
    }

    #[test]
    fn allows_more_than_one_relationship_between_the_same_entities() {
        let source =
            "erDiagram\nCAR ||--o{ PERSON : \"insured for\"\nCAR }o--|| PERSON : \"owned by\"";
        let ast = ast(source);
        assert_eq!(ast.entities.len(), 2);
        assert_eq!(ast.relationships.len(), 2);
    }

    #[test]
    fn reads_every_cardinality_pair() {
        let cases = [
            ("A ||--|{ B", Cardinality::OneOrMore, Cardinality::OnlyOne),
            ("A ||..o{ B", Cardinality::ZeroOrMore, Cardinality::OnlyOne),
            (
                "A |o..o{ B",
                Cardinality::ZeroOrMore,
                Cardinality::ZeroOrOne,
            ),
            ("A |o--|{ B", Cardinality::OneOrMore, Cardinality::ZeroOrOne),
            ("A }|--|| B", Cardinality::OnlyOne, Cardinality::OneOrMore),
            ("A }o--|| B", Cardinality::OnlyOne, Cardinality::ZeroOrMore),
            (
                "A }o..o| B",
                Cardinality::ZeroOrOne,
                Cardinality::ZeroOrMore,
            ),
            ("A }|..o| B", Cardinality::ZeroOrOne, Cardinality::OneOrMore),
            ("A |o..|| B", Cardinality::OnlyOne, Cardinality::ZeroOrOne),
            ("A ||..|| B", Cardinality::OnlyOne, Cardinality::OnlyOne),
            ("A ||--o| B", Cardinality::ZeroOrOne, Cardinality::OnlyOne),
            ("A |o..o| B", Cardinality::ZeroOrOne, Cardinality::ZeroOrOne),
            (
                "A }o--o{ B",
                Cardinality::ZeroOrMore,
                Cardinality::ZeroOrMore,
            ),
            ("A }|..|{ B", Cardinality::OneOrMore, Cardinality::OneOrMore),
            (
                "A }o--|{ B",
                Cardinality::OneOrMore,
                Cardinality::ZeroOrMore,
            ),
            (
                "A }|..o{ B",
                Cardinality::ZeroOrMore,
                Cardinality::OneOrMore,
            ),
        ];
        for (line, card_a, card_b) in cases {
            let source = format!("erDiagram\n{line} : has");
            let ast = ast(&source);
            assert_eq!(ast.entities.len(), 2, "{line}");
            assert_eq!(ast.relationships.len(), 1, "{line}");
            assert_eq!(relationship(&ast, 0).rel_spec.card_a, card_a, "{line}");
            assert_eq!(relationship(&ast, 0).rel_spec.card_b, card_b, "{line}");
        }
    }

    #[test]
    fn reads_the_word_aliases() {
        let cases = [
            (
                "A one or zero to many B",
                Cardinality::ZeroOrMore,
                Cardinality::ZeroOrOne,
            ),
            (
                "A one or many optionally to zero or one B",
                Cardinality::ZeroOrOne,
                Cardinality::OneOrMore,
            ),
            (
                "A zero or more to zero or many B",
                Cardinality::ZeroOrMore,
                Cardinality::ZeroOrMore,
            ),
            (
                "A many(0) to many(1) B",
                Cardinality::OneOrMore,
                Cardinality::ZeroOrMore,
            ),
            (
                "A many optionally to one B",
                Cardinality::OnlyOne,
                Cardinality::ZeroOrMore,
            ),
            (
                "A only one optionally to 1+ B",
                Cardinality::OneOrMore,
                Cardinality::OnlyOne,
            ),
            (
                "A 0+ optionally to 1 B",
                Cardinality::OnlyOne,
                Cardinality::ZeroOrMore,
            ),
        ];
        for (line, card_a, card_b) in cases {
            let source = format!("erDiagram\n{line} : has");
            let ast = ast(&source);
            assert_eq!(ast.entities.len(), 2, "{line}");
            assert_eq!(ast.relationships.len(), 1, "{line}");
            assert_eq!(relationship(&ast, 0).rel_spec.card_a, card_a, "{line}");
            assert_eq!(relationship(&ast, 0).rel_spec.card_b, card_b, "{line}");
        }
    }

    #[test]
    fn reads_the_one_alias_before_a_relationship_operator() {
        let cases = [
            (
                "CUSTOMER 1--one or more DELIVERY-ADDRESS",
                Identification::Identifying,
            ),
            (
                "CUSTOMER 1..one or more DELIVERY-ADDRESS",
                Identification::NonIdentifying,
            ),
            (
                "CUSTOMER 1.-one or more DELIVERY-ADDRESS",
                Identification::NonIdentifying,
            ),
            (
                "CUSTOMER 1-.one or more DELIVERY-ADDRESS",
                Identification::NonIdentifying,
            ),
        ];
        for (line, rel_type) in cases {
            let source = format!("erDiagram\n{line} : has");
            let ast = ast(&source);
            assert_eq!(ast.entities.len(), 2, "{line}");
            assert_eq!(
                relationship(&ast, 0).rel_spec.card_a,
                Cardinality::OneOrMore
            );
            assert_eq!(relationship(&ast, 0).rel_spec.card_b, Cardinality::OnlyOne);
            assert_eq!(relationship(&ast, 0).rel_spec.rel_type, rel_type, "{line}");
        }
    }

    #[test]
    fn allows_a_numeric_entity_after_a_cardinality_one() {
        let ast = ast("erDiagram\na many to 1 1: label");
        assert_eq!(ast.entities.len(), 2);
        assert!(ast.entity("a").is_some());
        assert!(ast.entity("1").is_some());
        assert_eq!(ast.relationships.len(), 1);
        assert_eq!(relationship(&ast, 0).rel_spec.card_a, Cardinality::OnlyOne);
        assert_eq!(
            relationship(&ast, 0).rel_spec.card_b,
            Cardinality::ZeroOrMore
        );
        assert_eq!(
            relationship(&ast, 0).rel_spec.rel_type,
            Identification::Identifying
        );
    }

    #[test]
    fn allows_a_multi_digit_numeric_entity_after_a_cardinality_one() {
        let ast = ast("erDiagram\na many to 1 12: label");
        assert_eq!(ast.entities.len(), 2);
        assert!(ast.entity("12").is_some());
        assert_eq!(relationship(&ast, 0).rel_spec.card_a, Cardinality::OnlyOne);
        assert_eq!(
            relationship(&ast, 0).rel_spec.card_b,
            Cardinality::ZeroOrMore
        );
    }

    #[test]
    fn represents_identifying_relationships() {
        let ast = ast("erDiagram\nHOUSE ||--|{ ROOM : contains");
        assert_eq!(
            relationship(&ast, 0).rel_spec.rel_type,
            Identification::Identifying
        );
    }

    #[test]
    fn represents_identifying_relationships_written_with_to() {
        let ast = ast("erDiagram\nHOUSE one to one ROOM : contains");
        assert_eq!(
            relationship(&ast, 0).rel_spec.rel_type,
            Identification::Identifying
        );
    }

    #[test]
    fn represents_non_identifying_relationships() {
        let ast = ast("erDiagram\n PERSON ||..o{ POSSESSION : owns");
        assert_eq!(
            relationship(&ast, 0).rel_spec.rel_type,
            Identification::NonIdentifying
        );
    }

    #[test]
    fn represents_non_identifying_relationships_written_with_optionally_to() {
        let ast = ast("erDiagram\n PERSON many optionally to many POSSESSION : owns");
        assert_eq!(
            relationship(&ast, 0).rel_spec.rel_type,
            Identification::NonIdentifying
        );
    }

    #[test]
    fn rejects_an_unknown_relationship_operator() {
        let error = failure("erDiagram\nA xxx B : has");
        assert_eq!(
            error.span.map(|span| (span.line, span.column)),
            Some((2, 9))
        );
    }

    #[test]
    fn allows_an_empty_quoted_label() {
        let ast = ast("erDiagram\nCUSTOMER ||--|{ ORDER : \"\"");
        assert_eq!(relationship(&ast, 0).role_a, "");
    }

    #[test]
    fn allows_a_non_empty_quoted_label() {
        let ast = ast("erDiagram\nCUSTOMER ||--|{ ORDER : \"places\"");
        assert_eq!(relationship(&ast, 0).role_a, "places");
    }

    #[test]
    fn allows_a_non_empty_unquoted_label() {
        let ast = ast("erDiagram\nCUSTOMER ||--|{ ORDER : places");
        assert_eq!(relationship(&ast, 0).role_a, "places");
    }

    #[test]
    fn represents_the_parent_child_relationship() {
        let ast = ast("erDiagram\nPROJECT u--o{ TEAM_MEMBER : \"parent\"");
        assert_eq!(ast.entities.len(), 2);
        assert_eq!(ast.relationships.len(), 1);
        assert_eq!(relationship(&ast, 0).rel_spec.card_b, Cardinality::MdParent);
        assert_eq!(
            relationship(&ast, 0).rel_spec.card_a,
            Cardinality::ZeroOrMore
        );
    }

    #[test]
    fn works_with_prototype_property_names() {
        for name in ["__proto__", "constructor", "prototype"] {
            let source = format!("erDiagram\n{name} ||--|{{ ORDER : place");
            let ast = ast(&source);
            assert!(ast.entity(name).is_some(), "{name}");
        }
    }
}

mod accessibility {
    use super::*;

    #[test]
    fn reads_the_accessible_title_and_description() {
        let source = "erDiagram
      accTitle: graph title
      accDescr: this graph is about stuff
      TEACHER }o--o{ STUDENT : \"is teacher of\"";
        let ast = ast(source);
        assert_eq!(ast.acc_title, "graph title");
        assert_eq!(ast.acc_description, "this graph is about stuff");
    }

    #[test]
    fn reads_a_multi_line_description() {
        let source = "erDiagram
      accTitle: graph title
      accDescr { this graph is
        about
        stuff
        }

      TEACHER }o--o{ STUDENT : \"is teacher of\"";
        let ast = ast(source);
        assert_eq!(ast.acc_title, "graph title");
        assert_eq!(ast.acc_description, "this graph is\nabout\nstuff");
    }
}

mod styles_and_classes {
    use super::*;

    #[test]
    fn applies_a_style_to_an_entity() {
        let source = "erDiagram
        CUSTOMER
        style CUSTOMER color:red
      ";
        let ast = ast(source);
        assert_eq!(entity(&ast, "CUSTOMER").css_styles, ["color:red"]);
    }

    #[test]
    fn applies_multiple_styles_at_once() {
        let source = "erDiagram
        CUSTOMER
        style CUSTOMER color:red,stroke:blue,fill:#f9f
      ";
        let ast = ast(source);
        assert_eq!(
            entity(&ast, "CUSTOMER").css_styles,
            ["color:red", "stroke:blue", "fill:#f9f"]
        );
    }

    #[test]
    fn applies_separately_defined_styles() {
        let source = "erDiagram
        CUSTOMER
        style CUSTOMER color:red
        style CUSTOMER fill:#f9f
      ";
        let ast = ast(source);
        assert_eq!(
            entity(&ast, "CUSTOMER").css_styles,
            ["color:red", "fill:#f9f"]
        );
    }

    #[test]
    fn assigns_a_class_to_an_entity() {
        let ast = ast("erDiagram\nCUSTOMER\nclass CUSTOMER myClass");
        assert_eq!(entity(&ast, "CUSTOMER").css_classes, "default myClass");
    }

    #[test]
    fn assigns_multiple_classes_at_once() {
        let ast = ast("erDiagram\nCUSTOMER\nclass CUSTOMER firstClass, secondClass, thirdClass");
        assert_eq!(
            entity(&ast, "CUSTOMER").css_classes,
            "default firstClass secondClass thirdClass"
        );
    }

    #[test]
    fn assigns_separately_defined_classes() {
        let source = "erDiagram\nCUSTOMER\nclass CUSTOMER firstClass\nclass CUSTOMER secondClass";
        let ast = ast(source);
        assert_eq!(
            entity(&ast, "CUSTOMER").css_classes,
            "default firstClass secondClass"
        );
    }

    #[test]
    fn defines_the_default_class_without_touching_the_entity_classes() {
        let source = "erDiagram
        ENTITY1
        ENTITY2
        classDef default fill:#f9f
      ";
        let ast = ast(source);
        assert_eq!(entity(&ast, "ENTITY1").css_classes, "default");
        assert_eq!(entity(&ast, "ENTITY2").css_classes, "default");
        let class = ast.class("default").expect("missing default class");
        assert_eq!(class.id, "default");
        assert_eq!(class.styles, ["fill:#f9f"]);
        assert!(class.text_styles.is_empty());
    }

    #[test]
    fn defines_a_class_with_styles() {
        let source = "erDiagram
        classDef myClass fill:#f9f, stroke: red, color: pink
       ";
        let ast = ast(source);
        let class = ast.class("myClass").expect("missing myClass");
        assert_eq!(class.styles, ["fill:#f9f", "stroke:red", "color:pink"]);
        assert_eq!(class.text_styles, ["color:pink"]);
    }

    #[test]
    fn defines_multiple_classes_at_once() {
        let source = "erDiagram
        classDef firstClass,secondClass fill:#f9f, stroke: red, color: pink
       ";
        let ast = ast(source);
        for name in ["firstClass", "secondClass"] {
            let class = ast.class(name).unwrap_or_else(|| panic!("missing {name}"));
            assert_eq!(class.id, name);
            assert_eq!(class.styles, ["fill:#f9f", "stroke:red", "color:pink"]);
            assert_eq!(class.text_styles, ["color:pink"]);
        }
    }

    #[test]
    fn assigns_a_class_with_the_shorthand_alone() {
        let ast = ast("erDiagram\nCUSTOMER:::myClass");
        assert_eq!(entity(&ast, "CUSTOMER").css_classes, "default myClass");
    }

    #[test]
    fn assigns_a_class_with_the_shorthand_and_an_empty_block() {
        let ast = ast("erDiagram\nCUSTOMER:::myClass {}");
        assert_eq!(entity(&ast, "CUSTOMER").css_classes, "default myClass");
    }

    #[test]
    fn assigns_a_class_with_the_shorthand_and_an_attribute_block() {
        let ast = ast("erDiagram\nCUSTOMER:::myClass {\nstring name\n}");
        assert_eq!(entity(&ast, "CUSTOMER").css_classes, "default myClass");
    }

    #[test]
    fn assigns_multiple_classes_with_the_shorthand() {
        let ast = ast("erDiagram\nCUSTOMER:::firstClass,secondClass");
        assert_eq!(
            entity(&ast, "CUSTOMER").css_classes,
            "default firstClass secondClass"
        );
    }

    #[test]
    fn assigns_classes_with_the_shorthand_after_an_alias() {
        let ast = ast("erDiagram\nc[CUSTOMER]:::myClass");
        assert_eq!(entity(&ast, "c").alias, "CUSTOMER");
        assert_eq!(entity(&ast, "c").css_classes, "default myClass");
    }

    #[test]
    fn assigns_classes_with_the_shorthand_in_a_relationship() {
        let ast = ast("erDiagram\nCUSTOMER:::myClass ||--o{ PERSON:::myClass : allows");
        assert_eq!(entity(&ast, "CUSTOMER").css_classes, "default myClass");
        assert_eq!(entity(&ast, "PERSON").css_classes, "default myClass");
    }
}

mod special_entity_names {
    use super::*;

    #[test]
    fn allows_a_stand_alone_one() {
        ast("erDiagram\nCUSTOMER }|..|{ DELIVERY-ADDRESS : has\n1");
    }

    #[test]
    fn allows_a_stand_alone_u() {
        let ast = ast("erDiagram\nCUSTOMER }|..|{ DELIVERY-ADDRESS : has\nu");
        assert!(ast.entity("u").is_some());
    }

    #[test]
    fn allows_stand_alone_decimals() {
        for name in ["2.5", "1.5", "0.1", "99.99"] {
            let source = format!("erDiagram\nCUSTOMER }}|..|{{ DELIVERY-ADDRESS : has\n{name}");
            let ast = ast(&source);
            assert!(ast.entity(name).is_some(), "{name}");
        }
    }

    #[test]
    fn allows_u_with_attributes() {
        ast("erDiagram\nu {\nstring name\nint id\n}");
    }

    #[test]
    fn allows_one_with_attributes() {
        ast("erDiagram\n1 {\nstring name\nint id\n}");
    }

    #[test]
    fn allows_decimals_with_attributes() {
        ast("erDiagram\n2.5 {\nstring name\nint id\n}");
        ast("erDiagram\n1.5 {\nstring value\n}");
    }

    #[test]
    fn allows_u_in_relationships() {
        ast("erDiagram\nCUSTOMER ||--|| u : has");
        ast("erDiagram\nu ||--|| ORDER : places");
        ast("erDiagram\nu ||--|| v : connects");
    }

    #[test]
    fn allows_numbers_in_relationships() {
        ast("erDiagram\nCUSTOMER ||--|| 1 : has");
        ast("erDiagram\n1 ||--|| ORDER : places");
        ast("erDiagram\n1 ||--|| 2 : connects");
    }

    #[test]
    fn allows_decimals_in_relationships() {
        ast("erDiagram\nCUSTOMER ||--|| 2.5 : has");
        ast("erDiagram\n1.5 ||--|| ORDER : places");
        ast("erDiagram\n2.5 ||--|| 5.5 : connects");
    }

    #[test]
    fn allows_a_complex_diagram_with_special_entity_names() {
        let source = "erDiagram
              CUSTOMER ||--o{ 1 : places
              1 ||--|{ u : contains
              u {
                string name
                int quantity
              }
              \"2.5\" ||--|| ORDER : processes
              ORDER {
                int id
                date created
              }
        ";
        ast(source);
    }

    #[test]
    fn allows_attributes_with_numbers_that_do_not_start_them() {
        let source = "erDiagram
              ENTITY {
                string name1
                int value2
                float point3_5
              }
        ";
        ast(source);
    }
}

mod direction {
    use super::*;

    #[test]
    fn defaults_to_top_to_bottom() {
        let ast = ast("erDiagram\nA");
        assert_eq!(ast.direction, "TB");
    }

    #[test]
    fn reads_a_direction_statement() {
        let ast = ast("erDiagram\ndirection LR\nA");
        assert_eq!(ast.direction, "LR");
    }

    #[test]
    fn keeps_a_direction_inside_a_subgraph() {
        let ast = ast("erDiagram\nsubgraph WithRL\ndirection RL\nA\nB\nend");
        assert_eq!(ast.direction, "TB");
        assert_eq!(ast.sub_graph("WithRL").unwrap().dir.as_deref(), Some("RL"));
    }
}

mod subgraphs {
    use super::*;

    #[test]
    fn reads_simple_nodes_and_the_id_and_title() {
        let ast = ast("erDiagram\nsubgraph Group\nA\nB\nend");
        assert_eq!(ast.sub_graphs.len(), 1);
        let sub_graph = &ast.sub_graphs[0];
        assert_eq!(sub_graph.nodes, ["A", "B"]);
        assert_eq!(sub_graph.title, "Group");
        assert_eq!(sub_graph.id, "Group");
    }

    #[test]
    fn reads_a_multi_word_quoted_title() {
        let ast = ast("erDiagram\nsubgraph \"Some Title\"\nA\nB\nend");
        let sub_graph = &ast.sub_graphs[0];
        assert_eq!(sub_graph.title, "Some Title");
        assert_eq!(sub_graph.id, "Some Title");
    }

    #[test]
    fn reads_the_id_and_title_notation() {
        let ast = ast("erDiagram\nsubgraph id [Title]\nA\nB\nend");
        let sub_graph = &ast.sub_graphs[0];
        assert_eq!(sub_graph.title, "Title");
        assert_eq!(sub_graph.id, "id");
    }

    #[test]
    fn reads_special_characters_in_a_title() {
        let ast = ast("erDiagram\nsubgraph \"Group 1.5\"\nA\nend");
        let sub_graph = &ast.sub_graphs[0];
        assert_eq!(sub_graph.nodes.len(), 1);
        assert_eq!(sub_graph.title, "Group 1.5");
        assert_eq!(sub_graph.id, "Group 1.5");
    }

    #[test]
    fn skips_empty_lines_inside() {
        let ast = ast("erDiagram\nsubgraph Group\n\nA\n\nB\n\nend");
        let sub_graph = &ast.sub_graphs[0];
        assert_eq!(sub_graph.nodes, ["A", "B"]);
        assert_eq!(sub_graph.title, "Group");
        assert_eq!(sub_graph.id, "Group");
    }

    #[test]
    fn ignores_whitespace_around_end() {
        let ast = ast("erDiagram\nsubgraph Group\nA\nend   \n");
        let sub_graph = &ast.sub_graphs[0];
        assert_eq!(sub_graph.nodes, ["A"]);
        assert_eq!(sub_graph.title, "Group");
        assert_eq!(sub_graph.id, "Group");
    }

    #[test]
    fn allows_an_empty_subgraph() {
        let ast = ast("erDiagram\nsubgraph empty\nend");
        let sub_graph = &ast.sub_graphs[0];
        assert!(sub_graph.nodes.is_empty());
        assert_eq!(sub_graph.title, "empty");
        assert_eq!(sub_graph.id, "empty");
    }

    #[test]
    fn includes_both_endpoints_of_a_relationship_inside() {
        let ast = ast("erDiagram\nsubgraph Group\nA }|--|| B : \"relates to\"\nend");
        let sub_graph = &ast.sub_graphs[0];
        assert_eq!(sub_graph.nodes, ["A", "B"]);
        assert_eq!(ast.relationships.len(), 1);
        assert_eq!(relationship(&ast, 0).entity_a, entity(&ast, "A").id);
        assert_eq!(relationship(&ast, 0).entity_b, entity(&ast, "B").id);
    }

    #[test]
    fn relates_a_subgraph_node_to_an_external_entity() {
        let source = "erDiagram\nsubgraph Group\nA\nend\nA }|--|| B : \"relates to\"\n";
        let ast = ast(source);
        assert_eq!(ast.sub_graphs.len(), 1);
        let sub_graph = &ast.sub_graphs[0];
        assert_eq!(sub_graph.nodes, ["A"]);
        assert!(!sub_graph.nodes.iter().any(|node| node.as_str() == "B"));
        assert_eq!(ast.relationships.len(), 1);
        assert_eq!(relationship(&ast, 0).entity_a, entity(&ast, "A").id);
        assert_eq!(relationship(&ast, 0).entity_b, entity(&ast, "B").id);
    }

    #[test]
    fn relates_two_subgraphs() {
        let source = "erDiagram\nsubgraph G1\n  A\nend\nsubgraph G2\n  B\nend\nG1 }|--|| G2 : \"relates to\"\n";
        let ast = ast(source);
        assert_eq!(ast.sub_graphs.len(), 2);
        assert_eq!(ast.relationships.len(), 1);
        assert_eq!(relationship(&ast, 0).entity_a, "G1");
        assert_eq!(relationship(&ast, 0).entity_b, "G2");
    }

    #[test]
    fn relates_entities_in_different_subgraphs() {
        let source = "erDiagram\nsubgraph G1\n  A\nend\nsubgraph G2\n  B\nend\nA }|--|| B : \"relates to\"\n";
        let ast = ast(source);
        assert_eq!(ast.sub_graphs.len(), 2);
        assert_eq!(ast.relationships.len(), 1);
        assert_eq!(ast.sub_graphs[0].nodes[0], "A");
        assert_eq!(ast.sub_graphs[1].nodes[0], "B");
        assert_eq!(relationship(&ast, 0).entity_a, entity(&ast, "A").id);
        assert_eq!(relationship(&ast, 0).entity_b, entity(&ast, "B").id);
    }

    #[test]
    fn relates_an_entity_to_a_subgraph() {
        let source = "erDiagram\nsubgraph Group\nA\nend\nGroup }|--|| B : \"relates to\"\n";
        let ast = ast(source);
        let sub_graph = &ast.sub_graphs[0];
        assert_eq!(sub_graph.nodes, ["A"]);
        assert!(!sub_graph.nodes.iter().any(|node| node.as_str() == "B"));
        assert_eq!(ast.relationships.len(), 1);
        assert_eq!(relationship(&ast, 0).entity_a, "Group");
        assert_eq!(relationship(&ast, 0).entity_b, entity(&ast, "B").id);
    }

    #[test]
    fn reads_multiple_relationships_inside() {
        let source = "erDiagram\nsubgraph Group\n  A }|--|| B : \"r1\"\n  B }|--|| C : \"r2\"\nend";
        let ast = ast(source);
        let sub_graph = &ast.sub_graphs[0];
        assert_eq!(sub_graph.nodes, ["A", "B", "C"]);
        assert_eq!(ast.relationships.len(), 2);
        assert_eq!(relationship(&ast, 0).entity_a, entity(&ast, "A").id);
        assert_eq!(relationship(&ast, 0).entity_b, entity(&ast, "B").id);
        assert_eq!(relationship(&ast, 1).entity_a, entity(&ast, "B").id);
        assert_eq!(relationship(&ast, 1).entity_b, entity(&ast, "C").id);
    }

    #[test]
    fn reads_nested_subgraphs() {
        let source = "erDiagram\nsubgraph outer\n  A\n  subgraph inner\n    B\n  end\nend";
        let ast = ast(source);
        assert_eq!(ast.sub_graphs.len(), 2);
        let outer = ast.sub_graph("outer").expect("missing outer");
        let inner = ast.sub_graph("inner").expect("missing inner");
        assert_eq!(outer.nodes, ["A", "inner"]);
        assert_eq!(inner.nodes, ["B"]);
        assert_eq!(outer.title, "outer");
        assert_eq!(inner.title, "inner");
    }

    #[test]
    fn reads_nested_subgraphs_with_irregular_indentation() {
        let source =
            "erDiagram\nsubgraph outer\n    A\n      B\n  subgraph inner\n        C\n  end\nend";
        let ast = ast(source);
        assert_eq!(ast.sub_graphs.len(), 2);
        let outer = ast.sub_graph("outer").expect("missing outer");
        let inner = ast.sub_graph("inner").expect("missing inner");
        assert_eq!(outer.nodes, ["A", "B", "inner"]);
        assert_eq!(inner.nodes, ["C"]);
        assert_eq!(outer.title, "outer");
        assert_eq!(inner.title, "inner");
    }

    #[test]
    fn reads_relationships_inside_nested_subgraphs() {
        let source = "erDiagram\nsubgraph outer\n  subgraph inner\n    A }|--|| B : \"relates to\"\n  end\nend";
        let ast = ast(source);
        assert_eq!(ast.sub_graphs.len(), 2);
        let outer = ast.sub_graph("outer").expect("missing outer");
        let inner = ast.sub_graph("inner").expect("missing inner");
        assert_eq!(outer.nodes, ["inner"]);
        assert_eq!(inner.nodes, ["A", "B"]);
        assert_eq!(ast.relationships.len(), 1);
        assert_eq!(relationship(&ast, 0).entity_a, entity(&ast, "A").id);
        assert_eq!(relationship(&ast, 0).entity_b, entity(&ast, "B").id);
    }

    #[test]
    fn relates_entities_of_an_outer_and_an_inner_subgraph() {
        let source = "erDiagram\nsubgraph outer\n  A\n  subgraph inner\n    B\n  end\nend\nA }|--|| B : \"relates to\"\n";
        let ast = ast(source);
        assert_eq!(ast.sub_graphs.len(), 2);
        let outer = ast.sub_graph("outer").expect("missing outer");
        let inner = ast.sub_graph("inner").expect("missing inner");
        assert_eq!(outer.nodes, ["A", "inner"]);
        assert_eq!(inner.nodes, ["B"]);
        assert_eq!(ast.relationships.len(), 1);
        assert_eq!(relationship(&ast, 0).entity_a, entity(&ast, "A").id);
        assert_eq!(relationship(&ast, 0).entity_b, entity(&ast, "B").id);
    }

    #[test]
    fn counts_style_class_and_accessibility_statements_as_nodes() {
        let source = "erDiagram\nsubgraph G\nA\nclass A x\nclassDef y fill:red\nstyle A color:red\naccTitle: hello\nend\n";
        let ast = ast(source);
        let sub_graph = ast.sub_graph("G").expect("missing G");
        assert_eq!(
            sub_graph.nodes,
            ["A", "class", "classDef", "style", "hello"]
        );
    }
}

mod grammar_quirks {
    use super::*;

    #[test]
    fn allows_statements_to_share_a_line() {
        let ast = ast("erDiagram\nA B");
        assert_eq!(ast.entities.len(), 2);
        assert!(ast.entity("A").is_some());
        assert!(ast.entity("B").is_some());
    }

    #[test]
    fn allows_an_entity_after_a_relationship_label_on_the_same_line() {
        let ast = ast("erDiagram\nCUSTOMER ||--o{ ORDER : has EXTRA");
        assert_eq!(ast.entities.len(), 3);
        assert_eq!(relationship(&ast, 0).role_a, "has");
        assert!(ast.entity("EXTRA").is_some());
    }

    #[test]
    fn a_direction_line_swallows_its_whole_line() {
        let ast = ast("erDiagram\nA direction TB\nB");
        assert_eq!(ast.direction, "TB");
        assert_eq!(ast.entities.len(), 1);
        assert!(ast.entity("B").is_some());
        assert!(ast.entity("A").is_none());
    }

    #[test]
    fn rejects_a_top_level_end() {
        assert!(parse("erDiagram\nA\nend").is_err());
        assert!(parse("erDiagram\nA end").is_err());
    }

    #[test]
    fn reads_keywords_regardless_of_case() {
        let subgraph_ast = ast("erDiagram\nA\nSUBGRAPH g\nB\nEND");
        assert_eq!(subgraph_ast.sub_graph("g").unwrap().nodes, ["B"]);
        let direction_ast = ast("erDiagram\nA\nDIRECTION lr\nB");
        assert_eq!(direction_ast.direction, "LR");
        let relationship_ast = ast("erDiagram\nA |O--O{ B : r");
        assert_eq!(
            relationship(&relationship_ast, 0).rel_spec.card_b,
            Cardinality::ZeroOrOne
        );
        assert_eq!(
            relationship(&relationship_ast, 0).rel_spec.card_a,
            Cardinality::ZeroOrMore
        );
    }

    #[test]
    fn a_style_statement_needs_a_trailing_line_break() {
        // Upstream cannot emit its `EOF` terminal from the exclusive style condition.
        assert!(parse("erDiagram\nA\nclassDef x fill:red").is_err());
        assert!(parse("erDiagram\nA\nclassDef x fill:red\n").is_ok());
        assert!(parse("erDiagram\nA\nstyle A color:red").is_err());
        assert!(parse("erDiagram\nA\nstyle A color:red\n").is_ok());
    }

    #[test]
    fn rejects_a_character_the_style_grammar_does_not_know() {
        let error = failure("erDiagram\nA\nstyle A color:rgb(1,2,3)\n");
        assert_eq!(
            error.span.map(|span| (span.line, span.column)),
            Some((3, 18))
        );
    }
}

mod database {
    use super::*;
    use crate::diagrams::er::db::{Db, Item};

    fn ids(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    fn nodes(names: &[&str]) -> Vec<Item> {
        names
            .iter()
            .map(|name| Item::Node(name.to_string()))
            .collect()
    }

    fn spec(card_a: Cardinality, card_b: Cardinality) -> RelSpec {
        RelSpec {
            card_a,
            card_b,
            rel_type: Identification::Identifying,
        }
    }

    #[test]
    fn adds_subgraphs_and_returns_the_id_title_and_nodes() {
        let mut db = Db::new();
        let id = db.add_sub_graph("sub1", "Group One", &nodes(&["A", "B"]), Some("text"));
        let ast = db.into_ast();
        assert_eq!(id, "sub1");
        assert_eq!(ast.sub_graphs.len(), 1);
        assert_eq!(ast.sub_graphs[0].id, "sub1");
        assert_eq!(ast.sub_graphs[0].title, "Group One");
        assert_eq!(ast.sub_graphs[0].nodes, ["A", "B"]);
    }

    #[test]
    fn handles_a_subgraph_without_a_title() {
        let mut db = Db::new();
        db.add_sub_graph("sub1", "", &nodes(&["A"]), Some("text"));
        let ast = db.into_ast();
        assert_eq!(ast.sub_graphs[0].id, "sub1");
        assert_eq!(ast.sub_graphs[0].title, "");
    }

    #[test]
    fn prevents_nodes_from_joining_a_later_subgraph() {
        let mut db = Db::new();
        db.add_sub_graph("sub1", "sub1", &nodes(&["A", "B"]), Some("text"));
        db.add_sub_graph("sub2", "sub2", &nodes(&["A", "C"]), Some("text"));
        let ast = db.into_ast();
        assert_eq!(ast.sub_graphs.len(), 2);
        assert!(
            ast.sub_graphs[1]
                .nodes
                .iter()
                .any(|node| node.as_str() == "C")
        );
        assert!(
            !ast.sub_graphs[1]
                .nodes
                .iter()
                .any(|node| node.as_str() == "A")
        );
    }

    #[test]
    fn allows_a_full_removal_when_every_node_exists_elsewhere() {
        let mut db = Db::new();
        db.add_sub_graph("sub1", "sub1", &nodes(&["A"]), Some("text"));
        db.add_sub_graph("sub2", "sub2", &nodes(&["A"]), Some("text"));
        let ast = db.into_ast();
        let sub2 = ast.sub_graph("sub2").expect("missing sub2");
        assert!(sub2.nodes.is_empty());
    }

    #[test]
    fn stores_the_subgraph_id_when_an_entity_is_a_subgraph() {
        let mut db = Db::new();
        db.add_sub_graph("sub1", "sub1", &nodes(&["X"]), Some("text"));
        db.add_relationship(
            "sub1",
            "roleA",
            "Y",
            spec(Cardinality::OnlyOne, Cardinality::ZeroOrOne),
        );
        let ast = db.into_ast();
        assert_eq!(ast.relationships.len(), 1);
        assert_eq!(ast.relationships[0].entity_a, "sub1");
        assert!(ast.entity("Y").is_some());
        assert_eq!(ast.relationships[0].entity_b, entity(&ast, "Y").id);
    }

    #[test]
    fn returns_nested_clusters_with_the_correct_parents() {
        let mut db = Db::new();
        let outer_id =
            db.add_sub_graph("sub1", "Group One", &nodes(&["sub1_1", "A"]), Some("text"));
        let inner_id = db.add_sub_graph("sub1_1", "Group Two", &nodes(&["B"]), Some("text"));
        db.add_entity("A", "");
        db.add_entity("B", "");
        let ast = db.into_ast();
        let data = ast.data();
        assert_eq!(data.nodes.len(), 4);
        let outer = data
            .nodes
            .iter()
            .find(|node| node.id == outer_id)
            .expect("missing outer cluster");
        let inner = data
            .nodes
            .iter()
            .find(|node| node.id == inner_id)
            .expect("missing inner cluster");
        let customer = data
            .nodes
            .iter()
            .find(|node| node.label == "A")
            .expect("missing A");
        let account = data
            .nodes
            .iter()
            .find(|node| node.label == "B")
            .expect("missing B");
        assert!(outer.is_group);
        assert!(inner.is_group);
        assert_eq!(outer.label, "Group One");
        assert_eq!(inner.label, "Group Two");
        assert_eq!(inner.parent_id.as_deref(), Some(outer_id.as_str()));
        assert_eq!(customer.parent_id.as_deref(), Some(outer_id.as_str()));
        assert!(!customer.is_group);
        assert_eq!(account.parent_id.as_deref(), Some(inner_id.as_str()));
        assert!(!account.is_group);
    }

    #[test]
    fn returns_edges_connected_to_subgraphs() {
        let mut db = Db::new();
        db.add_sub_graph("sub1", "Group One", &nodes(&["A"]), Some("text"));
        db.add_entity("A", "");
        db.add_entity("B", "");
        db.add_relationship(
            "sub1",
            "roleA",
            "B",
            spec(Cardinality::OnlyOne, Cardinality::ZeroOrMore),
        );
        let ast = db.into_ast();
        let data = ast.data();
        assert_eq!(data.edges.len(), 1);
        assert_eq!(data.edges[0].start, "sub1");
        assert_eq!(data.edges[0].end, entity(&ast, "B").id);
    }

    #[test]
    fn applies_classes_and_styles_to_subgraphs() {
        let mut db = Db::new();
        db.add_sub_graph("sub1", "sub1", &nodes(&["A"]), Some("text"));
        db.set_class(&ids(&["sub1"]), &ids(&["B"]));
        db.add_css_styles(&ids(&["sub1"]), &ids(&["fill:red"]));
        let ast = db.into_ast();
        assert_eq!(ast.sub_graphs.len(), 1);
        assert!(
            ast.sub_graphs[0]
                .classes
                .iter()
                .any(|class| class.as_str() == "B")
        );
        assert!(
            ast.sub_graphs[0]
                .css_styles
                .iter()
                .any(|style| style.as_str() == "fill:red")
        );
    }
}
