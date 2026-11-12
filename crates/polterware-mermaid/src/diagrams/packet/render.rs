//! Source-to-[`Scene`] rendering for packet diagrams: the Rust side of
//! `packet/renderer.ts` `draw`.
//!
//! A packet diagram does not go through the unified layout pipeline: `draw` reads the rows from
//! [`PacketDB`](super::db), sizes the SVG, and appends plain SVG through d3. This module ports
//! that: [`draw`] parses, [`draw_ast`] renders an already parsed database, and [`render`] uses
//! the crate's monospace fallback measurement.
//!
//! The constants are `draw`'s: the view box is `bitWidth * bitsPerRow + 2` wide and
//! `(rowHeight + paddingY) * (rows + 1) - (title ? 0 : rowHeight)` high; a row's top is
//! `row * (rowHeight + paddingY) + paddingY`; a block starts at `column * bitWidth + 1` and is
//! `bits * bitWidth - paddingX` wide. `getConfig` adds 10 to `paddingY` when `showBits` is on,
//! which is how the bit numbers fit above the row. `ascending` fills a row from its lowest bit
//! left to right; `descending` mirrors it (`bitsPerRow - firstColumn - bits`), so the row reads
//! from its highest bit down to its lowest. A single-bit block centres its one number, a longer
//! block anchors its first bit at the left edge and its last at the right edge; a mirrored
//! block's left number is its `end` bit.
//!
//! Text is measured in the configured `themeVariables.fontFamily`, the family the SVG's root
//! style sets and the packet CSS inherits. None of the run positions depend on a measurement:
//! the renderer writes every `x` and `y` from the arithmetic above.
//!
//! Colors and sizes come from the packet stylesheet (`packet/styles.ts`): the
//! `themeVariables.packet` block when the theme sets one, otherwise its defaults (black text, a
//! `#efefef` fill, a black 1-unit stroke; `packetByte` 10px, `packetLabel` 12px, `packetTitle`
//! 14px). Where the port stops short of upstream: the class names (`packetBlock`,
//! `packetLabel`, `packetByte start`, ...) matter only to CSS, and a [`Scene`] carries paints
//! rather than selectors, so the stylesheet's declarations are resolved into the shapes. The
//! rows are one untransformed [`Group`] each, as `drawWord` appends one `<g>` per word.
//!
//! `bitsPerRow` positions the columns as upstream does; the parser still splits rows at
//! [`DEFAULT_BITS_PER_ROW`](super::DEFAULT_BITS_PER_ROW), so a configured `bitsPerRow` only
//! reaches the drawing here (see `mod.rs`).
//!
//! GPUI-kit has no counterpart: a packet diagram is not a control, so there is no state model
//! or keyboard behavior to port.

use serde_json::Value;

use crate::config::Config;
use crate::error::ParseError;
use crate::measure::{Font, TextMeasure, plain_text_block};
use crate::render::shapes::label_image_utils::parse_font_size;
use crate::scene::{
    Color, Group, PathStyle, Point, Rect, RectShape, Scene, TextAnchor, TextBaseline, TextRun,
};

use super::Ast;
use super::ast::Block;

/// `defaultPacketStyleOptions.byteFontSize`.
const DEFAULT_BYTE_FONT_SIZE: f64 = 10.0;
/// `defaultPacketStyleOptions.startByteColor`.
const DEFAULT_START_BYTE_COLOR: &str = "black";
/// `defaultPacketStyleOptions.endByteColor`.
const DEFAULT_END_BYTE_COLOR: &str = "black";
/// `defaultPacketStyleOptions.labelColor`.
const DEFAULT_LABEL_COLOR: &str = "black";
/// `defaultPacketStyleOptions.labelFontSize`.
const DEFAULT_LABEL_FONT_SIZE: f64 = 12.0;
/// `defaultPacketStyleOptions.titleColor`.
const DEFAULT_TITLE_COLOR: &str = "black";
/// `defaultPacketStyleOptions.titleFontSize`.
const DEFAULT_TITLE_FONT_SIZE: f64 = 14.0;
/// `defaultPacketStyleOptions.blockStrokeColor`.
const DEFAULT_BLOCK_STROKE_COLOR: &str = "black";
/// `defaultPacketStyleOptions.blockStrokeWidth`.
const DEFAULT_BLOCK_STROKE_WIDTH: f64 = 1.0;
/// `defaultPacketStyleOptions.blockFillColor`.
const DEFAULT_BLOCK_FILL_COLOR: &str = "#efefef";
/// `getConfig`'s extra row padding that makes room for the bit numbers.
const SHOW_BITS_PADDING: f64 = 10.0;

/// Renders a packet body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(body: &str, config: &Config, measure: &dyn TextMeasure) -> Result<Scene, ParseError> {
    let ast = super::parse(body)?;
    Ok(draw_ast(&ast, config, measure))
}

/// Renders an already parsed packet database.
pub fn draw_ast(ast: &Ast, config: &Config, measure: &dyn TextMeasure) -> Scene {
    let packet = &config.packet;
    let style = PacketStyle::from_config(config);

    // `getConfig` adds the bit-number padding after merging the configured values.
    let padding_y = packet.padding_y
        + if packet.show_bits {
            SHOW_BITS_PADDING
        } else {
            0.0
        };
    let row_height = packet.row_height;
    let total_row_height = row_height + padding_y;
    let title = ast.diagram_title.as_str();
    let has_title = !title.is_empty();
    let svg_height = total_row_height * (ast.packet.len() as f64 + 1.0)
        - if has_title { 0.0 } else { row_height };
    let svg_width = packet.bit_width * packet.bits_per_row + 2.0;

    let family = config
        .theme_variables
        .get("fontFamily")
        .and_then(Value::as_str)
        .unwrap_or("trebuchet ms");
    let byte_font = Font::new(family, style.byte_font_size);
    let label_font = Font::new(family, style.label_font_size);
    let title_font = Font::new(family, style.title_font_size);

    let mut scene = Scene::new(Rect::new(0.0, 0.0, svg_width, svg_height));
    for (row_number, word) in ast.packet.iter().enumerate() {
        let word_y = row_number as f64 * total_row_height + padding_y;
        let mut group = Group::new();
        for block in word {
            draw_block(
                &mut group,
                block,
                word_y,
                packet,
                &style,
                &byte_font,
                &label_font,
                measure,
            );
        }
        scene.push(group);
    }

    // `draw` appends the title text element even when there is no title; an empty run paints
    // nothing, so the scene leaves it out.
    if has_title {
        scene.push(text_run(
            title,
            Point::new(svg_width / 2.0, svg_height - total_row_height / 2.0),
            TextAnchor::Middle,
            TextBaseline::Middle,
            &title_font,
            &style.title_color,
            measure,
        ));
    }
    scene
}

/// Renders a packet body with the crate's fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, ParseError> {
    draw(body, config, &crate::measure::FallbackMeasure)
}

/// The packet stylesheet's resolved declarations.
struct PacketStyle {
    byte_font_size: f64,
    start_byte_color: Color,
    end_byte_color: Color,
    label_color: Color,
    label_font_size: f64,
    title_color: Color,
    title_font_size: f64,
    block_stroke_color: Color,
    block_stroke_width: f64,
    block_fill_color: Color,
}

impl PacketStyle {
    /// `cleanAndMerge(defaultPacketStyleOptions, themeVariables.packet)`: a value the theme
    /// sets wins, everything else keeps the stylesheet's default.
    fn from_config(config: &Config) -> Self {
        let packet = config.theme_variables.get("packet");
        Self {
            byte_font_size: style_size(packet, "byteFontSize").unwrap_or(DEFAULT_BYTE_FONT_SIZE),
            start_byte_color: style_color(packet, "startByteColor")
                .unwrap_or_else(|| Color::new(DEFAULT_START_BYTE_COLOR)),
            end_byte_color: style_color(packet, "endByteColor")
                .unwrap_or_else(|| Color::new(DEFAULT_END_BYTE_COLOR)),
            label_color: style_color(packet, "labelColor")
                .unwrap_or_else(|| Color::new(DEFAULT_LABEL_COLOR)),
            label_font_size: style_size(packet, "labelFontSize").unwrap_or(DEFAULT_LABEL_FONT_SIZE),
            title_color: style_color(packet, "titleColor")
                .unwrap_or_else(|| Color::new(DEFAULT_TITLE_COLOR)),
            title_font_size: style_size(packet, "titleFontSize").unwrap_or(DEFAULT_TITLE_FONT_SIZE),
            block_stroke_color: style_color(packet, "blockStrokeColor")
                .unwrap_or_else(|| Color::new(DEFAULT_BLOCK_STROKE_COLOR)),
            block_stroke_width: style_size(packet, "blockStrokeWidth")
                .unwrap_or(DEFAULT_BLOCK_STROKE_WIDTH),
            block_fill_color: style_color(packet, "blockFillColor")
                .unwrap_or_else(|| Color::new(DEFAULT_BLOCK_FILL_COLOR)),
        }
    }
}

/// One style value from the theme's `packet` block, as a CSS color string.
fn style_color(packet: Option<&Value>, key: &str) -> Option<Color> {
    let value = packet?.get(key)?;
    match value {
        Value::String(text) => Some(Color::new(text.clone())),
        _ => None,
    }
}

/// One style value from the theme's `packet` block, as a length in px.
fn style_size(packet: Option<&Value>, key: &str) -> Option<f64> {
    let value = packet?.get(key)?;
    match value {
        Value::String(text) => parse_font_size(text),
        Value::Number(number) => number.as_f64(),
        _ => None,
    }
}

/// `drawWord`'s body for one row: the block rectangles, their labels, and their bit numbers.
#[allow(clippy::too_many_arguments)]
fn draw_block(
    group: &mut Group,
    block: &Block,
    word_y: f64,
    packet: &crate::config::PacketConfig,
    style: &PacketStyle,
    byte_font: &Font,
    label_font: &Font,
    measure: &dyn TextMeasure,
) {
    let bits = (block.end - block.start + 1) as f64;
    let first_column = block.start as f64 % packet.bits_per_row;
    let column = if packet.bit_order == "descending" {
        packet.bits_per_row - first_column - bits
    } else {
        first_column
    };
    let block_x = column * packet.bit_width + 1.0;
    let width = bits * packet.bit_width - packet.padding_x;

    let block_style = PathStyle {
        fill: Some(style.block_fill_color.clone()),
        stroke: Some(style.block_stroke_color.clone()),
        stroke_width: style.block_stroke_width,
        ..PathStyle::default()
    };
    group.push(RectShape::new(
        Rect::new(block_x, word_y, width, packet.row_height),
        0.0,
        block_style,
    ));

    group.push(text_run(
        &block.label,
        Point::new(block_x + width / 2.0, word_y + packet.row_height / 2.0),
        TextAnchor::Middle,
        TextBaseline::Middle,
        label_font,
        &style.label_color,
        measure,
    ));

    if !packet.show_bits {
        return;
    }

    // A mirrored block runs from its highest bit on the left down to its lowest on the right.
    let (leading_bit, trailing_bit) = if packet.bit_order == "descending" {
        (block.end, block.start)
    } else {
        (block.start, block.end)
    };
    let single_block = bits == 1.0;
    let bit_number_y = word_y - 2.0;
    group.push(text_run(
        &leading_bit.to_string(),
        Point::new(
            block_x + if single_block { width / 2.0 } else { 0.0 },
            bit_number_y,
        ),
        if single_block {
            TextAnchor::Middle
        } else {
            TextAnchor::Start
        },
        TextBaseline::Alphabetic,
        byte_font,
        &style.start_byte_color,
        measure,
    ));
    if !single_block {
        group.push(text_run(
            &trailing_bit.to_string(),
            Point::new(block_x + width, bit_number_y),
            TextAnchor::End,
            TextBaseline::Alphabetic,
            byte_font,
            &style.end_byte_color,
            measure,
        ));
    }
}

/// A one-line text run at `position`, shaped and measured like a plain SVG `<text>`.
fn text_run(
    text: &str,
    position: Point,
    anchor: TextAnchor,
    baseline: TextBaseline,
    font: &Font,
    fill: &Color,
    measure: &dyn TextMeasure,
) -> TextRun {
    let block = plain_text_block(text, font, measure);
    let mut run = TextRun::new(position, font.clone(), block);
    run.anchor = anchor;
    run.baseline = baseline;
    run.fill = Some(fill.clone());
    run
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Item;

    const TOLERANCE: f64 = 0.01;

    fn fixtures() -> Vec<(String, Value)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/diagrams/packet/tests/render");
        let mut fixtures: Vec<(String, Value)> = std::fs::read_dir(&dir)
            .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
            .map(|entry| entry.expect("dir entry").path())
            .filter(|path| {
                path.extension().and_then(|extension| extension.to_str()) == Some("json")
            })
            .map(|path| {
                let text = std::fs::read_to_string(&path).expect("read fixture");
                let value: Value = serde_json::from_str(&text).expect("parse fixture");
                (
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or_default()
                        .to_string(),
                    value,
                )
            })
            .collect();
        fixtures.sort_by(|a, b| a.0.cmp(&b.0));
        fixtures
    }

    fn close(actual: f64, expected: f64, what: &str, file: &str) {
        assert!(
            (actual - expected).abs() <= TOLERANCE,
            "{file}: {what}: actual {actual} vs expected {expected}"
        );
    }

    fn text_of(run: &TextRun) -> String {
        run.lines
            .iter()
            .map(|line| line.text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn anchor_of(anchor: &str) -> TextAnchor {
        match anchor {
            "middle" => TextAnchor::Middle,
            "end" => TextAnchor::End,
            _ => TextAnchor::Start,
        }
    }

    fn baseline_of(baseline: &str) -> TextBaseline {
        match baseline {
            "middle" => TextBaseline::Middle,
            _ => TextBaseline::Alphabetic,
        }
    }

    fn check_text(actual: &TextRun, expected: &Value, what: &str, file: &str) {
        assert_eq!(
            text_of(actual),
            expected["text"].as_str().expect("text"),
            "{file}: {what} text"
        );
        close(
            actual.position.x,
            expected["x"].as_f64().expect("text x"),
            &format!("{what} x"),
            file,
        );
        close(
            actual.position.y,
            expected["y"].as_f64().expect("text y"),
            &format!("{what} y"),
            file,
        );
        assert_eq!(
            actual.anchor,
            anchor_of(expected["anchor"].as_str().unwrap_or("start")),
            "{file}: {what} anchor"
        );
        assert_eq!(
            actual.baseline,
            baseline_of(expected["baseline"].as_str().unwrap_or("auto")),
            "{file}: {what} baseline"
        );
    }

    #[test]
    fn renders_recorded_packet_fixtures() {
        let fixtures = fixtures();
        assert!(
            fixtures.len() >= 8,
            "expected at least 8 fixtures, found {}",
            fixtures.len()
        );

        for (file, fixture) in fixtures {
            let source = fixture["source"].as_str().expect("fixture source");
            let preprocessed = crate::preprocess::preprocess(source);
            let config = Config::from_source_config(&preprocessed.config)
                .unwrap_or_else(|error| panic!("{file}: config: {error}"));
            let ast = super::super::parse(&preprocessed.body)
                .unwrap_or_else(|error| panic!("{file}: parse: {error}"));
            let scene = draw_ast(&ast, &config, &crate::measure::FallbackMeasure);
            let expected = &fixture["expected"];

            let expected_view_box = &expected["viewBox"];
            close(
                scene.view_box.x,
                expected_view_box["x"].as_f64().expect("viewBox x"),
                "viewBox x",
                &file,
            );
            close(
                scene.view_box.y,
                expected_view_box["y"].as_f64().expect("viewBox y"),
                "viewBox y",
                &file,
            );
            close(
                scene.view_box.width,
                expected_view_box["width"].as_f64().expect("viewBox width"),
                "viewBox width",
                &file,
            );
            close(
                scene.view_box.height,
                expected_view_box["height"]
                    .as_f64()
                    .expect("viewBox height"),
                "viewBox height",
                &file,
            );

            let expected_rows = expected["rows"].as_array().expect("rows");
            let actual_rows: Vec<&Group> = scene
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Group(group) => Some(group),
                    _ => None,
                })
                .collect();
            assert_eq!(actual_rows.len(), expected_rows.len(), "{file}: row count");

            for (row_index, (actual_row, expected_row)) in
                actual_rows.iter().zip(expected_rows).enumerate()
            {
                let expected_blocks = expected_row["blocks"].as_array().expect("blocks");
                let actual_blocks: Vec<&RectShape> = actual_row
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        Item::Rect(rect) => Some(rect),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    actual_blocks.len(),
                    expected_blocks.len(),
                    "{file}: row {row_index} block count"
                );

                let expected_text_count = expected_blocks
                    .iter()
                    .map(|block| {
                        ["label", "startBit", "endBit"]
                            .iter()
                            .filter(|key| !block[*key].is_null())
                            .count()
                    })
                    .sum::<usize>();
                let actual_text_count = actual_row
                    .items
                    .iter()
                    .filter(|item| matches!(item, Item::Text(_)))
                    .count();
                assert_eq!(
                    actual_text_count, expected_text_count,
                    "{file}: row {row_index} text count"
                );

                let mut text_index = 0;
                for (block_index, (actual_block, expected_block)) in
                    actual_blocks.iter().zip(expected_blocks).enumerate()
                {
                    let what = format!("row {row_index} block {block_index}");
                    close(
                        actual_block.rect.x,
                        expected_block["x"].as_f64().expect("block x"),
                        &format!("{what} x"),
                        &file,
                    );
                    close(
                        actual_block.rect.y,
                        expected_block["y"].as_f64().expect("block y"),
                        &format!("{what} y"),
                        &file,
                    );
                    close(
                        actual_block.rect.width,
                        expected_block["width"].as_f64().expect("block width"),
                        &format!("{what} width"),
                        &file,
                    );
                    close(
                        actual_block.rect.height,
                        expected_block["height"].as_f64().expect("block height"),
                        &format!("{what} height"),
                        &file,
                    );

                    for key in ["label", "startBit", "endBit"] {
                        let expected_text = &expected_block[key];
                        if expected_text.is_null() {
                            continue;
                        }
                        let actual_text = actual_row
                            .items
                            .iter()
                            .filter_map(|item| match item {
                                Item::Text(text) => Some(text),
                                _ => None,
                            })
                            .nth(text_index)
                            .unwrap_or_else(|| panic!("{file}: {what} missing {key}"));
                        // The text elements follow their block in draw order.
                        text_index += 1;
                        check_text(actual_text, expected_text, &format!("{what} {key}"), &file);
                    }
                }
            }

            let expected_title = &expected["title"];
            let title = scene.items.iter().find_map(|item| match item {
                Item::Text(text) => Some(text),
                _ => None,
            });
            let expected_title_text = expected_title["text"].as_str().unwrap_or_default();
            if expected_title.is_null() || expected_title_text.is_empty() {
                assert!(title.is_none(), "{file}: unexpected title text");
            } else {
                let title = title.unwrap_or_else(|| panic!("{file}: missing title text"));
                check_text(title, expected_title, "title", &file);
            }
        }
    }

    #[test]
    fn lays_out_a_basic_packet() {
        let scene =
            render("packet\n  0-7: \"flags\"\n", &Config::default()).expect("render packet");
        assert!(scene.view_box.width > 0.0);
        assert!(scene.view_box.height > 0.0);
        assert!(!scene.items.is_empty());
    }

    #[test]
    fn styles_every_shape_and_text_from_the_stylesheet_defaults() {
        let scene = render(
            "packet\n  title Header\n  0-7: \"flags\"\n  8: \"bit\"\n",
            &Config::default(),
        )
        .expect("render packet");

        let groups: Vec<&Group> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Group(group) => Some(group),
                _ => None,
            })
            .collect();
        assert_eq!(groups.len(), 1);
        let group = groups[0];

        let rects: Vec<&RectShape> = group
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Rect(rect) => Some(rect),
                _ => None,
            })
            .collect();
        assert_eq!(rects.len(), 2);
        for rect in &rects {
            assert_eq!(
                rect.style.fill.as_ref().map(|color| color.0.clone()),
                Some(DEFAULT_BLOCK_FILL_COLOR.to_string())
            );
            assert_eq!(
                rect.style.stroke.as_ref().map(|color| color.0.clone()),
                Some(DEFAULT_BLOCK_STROKE_COLOR.to_string())
            );
            assert_eq!(rect.style.stroke_width, DEFAULT_BLOCK_STROKE_WIDTH);
            assert_eq!(rect.rect.height, 32.0);
        }

        let texts: Vec<&TextRun> = group
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Text(text) => Some(text),
                _ => None,
            })
            .collect();
        let labels: Vec<&&TextRun> = texts
            .iter()
            .filter(|text| text.font.size == DEFAULT_LABEL_FONT_SIZE)
            .collect();
        assert_eq!(labels.len(), 2);
        for label in &labels {
            assert_eq!(
                label.fill.as_ref().map(|color| color.0.clone()),
                Some(DEFAULT_LABEL_COLOR.to_string())
            );
        }
        let numbers: Vec<&&TextRun> = texts
            .iter()
            .filter(|text| text.font.size == DEFAULT_BYTE_FONT_SIZE)
            .collect();
        assert_eq!(numbers.len(), 3);
        for number in &numbers {
            assert_eq!(
                number.fill.as_ref().map(|color| color.0.clone()),
                Some(DEFAULT_START_BYTE_COLOR.to_string())
            );
        }

        let title = scene
            .items
            .iter()
            .find_map(|item| match item {
                Item::Text(text) => Some(text),
                _ => None,
            })
            .expect("title");
        assert_eq!(text_of(title), "Header");
        assert_eq!(title.font.size, DEFAULT_TITLE_FONT_SIZE);
        assert_eq!(
            title.fill.as_ref().map(|color| color.0.clone()),
            Some(DEFAULT_TITLE_COLOR.to_string())
        );
    }

    #[test]
    fn reads_the_packet_colors_a_theme_sets() {
        let theme = crate::theme::Theme::Dark;
        let mut config = Config::default();
        config.theme = theme.name().to_string();
        config.theme_variables = theme
            .variables(&Default::default())
            .expect("theme variables")
            .into_map();
        let packet = config.theme_variables["packet"]
            .as_object()
            .expect("the dark theme sets packet colors")
            .clone();
        let theme_value = |key: &str| {
            packet
                .get(key)
                .and_then(Value::as_str)
                .expect("packet color")
                .to_string()
        };

        let scene =
            render("packet\n  title Header\n  0-7: \"flags\"\n", &config).expect("render packet");
        let group = scene
            .items
            .iter()
            .find_map(|item| match item {
                Item::Group(group) => Some(group),
                _ => None,
            })
            .expect("row group");

        let rect = group
            .items
            .iter()
            .find_map(|item| match item {
                Item::Rect(rect) => Some(rect),
                _ => None,
            })
            .expect("block");
        assert_eq!(
            rect.style.fill.as_ref().map(|color| color.0.clone()),
            Some(theme_value("blockFillColor"))
        );
        assert_eq!(
            rect.style.stroke.as_ref().map(|color| color.0.clone()),
            Some(theme_value("blockStrokeColor"))
        );

        let label = group
            .items
            .iter()
            .find_map(|item| match item {
                Item::Text(text) if text.font.size == DEFAULT_LABEL_FONT_SIZE => Some(text),
                _ => None,
            })
            .expect("block label");
        assert_eq!(
            label.fill.as_ref().map(|color| color.0.clone()),
            Some(theme_value("labelColor"))
        );

        let title = scene
            .items
            .iter()
            .find_map(|item| match item {
                Item::Text(text) => Some(text),
                _ => None,
            })
            .expect("title");
        assert_eq!(
            title.fill.as_ref().map(|color| color.0.clone()),
            Some(theme_value("titleColor"))
        );
    }

    #[test]
    fn rejects_a_body_without_a_packet_header() {
        let error = render("0-7: \"x\"\n", &Config::default()).unwrap_err();
        assert!(
            error.message.contains("packet or packet-beta"),
            "unexpected message: {}",
            error.message
        );
    }
}
