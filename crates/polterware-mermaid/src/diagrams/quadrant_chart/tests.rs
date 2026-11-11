//! Tests ported from the upstream quadrant chart specs:
//! `packages/mermaid/src/diagrams/quadrant-chart/parser/quadrant.jison.spec.ts` (the grammar)
//! and `packages/mermaid/src/diagrams/quadrant-chart/quadrantDb.spec.ts` (the database).
//! Inputs and expectations are upstream's.
//!
//! The parser spec runs the grammar with a mock database and asserts the arguments each action
//! passes to `yy`. [`super::parser::parse`] hands the `addPoint` and `addClass` calls to a
//! [`Sink`] (the [`Recorder`] below plays the mock's part) and returns the single-valued setters
//! as [`ParsedDiagram`], so the assertions below are the mock's recorded calls. The `type` field
//! of the `text` objects is not kept (the database drops it); the expected text is compared.
//!
//! Skipped, and why:
//! - `quadrantDb.spec.ts`'s `quadrant db runtime config` describe (`getConfig` is read at
//!   sanitization time): this crate owns no configuration.
//! - `clearMocks` between cases: every case parses its own input, so there is no shared mock.
//! - `quadrantRenderer.spec.ts` renders; nothing here renders.
//!
//! Added on top of upstream's cases, all about the real database the parser spec mocks: a
//! `classDef` style the database rejects, the position a style error reports, and the order of
//! an earlier style error against a later parse error.

use super::Styles;
use super::db::{Sink, StyleText, parse_styles};
use super::parse;
use super::parser::{self, ParsedClass, ParsedDiagram, ParsedPoint};
use crate::error::ParseError;

/// The mock database of the upstream parser spec: it records the calls and never validates.
#[derive(Default)]
struct Recorder {
    points: Vec<ParsedPoint>,
    classes: Vec<ParsedClass>,
}

impl Sink for Recorder {
    fn add_point(&mut self, point: ParsedPoint) -> Result<(), ParseError> {
        self.points.push(point);
        Ok(())
    }

    fn add_class(&mut self, class: ParsedClass) -> Result<(), ParseError> {
        self.classes.push(class);
        Ok(())
    }
}

/// What the grammar parsed: the setters' values and the recorded `addPoint`/`addClass` calls.
struct Recorded {
    diagram: ParsedDiagram,
    points: Vec<ParsedPoint>,
    classes: Vec<ParsedClass>,
}

impl std::ops::Deref for Recorded {
    type Target = ParsedDiagram;

    fn deref(&self) -> &ParsedDiagram {
        &self.diagram
    }
}

fn parsed(body: &str) -> Recorded {
    let mut recorder = Recorder::default();
    let diagram = parser::parse(body, &mut recorder)
        .unwrap_or_else(|error| panic!("expected {body:?} to parse, got {error}"));
    Recorded {
        diagram,
        points: recorder.points,
        classes: recorder.classes,
    }
}

fn parse_error(body: &str) -> ParseError {
    let mut recorder = Recorder::default();
    match parser::parse(body, &mut recorder) {
        Ok(parsed) => panic!("expected {body:?} to fail, parsed {parsed:?}"),
        Err(error) => error,
    }
}

fn points(recorded: &Recorded) -> Vec<&ParsedPoint> {
    recorded.points.iter().collect()
}

fn classes(recorded: &Recorded) -> Vec<&ParsedClass> {
    recorded.classes.iter().collect()
}

fn style_texts(point: &ParsedPoint) -> Vec<&str> {
    point
        .styles
        .iter()
        .map(|style| style.text.as_str())
        .collect()
}

fn style_texts_of_class(class: &ParsedClass) -> Vec<&str> {
    class
        .styles
        .iter()
        .map(|style| style.text.as_str())
        .collect()
}

// quadrant.jison.spec.ts

#[test]
fn throws_without_the_header() {
    let body = "quadrant-1 do";
    assert!(parse(body).is_err(), "{body:?}");
}

#[test]
fn parses_a_lone_header() {
    let recorded = parsed("quadrantChart");
    assert_eq!(recorded.diagram, ParsedDiagram::default());
    assert!(recorded.points.is_empty());
    assert!(recorded.classes.is_empty());
}

#[test]
fn parses_x_axis_text() {
    let body = "quadrantChart\nx-axis urgent --> not urgent";
    let diagram = parsed(body);
    assert_eq!(diagram.x_axis_left.as_deref(), Some("urgent"));
    assert_eq!(diagram.x_axis_right.as_deref(), Some("not urgent"));

    let body = "quadrantChart\n       x-AxIs         Urgent     -->        Not Urgent    \n";
    let diagram = parsed(body);
    assert_eq!(diagram.x_axis_left.as_deref(), Some("Urgent"));
    assert_eq!(diagram.x_axis_right.as_deref(), Some("Not Urgent    "));

    let body = "quadrantChart\n       x-AxIs         \"Urgent(* +=[❤\"     -->        \"Not Urgent (* +=[❤\"\n    ";
    let diagram = parsed(body);
    assert_eq!(diagram.x_axis_left.as_deref(), Some("Urgent(* +=[❤"));
    assert_eq!(diagram.x_axis_right.as_deref(), Some("Not Urgent (* +=[❤"));

    let body = "quadrantChart\n       x-AxIs         \"Urgent(* +=[❤\"";
    let diagram = parsed(body);
    assert_eq!(diagram.x_axis_left.as_deref(), Some("Urgent(* +=[❤"));
    assert_eq!(diagram.x_axis_right, None);

    let body = "quadrantChart\n       x-AxIs         \"Urgent(* +=[❤\"  --> ";
    let diagram = parsed(body);
    assert_eq!(diagram.x_axis_left.as_deref(), Some("Urgent(* +=[❤ ⟶ "));
    assert_eq!(diagram.x_axis_right, None);
}

#[test]
fn parses_y_axis_text() {
    let body = "quadrantChart\ny-axis urgent --> not urgent";
    let diagram = parsed(body);
    assert_eq!(diagram.y_axis_bottom.as_deref(), Some("urgent"));
    assert_eq!(diagram.y_axis_top.as_deref(), Some("not urgent"));

    let body = "quadrantChart\n       y-AxIs         Urgent     -->        Not Urgent    \n";
    let diagram = parsed(body);
    assert_eq!(diagram.y_axis_bottom.as_deref(), Some("Urgent"));
    assert_eq!(diagram.y_axis_top.as_deref(), Some("Not Urgent    "));

    let body = "quadrantChart\n       Y-AxIs         \"Urgent(* +=[❤\"     -->        \"Not Urgent (* +=[❤\"\n    ";
    let diagram = parsed(body);
    assert_eq!(diagram.y_axis_bottom.as_deref(), Some("Urgent(* +=[❤"));
    assert_eq!(diagram.y_axis_top.as_deref(), Some("Not Urgent (* +=[❤"));

    let body = "quadrantChart\n       y-AxIs         \"Urgent(* +=[❤\"";
    let diagram = parsed(body);
    assert_eq!(diagram.y_axis_bottom.as_deref(), Some("Urgent(* +=[❤"));
    assert_eq!(diagram.y_axis_top, None);

    let body = "quadrantChart\n       y-AxIs         \"Urgent(* +=[❤\"  --> ";
    let diagram = parsed(body);
    assert_eq!(diagram.y_axis_bottom.as_deref(), Some("Urgent(* +=[❤ ⟶ "));
    assert_eq!(diagram.y_axis_top, None);
}

#[test]
fn parses_quadrant_1_text() {
    let diagram = parsed("quadrantChart\nquadrant-1 Plan");
    assert_eq!(diagram.quadrant_texts[0].as_deref(), Some("Plan"));

    let diagram = parsed("QuadRantChart   \n     QuaDrant-1 Plan    ");
    assert_eq!(diagram.quadrant_texts[0].as_deref(), Some("Plan    "));

    let diagram = parsed("QuadRantChart   \n     QuaDrant-1 \"Plan(* +=[❤\"");
    assert_eq!(diagram.quadrant_texts[0].as_deref(), Some("Plan(* +=[❤"));
}

#[test]
fn parses_quadrant_2_text() {
    let diagram = parsed("quadrantChart\nquadrant-2 do");
    assert_eq!(diagram.quadrant_texts[1].as_deref(), Some("do"));

    let diagram = parsed("QuadRantChart   \n     QuaDrant-2 Do    ");
    assert_eq!(diagram.quadrant_texts[1].as_deref(), Some("Do    "));

    let diagram = parsed("QuadRantChart   \n     QuaDrant-2 \"Do(* +=[❤\"");
    assert_eq!(diagram.quadrant_texts[1].as_deref(), Some("Do(* +=[❤"));
}

#[test]
fn parses_quadrant_3_text() {
    let diagram = parsed("quadrantChart\nquadrant-3 delegate");
    assert_eq!(diagram.quadrant_texts[2].as_deref(), Some("delegate"));

    let diagram = parsed("QuadRantChart   \n     QuaDrant-3 Delegate    ");
    assert_eq!(diagram.quadrant_texts[2].as_deref(), Some("Delegate    "));

    let diagram = parsed("QuadRantChart   \n     QuaDrant-3 \"Delegate(* +=[❤\"");
    assert_eq!(
        diagram.quadrant_texts[2].as_deref(),
        Some("Delegate(* +=[❤")
    );
}

#[test]
fn parses_quadrant_4_text() {
    let diagram = parsed("quadrantChart\nquadrant-4 delete");
    assert_eq!(diagram.quadrant_texts[3].as_deref(), Some("delete"));

    let diagram = parsed("QuadRantChart   \n     QuaDrant-4 Delete    ");
    assert_eq!(diagram.quadrant_texts[3].as_deref(), Some("Delete    "));

    let diagram = parsed("QuadRantChart   \n     QuaDrant-4 \"Delete(* +=[❤\"");
    assert_eq!(diagram.quadrant_texts[3].as_deref(), Some("Delete(* +=[❤"));
}

#[test]
fn parses_title() {
    let diagram = parsed("quadrantChart\ntitle this is title");
    assert_eq!(diagram.title.as_deref(), Some("this is title"));

    let diagram = parsed("QuadRantChart   \n     TiTle this Is title    ");
    assert_eq!(diagram.title.as_deref(), Some("this Is title"));

    let diagram = parsed("QuadRantChart   \n     title \"this is title (* +=[❤\"");
    assert_eq!(diagram.title.as_deref(), Some("\"this is title (* +=[❤\""));
}

#[test]
fn parses_points() {
    let diagram = parsed("quadrantChart\npoint1: [0.1, 0.4]");
    assert_eq!(
        points(&diagram),
        [&ParsedPoint {
            text: "point1".to_string(),
            class_name: String::new(),
            x: "0.1".to_string(),
            y: "0.4".to_string(),
            styles: Vec::new(),
        }]
    );

    let diagram = parsed("QuadRantChart   \n     Point1      : [0.1, 0.4]   ");
    assert_eq!(
        points(&diagram),
        [&ParsedPoint {
            text: "Point1".to_string(),
            class_name: String::new(),
            x: "0.1".to_string(),
            y: "0.4".to_string(),
            styles: Vec::new(),
        }]
    );

    let diagram = parsed("QuadRantChart   \n     \"Point1 : (* +=[❤\": [1, 0]   ");
    assert_eq!(
        points(&diagram),
        [&ParsedPoint {
            text: "Point1 : (* +=[❤".to_string(),
            class_name: String::new(),
            x: "1".to_string(),
            y: "0".to_string(),
            styles: Vec::new(),
        }]
    );

    // `[1.2, 0.4]`: the `point_x` rule only reads `1` or `0.<digits>`.
    let error = parse_error("QuadRantChart   \n     Point1 : [1.2, 0.4]   ");
    assert_eq!(error.message, "unexpected character '.'");
}

#[test]
fn parses_the_whole_chart() {
    let body = "quadrantChart
      title Analytics and Business Intelligence Platforms
      x-axis \"Completeness of Vision ❤\" --> \"x-axis-2\"
      y-axis Ability to Execute --> \"y-axis-2\"
      quadrant-1 Leaders
      quadrant-2 Challengers
      quadrant-3 Niche
      quadrant-4 Visionaries
      Microsoft: [0.75, 0.75]
      Salesforce: [0.55, 0.60]
      IBM: [0.51, 0.40]
      Incorta: [0.20, 0.30]";
    let diagram = parsed(body);
    assert_eq!(
        diagram.x_axis_left.as_deref(),
        Some("Completeness of Vision ❤")
    );
    assert_eq!(diagram.x_axis_right.as_deref(), Some("x-axis-2"));
    assert_eq!(diagram.y_axis_top.as_deref(), Some("y-axis-2"));
    assert_eq!(diagram.y_axis_bottom.as_deref(), Some("Ability to Execute"));
    assert_eq!(diagram.quadrant_texts[0].as_deref(), Some("Leaders"));
    assert_eq!(diagram.quadrant_texts[1].as_deref(), Some("Challengers"));
    assert_eq!(diagram.quadrant_texts[2].as_deref(), Some("Niche"));
    assert_eq!(diagram.quadrant_texts[3].as_deref(), Some("Visionaries"));
    assert_eq!(
        diagram.title.as_deref(),
        Some("Analytics and Business Intelligence Platforms")
    );
    assert_eq!(
        points(&diagram)
            .iter()
            .map(|point| (point.text.as_str(), point.x.as_str(), point.y.as_str()))
            .collect::<Vec<_>>(),
        [
            ("Microsoft", "0.75", "0.75"),
            ("Salesforce", "0.55", "0.60"),
            ("IBM", "0.51", "0.40"),
            ("Incorta", "0.20", "0.30"),
        ]
    );
}

#[test]
fn parses_the_whole_chart_with_point_styling() {
    let body = "quadrantChart
      title Analytics and Business Intelligence Platforms
      x-axis \"Completeness of Vision ❤\" --> \"x-axis-2\"
      y-axis Ability to Execute --> \"y-axis-2\"
      quadrant-1 Leaders
      quadrant-2 Challengers
      quadrant-3 Niche
      quadrant-4 Visionaries
      Microsoft: [0.75, 0.75] radius: 10
      Salesforce: [0.55, 0.60] radius: 10, color: #ff0000
      IBM: [0.51, 0.40] radius: 10, color: #ff0000, stroke-color: #ff00ff
      Incorta: [0.20, 0.30] radius: 10 ,color: #ff0000 ,stroke-color: #ff00ff ,stroke-width: 10px";
    let diagram = parsed(body);
    let points = points(&diagram);
    assert_eq!(
        points
            .iter()
            .map(|point| (point.text.as_str(), point.x.as_str(), point.y.as_str()))
            .collect::<Vec<_>>(),
        [
            ("Microsoft", "0.75", "0.75"),
            ("Salesforce", "0.55", "0.60"),
            ("IBM", "0.51", "0.40"),
            ("Incorta", "0.20", "0.30"),
        ]
    );
    assert_eq!(style_texts(points[0]), ["radius: 10"]);
    assert_eq!(style_texts(points[1]), ["radius: 10", "color: #ff0000"]);
    assert_eq!(
        style_texts(points[2]),
        ["radius: 10", "color: #ff0000", "stroke-color: #ff00ff"]
    );
    assert_eq!(
        style_texts(points[3]),
        [
            "radius: 10",
            "color: #ff0000",
            "stroke-color: #ff00ff",
            "stroke-width: 10px"
        ]
    );
}

#[test]
fn parses_the_whole_chart_with_point_styling_in_a_random_order_and_class_names() {
    let body = "quadrantChart
      title Analytics and Business Intelligence Platforms
      x-axis \"Completeness of Vision ❤\" --> \"x-axis-2\"
      y-axis Ability to Execute --> \"y-axis-2\"
      quadrant-1 Leaders
      quadrant-2 Challengers
      quadrant-3 Niche
      quadrant-4 Visionaries
      Microsoft: [0.75, 0.75] stroke-color: #ff00ff ,stroke-width: 10px, color: #ff0000, radius: 10
      Salesforce:::class1: [0.55, 0.60] radius: 10, color: #ff0000
      IBM: [0.51, 0.40] stroke-color: #ff00ff ,stroke-width: 10px
      Incorta: [0.20, 0.30] stroke-width: 10px";
    let diagram = parsed(body);
    let points = points(&diagram);
    assert_eq!(
        points
            .iter()
            .map(|point| (point.text.as_str(), point.x.as_str(), point.y.as_str()))
            .collect::<Vec<_>>(),
        [
            ("Microsoft", "0.75", "0.75"),
            ("Salesforce", "0.55", "0.60"),
            ("IBM", "0.51", "0.40"),
            ("Incorta", "0.20", "0.30"),
        ]
    );
    assert_eq!(
        style_texts(points[0]),
        [
            "stroke-color: #ff00ff",
            "stroke-width: 10px",
            "color: #ff0000",
            "radius: 10"
        ]
    );
    assert_eq!(points[1].class_name, "class1");
    assert_eq!(style_texts(points[1]), ["radius: 10", "color: #ff0000"]);
    assert_eq!(
        style_texts(points[2]),
        ["stroke-color: #ff00ff", "stroke-width: 10px"]
    );
    assert_eq!(style_texts(points[3]), ["stroke-width: 10px"]);
}

#[test]
fn handles_constructor_as_a_class_name() {
    let body = "quadrantChart
    classDef constructor fill:#ff0000
    Microsoft:::constructor: [0.75, 0.75]
    ";
    let diagram = parsed(body);
    let classes = classes(&diagram);
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].name, "constructor");
    assert_eq!(style_texts_of_class(classes[0]), ["fill:#ff0000"]);
    assert_eq!(points(&diagram)[0].class_name, "constructor");
}

mod unicode_support {
    use super::parsed;
    use super::points;

    #[test]
    fn parses_chinese_text_in_quadrant_labels() {
        let diagram = parsed("quadrantChart\nquadrant-1 需要扩展");
        assert_eq!(diagram.quadrant_texts[0].as_deref(), Some("需要扩展"));

        let diagram =
            parsed("quadrantChart\nquadrant-2 需要推广\nquadrant-3 重新评估\nquadrant-4 可以改进");
        assert_eq!(diagram.quadrant_texts[1].as_deref(), Some("需要推广"));
        assert_eq!(diagram.quadrant_texts[2].as_deref(), Some("重新评估"));
        assert_eq!(diagram.quadrant_texts[3].as_deref(), Some("可以改进"));
    }

    #[test]
    fn parses_chinese_text_in_x_axis() {
        let diagram = parsed("quadrantChart\nx-axis 低覆盖率 --> 高覆盖率");
        assert_eq!(diagram.x_axis_left.as_deref(), Some("低覆盖率"));
        assert_eq!(diagram.x_axis_right.as_deref(), Some("高覆盖率"));
    }

    #[test]
    fn parses_chinese_text_in_y_axis() {
        let diagram = parsed("quadrantChart\ny-axis 低参与度 --> 高参与度");
        assert_eq!(diagram.y_axis_bottom.as_deref(), Some("低参与度"));
        assert_eq!(diagram.y_axis_top.as_deref(), Some("高参与度"));
    }

    #[test]
    fn parses_chinese_point_names() {
        let diagram = parsed("quadrantChart\n产品A: [0.3, 0.6]");
        assert_eq!(points(&diagram)[0].text, "产品A");
        assert_eq!(points(&diagram)[0].x, "0.3");
        assert_eq!(points(&diagram)[0].y, "0.6");
    }

    #[test]
    fn parses_a_full_chart_with_chinese() {
        let body = "quadrantChart
title 分析象限图
x-axis 低覆盖率 --> 高覆盖率
y-axis 低参与度 --> 高参与度
quadrant-1 需要扩展
quadrant-2 需要推广
quadrant-3 重新评估
quadrant-4 可以改进
产品A: [0.3, 0.6]
产品B: [0.45, 0.23]";
        let diagram = parsed(body);
        assert_eq!(diagram.title.as_deref(), Some("分析象限图"));
        assert_eq!(diagram.x_axis_left.as_deref(), Some("低覆盖率"));
        assert_eq!(diagram.x_axis_right.as_deref(), Some("高覆盖率"));
        assert_eq!(diagram.y_axis_bottom.as_deref(), Some("低参与度"));
        assert_eq!(diagram.y_axis_top.as_deref(), Some("高参与度"));
        assert_eq!(diagram.quadrant_texts[0].as_deref(), Some("需要扩展"));
        let points = points(&diagram);
        assert_eq!(
            (points[0].text.as_str(), points[0].x.as_str()),
            ("产品A", "0.3")
        );
        assert_eq!(
            (points[1].text.as_str(), points[1].x.as_str()),
            ("产品B", "0.45")
        );
    }

    #[test]
    fn parses_japanese_text() {
        let diagram = parsed("quadrantChart\nquadrant-1 拡張が必要\nx-axis 低い --> 高い");
        assert_eq!(diagram.quadrant_texts[0].as_deref(), Some("拡張が必要"));
    }

    #[test]
    fn parses_korean_text() {
        let diagram = parsed("quadrantChart\nx-axis 낮음 --> 높음");
        assert_eq!(diagram.x_axis_left.as_deref(), Some("낮음"));
    }

    #[test]
    fn parses_emoji_in_text() {
        let diagram = parsed("quadrantChart\nquadrant-2 🚀Growth");
        assert_eq!(diagram.quadrant_texts[1].as_deref(), Some("🚀Growth"));
    }

    #[test]
    fn parses_latin_1_accented_text() {
        let diagram = parsed("quadrantChart\nx-axis Café --> Größe");
        assert_eq!(diagram.x_axis_left.as_deref(), Some("Café"));
        assert_eq!(diagram.x_axis_right.as_deref(), Some("Größe"));
    }

    #[test]
    fn parses_accented_characters_in_quadrant_labels() {
        let diagram = parsed("quadrantChart\nquadrant-1 catégoría\nquadrant-2 naïve");
        assert_eq!(diagram.quadrant_texts[0].as_deref(), Some("catégoría"));
        assert_eq!(diagram.quadrant_texts[1].as_deref(), Some("naïve"));
    }
}

// quadrantDb.spec.ts

#[test]
fn parses_styles_into_a_styles_object() {
    let styles = parse_styles(&[
        StyleText::new("radius: 10"),
        StyleText::new("color: #ff0000"),
        StyleText::new("stroke-color: #ff00ff"),
        StyleText::new("stroke-width: 10px"),
    ])
    .expect("the four supported styles");
    assert_eq!(
        styles,
        Styles {
            radius: Some(10.0),
            color: Some("#ff0000".to_string()),
            stroke_color: Some("#ff00ff".to_string()),
            stroke_width: Some("10px".to_string()),
        }
    );
}

#[test]
fn throws_for_a_non_supported_style_name() {
    let error =
        parse_styles(&[StyleText::new("test_name: value")]).expect_err("an unsupported style name");
    assert_eq!(error.message, "style named test_name is not supported.");
}

#[test]
fn returns_an_empty_styles_object_for_an_empty_input() {
    assert_eq!(parse_styles(&[]), Ok(Styles::default()));
}

#[test]
fn reads_only_the_first_two_parts_of_a_style() {
    let styles = parse_styles(&[StyleText::new("radius: 10:20")]).expect("a valid radius");
    assert_eq!(styles.radius, Some(10.0));

    let error = parse_styles(&[StyleText::new("color: #ff:00")]).expect_err("an invalid hex code");
    assert_eq!(
        error.message,
        "value for color #ff is invalid, please use a valid hex code"
    );
}

#[test]
fn throws_for_a_non_supported_style_value() {
    let error = parse_styles(&[StyleText::new("radius: f")]).expect_err("a non-numeric radius");
    assert_eq!(
        error.message,
        "value for radius f is invalid, please use a valid number"
    );

    let error = parse_styles(&[StyleText::new("color: ffaa")]).expect_err("a short hex code");
    assert_eq!(
        error.message,
        "value for color ffaa is invalid, please use a valid hex code"
    );

    let error =
        parse_styles(&[StyleText::new("stroke-color: #f677779")]).expect_err("a long hex code");
    assert_eq!(
        error.message,
        "value for stroke-color #f677779 is invalid, please use a valid hex code"
    );

    let error =
        parse_styles(&[StyleText::new("stroke-width: 30")]).expect_err("a width without px");
    assert_eq!(
        error.message,
        "value for stroke-width 30 is invalid, please use a valid number of pixels (eg. 10px)"
    );
}

// The database as `parse` applies it.

#[test]
fn trims_and_sanitizes_the_texts() {
    let ast = parse(
        "quadrantChart\ntitle  <script>x</script>Title  \nx-axis \"  left  \" --> right\nquadrant-1 \"<b>bold</b>\"\n\"<script>x</script>A\": [0.1, 0.2]",
    )
    .expect("a valid chart");
    assert_eq!(ast.diagram_title, "Title");
    assert_eq!(ast.x_axis_left_text, "left");
    assert_eq!(ast.x_axis_right_text, "right");
    assert_eq!(ast.quadrant1_text, "<b>bold</b>");
    assert_eq!(ast.points[0].text, "A");
}

#[test]
fn rejects_a_point_style_the_database_does_not_support() {
    let error = parse("quadrantChart\nA: [0.1, 0.2] fill: red").expect_err("an invalid style");
    assert_eq!(error.message, "style named fill is not supported.");
}

#[test]
fn rejects_a_class_def_style_the_database_does_not_support() {
    // Upstream's parser spec mocks `addClass`, so it never reaches `parseStyles`; with the real
    // database `fill` is rejected.
    let error = parse(
        "quadrantChart\nclassDef constructor fill:#ff0000\nMicrosoft:::constructor: [0.75, 0.75]",
    )
    .expect_err("an invalid class style");
    assert_eq!(error.message, "style named fill is not supported.");
}

#[test]
fn reports_the_position_of_a_style_error() {
    let error = parse("quadrantChart\nA: [0.1, 0.2] radius: nope").expect_err("an invalid radius");
    let span = error.span.expect("a position");
    assert_eq!((span.line, span.column), (2, 15));
}

#[test]
fn reports_an_earlier_style_error_before_a_later_parse_error() {
    // The database runs as the grammar parses, so the second line's style is rejected before
    // the third line is read.
    let error = parse("quadrantChart\nA: [0.1, 0.2] fill: red\nA: [1.2, 0.4]")
        .expect_err("an invalid style");
    assert_eq!(error.message, "style named fill is not supported.");
}

#[test]
fn keeps_point_values_and_radius_as_the_database_does() {
    let ast = parse("quadrantChart\nA: [0.1, 0.2] radius: 010").expect("a valid chart");
    assert_eq!(ast.points[0].x, "0.1");
    assert_eq!(ast.points[0].styles.radius, Some(10.0));
}

#[test]
fn prepends_points_like_add_points() {
    let ast = parse("quadrantChart\nA: [0.1, 0.2]\nB: [0.3, 0.4]").expect("a valid chart");
    assert_eq!(
        ast.points
            .iter()
            .map(|point| point.text.as_str())
            .collect::<Vec<_>>(),
        ["B", "A"]
    );
}

#[test]
fn keeps_a_redeclared_class_in_place_and_takes_the_last_styles() {
    let ast = parse(
        "quadrantChart\nclassDef c radius: 5\nclassDef d color: #fff\nclassDef c color: #000",
    )
    .expect("a valid chart");
    assert_eq!(
        ast.classes
            .iter()
            .map(|class| class.name.as_str())
            .collect::<Vec<_>>(),
        ["c", "d"]
    );
    assert_eq!(
        ast.class("c"),
        Some(&Styles {
            radius: None,
            color: Some("#000".to_string()),
            stroke_color: None,
            stroke_width: None,
        })
    );
}

#[test]
fn carries_the_accessibility_fields() {
    let ast = parse("quadrantChart\naccTitle:  my title  \naccDescr {\n  first\n    second\n}")
        .expect("a valid chart");
    assert_eq!(ast.acc_title, "my title");
    assert_eq!(ast.acc_description, "first\nsecond");
}

#[test]
fn accepts_comments_and_semicolon_separated_statements() {
    let ast = parse("%% a comment\nquadrantChart; title x\nx-axis a --> b; A: [0.1, 0.2]")
        .expect("a valid chart");
    assert_eq!(ast.diagram_title, "x");
    assert_eq!(ast.x_axis_left_text, "a");
    assert_eq!(ast.x_axis_right_text, "b");
    assert_eq!(ast.points[0].text, "A");
}

#[test]
fn takes_the_first_matching_lexer_rule() {
    // `title\b` loses to `[A-Za-z]+` only when a word character follows, so `titles` is an
    // `ALPHA`; `x-axis` has no word boundary and wins over `ALPHA`.
    assert!(
        parse_error("quadrantChart\ntitles: hi")
            .message
            .contains("expected point_start")
    );
    let diagram = parsed("quadrantChart\nx-axisXYZ");
    assert_eq!(diagram.x_axis_left.as_deref(), Some("XYZ"));
    let diagram = parsed("quadrantChart\nquadrant-1X");
    assert_eq!(diagram.quadrant_texts[0].as_deref(), Some("X"));
}

#[test]
fn a_newline_wins_over_a_point_start() {
    // The newline rule precedes `\s*:\s*\[\s*`, so the point cannot continue on the next line.
    assert!(parse("quadrantChart\nA\n: [0.1, 0.2]").is_err());
}

#[test]
fn an_inline_comment_swallows_the_character_before_it() {
    let ast = parse("quadrantChart\nA: [0.1, 0.2]x%% trailing").expect("a valid chart");
    assert_eq!(ast.points[0].text, "A");
}

#[test]
fn ends_only_where_the_initial_mode_is_active() {
    // A statement that leaves the lexer in a mode cannot use the end of input as its `eol`.
    assert!(parse("quadrantChart\ntitle").is_err());
    let ast = parse("quadrantChart\ntitle\n").expect("a title with its newline");
    assert_eq!(ast.diagram_title, "");
    assert!(parse("quadrantChart\nx-axis \"unterminated").is_err());
}
