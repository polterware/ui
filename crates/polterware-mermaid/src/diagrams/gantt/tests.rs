//! Tests ported from the upstream gantt specs: `parser/gantt.spec.js` and `ganttDb.spec.ts`.
//!
//! Inputs and expectations are kept as upstream wrote them. Assertions that spy on a
//! database function are expressed through the state the call leaves behind (the spec's
//! `setTodayMarker` spy becomes the parsed `today_marker`), and the `setClickEvent` spies are
//! reduced to the `clickable` class, because the function name and its arguments only build
//! DOM handlers this crate does not model.
//!
//! A few tests at the end cover this crate's adaptations rather than upstream cases: the
//! blank lines `preprocess` leaves where it removed frontmatter, the errors `parse` carries
//! with a span, and the `polterware_mermaid::parse` entry point.
//!
//! Skipped, and why:
//! - `ganttDb.spec.ts`'s "when using a timezone with daylight savings" block: upstream itself
//!   only runs it with `TZ="America/Los_Angeles"`. This crate's dates are wall-clock and have
//!   no zone, so a 25-hour day does not exist here.
//! - The `clear` spec's `getDisplayMode` row: only configuration sets `displayMode`, and this
//!   crate owns no configuration.
//! - Rendering and configuration assertions elsewhere in the gantt folder (the renderer spec)
//!   are out of scope for a parser port.

use super::ast::Task;
use super::date::{self, Date, Unit};
use super::db::Db;
use super::parse;
use crate::error::Span;

fn ast(body: &str) -> super::Ast {
    parse(body).unwrap_or_else(|error| panic!("expected a parse, got {error}"))
}

fn ymd(year: i64, month: u32, day: u32) -> Date {
    Date::new(year, month, day, 0, 0, 0, 0).expect("valid date")
}

fn ymd_hms(year: i64, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> Date {
    Date::new(year, month, day, hour, minute, second, 0).expect("valid date")
}

/// A database with the date format the database specs set up.
fn db() -> Db {
    let mut db = Db::new();
    db.set_date_format("YYYY-MM-DD");
    db
}

fn tasks(db: &mut Db) -> &[Task] {
    db.get_tasks()
        .unwrap_or_else(|error| panic!("expected tasks, got {error}"))
}

fn add(db: &mut Db, description: &str, data: &str) {
    db.add_task(description, data, Span::default());
}

// gantt.spec.js

#[test]
fn handles_a_date_format_definition() {
    let ast = ast("gantt\ndateFormat yyyy-mm-dd");
    assert_eq!(ast.date_format, "yyyy-mm-dd");
}

#[test]
fn handles_an_inclusive_end_date_definition() {
    let ast = ast("gantt\ndateFormat yyyy-mm-dd\ninclusiveEndDates");
    assert!(ast.inclusive_end_dates);
}

#[test]
fn handles_a_title_definition() {
    let title =
        ast("gantt\ndateFormat yyyy-mm-dd\ntitle Adding gantt diagram functionality to mermaid");
    assert_eq!(
        title.diagram_title,
        "Adding gantt diagram functionality to mermaid"
    );
    let semicolons =
        ast("gantt\ndateFormat yyyy-mm-dd\ntitle ;Gantt diagram titles support semicolons");
    assert_eq!(
        semicolons.diagram_title,
        ";Gantt diagram titles support semicolons"
    );
    let hashtags =
        ast("gantt\ndateFormat yyyy-mm-dd\ntitle #Gantt diagram titles support hashtags");
    assert_eq!(
        hashtags.diagram_title,
        "#Gantt diagram titles support hashtags"
    );
}

#[test]
fn handles_an_excludes_definition() {
    let ast = ast(
        "gantt\ndateFormat yyyy-mm-dd\ntitle Adding gantt diagram functionality to mermaid\nexcludes weekdays 2019-02-01",
    );
    assert_eq!(ast.excludes, ["weekdays", "2019-02-01"]);
}

#[test]
fn concatenates_tokens_across_multiple_excludes_lines() {
    // Issue #6270.
    let body = [
        "gantt",
        "dateFormat DD-MM-YYYY",
        "excludes weekends",
        "%% week 7 is winter break",
        "excludes 10-02-2025 11-02-2025 12-02-2025",
        "%% workers holiday 1 maj",
        "excludes 01-05-2025",
    ]
    .join("\n");
    let ast = ast(&body);
    assert_eq!(
        ast.excludes,
        [
            "weekends",
            "10-02-2025",
            "11-02-2025",
            "12-02-2025",
            "01-05-2025"
        ]
    );
}

#[test]
fn handles_a_today_marker_definition() {
    let ast = ast(
        "gantt\ndateFormat yyyy-mm-dd\ntitle Adding gantt diagram functionality to mermaid\nexcludes weekdays 2019-02-01\ntodayMarker off",
    );
    assert_eq!(ast.today_marker, "off");
}

#[test]
fn handles_a_section_definition() {
    for (name, expected) in [
        ("Documentation", "Documentation"),
        (";Documentation", ";Documentation"),
        ("#Documentation", "#Documentation"),
    ] {
        let body = format!(
            "gantt\ndateFormat yyyy-mm-dd\ntitle Adding gantt diagram functionality to mermaid\nexcludes weekdays 2019-02-01\nsection {name}"
        );
        let ast = ast(&body);
        assert_eq!(ast.sections, [expected], "section: {name}");
    }
}

#[test]
fn handles_multiline_section_titles_with_different_line_breaks() {
    let ast = ast(
        "gantt\ndateFormat yyyy-mm-dd\ntitle Adding gantt diagram functionality to mermaid\nexcludes weekdays 2019-02-01\nsection Line1<br>Line2<br/>Line3</br />Line4<br\t/>Line5",
    );
    assert_eq!(
        ast.sections,
        ["Line1<br>Line2<br/>Line3</br />Line4<br\t/>Line5"]
    );
}

#[test]
fn handles_a_task_definition() {
    for name in [
        "Design jison grammar",
        ";Design jison grammar",
        "#Design jison grammar",
    ] {
        let body = format!(
            "gantt\ndateFormat YYYY-MM-DD\ntitle Adding gantt diagram functionality to mermaid\nsection Documentation\n{name}:des1, 2014-01-01, 2014-01-04"
        );
        let ast = ast(&body);
        assert_eq!(
            ast.tasks[0].start_time,
            Some(ymd(2014, 1, 1)),
            "name: {name}"
        );
        assert_eq!(ast.tasks[0].end_time, Some(ymd(2014, 1, 4)), "name: {name}");
        assert_eq!(ast.tasks[0].id, "des1", "name: {name}");
        assert_eq!(ast.tasks[0].task, name, "name: {name}");
    }
}

#[test]
fn handles_a_task_with_start_and_end_times_relative_to_other_tasks() {
    let ast = ast(
        "gantt\ndateFormat YYYY-MM-DD\ntitle Adding gantt diagram functionality to mermaid\nsection Documentation\ntask A: a, 2024-01-27, 2024-01-28\ntask B: b, after a, 2024-01-30\ntask C: c, 2024-01-20, until a\ntask D: d, after c, until b",
    );
    let tasks = &ast.tasks;
    assert_eq!(tasks[0].start_time, Some(ymd(2024, 1, 27)));
    assert_eq!(tasks[0].end_time, Some(ymd(2024, 1, 28)));
    assert_eq!(tasks[0].id, "a");
    assert_eq!(tasks[0].task, "task A");
    assert_eq!(tasks[1].start_time, Some(ymd(2024, 1, 28)));
    assert_eq!(tasks[1].end_time, Some(ymd(2024, 1, 30)));
    assert_eq!(tasks[1].id, "b");
    assert_eq!(tasks[1].task, "task B");
    assert_eq!(tasks[2].start_time, Some(ymd(2024, 1, 20)));
    assert_eq!(tasks[2].end_time, Some(ymd(2024, 1, 27)));
    assert_eq!(tasks[2].id, "c");
    assert_eq!(tasks[2].task, "task C");
    assert_eq!(tasks[3].start_time, Some(ymd(2024, 1, 27)));
    assert_eq!(tasks[3].end_time, Some(ymd(2024, 1, 28)));
    assert_eq!(tasks[3].id, "d");
    assert_eq!(tasks[3].task, "task D");
}

#[test]
fn handles_a_task_with_tags() {
    let cases = [
        ("milestone", true, false, false, false),
        ("done", false, true, false, false),
        ("crit", false, false, true, false),
        ("active", false, false, false, true),
        ("crit,milestone,done", true, true, true, false),
    ];
    for (tags, milestone, done, crit, active) in cases {
        let body = format!(
            "gantt\ndateFormat YYYY-MM-DD\ntitle Adding gantt diagram functionality to mermaid\nsection Documentation\ntest task:{tags}, 2014-01-01, 2014-01-04"
        );
        let ast = ast(&body);
        let task = &ast.tasks[0];
        assert_eq!(task.milestone, milestone, "tags: {tags}");
        assert_eq!(task.done, done, "tags: {tags}");
        assert_eq!(task.crit, crit, "tags: {tags}");
        assert_eq!(task.active, active, "tags: {tags}");
    }
}

#[test]
fn parses_a_callback_specifier_with_no_args() {
    let body = "gantt\ndateFormat  YYYY-MM-DD\nsection Clickable\nVisit mermaidjs           :active, cl1, 2014-01-07, 3d\nCalling a callback        :cl2, after cl1, 3d\n\nclick cl1 href \"https://mermaidjs.github.io/\"\nclick cl2 call ganttTestClick()\n";
    let ast = ast(body);
    // The spy asserted `setClickEvent('cl2', 'ganttTestClick', null)`; the callback is a DOM
    // handler, so its observable effect is the class.
    let cl2 = ast
        .tasks
        .iter()
        .find(|task| task.id == "cl2")
        .expect("cl2 exists");
    assert_eq!(cl2.classes, ["clickable"]);
    assert_eq!(
        ast.links,
        [super::Link {
            id: "cl1".to_string(),
            url: "https://mermaidjs.github.io/".to_string(),
        }]
    );
}

#[test]
fn parses_a_callback_specifier_with_an_arbitrary_number_of_args() {
    let body = "gantt\ndateFormat  YYYY-MM-DD\nsection Clickable\nVisit mermaidjs           :active, cl1, 2014-01-07, 3d\nCalling a callback        :cl2, after cl1, 3d\n\nclick cl1 href \"https://mermaidjs.github.io/\"\nclick cl2 call ganttTestClick(\"test0\", test1, test2)\n";
    let ast = ast(body);
    let cl2 = ast
        .tasks
        .iter()
        .find(|task| task.id == "cl2")
        .expect("cl2 exists");
    assert_eq!(cl2.classes, ["clickable"]);
}

#[test]
fn allows_a_accessibility_title_and_description() {
    let body = "gantt
       accTitle: Gantt Diagram
       accDescr: Tasks for Q4
       dateFormat  YYYY-MM-DD
       section Section
       A task :a1, 2014-01-01, 30d\n";
    let ast = ast(body);
    assert_eq!(ast.acc_title, "Gantt Diagram");
    assert_eq!(ast.acc_description, "Tasks for Q4");
}

#[test]
fn allows_a_accessibility_title_and_multiline_description() {
    let body = "gantt
       accTitle: Gantt Diagram
       accDescr {
         Tasks for Q4 row1
row2
       }
       dateFormat  YYYY-MM-DD
       section Section
       A task :a1, 2014-01-01, 30d\n";
    let ast = ast(body);
    assert_eq!(ast.acc_title, "Gantt Diagram");
    assert_eq!(ast.acc_description, "Tasks for Q4 row1\nrow2");
}

#[test]
fn allows_setting_the_starting_weekday() {
    for day in [
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
        "sunday",
    ] {
        let ast = ast(&format!("gantt\nweekday {day}"));
        assert_eq!(ast.weekday, day);
    }
}

#[test]
fn allows_a_link_to_an_object_prototype_id() {
    // Upstream's plain-object `taskDb` drops `__proto__`; this port's map keeps it, and both
    // accept the statement.
    for property in ["__proto__", "constructor"] {
        let body = format!(
            "gantt
    dateFormat YYYY-MM-DD
    section Section
    A task :{property}, 2024-10-01, 3d
    click {property} href \"https://mermaid.js.org/\"
    "
        );
        let ast = ast(&body);
        assert!(
            ast.links.iter().any(|link| link.id == property),
            "property: {property}"
        );
    }
}

// ganttDb.spec.ts

#[test]
fn parses_durations() {
    assert_eq!(date::parse_duration("1d"), (1.0, Unit::Day));
    for (text, expected) in [
        ("1d", (1.0, Unit::Day)),
        ("2w", (2.0, Unit::Week)),
        ("1ms", (1.0, Unit::Millisecond)),
        ("0.1s", (0.1, Unit::Second)),
    ] {
        assert_eq!(date::parse_duration(text), expected, "duration: {text}");
    }
    let (value, unit) = date::parse_duration("1f");
    assert!(value.is_nan(), "1f is not a duration");
    assert_eq!(unit, Unit::Millisecond);
}

#[test]
fn clears_the_database() {
    let mut db = Db::new();
    db.set_date_format("YYYY-MM-DD");
    db.enable_inclusive_end_dates();
    db.set_today_marker("off");
    db.set_excludes("weekends 2019-02-06,friday");
    db.add_section("weekends skip test");
    add(&mut db, "test1", "id1,2019-02-01,1d");
    add(&mut db, "test2", "id2,after id1,2d");
    db.clear();
    assert!(tasks(&mut db).is_empty(), "getTasks");
    assert_eq!(db.acc_title(), "", "getAccTitle");
    assert_eq!(db.acc_description(), "", "getAccDescription");
    assert_eq!(db.date_format(), "", "getDateFormat");
    assert_eq!(db.axis_format(), "", "getAxisFormat");
    assert_eq!(db.today_marker(), "", "getTodayMarker");
    assert!(db.excludes().is_empty(), "getExcludes");
    assert!(db.sections().is_empty(), "getSections");
    assert!(!db.end_dates_are_inclusive(), "endDatesAreInclusive");
}

#[test]
fn handles_fixed_dates_and_durations() {
    let cases: [(&str, &str, Date, Date, &str, &str); 8] = [
        (
            "test1",
            "id1,2013-01-01,2013-01-12",
            ymd(2013, 1, 1),
            ymd(2013, 1, 12),
            "id1",
            "test1",
        ),
        (
            "test1",
            "id1,2013-01-01,2d",
            ymd(2013, 1, 1),
            ymd(2013, 1, 3),
            "id1",
            "test1",
        ),
        (
            "test1",
            "id1,2013-01-01,2h",
            ymd(2013, 1, 1),
            ymd_hms(2013, 1, 1, 2, 0, 0),
            "id1",
            "test1",
        ),
        (
            "test1",
            "id1,2013-01-01,2m",
            ymd(2013, 1, 1),
            Date::new(2013, 1, 1, 0, 2, 0, 0).expect("valid date"),
            "id1",
            "test1",
        ),
        (
            "test1",
            "id1,2013-01-01,2s",
            ymd(2013, 1, 1),
            Date::new(2013, 1, 1, 0, 0, 2, 0).expect("valid date"),
            "id1",
            "test1",
        ),
        (
            "test1",
            "id1,2013-01-01,2w",
            ymd(2013, 1, 1),
            ymd(2013, 1, 15),
            "id1",
            "test1",
        ),
        (
            "test1",
            "2013-01-01,2013-01-12",
            ymd(2013, 1, 1),
            ymd(2013, 1, 12),
            "task1",
            "test1",
        ),
        (
            "test1",
            "2013-01-01,4d",
            ymd(2013, 1, 1),
            ymd(2013, 1, 5),
            "task1",
            "test1",
        ),
    ];
    for (name, data, start, end, id, task) in cases {
        let mut db = db();
        db.add_section("testa1");
        add(&mut db, name, data);
        let tasks = tasks(&mut db);
        assert_eq!(tasks[0].start_time, Some(start), "data: {data}");
        assert_eq!(tasks[0].end_time, Some(end), "data: {data}");
        assert_eq!(tasks[0].id, id, "data: {data}");
        assert_eq!(tasks[0].task, task, "data: {data}");
    }
}

/// The columns of the relative-start table in `ganttDb.spec.ts`.
type RelativeStart = (
    &'static str,
    &'static str,
    Date,
    Option<Date>,
    &'static str,
    &'static str,
);

#[test]
fn handles_relative_start_dates() {
    let today = Date::today();
    let cases: [RelativeStart; 5] = [
        (
            "id1,2013-01-01,2w",
            "id2,after id1,1d",
            ymd(2013, 1, 15),
            None,
            "id2",
            "test2",
        ),
        (
            "id1,2013-01-01,2w",
            "id2,after id3,1d",
            today,
            None,
            "id2",
            "test2",
        ),
        (
            "id1,2013-01-01,2w",
            "after id1,1d",
            ymd(2013, 1, 15),
            None,
            "task1",
            "test2",
        ),
        (
            "id1,2013-01-01,2w",
            "2013-01-26",
            ymd(2013, 1, 15),
            Some(ymd(2013, 1, 26)),
            "task1",
            "test2",
        ),
        (
            "id1,2013-01-01,2w",
            "2d",
            ymd(2013, 1, 15),
            Some(ymd(2013, 1, 17)),
            "task1",
            "test2",
        ),
    ];
    for (first_data, second_data, start, end, id, task) in cases {
        let mut db = db();
        db.add_section("testa1");
        add(&mut db, "test1", first_data);
        add(&mut db, "test2", second_data);
        let tasks = tasks(&mut db);
        assert_eq!(tasks[1].start_time, Some(start), "data: {second_data}");
        if let Some(end) = end {
            assert_eq!(tasks[1].end_time, Some(end), "data: {second_data}");
        }
        assert_eq!(tasks[1].id, id, "data: {second_data}");
        assert_eq!(tasks[1].task, task, "data: {second_data}");
    }
}

#[test]
fn handles_milliseconds() {
    let mut db = Db::new();
    db.set_date_format("x");
    db.add_section("testa1");
    add(&mut db, "test1", "id1,0,20ms");
    add(&mut db, "test2", "id2,after id1,5ms");
    db.add_section("testa2");
    add(&mut db, "test3", "id3,20,10ms");
    add(&mut db, "test4", "id4,after id3,0.005s");
    let tasks = tasks(&mut db);
    let at = |milliseconds| Date::new(1970, 1, 1, 0, 0, 0, milliseconds).expect("valid date");
    assert_eq!(tasks[0].start_time, Some(at(0)));
    assert_eq!(tasks[0].end_time, Some(at(20)));
    assert_eq!(tasks[1].start_time, Some(at(20)));
    assert_eq!(tasks[1].end_time, Some(at(25)));
    assert_eq!(tasks[2].start_time, Some(at(20)));
    assert_eq!(tasks[2].end_time, Some(at(30)));
    assert_eq!(tasks[3].start_time, Some(at(30)));
    assert_eq!(tasks[3].end_time, Some(at(35)));
}

#[test]
fn handles_a_relative_start_date_based_on_id_regardless_of_sections() {
    let mut db = db();
    db.add_section("sec1");
    add(&mut db, "test1", "id1,2013-01-01,2w");
    add(&mut db, "test2", "id2,after id3,1d");
    db.add_section("sec2");
    add(&mut db, "test3", "id3,after id1,2d");
    let tasks = tasks(&mut db);
    assert_eq!(tasks[1].start_time, Some(ymd(2013, 1, 17)));
    assert_eq!(tasks[1].end_time, Some(ymd(2013, 1, 18)));
    assert_eq!(tasks[1].id, "id2");
    assert_eq!(tasks[1].task, "test2");
    assert_eq!(tasks[2].id, "id3");
    assert_eq!(tasks[2].task, "test3");
    assert_eq!(tasks[2].start_time, Some(ymd(2013, 1, 15)));
    assert_eq!(tasks[2].end_time, Some(ymd(2013, 1, 17)));
}

#[test]
fn handles_a_relative_end_date_based_on_id_regardless_of_sections() {
    let mut db = db();
    db.add_section("sec1");
    add(&mut db, "task1", "id1,2013-01-01,until id3");
    db.add_section("sec2");
    add(&mut db, "task2", "id2,2013-01-10,until id3");
    add(&mut db, "task3", "id3,2013-02-01,2d");
    let tasks = tasks(&mut db);
    assert_eq!(tasks[0].start_time, Some(ymd(2013, 1, 1)));
    assert_eq!(tasks[0].end_time, Some(ymd(2013, 2, 1)));
    assert_eq!(tasks[0].id, "id1");
    assert_eq!(tasks[0].task, "task1");
    assert_eq!(tasks[1].id, "id2");
    assert_eq!(tasks[1].task, "task2");
    assert_eq!(tasks[1].start_time, Some(ymd(2013, 1, 10)));
    assert_eq!(tasks[1].end_time, Some(ymd(2013, 2, 1)));
}

#[test]
fn handles_a_relative_start_date_based_on_multiple_ids() {
    let mut db = db();
    db.add_section("sec1");
    add(&mut db, "task1", "id1,after id2 id3 id4,1d");
    add(&mut db, "task2", "id2,2013-01-01,1d");
    add(&mut db, "task3", "id3,2013-02-01,3d");
    add(&mut db, "task4", "id4,2013-02-01,2d");
    let tasks = tasks(&mut db);
    assert_eq!(tasks[0].end_time, Some(ymd(2013, 2, 5)));
    assert_eq!(tasks[0].id, "id1");
    assert_eq!(tasks[0].task, "task1");
}

#[test]
fn handles_a_relative_end_date_based_on_multiple_ids() {
    let mut db = db();
    db.add_section("sec1");
    add(&mut db, "task1", "id1,2013-01-01,until id2 id3 id4");
    add(&mut db, "task2", "id2,2013-01-11,1d");
    add(&mut db, "task3", "id3,2013-02-10,1d");
    add(&mut db, "task4", "id4,2013-02-12,1d");
    let tasks = tasks(&mut db);
    assert_eq!(tasks[0].end_time, Some(ymd(2013, 1, 11)));
    assert_eq!(tasks[0].id, "id1");
    assert_eq!(tasks[0].task, "task1");
}

#[test]
fn ignores_weekends() {
    let mut db = db();
    db.set_excludes("weekends 2019-02-06,friday");
    db.add_section("weekends skip test");
    add(&mut db, "test1", "id1,2019-02-01,1d");
    add(&mut db, "test2", "id2,after id1,2d");
    add(&mut db, "test3", "id3,after id2,7d");
    add(&mut db, "test4", "id4,2019-02-01,2019-02-20");
    add(&mut db, "test5", "id5,after id4,1d");
    db.add_section("full ending task on last day");
    add(&mut db, "test6", "id6,2019-02-13,2d");
    add(&mut db, "test7", "id7,after id6,1d");
    let tasks = tasks(&mut db);

    assert_eq!(tasks[0].start_time, Some(ymd(2019, 2, 1)));
    assert_eq!(tasks[0].end_time, Some(ymd(2019, 2, 4)));
    assert_eq!(tasks[0].render_end_time, Some(ymd(2019, 2, 2)));
    assert_eq!(tasks[0].id, "id1");
    assert_eq!(tasks[0].task, "test1");

    assert_eq!(tasks[1].start_time, Some(ymd(2019, 2, 4)));
    assert_eq!(tasks[1].end_time, Some(ymd(2019, 2, 7)));
    assert_eq!(tasks[1].render_end_time, Some(ymd(2019, 2, 6)));
    assert_eq!(tasks[1].id, "id2");
    assert_eq!(tasks[1].task, "test2");

    assert_eq!(tasks[2].start_time, Some(ymd(2019, 2, 7)));
    assert_eq!(tasks[2].end_time, Some(ymd(2019, 2, 20)));
    assert_eq!(tasks[2].render_end_time, Some(ymd(2019, 2, 20)));
    assert_eq!(tasks[2].id, "id3");
    assert_eq!(tasks[2].task, "test3");

    assert_eq!(tasks[3].start_time, Some(ymd(2019, 2, 1)));
    assert_eq!(tasks[3].end_time, Some(ymd(2019, 2, 20)));
    assert_eq!(tasks[3].render_end_time, None);
    assert_eq!(tasks[3].id, "id4");
    assert_eq!(tasks[3].task, "test4");

    assert_eq!(tasks[4].start_time, Some(ymd(2019, 2, 20)));
    assert_eq!(tasks[4].end_time, Some(ymd(2019, 2, 21)));
    assert_eq!(tasks[4].render_end_time, Some(ymd(2019, 2, 21)));
    assert_eq!(tasks[4].id, "id5");
    assert_eq!(tasks[4].task, "test5");

    assert_eq!(tasks[5].start_time, Some(ymd(2019, 2, 13)));
    assert_eq!(tasks[5].end_time, Some(ymd(2019, 2, 18)));
    assert_eq!(tasks[5].render_end_time, Some(ymd(2019, 2, 15)));
    assert_eq!(tasks[5].id, "id6");
    assert_eq!(tasks[5].task, "test6");

    assert_eq!(tasks[6].start_time, Some(ymd(2019, 2, 18)));
    assert_eq!(tasks[6].end_time, Some(ymd(2019, 2, 19)));
    assert_eq!(tasks[6].id, "id7");
    assert_eq!(tasks[6].task, "test7");
}

#[test]
fn ignores_weekends_starting_on_friday() {
    let mut db = db();
    db.set_excludes("weekends");
    db.set_weekend("friday");
    db.add_section("friday-saturday weekends skip test");
    add(&mut db, "test1", "id1,2024-02-28, 3d");
    let tasks = tasks(&mut db);
    assert_eq!(tasks[0].start_time, Some(ymd(2024, 2, 28)));
    assert_eq!(tasks[0].end_time, Some(ymd(2024, 3, 4)));
    assert_eq!(tasks[0].id, "id1");
    assert_eq!(tasks[0].task, "test1");
}

#[test]
fn does_not_infinite_loop_when_excluding_everything() {
    let mut db = db();
    db.set_excludes("weekends,monday,tuesday,wednesday,thursday,friday");
    db.set_weekend("saturday");
    db.add_section("weekends skip test");
    add(&mut db, "test1", "id1,2019-02-01,7d");
    let error = db.get_tasks().expect_err("excludes every day");
    assert!(
        error.message.contains("Failed to find a valid date"),
        "message: {error}"
    );

    // Fridays are now allowed, so it should not throw an error.
    db.clear();
    db.set_date_format("YYYY-MM-DD");
    db.set_excludes("weekends,monday,tuesday,wednesday,thursday");
    db.set_weekend("saturday");
    db.add_section("weekends skip test");
    add(&mut db, "test1", "id1,2019-02-01,7d");
    assert!(db.get_tasks().is_ok());
}

#[test]
fn merges_tokens_across_multiple_set_excludes_calls() {
    let mut db = Db::new();
    db.set_excludes("weekends");
    db.set_excludes("2019-02-06");
    db.set_excludes("friday, monday");
    assert_eq!(
        db.excludes(),
        ["weekends", "2019-02-06", "friday", "monday"]
    );
}

#[test]
fn dedupes_tokens_across_multiple_set_excludes_calls() {
    let mut db = Db::new();
    db.set_excludes("weekends,2019-02-06");
    db.set_excludes("weekends 2019-02-07");
    assert_eq!(db.excludes(), ["weekends", "2019-02-06", "2019-02-07"]);
}

#[test]
fn merges_tokens_across_multiple_set_includes_calls() {
    let mut db = Db::new();
    db.set_includes("2019-02-06");
    db.set_includes("2019-02-07,2019-02-08");
    assert_eq!(db.includes(), ["2019-02-06", "2019-02-07", "2019-02-08"]);
}

#[test]
fn maintains_the_order_in_which_tasks_are_created() {
    let mut db = Db::new();
    db.set_acc_title("Project Execution");
    db.set_date_format("YYYY-MM-DD");
    db.add_section("section A section");
    add(
        &mut db,
        "Completed task",
        "done,    des1, 2014-01-06,2014-01-08",
    );
    add(&mut db, "Active task", "active,  des2, 2014-01-09, 3d");
    add(&mut db, "Future task", "des3, after des2, 5d");
    add(&mut db, "Future task2", "des4, after des3, 5d");

    db.add_section("section Critical tasks");
    add(
        &mut db,
        "Completed task in the critical line",
        "crit, done, 2014-01-06,24h",
    );
    add(
        &mut db,
        "Implement parser and jison",
        "crit, done, after des1, 2d",
    );
    add(&mut db, "Create tests for parser", "crit, active, 3d");
    add(&mut db, "Future task in critical line", "crit, 5d");
    add(&mut db, "Create tests for renderer", "2d");
    add(&mut db, "Add to mermaid", "1d");

    db.add_section("section Documentation");
    add(
        &mut db,
        "Describe gantt syntax",
        "active, a1, after des1, 3d",
    );
    add(&mut db, "Add gantt diagram to demo page", "after a1  , 20h");
    add(
        &mut db,
        "Add another diagram to demo page",
        "doc1, after a1  , 48h",
    );

    db.add_section("section Last section");
    add(&mut db, "Describe gantt syntax", "after doc1, 3d");
    add(&mut db, "Add gantt diagram to demo page", "20h");
    add(&mut db, "Add another diagram to demo page", "48h");

    let tasks = tasks(&mut db);

    let expectations: [(Date, Date, i64, &str, &str); 16] = [
        (
            ymd(2014, 1, 6),
            ymd(2014, 1, 8),
            0,
            "des1",
            "Completed task",
        ),
        (ymd(2014, 1, 9), ymd(2014, 1, 12), 1, "des2", "Active task"),
        (ymd(2014, 1, 12), ymd(2014, 1, 17), 2, "des3", "Future task"),
        (
            ymd(2014, 1, 17),
            ymd(2014, 1, 22),
            3,
            "des4",
            "Future task2",
        ),
        (
            ymd(2014, 1, 6),
            ymd(2014, 1, 7),
            4,
            "task1",
            "Completed task in the critical line",
        ),
        (
            ymd(2014, 1, 8),
            ymd(2014, 1, 10),
            5,
            "task2",
            "Implement parser and jison",
        ),
        (
            ymd(2014, 1, 10),
            ymd(2014, 1, 13),
            6,
            "task3",
            "Create tests for parser",
        ),
        (
            ymd(2014, 1, 13),
            ymd(2014, 1, 18),
            7,
            "task4",
            "Future task in critical line",
        ),
        (
            ymd(2014, 1, 18),
            ymd(2014, 1, 20),
            8,
            "task5",
            "Create tests for renderer",
        ),
        (
            ymd(2014, 1, 20),
            ymd(2014, 1, 21),
            9,
            "task6",
            "Add to mermaid",
        ),
        (
            ymd(2014, 1, 8),
            ymd(2014, 1, 11),
            10,
            "a1",
            "Describe gantt syntax",
        ),
        (
            ymd(2014, 1, 11),
            ymd_hms(2014, 1, 11, 20, 0, 0),
            11,
            "task7",
            "Add gantt diagram to demo page",
        ),
        (
            ymd(2014, 1, 11),
            ymd(2014, 1, 13),
            12,
            "doc1",
            "Add another diagram to demo page",
        ),
        (
            ymd(2014, 1, 13),
            ymd(2014, 1, 16),
            13,
            "task8",
            "Describe gantt syntax",
        ),
        (
            ymd(2014, 1, 16),
            ymd_hms(2014, 1, 16, 20, 0, 0),
            14,
            "task9",
            "Add gantt diagram to demo page",
        ),
        (
            ymd_hms(2014, 1, 16, 20, 0, 0),
            ymd_hms(2014, 1, 18, 20, 0, 0),
            15,
            "task10",
            "Add another diagram to demo page",
        ),
    ];
    for (index, (start, end, order, id, task)) in expectations.into_iter().enumerate() {
        assert_eq!(tasks[index].start_time, Some(start), "task {index} start");
        assert_eq!(tasks[index].end_time, Some(end), "task {index} end");
        assert_eq!(tasks[index].order, order, "task {index} order");
        assert_eq!(tasks[index].id, id, "task {index} id");
        assert_eq!(tasks[index].task, task, "task {index} task");
    }
}

#[test]
fn works_when_the_end_date_is_the_31st() {
    let mut db = db();
    db.add_section("Task endTime is on the 31st day of the month");
    add(&mut db, "test1", "id1,2019-09-30,11d");
    add(&mut db, "test2", "id2,after id1,20d");
    let tasks = tasks(&mut db);
    assert_eq!(tasks[0].start_time, Some(ymd(2019, 9, 30)));
    assert_eq!(tasks[0].end_time, Some(ymd(2019, 10, 11)));
    assert_eq!(tasks[0].render_end_time, None);
    assert_eq!(tasks[0].id, "id1");
    assert_eq!(tasks[0].task, "test1");
    assert_eq!(tasks[1].start_time, Some(ymd(2019, 10, 11)));
    assert_eq!(tasks[1].end_time, Some(ymd(2019, 10, 31)));
    assert_eq!(tasks[1].render_end_time, None);
    assert_eq!(tasks[1].id, "id2");
    assert_eq!(tasks[1].task, "test2");
}

#[test]
fn adds_one_day_to_all_end_dates_when_inclusive() {
    let mut db = db();
    db.enable_inclusive_end_dates();
    add(&mut db, "test1", "id1,2019-02-01,1d");
    add(&mut db, "test2", "id2,2019-02-01,2019-02-03");
    let tasks = tasks(&mut db);
    assert_eq!(tasks[0].start_time, Some(ymd(2019, 2, 1)));
    assert_eq!(tasks[0].end_time, Some(ymd(2019, 2, 2)));
    assert_eq!(tasks[0].id, "id1");
    assert_eq!(tasks[0].task, "test1");
    assert_eq!(tasks[1].start_time, Some(ymd(2019, 2, 1)));
    assert_eq!(tasks[1].end_time, Some(ymd(2019, 2, 4)));
    assert_eq!(tasks[1].render_end_time, None);
    assert!(tasks[1].manual_end_time);
    assert_eq!(tasks[1].id, "id2");
    assert_eq!(tasks[1].task, "test2");
}

#[test]
fn sets_the_today_marker() {
    for expected in ["off", "stoke:stroke-width:5px,stroke:#00f,opacity:0.5"] {
        let mut db = Db::new();
        db.set_today_marker(expected);
        assert_eq!(db.today_marker(), expected);
    }
}

#[test]
fn rejects_dates_with_ridiculous_years() {
    let mut db = Db::new();
    db.set_date_format("YYYYMMDD");
    add(&mut db, "test1", "id1,202304,1d");
    let error = db.get_tasks().expect_err("year is out of range");
    assert_eq!(error.message, "Invalid date:202304");
}

#[test]
fn handles_a_seconds_only_format_with_valid_numeric_values() {
    // Issue #5496.
    let mut db = Db::new();
    db.set_date_format("ss");
    db.add_section("Network Request");
    add(&mut db, "RTT", "rtt, 0, 20");
    let tasks = tasks(&mut db);
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].task, "RTT");
    assert_eq!(tasks[0].id, "rtt");
}

#[test]
fn handles_dates_with_a_year_typo() {
    // Issue #5496.
    let mut db = db();
    db.add_section("Vacation");
    add(&mut db, "London Trip 1", "2024-12-01, 7d");
    add(&mut db, "London Trip 2", "202-12-01, 7d");
    let tasks = tasks(&mut db);
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].start_time.map(|date| date.year), Some(2024));
    // The second start falls back to `new Date()`, which reads year 202.
    assert_eq!(tasks[1].start_time.map(|date| date.year), Some(202));
}

#[test]
fn warns_about_unknown_ids_in_an_after_statement() {
    // Issue #4121.
    let mut db = db();
    add(&mut db, "task1", "id1,2013-01-01,2d");
    add(&mut db, "task2", "id2,after m1,1d");
    db.get_tasks().expect("compiles");
    assert!(
        db.warnings().iter().any(|warning| warning.contains("m1")),
        "warnings: {:?}",
        db.warnings()
    );
}

#[test]
fn warns_about_unknown_ids_in_an_until_statement() {
    let mut db = db();
    add(&mut db, "task1", "id1,2013-01-01,until m1");
    db.get_tasks().expect("compiles");
    assert!(
        db.warnings().iter().any(|warning| warning.contains("m1")),
        "warnings: {:?}",
        db.warnings()
    );
}

#[test]
fn does_not_warn_when_every_referenced_id_exists() {
    let mut db = db();
    add(&mut db, "task1", "id1,2013-01-01,2d");
    add(&mut db, "task2", "id2,after id1,1d");
    db.get_tasks().expect("compiles");
    assert!(db.warnings().is_empty(), "warnings: {:?}", db.warnings());
}

#[test]
fn warns_when_a_milestone_is_declared_without_a_duration() {
    // Issue #4121: without a duration, `m1` is parsed as the duration instead of the id.
    let mut db = db();
    add(&mut db, "M1", "milestone, 2023-01-01, m1");
    let tasks = tasks(&mut db);
    assert_eq!(tasks[0].end_time, tasks[0].start_time);
    assert!(
        db.warnings().iter().any(|warning| warning.contains("m1")),
        "warnings: {:?}",
        db.warnings()
    );
}

#[test]
fn parses_remarks_and_blank_lines_around_statements() {
    let ast = ast("gantt
%% a comment after the header

dateFormat YYYY-MM-DD
section A

task1 :t1, 2020-01-01, 2d
%% trailing comment
");
    assert_eq!(ast.date_format, "YYYY-MM-DD");
    assert_eq!(ast.sections, ["A"]);
    assert_eq!(ast.tasks[0].id, "t1");
}

#[test]
fn reports_syntax_errors_with_a_span() {
    let cases = [
        ("not a gantt", "Expecting 'gantt'"),
        ("gantt\ntask without data", "Expecting 'taskData'"),
        ("gantt\ndateFormat", "Expecting 'taskData'"),
        ("gantt\nsection", "Expecting 'taskData'"),
        ("gantt\n:orphan, 2014-01-01, 2d", "Expecting a statement"),
        ("gantt\naccDescription hidden", "Expecting a statement"),
        ("gantt\nclick cl1", "Expecting 'callbackname' or 'href'"),
        ("%%{init: {'theme': 'dark'}}%%\ngantt", "Unexpected '%%{'"),
    ];
    for (body, message) in cases {
        let error = parse(body).expect_err(body);
        assert!(
            error.message.contains(message),
            "body: {body:?}, message: {error}"
        );
        assert!(error.span.is_some(), "body: {body:?} has a span");
    }
}

#[test]
fn reports_task_data_errors_with_a_span() {
    let cases = [
        (
            "gantt\ndateFormat YYYY-MM-DD\ntask :1d",
            "needs one",
            "a first task cannot start after the previous one",
        ),
        (
            "gantt\ndateFormat YYYY-MM-DD\ntask :a, 2014-01-01, 3d, extra",
            "Invalid task data",
            "more than three fields",
        ),
        (
            "gantt\ndateFormat YYYY-MM-DD\ntask :a, 2014-13-01, 3d",
            "Invalid date:2014-13-01",
            "an impossible date",
        ),
    ];
    for (body, message, why) in cases {
        let error = parse(body).expect_err(why);
        assert!(
            error.message.contains(message),
            "{why}: expected {message:?}, got {error}"
        );
        let span = error.span.expect("has a span");
        assert_eq!(span.line, 3, "{why}");
    }
}

#[test]
fn skips_the_blank_lines_preprocess_leaves_behind() {
    // `preprocess` blanks removed frontmatter and directive lines instead of deleting them,
    // so the header can arrive after empty lines.
    let ast = ast("\n\n\ngantt\ndateFormat YYYY-MM-DD\ntask :t1, 2020-01-01, 2d");
    assert_eq!(ast.date_format, "YYYY-MM-DD");
    assert_eq!(ast.tasks.len(), 1);
}

#[test]
fn skips_a_directive_line_after_the_header_as_a_comment() {
    // The comment rule `[^\}]\%\%*[^\n]*` starts at the line break before the line, so a
    // `%%{` line after the header is consumed like a comment instead of failing. Upstream's
    // lexer does the same; preprocess removes directives before the parser runs.
    let ast = ast("gantt\n%%{init: {'theme': 'dark'}}%%\ntitle After\n");
    assert_eq!(ast.diagram_title, "After");
}

#[test]
fn includes_override_excludes_for_a_date() {
    let mut db = db();
    db.set_excludes("weekends");
    db.set_includes("2019-02-02");
    db.add_section("includes win over excludes");
    add(&mut db, "test1", "id1,2019-02-01,1d");
    let tasks = tasks(&mut db);
    // Saturday 2019-02-02 is a weekend, but it is included, so the end is not pushed.
    assert_eq!(tasks[0].start_time, Some(ymd(2019, 2, 1)));
    assert_eq!(tasks[0].end_time, Some(ymd(2019, 2, 2)));
    assert_eq!(tasks[0].render_end_time, Some(ymd(2019, 2, 2)));
}

#[test]
fn parses_through_the_crate_entry_point() {
    let body = "---\ntitle: Release\n---\ngantt\n    dateFormat YYYY-MM-DD\n    title Release plan\n    section Build\n    Compile :done, des1, 2024-03-01, 2d\n    Ship :milestone, m1, after des1, 0d\n";
    match crate::parse(body).expect("the crate parses the diagram") {
        crate::Diagram::Gantt(ast) => {
            assert_eq!(ast.diagram_title, "Release plan");
            assert_eq!(ast.sections, ["Build"]);
            assert_eq!(ast.tasks.len(), 2);
            assert_eq!(ast.tasks[1].start_time, Some(ymd(2024, 3, 3)));
            assert!(ast.tasks[1].milestone);
        }
        other => panic!("expected a gantt diagram, got {other:?}"),
    }
}
