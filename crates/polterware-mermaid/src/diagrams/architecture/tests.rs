//! Tests ported from the upstream architecture specs:
//! `packages/parser/tests/architecture.test.ts` (the Langium grammar) and
//! `packages/mermaid/src/diagrams/architecture/architecture.spec.ts` (the database). Inputs
//! and expectations are upstream's.
//!
//! The grammar tests call [`parse_grammar`], which is what upstream's parser tests exercise:
//! Langium parses without populating the database, so their `in api` parents need no group.
//! The database tests call [`parse`], which also populates it.
//!
//! Skipped, and why:
//! - `expectNoErrorsOrAlternatives` checks Langium's lexer/parser error arrays and that no
//!   `console.log` ran. Here `parse` returning `Ok` is the error-free assertion, and there is
//!   no console.
//! - `result.value.$type` is Langium's generated AST type marker, not database data.
//! - `architecture.spec.ts`'s `cytoscape stylesheet warnings` describe renders a stylesheet.
//! - `architecture.spec.ts`'s `randomize config` and `fcose layout config` describes read
//!   configuration, which this crate does not own; the complex nested diagram is still parsed
//!   and its services, groups, and edges counted.
//! - `architectureSeed.spec.ts` tests the seeded `Math.random` the renderer uses for its
//!   randomized layout.

use super::ast::{
    Alignment, AlignmentDirection, Ast, Direction, DirectionPair, Group, Junction, LayoutHint,
    Service,
};
use super::db::Db;
use super::parse;
use super::parser::{ParsedArchitecture, parse_grammar};
use crate::error::{ParseError, Span};

fn ast(body: &str) -> Ast {
    parse(body).unwrap_or_else(|error| panic!("expected {body:?} to parse, got {error}"))
}

fn grammar(body: &str) -> ParsedArchitecture {
    parse_grammar(body).unwrap_or_else(|error| panic!("expected {body:?} to parse, got {error}"))
}

fn error(body: &str) -> ParseError {
    match parse(body) {
        Ok(ast) => panic!("expected {body:?} to fail, parsed {ast:?}"),
        Err(error) => error,
    }
}

fn service_ids(ast: &Ast) -> Vec<&str> {
    ast.services().map(|service| service.id.as_str()).collect()
}

fn group_ids(ast: &Ast) -> Vec<&str> {
    ast.groups.iter().map(|group| group.id.as_str()).collect()
}

// packages/parser/tests/architecture.test.ts

#[test]
fn parses_the_header_with_any_surrounding_whitespace() {
    for body in [
        "architecture-beta",
        "  architecture-beta  ",
        "\tarchitecture-beta\t",
        "\n        \tarchitecture-beta\n        ",
    ] {
        let parsed = grammar(body);
        assert!(parsed.groups.is_empty(), "{body:?}");
        assert!(parsed.services.is_empty(), "{body:?}");
        assert!(parsed.edges.is_empty(), "{body:?}");
    }
}

#[test]
fn parses_a_title_on_the_header_line() {
    for body in [
        "architecture-beta title sample title",
        "  architecture-beta  title sample title  ",
        "\tarchitecture-beta\ttitle sample title\t",
        "architecture-beta\n            \ttitle sample title\n            ",
    ] {
        assert_eq!(
            grammar(body).title.as_deref(),
            Some("sample title"),
            "{body:?}"
        );
    }
}

#[test]
fn parses_a_title_on_the_next_line() {
    for body in [
        "architecture-beta\n            title sample title",
        "architecture-beta\n            title sample title\n            ",
    ] {
        assert_eq!(
            grammar(body).title.as_deref(),
            Some("sample title"),
            "{body:?}"
        );
    }
}

#[test]
fn parses_a_title_an_accessible_title_and_an_accessible_description() {
    let parsed = grammar(
        "architecture-beta
            title sample title
            accTitle: sample accTitle
            accDescr: sample accDescr
            ",
    );
    assert_eq!(parsed.title.as_deref(), Some("sample title"));
    assert_eq!(parsed.acc_title.as_deref(), Some("sample accTitle"));
    assert_eq!(parsed.acc_description.as_deref(), Some("sample accDescr"));
}

#[test]
fn parses_a_multiline_accessible_description() {
    let parsed = grammar(
        "architecture-beta
            title sample title
            accTitle: sample accTitle
            accDescr {
                sample accDescr
            }
            ",
    );
    assert_eq!(parsed.title.as_deref(), Some("sample title"));
    assert_eq!(parsed.acc_title.as_deref(), Some("sample accTitle"));
    assert_eq!(parsed.acc_description.as_deref(), Some("sample accDescr"));
}

#[test]
fn parses_a_service_title_in_double_quotes() {
    let parsed = grammar(
        "architecture-beta
            service db(database)[\"Database\"] in api
            ",
    );
    assert_eq!(parsed.services[0].0.title.as_deref(), Some("Database"));
}

#[test]
fn parses_a_service_title_in_single_quotes() {
    let parsed = grammar(
        "architecture-beta
            service db(database)['Database'] in api
            ",
    );
    assert_eq!(parsed.services[0].0.title.as_deref(), Some("Database"));
}

#[test]
fn parses_a_service_title_without_quotes() {
    let parsed = grammar(
        "architecture-beta
            service db(database)[Database] in api
            ",
    );
    assert_eq!(parsed.services[0].0.title.as_deref(), Some("Database"));
}

#[test]
fn parses_a_group_title_in_quotes() {
    let parsed = grammar(
        "architecture-beta
            group api(cloud)[\"API\"]
            ",
    );
    assert_eq!(parsed.groups[0].0.title.as_deref(), Some("API"));
}

#[test]
fn keeps_apostrophes_in_a_service_title() {
    let parsed = grammar(
        "architecture-beta
            service db(database)[\"John's Database\"] in api
            ",
    );
    assert_eq!(
        parsed.services[0].0.title.as_deref(),
        Some("John's Database")
    );
}

#[test]
fn unescapes_inner_quotes_in_a_service_title() {
    let parsed = grammar(
        "architecture-beta
            service api(server)[\"The \\\"Main\\\" API\"] in cloud
            ",
    );
    assert_eq!(
        parsed.services[0].0.title.as_deref(),
        Some("The \"Main\" API")
    );
}

#[test]
fn keeps_non_ascii_and_punctuation_in_an_unquoted_title() {
    for title in [
        "采集器",
        "Hello 世界",
        "Сервер",
        "Café",
        "Structure + Src",
        "Multi-Agent",
        "A, B, C",
        "TCP/IP: v6",
    ] {
        let body = format!("architecture-beta\n            service a(server)[{title}] in g1\n");
        assert_eq!(
            grammar(&body).services[0].0.title.as_deref(),
            Some(title),
            "{body:?}"
        );
    }
}

#[test]
fn keeps_chinese_characters_in_a_group_title() {
    let parsed = grammar(
        "architecture-beta
            group g1(server)[你好世界]
            ",
    );
    assert_eq!(parsed.groups[0].0.title.as_deref(), Some("你好世界"));
}

#[test]
fn parses_icons_and_an_icon_text() {
    let parsed = grammar(
        "architecture-beta
            group api(cloud)[API]
            service one(aws:ec2)[One] in api
            service two \"disk icon\"[Two] in api
            ",
    );
    assert_eq!(parsed.groups[0].0.icon.as_deref(), Some("cloud"));
    assert_eq!(parsed.services[0].0.icon.as_deref(), Some("aws:ec2"));
    assert_eq!(parsed.services[1].0.icon_text.as_deref(), Some("disk icon"));
    assert!(parsed.services[1].0.icon.is_none());
}

#[test]
fn parses_junctions_and_the_group_modifier() {
    let parsed = grammar(
        "architecture-beta
            group app(cloud)[App]
            service a(server)[A] in app
            junction mid in app
            a{group}:B --> T:mid
            ",
    );
    assert_eq!(parsed.junctions[0].0.container.as_deref(), Some("app"));
    let (edge, _) = &parsed.edges[0];
    assert!(edge.lhs_group);
    assert!(!edge.rhs_group);
    assert!(edge.rhs_into);
    assert_eq!(edge.lhs_dir, Direction::Bottom);
    assert_eq!(edge.rhs_dir, Direction::Top);
}

#[test]
fn parses_edge_arrowheads_and_a_title() {
    let parsed = grammar(
        "architecture-beta
            service a(server)[A]
            service b(server)[B]
            a:R <--> L:b
            a:R -[Link]- L:b
            ",
    );
    let (arrow, _) = &parsed.edges[0];
    assert!(arrow.lhs_into);
    assert!(arrow.rhs_into);
    assert_eq!(arrow.title, None);
    let (titled, _) = &parsed.edges[1];
    assert_eq!(titled.title.as_deref(), Some("Link"));
    assert!(!titled.lhs_into);
    assert!(!titled.rhs_into);
}

#[test]
fn a_later_title_line_overwrites_an_earlier_one() {
    let parsed = grammar(
        "architecture-beta
            title first
            title second
            ",
    );
    assert_eq!(parsed.title.as_deref(), Some("second"));
}

// packages/mermaid/src/diagrams/architecture/architecture.spec.ts

#[test]
fn parses_the_architecture_keyword() {
    ast("architecture-beta");
}

#[test]
fn parses_a_service() {
    let ast = ast("architecture-beta
            service db
            ");
    assert_eq!(service_ids(&ast), ["db"]);
}

#[test]
fn parses_a_title_into_the_database() {
    let ast = ast("architecture-beta title Simple Architecture Diagram");
    assert_eq!(ast.diagram_title, "Simple Architecture Diagram");
}

#[test]
fn parses_a_title_on_another_line_into_the_database() {
    let ast = ast("architecture-beta
            title Simple Architecture Diagram
            ");
    assert_eq!(ast.diagram_title, "Simple Architecture Diagram");
}

#[test]
fn parses_the_accessibility_fields_into_the_database() {
    let ast = ast("architecture-beta
            accTitle: Accessibility Title
            accDescr: Accessibility Description
            ");
    assert_eq!(ast.acc_title, "Accessibility Title");
    assert_eq!(ast.acc_description, "Accessibility Description");
}

#[test]
fn parses_a_multiline_accessibility_description_into_the_database() {
    let ast = ast("architecture-beta
            accDescr {
                Accessibility Description
            }
            ");
    assert_eq!(ast.acc_description, "Accessibility Description");
}

#[test]
fn parses_a_complex_deeply_nested_diagram() {
    let ast = ast("architecture-beta
    group sub1(cloud)[Subscription A]
    group vnet1(cloud)[VNet A] in sub1
    service vm1(server)[VM] in vnet1

    group sub2(cloud)[Subscription B]
    group shared(cloud)[Shared] in sub2
    service reg(database)[Registry] in shared

    group env(cloud)[Environment] in sub2
    group vnet(cloud)[VNet] in env
    group snet1(cloud)[App Subnet] in vnet
    service nsg(server)[NSG] in snet1
    service asp(server)[App Plan] in snet1
    service web(server)[Web App] in snet1

    group snet2(cloud)[PE Subnet] in vnet
    service pe1(server)[PE Blob] in snet2
    service pe2(server)[PE Bus] in snet2

    service storage(disk)[Storage] in env
    service container(disk)[Container] in env
    service bus(server)[Service Bus] in env
    service dns(server)[DNS Zone] in env

    service client(internet)[Client]

    reg:B --> T:web
    nsg:R --> L:asp
    asp:R --> L:web
    web:R --> L:pe1
    pe1:R --> L:storage
    storage:B --> T:container
    web:B --> T:pe2
    pe2:R --> L:bus
    vm1:R --> L:pe2");
    assert_eq!(ast.services().count(), 12);
    assert_eq!(ast.groups.len(), 8);
    assert_eq!(ast.edges.len(), 9);
}

#[test]
fn throws_when_a_group_is_missing() {
    let error = error(
        "architecture-beta
      group mermaidPrototypePollutionMarker(cloud)[Marker]
      service a(server)[A] in __proto__
      service b(server)[B] in mermaidPrototypePollutionMarker
      a:R -- L:b",
    );
    assert!(
        error
            .message
            .contains("The service [a]'s parent does not exist."),
        "{error}"
    );
}

#[test]
fn allows_a_group_named_dunder_proto() {
    let ast = ast("architecture-beta
      group __proto__(cloud)[Marker]
      service a(server)[A] in __proto__
      service b(server)[B] in __proto__
      a:R -- L:b");
    assert!(group_ids(&ast).contains(&"__proto__"));
}

#[test]
fn blocks_proto_pollution_through_a_service_id() {
    let ast = ast("architecture-beta
      group __proto__(cloud)[P]
      group myPrototypePollutionKey(cloud)[Real]
      service a(server)[A] in __proto__
      service b(server)[B] in myPrototypePollutionKey
      a:R -- L:b");
    let structures = ast.data_structures();
    assert_eq!(
        structures.group_alignment("__proto__", "myPrototypePollutionKey"),
        Some(Alignment::Horizontal)
    );
    assert_eq!(
        super::group_alignment_key("myPrototypePollutionKey", "__proto__"),
        "\"__proto__\"-\"myPrototypePollutionKey\""
    );
}

#[test]
fn stores_services_in_the_order_they_were_added() {
    let ast = ast("architecture-beta
      service a(server)[A]
      service 20(server)[B]
      service 10(server)[C]");
    assert_eq!(service_ids(&ast), ["a", "20", "10"]);
}

#[test]
fn parses_a_row_alignment() {
    let ast = ast("architecture-beta
  group api(cloud)[API]
  service db1(database)[DB1] in api
  service db2(database)[DB2] in api
  service db3(database)[DB3] in api
  align row db1 db2 db3");
    assert_eq!(ast.layout_hints.len(), 1);
    assert_eq!(ast.layout_hints[0].direction, AlignmentDirection::Row);
    assert_eq!(ast.layout_hints[0].members, ["db1", "db2", "db3"]);
}

#[test]
fn parses_a_column_alignment() {
    let ast = ast("architecture-beta
  service a(server)[A]
  service b(server)[B]
  align column a b");
    assert_eq!(ast.layout_hints.len(), 1);
    assert_eq!(ast.layout_hints[0].direction, AlignmentDirection::Column);
    assert_eq!(ast.layout_hints[0].members, ["a", "b"]);
}

#[test]
fn rejects_an_align_member_that_is_not_a_node() {
    let error = error(
        "architecture-beta
  service a(server)[A]
  service b(server)[B]
  align row a b ghost",
    );
    assert!(error.message.contains("ghost"), "{error}");
}

#[test]
fn rejects_an_align_member_listed_twice() {
    let error = error(
        "architecture-beta
  service a(server)[A]
  align row a a",
    );
    assert!(error.message.contains("more than once"), "{error}");
}

#[test]
fn rejects_a_layout_hint_with_fewer_than_two_members() {
    let mut db = Db::new();
    let row = db.add_layout_hint(
        LayoutHint {
            direction: AlignmentDirection::Row,
            members: Vec::new(),
        },
        Span::default(),
    );
    assert!(
        row.unwrap_err().message.contains("at least two members"),
        "empty row"
    );
    let column = db.add_layout_hint(
        LayoutHint {
            direction: AlignmentDirection::Column,
            members: vec!["only_one".to_string()],
        },
        Span::default(),
    );
    assert!(
        column.unwrap_err().message.contains("at least two members"),
        "one column member"
    );
}

#[test]
fn does_not_collide_with_ids_that_start_with_a_keyword() {
    let ast = ast("architecture-beta
  service rowspan(server)[Rowspan]
  service columnar(server)[Columnar]
  align row rowspan columnar");
    assert_eq!(service_ids(&ast), ["rowspan", "columnar"]);
    assert_eq!(ast.layout_hints[0].members, ["rowspan", "columnar"]);
}

#[test]
fn rejects_a_service_id_that_is_a_reserved_keyword() {
    for keyword in ["align", "row", "column"] {
        let body = format!(
            "architecture-beta
  service {keyword}(database)[Service Using Reserved Word]"
        );
        error(&body);
    }
}

#[test]
fn validates_junctions() {
    let mut db = Db::new();
    db.add_group(group("g1"), Span::default()).unwrap();
    db.add_service(service_in("svc", Some("g1")), Span::default())
        .unwrap();
    assert!(
        db.add_junction(junction("svc"), Span::default())
            .unwrap_err()
            .message
            .contains("already in use")
    );
    assert!(
        db.add_junction(junction("g1"), Span::default())
            .unwrap_err()
            .message
            .contains("already in use")
    );
    assert!(
        db.add_junction(junction_in("j1", "nonexistent"), Span::default())
            .unwrap_err()
            .message
            .contains("parent does not exist")
    );
    assert!(
        db.add_junction(junction_in("j1", "svc"), Span::default())
            .unwrap_err()
            .message
            .contains("parent is not a group")
    );
    assert!(
        db.add_junction(junction_in("j1", "j1"), Span::default())
            .unwrap_err()
            .message
            .contains("cannot be placed within itself")
    );
    db.add_group(group("app_env"), Span::default()).unwrap();
    db.add_junction(junction_in("mid", "app_env"), Span::default())
        .unwrap();
    assert!(db.into_ast().junctions().any(|node| node.id == "mid"));
}

#[test]
fn keeps_a_junction_in_its_group() {
    let ast = ast("architecture-beta
  group app_env(cloud)[App Env]
  service app(server)[Web App] in app_env
  service db(database)[DB] in app_env
  service api(server)[API] in app_env
  junction mid in app_env
  app:B -- T:mid
  mid:R -- L:db
  mid:B -- T:api");
    assert_eq!(ast.junctions().count(), 1);
    assert_eq!(
        ast.junctions().next().unwrap().container.as_deref(),
        Some("app_env")
    );
}

#[test]
fn registers_groups_before_services() {
    // `populateDb` writes every group before every service, so a service whose group comes
    // later in the text still finds its parent.
    let ast = ast("architecture-beta
  service s(server)[S] in g
  group g(cloud)[G]");
    assert_eq!(service_ids(&ast), ["s"]);
    assert_eq!(group_ids(&ast), ["g"]);
}

#[test]
fn registers_junctions_after_services() {
    // Junctions are written after services, so a service cannot live in a junction even when
    // the junction comes first in the text.
    let error = error(
        "architecture-beta
  junction j in g
  group g(cloud)[G]
  service s(server)[S] in j",
    );
    assert!(
        error
            .message
            .contains("The service [s]'s parent does not exist."),
        "{error}"
    );
}

#[test]
fn rejects_an_edge_to_an_id_that_does_not_exist() {
    let error = error(
        "architecture-beta
  service a(server)[A]
  a:R -- L:ghost",
    );
    assert!(
        error
            .message
            .contains("The right-hand id [ghost] does not yet exist"),
        "{error}"
    );
}

#[test]
fn rejects_a_group_boundary_that_stays_inside_one_group() {
    let error = error(
        "architecture-beta
  group api(cloud)[API]
  service a(server)[A] in api
  service b(server)[B] in api
  a{group}:R -- L:b",
    );
    assert!(
        error.message.contains(
            "The left-hand id [a] is modified to traverse the group boundary, but the edge does not pass through two groups."
        ),
        "{error}"
    );
}

#[test]
fn errors_on_a_group_endpoint_like_upstream() {
    // `addEdge` allows a group as an endpoint, then dereferences both ends as nodes.
    let error = error(
        "architecture-beta
  group api(cloud)[API]
  service a(server)[A] in api
  api:R -- L:a",
    );
    assert_eq!(
        error.message,
        "Cannot read properties of undefined (reading 'in')"
    );
}

#[test]
fn builds_the_adjacency_list_and_spatial_maps() {
    let ast = ast("architecture-beta
  service a(server)[A]
  service b(server)[B]
  a:R -- L:b");
    let structures = ast.data_structures();
    let directions = structures.adjacency_of("a").expect("a is a node");
    assert_eq!(
        directions,
        [(
            DirectionPair::new(Direction::Right, Direction::Left).unwrap(),
            "b".to_string()
        )]
    );
    assert_eq!(
        structures.spatial_maps,
        [vec![("a".to_string(), (0, 0)), ("b".to_string(), (1, 0)),]]
    );
}

#[test]
fn gives_every_disconnected_node_its_own_spatial_map() {
    let ast = ast("architecture-beta
  service a(server)[A]
  service b(server)[B]");
    let structures = ast.data_structures();
    assert_eq!(structures.spatial_maps.len(), 2);
    assert_eq!(structures.spatial_maps[0], [("a".to_string(), (0, 0))]);
    assert_eq!(structures.spatial_maps[1], [("b".to_string(), (0, 0))]);
}

fn group(id: &str) -> Group {
    Group {
        id: id.to_string(),
        icon: None,
        title: None,
        container: None,
    }
}

fn service_in(id: &str, container: Option<&str>) -> Service {
    Service {
        id: id.to_string(),
        icon: None,
        icon_text: None,
        title: None,
        container: container.map(str::to_string),
        edges: Vec::new(),
    }
}

fn junction(id: &str) -> Junction {
    Junction {
        id: id.to_string(),
        container: None,
        edges: Vec::new(),
    }
}

fn junction_in(id: &str, container: &str) -> Junction {
    Junction {
        id: id.to_string(),
        container: Some(container.to_string()),
        edges: Vec::new(),
    }
}
