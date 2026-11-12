//! Source-to-[`Scene`] rendering for the error diagram: the Rust side of
//! `error/errorRenderer.ts` `draw`.
//!
//! Upstream appends one `<g>` to the SVG and, inside it, in order: six `path.error-icon`
//! shapes that make the bomb icon, a `text.error-text` headline (`Syntax error in text`, 150px,
//! centred at x=1440, baseline y=250), a second line with the Mermaid version (100px, centred at
//! x=1250, y=400), and one 42px line per wrapped piece of the error message (centred at x=1440,
//! first baseline y=510, then 56px apart). The viewBox is always `0 0 2412 h`: `h` is 512 with
//! no message and `500 + lines * 56` with one. `configureSvgSize` additionally sets the SVG
//! `width` to `100%` and `max-width` to `h` px; a [`Scene`] carries the view box, not the CSS
//! viewport attributes. [`draw`] ports the drawing; [`render`] uses the crate's fallback
//! measurement.
//!
//! The bomb's `d` strings are copied verbatim from `errorRenderer.ts` and parsed by
//! [`parse_svg_path`]: the source strings stay readable as upstream's, and the scene stores the
//! absolute commands a painter draws. [`wrap_error_message`] ports `wrapErrorMessage` exactly,
//! including its code-point (not UTF-16) wrapping and the four-line cap ending in `...`.
//!
//! Colors are the theme's `errorBkgColor` for the icon fill and `errorTextColor` for the text
//! fill, the declarations `styles.ts` builds for `.error-icon` and `.error-text`. CSS also
//! sets `stroke: errorTextColor` on the text; a [`TextRun`] has no stroke, so the scene carries
//! the fill only, and a painter draws the glyphs without the hairline stroke a browser adds in
//! the same color (about a one-unit difference on a 150px headline).
//!
//! Shapes in the scene are styled rather than classed, as a [`Scene`] has no selectors; the
//! fixture records each path's `error-icon` class. Text is measured in the configured
//! `fontFamily`, the family the SVG's `#id` style inherits; the fixture renders with
//! `monospace`, the shared measurement rule, and the tests configure that family so each run's
//! box can be compared with the harness's `getBBox`. A painter's own face may lay the glyphs
//! out differently, but every position here is the renderer's `x`/`y` attribute, not a
//! measured one.
//!
//! gpui-kit has no error-diagram counterpart; the behavior reference is upstream's own
//! `errorRenderer.spec.ts`, whose wrapping and height cases are ported below.

use crate::common::is_js_whitespace;
use crate::config::Config;
use crate::measure::{FallbackMeasure, Font, TextMeasure, plain_text_block};
use crate::scene::{
    Color, Group, NORMAL_WEIGHT, Path, PathCommand, PathShape, PathStyle, Point, Rect, Scene,
    TextAnchor, TextRun,
};

/// `wrapErrorMessage`'s `MAX_LINES`: a fifth line is dropped for an ellipsis.
pub const MAX_LINES: usize = 4;
/// `wrapErrorMessage`'s `MAX_LINE_LENGTH`, in code points.
pub const MAX_LINE_LENGTH: usize = 75;

/// `configureSvgSize`'s width, and the viewBox's width.
pub const VIEWPORT_WIDTH: f64 = 2412.0;
/// `draw`'s height without a message (and the SVG height in that case).
pub const BASE_HEIGHT: f64 = 512.0;
/// `draw`'s height with a message before its lines are counted.
pub const MESSAGE_BASE_HEIGHT: f64 = 500.0;
/// The baseline advance of each message line.
pub const MESSAGE_LINE_HEIGHT: f64 = 56.0;

/// The headline's baseline `y`.
pub const HEADLINE_Y: f64 = 250.0;
/// The headline's `font-size`.
pub const HEADLINE_FONT_SIZE: f64 = 150.0;
/// The headline's `x`, with `text-anchor: middle`.
pub const HEADLINE_X: f64 = 1440.0;

/// The version line's baseline `y`.
pub const VERSION_Y: f64 = 400.0;
/// The version line's `font-size`.
pub const VERSION_FONT_SIZE: f64 = 100.0;
/// The version line's `x`, with `text-anchor: middle`.
pub const VERSION_X: f64 = 1250.0;

/// The first message line's baseline `y`.
pub const MESSAGE_Y: f64 = 510.0;
/// A message line's `font-size`.
pub const MESSAGE_FONT_SIZE: f64 = 42.0;
/// A message line's `x`, with `text-anchor: middle`.
pub const MESSAGE_X: f64 = 1440.0;

/// The headline `draw` writes above the version.
pub const HEADLINE: &str = "Syntax error in text";

/// The Mermaid version this crate ports. `draw` writes `mermaid version {version}` from its
/// third argument, which `mermaidAPI.render` supplies as the build's version.
pub const VERSION: &str = "12.3.0";

/// The six `path.error-icon` `d` strings, in the order `errorRenderer.ts` appends them.
const ERROR_ICON_PATHS: [&str; 6] = [
    "m411.313,123.313c6.25-6.25 6.25-16.375 0-22.625s-16.375-6.25-22.625,0l-32,32-9.375,9.375-20.688-20.688c-12.484-12.5-32.766-12.5-45.25,0l-16,16c-1.261,1.261-2.304,2.648-3.31,4.051-21.739-8.561-45.324-13.426-70.065-13.426-105.867,0-192,86.133-192,192s86.133,192 192,192 192-86.133 192-192c0-24.741-4.864-48.327-13.426-70.065 1.402-1.007 2.79-2.049 4.051-3.31l16-16c12.5-12.492 12.5-32.758 0-45.25l-20.688-20.688 9.375-9.375 32.001-31.999zm-219.313,100.687c-52.938,0-96,43.063-96,96 0,8.836-7.164,16-16,16s-16-7.164-16-16c0-70.578 57.422-128 128-128 8.836,0 16,7.164 16,16s-7.164,16-16,16z",
    "m459.02,148.98c-6.25-6.25-16.375-6.25-22.625,0s-6.25,16.375 0,22.625l16,16c3.125,3.125 7.219,4.688 11.313,4.688 4.094,0 8.188-1.563 11.313-4.688 6.25-6.25 6.25-16.375 0-22.625l-16.001-16z",
    "m340.395,75.605c3.125,3.125 7.219,4.688 11.313,4.688 4.094,0 8.188-1.563 11.313-4.688 6.25-6.25 6.25-16.375 0-22.625l-16-16c-6.25-6.25-16.375-6.25-22.625,0s-6.25,16.375 0,22.625l15.999,16z",
    "m400,64c8.844,0 16-7.164 16-16v-32c0-8.836-7.156-16-16-16-8.844,0-16,7.164-16,16v32c0,8.836 7.156,16 16,16z",
    "m496,96.586h-32c-8.844,0-16,7.164-16,16 0,8.836 7.156,16 16,16h32c8.844,0 16-7.164 16-16 0-8.836-7.156-16-16-16z",
    "m436.98,75.605c3.125,3.125 7.219,4.688 11.313,4.688 4.094,0 8.188-1.563 11.313-4.688l32-32c6.25-6.25 6.25-16.375 0-22.625s-16.375-6.25-22.625,0l-32,32c-6.251,6.25-6.251,16.375-0.001,22.625z",
];

/// Wraps an error message into centred lines, the port of `wrapErrorMessage`.
///
/// Whitespace is normalized, words longer than a line are hard-wrapped by code point, and a
/// message with more than [`MAX_LINES`] lines is capped, its last line ending in `...`.
/// An empty (or whitespace-only) message has no lines.
pub fn wrap_error_message(message: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for token in split_on_js_whitespace(message) {
        let mut rest: Vec<char> = token.chars().collect();
        while rest.len() > MAX_LINE_LENGTH {
            words.push(rest.drain(..MAX_LINE_LENGTH).collect());
        }
        if !rest.is_empty() {
            words.push(rest.into_iter().collect());
        }
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in words {
        let candidate_length = if current.is_empty() {
            word.chars().count()
        } else {
            current.chars().count() + 1 + word.chars().count()
        };
        if candidate_length > MAX_LINE_LENGTH {
            lines.push(std::mem::take(&mut current));
            current = word;
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(&word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }

    if lines.len() > MAX_LINES {
        lines.truncate(MAX_LINES);
        let last = lines[MAX_LINES - 1]
            .chars()
            .take(MAX_LINE_LENGTH - 3)
            .collect::<String>();
        lines[MAX_LINES - 1] = format!("{last}...");
    }
    lines
}

/// `message.split(/\s+/)`, without JavaScript's empty leading and trailing tokens (the renderer
/// skips them anyway): the runs between JavaScript whitespace characters.
fn split_on_js_whitespace(message: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (index, character) in message.char_indices() {
        if is_js_whitespace(character) {
            if let Some(from) = start.take() {
                tokens.push(&message[from..index]);
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(from) = start {
        tokens.push(&message[from..]);
    }
    tokens
}

/// Renders an error message to a [`Scene`], with the measurement `draw` uses.
pub fn draw(message: &str, config: &Config, measure: &dyn TextMeasure) -> Scene {
    let lines = wrap_error_message(message);
    let height = if lines.is_empty() {
        BASE_HEIGHT
    } else {
        MESSAGE_BASE_HEIGHT + lines.len() as f64 * MESSAGE_LINE_HEIGHT
    };

    let theme = &config.theme_variables;
    let family = theme
        .get("fontFamily")
        .and_then(|value| value.as_str())
        .unwrap_or("trebuchet ms")
        .to_string();
    let icon_color = theme_string(theme, "errorBkgColor").map(Color::new);
    let text_color = theme_string(theme, "errorTextColor").map(Color::new);

    let mut group = Group::new();

    let icon_style = PathStyle {
        fill: icon_color,
        stroke: None,
        ..PathStyle::default()
    };
    for d in ERROR_ICON_PATHS {
        group.push(PathShape::new(parse_svg_path(d), icon_style.clone()));
    }

    group.push(text_run(
        HEADLINE,
        Point::new(HEADLINE_X, HEADLINE_Y),
        HEADLINE_FONT_SIZE,
        &family,
        &text_color,
        measure,
    ));
    group.push(text_run(
        &format!("mermaid version {VERSION}"),
        Point::new(VERSION_X, VERSION_Y),
        VERSION_FONT_SIZE,
        &family,
        &text_color,
        measure,
    ));
    for (index, line) in lines.iter().enumerate() {
        group.push(text_run(
            line,
            Point::new(MESSAGE_X, MESSAGE_Y + index as f64 * MESSAGE_LINE_HEIGHT),
            MESSAGE_FONT_SIZE,
            &family,
            &text_color,
            measure,
        ));
    }

    let mut scene = Scene::new(Rect::new(0.0, 0.0, VIEWPORT_WIDTH, height));
    scene.push(group);
    scene
}

/// Renders an error message with the crate's monospace fallback measurement.
pub fn render(message: &str, config: &Config) -> Scene {
    draw(message, config, &FallbackMeasure)
}

/// A theme variable that is a string.
fn theme_string<'a>(
    theme: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    theme.get(key).and_then(|value| value.as_str())
}

/// One centred `text.error-text` line at `position` (its alphabetic baseline).
fn text_run(
    text: &str,
    position: Point,
    font_size: f64,
    family: &str,
    fill: &Option<Color>,
    measure: &dyn TextMeasure,
) -> TextRun {
    let font = Font {
        family: family.to_string(),
        size: font_size,
        weight: NORMAL_WEIGHT,
        italic: false,
    };
    let block = plain_text_block(text, &font, measure);
    let mut run = TextRun::new(position, font, block);
    run.anchor = TextAnchor::Middle;
    run.fill = fill.clone();
    run
}

/// Parses the SVG path grammar the error icon's `d` strings use: `M`/`m`, `L`/`l`, `H`/`h`,
/// `V`/`v`, `C`/`c`, `S`/`s`, and `Z`/`z`, with implicit repeated coordinate sets (a second
/// pair after `M` is a line) and both comma and whitespace separators. Relative commands are
/// resolved against the current point, so the commands are absolute, as a [`Scene`] stores
/// them. The icon paths contain no quads or arcs; those commands stop the parse.
pub fn parse_svg_path(d: &str) -> Path {
    let chars: Vec<char> = d.chars().collect();
    let mut index = 0;
    let mut path = Path::new();
    let mut point = Point::ZERO;
    let mut start = Point::ZERO;
    let mut last_cubic_control: Option<Point> = None;
    let mut operation: Option<char> = None;

    while index < chars.len() {
        skip_path_separators(&chars, &mut index);
        if index >= chars.len() {
            break;
        }
        if chars[index].is_ascii_alphabetic() {
            operation = Some(chars[index]);
            index += 1;
        }
        let Some(current) = operation else {
            break;
        };
        let command = current.to_ascii_uppercase();
        let relative = current.is_ascii_lowercase();
        let offset = if relative { point } else { Point::ZERO };
        match command {
            'M' => {
                point = Point::new(
                    next_path_number(&chars, &mut index) + offset.x,
                    next_path_number(&chars, &mut index) + offset.y,
                );
                start = point;
                path.commands.push(PathCommand::move_to(point));
                last_cubic_control = None;
                // Further coordinate pairs after `M` are implicit `L` commands.
                operation = Some(if relative { 'l' } else { 'L' });
            }
            'L' => {
                point = Point::new(
                    next_path_number(&chars, &mut index) + offset.x,
                    next_path_number(&chars, &mut index) + offset.y,
                );
                path.commands.push(PathCommand::line_to(point));
                last_cubic_control = None;
            }
            'H' => {
                point.x = next_path_number(&chars, &mut index) + offset.x;
                path.commands.push(PathCommand::line_to(point));
                last_cubic_control = None;
            }
            'V' => {
                point.y = next_path_number(&chars, &mut index) + offset.y;
                path.commands.push(PathCommand::line_to(point));
                last_cubic_control = None;
            }
            'C' => {
                let control1 = Point::new(
                    next_path_number(&chars, &mut index) + offset.x,
                    next_path_number(&chars, &mut index) + offset.y,
                );
                let control2 = Point::new(
                    next_path_number(&chars, &mut index) + offset.x,
                    next_path_number(&chars, &mut index) + offset.y,
                );
                point = Point::new(
                    next_path_number(&chars, &mut index) + offset.x,
                    next_path_number(&chars, &mut index) + offset.y,
                );
                path.commands
                    .push(PathCommand::cubic_to(control1, control2, point));
                last_cubic_control = Some(control2);
            }
            'S' => {
                let control1 = last_cubic_control
                    .map(|control| Point::new(2.0 * point.x - control.x, 2.0 * point.y - control.y))
                    .unwrap_or(point);
                let control2 = Point::new(
                    next_path_number(&chars, &mut index) + offset.x,
                    next_path_number(&chars, &mut index) + offset.y,
                );
                point = Point::new(
                    next_path_number(&chars, &mut index) + offset.x,
                    next_path_number(&chars, &mut index) + offset.y,
                );
                path.commands
                    .push(PathCommand::cubic_to(control1, control2, point));
                last_cubic_control = Some(control2);
            }
            'Z' => {
                point = start;
                path.commands.push(PathCommand::Close);
                last_cubic_control = None;
                operation = None;
            }
            _ => break,
        }
    }
    path
}

/// Skips the whitespace and commas between path tokens.
fn skip_path_separators(chars: &[char], index: &mut usize) {
    while *index < chars.len() && (chars[*index].is_ascii_whitespace() || chars[*index] == ',') {
        *index += 1;
    }
}

/// Reads one SVG number, which may start with a sign and repeat a sign only in its exponent
/// (`6.25-16.375` is two numbers).
fn next_path_number(chars: &[char], index: &mut usize) -> f64 {
    skip_path_separators(chars, index);
    let start = *index;
    if *index < chars.len() && (chars[*index] == '+' || chars[*index] == '-') {
        *index += 1;
    }
    while *index < chars.len() && chars[*index].is_ascii_digit() {
        *index += 1;
    }
    if *index < chars.len() && chars[*index] == '.' {
        *index += 1;
        while *index < chars.len() && chars[*index].is_ascii_digit() {
            *index += 1;
        }
    }
    if *index < chars.len() && (chars[*index] == 'e' || chars[*index] == 'E') {
        *index += 1;
        if *index < chars.len() && (chars[*index] == '+' || chars[*index] == '-') {
            *index += 1;
        }
        while *index < chars.len() && chars[*index].is_ascii_digit() {
            *index += 1;
        }
    }
    chars[start..*index]
        .iter()
        .collect::<String>()
        .parse()
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const TOLERANCE: f64 = 0.01;

    fn close(actual: f64, expected: f64, what: &str, file: &str) {
        assert!(
            (actual - expected).abs() <= TOLERANCE,
            "{file}: {what}: actual {actual} vs expected {expected}"
        );
    }

    fn fixtures() -> Vec<(String, Value)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/diagrams/error/tests/render");
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

    /// The harness renders with `fontFamily: 'monospace'`, the shared measurement rule.
    fn fixture_config() -> Config {
        let mut config = Config::default();
        config.theme_variables.insert(
            "fontFamily".to_string(),
            Value::String("monospace".to_string()),
        );
        config
    }

    fn items(scene: &Scene) -> Vec<&crate::scene::Item> {
        let mut items = Vec::new();
        for item in &scene.items {
            match item {
                crate::scene::Item::Group(group) => items.extend(group.items.iter()),
                other => items.push(other),
            }
        }
        items
    }

    /// Asserts a fixture command equals a scene command within [`TOLERANCE`].
    fn assert_command_close(actual: &PathCommand, expected: &Value, file: &str, index: usize) {
        let expected_number = |key: &str| {
            expected[key]
                .as_f64()
                .unwrap_or_else(|| panic!("{file}: command {index} missing {key}: {expected}"))
        };
        let what = |name: &str| format!("command {index} ({name})");
        match actual {
            PathCommand::MoveTo { x, y } => {
                assert_eq!(expected["op"], "moveTo", "{file}: command {index}");
                close(*x, expected_number("x"), &what("moveTo x"), file);
                close(*y, expected_number("y"), &what("moveTo y"), file);
            }
            PathCommand::LineTo { x, y } => {
                assert_eq!(expected["op"], "lineTo", "{file}: command {index}");
                close(*x, expected_number("x"), &what("lineTo x"), file);
                close(*y, expected_number("y"), &what("lineTo y"), file);
            }
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                assert_eq!(expected["op"], "cubicTo", "{file}: command {index}");
                close(*c1x, expected_number("c1x"), &what("cubicTo c1x"), file);
                close(*c1y, expected_number("c1y"), &what("cubicTo c1y"), file);
                close(*c2x, expected_number("c2x"), &what("cubicTo c2x"), file);
                close(*c2y, expected_number("c2y"), &what("cubicTo c2y"), file);
                close(*x, expected_number("x"), &what("cubicTo x"), file);
                close(*y, expected_number("y"), &what("cubicTo y"), file);
            }
            PathCommand::QuadTo { cx, cy, x, y } => {
                assert_eq!(expected["op"], "quadTo", "{file}: command {index}");
                close(*cx, expected_number("cx"), &what("quadTo cx"), file);
                close(*cy, expected_number("cy"), &what("quadTo cy"), file);
                close(*x, expected_number("x"), &what("quadTo x"), file);
                close(*y, expected_number("y"), &what("quadTo y"), file);
            }
            PathCommand::ArcTo {
                rx,
                ry,
                rotation,
                large_arc,
                sweep,
                x,
                y,
            } => {
                assert_eq!(expected["op"], "arcTo", "{file}: command {index}");
                close(*rx, expected_number("rx"), &what("arcTo rx"), file);
                close(*ry, expected_number("ry"), &what("arcTo ry"), file);
                close(
                    *rotation,
                    expected_number("rotation"),
                    &what("arcTo rotation"),
                    file,
                );
                assert_eq!(
                    *large_arc,
                    expected["large_arc"].as_bool().expect("arc large_arc"),
                    "{file}: command {index} large_arc"
                );
                assert_eq!(
                    *sweep,
                    expected["sweep"].as_bool().expect("arc sweep"),
                    "{file}: command {index} sweep"
                );
                close(*x, expected_number("x"), &what("arcTo x"), file);
                close(*y, expected_number("y"), &what("arcTo y"), file);
            }
            PathCommand::Close => {
                assert_eq!(expected["op"], "close", "{file}: command {index}");
            }
        }
    }

    #[test]
    fn renders_recorded_error_fixtures() {
        let fixtures = fixtures();
        assert!(!fixtures.is_empty(), "expected error fixtures");

        for (file, fixture) in fixtures {
            let message = fixture["message"].as_str().expect("fixture message");
            let scene = render(message, &fixture_config());
            let expected = &fixture["expected"];

            let view_box = &expected["viewBox"];
            close(
                scene.view_box.x,
                view_box["x"].as_f64().expect("viewBox x"),
                "viewBox x",
                &file,
            );
            close(
                scene.view_box.y,
                view_box["y"].as_f64().expect("viewBox y"),
                "viewBox y",
                &file,
            );
            close(
                scene.view_box.width,
                view_box["width"].as_f64().expect("viewBox width"),
                "viewBox width",
                &file,
            );
            close(
                scene.view_box.height,
                view_box["height"].as_f64().expect("viewBox height"),
                "viewBox height",
                &file,
            );

            let items = items(&scene);
            let paths: Vec<&PathShape> = items
                .iter()
                .filter_map(|item| match item {
                    crate::scene::Item::Path(path) => Some(path),
                    _ => None,
                })
                .collect();
            let texts: Vec<&TextRun> = items
                .iter()
                .filter_map(|item| match item {
                    crate::scene::Item::Text(text) => Some(text),
                    _ => None,
                })
                .collect();

            let expected_paths = expected["paths"].as_array().expect("expected paths");
            assert_eq!(paths.len(), expected_paths.len(), "{file}: path count");
            for (index, (path, expected_path)) in paths.iter().zip(expected_paths).enumerate() {
                let expected_commands = expected_path["commands"]
                    .as_array()
                    .expect("expected path commands");
                assert_eq!(
                    path.path.commands.len(),
                    expected_commands.len(),
                    "{file}: path {index} command count"
                );
                for (command, expected_command) in path.path.commands.iter().zip(expected_commands)
                {
                    assert_command_close(command, expected_command, &file, index);
                }
            }

            let expected_texts = expected["texts"].as_array().expect("expected texts");
            assert_eq!(texts.len(), expected_texts.len(), "{file}: text count");
            for (text, expected_text) in texts.iter().zip(expected_texts) {
                assert_eq!(
                    text.lines
                        .iter()
                        .map(|line| line.text())
                        .collect::<String>(),
                    expected_text["text"].as_str().expect("text content"),
                    "{file}: text content"
                );
                close(
                    text.position.x,
                    expected_text["x"].as_f64().expect("text x"),
                    "text x",
                    &file,
                );
                close(
                    text.position.y,
                    expected_text["y"].as_f64().expect("text y"),
                    "text y",
                    &file,
                );
                close(
                    text.font.size,
                    expected_text["fontSize"].as_f64().expect("text font size"),
                    "text font size",
                    &file,
                );
                let bbox = &expected_text["bbox"];
                close(
                    text.size.width,
                    bbox["width"].as_f64().expect("text width"),
                    "text width",
                    &file,
                );
                close(
                    text.size.height,
                    bbox["height"].as_f64().expect("text height"),
                    "text height",
                    &file,
                );
                assert_eq!(
                    text.anchor,
                    match expected_text["anchor"].as_str().expect("text anchor") {
                        "middle" => TextAnchor::Middle,
                        "end" => TextAnchor::End,
                        _ => TextAnchor::Start,
                    },
                    "{file}: text anchor"
                );
            }
        }
    }

    #[test]
    fn draws_the_theme_colors() {
        let config = Config::default();
        let scene = render("a failure", &config);
        let items = items(&scene);
        let icon = theme_string(&config.theme_variables, "errorBkgColor")
            .expect("default theme errorBkgColor");
        let text = theme_string(&config.theme_variables, "errorTextColor")
            .expect("default theme errorTextColor");

        let mut paths = 0;
        let mut texts = 0;
        for item in items {
            match item {
                crate::scene::Item::Path(path) => {
                    assert_eq!(path.style.fill, Some(Color::new(icon)), "icon fill");
                    assert_eq!(path.style.stroke, None, "icon stroke");
                    paths += 1;
                }
                crate::scene::Item::Text(run) => {
                    assert_eq!(run.fill, Some(Color::new(text)), "text fill");
                    texts += 1;
                }
                other => panic!("unexpected error item: {other:?}"),
            }
        }
        assert_eq!(paths, 6);
        assert_eq!(texts, 3);
    }

    // errorRenderer.spec.ts: `wrapErrorMessage`

    #[test]
    fn returns_no_lines_for_an_empty_message() {
        assert_eq!(wrap_error_message(""), Vec::<String>::new());
        assert_eq!(wrap_error_message("   "), Vec::<String>::new());
    }

    #[test]
    fn keeps_a_short_message_on_a_single_line() {
        let message = "Edge limit exceeded. 501 edges found, but the limit is 500.";
        assert_eq!(wrap_error_message(message), vec![message.to_string()]);
    }

    #[test]
    fn normalizes_whitespace_and_wraps_at_word_boundaries() {
        let message = format!("a{} {} c{}", " ".repeat(40), "b".repeat(40), "d".repeat(39));
        assert_eq!(
            wrap_error_message(&message),
            vec![
                format!("a {}", "b".repeat(40)),
                format!("c{}", "d".repeat(39))
            ]
        );
    }

    #[test]
    fn hard_wraps_words_longer_than_a_line() {
        assert_eq!(
            wrap_error_message(&"x".repeat(160)),
            vec!["x".repeat(75), "x".repeat(75), "x".repeat(10)]
        );
    }

    #[test]
    fn does_not_split_surrogate_pairs_when_hard_wrapping() {
        let emoji = "\u{1F600}";
        assert_eq!(
            wrap_error_message(&emoji.repeat(80)),
            vec![emoji.repeat(75), emoji.repeat(5)]
        );
    }

    #[test]
    fn caps_the_message_at_four_lines_with_an_ellipsis() {
        let word = "w".repeat(70);
        let message = [word.as_str(); 6].join(" ");
        let lines = wrap_error_message(&message);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[3], format!("{word}..."));
    }

    #[test]
    fn does_not_split_surrogate_pairs_when_truncating() {
        let emoji = "\u{1F600}";
        let message = std::iter::repeat_n(emoji.repeat(70), 6)
            .collect::<Vec<_>>()
            .join(" ");
        let lines = wrap_error_message(&message);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[3], format!("{}...", emoji.repeat(70)));
    }

    // errorRenderer.spec.ts: `errorRenderer`

    #[test]
    fn draws_the_plain_graphic_when_there_is_no_message() {
        let scene = render("", &Config::default());
        assert_eq!(scene.view_box, Rect::new(0.0, 0.0, 2412.0, 512.0));
        let texts: Vec<String> = items(&scene)
            .into_iter()
            .filter_map(|item| match item {
                crate::scene::Item::Text(run) => {
                    Some(run.lines.iter().map(|line| line.text()).collect())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            texts,
            vec!["Syntax error in text", "mermaid version 12.3.0"]
        );
    }

    #[test]
    fn draws_the_error_message_below_the_headline() {
        let message = "Edge limit exceeded. 501 edges found, but the limit is 500.";
        let scene = render(message, &Config::default());
        assert_eq!(scene.view_box.height, 556.0);
        let texts: Vec<String> = items(&scene)
            .into_iter()
            .filter_map(|item| match item {
                crate::scene::Item::Text(run) => {
                    Some(run.lines.iter().map(|line| line.text()).collect())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            texts,
            vec!["Syntax error in text", "mermaid version 12.3.0", message]
        );
    }

    #[test]
    fn draws_each_wrapped_line_separately() {
        let long = "m".repeat(75);
        let message = format!("{long} one two three four five");
        let scene = render(&message, &Config::default());
        let texts: Vec<String> = items(&scene)
            .into_iter()
            .filter_map(|item| match item {
                crate::scene::Item::Text(run) => {
                    Some(run.lines.iter().map(|line| line.text()).collect())
                }
                _ => None,
            })
            .collect();
        assert_eq!(&texts[2..], &[long, "one two three four five".to_string()]);
        assert_eq!(scene.view_box.height, 612.0);
    }

    #[test]
    fn parses_relative_icon_paths_into_absolute_commands() {
        let path = parse_svg_path("m10,20l5-5h10v10c1,2 3,4 5,6s1,1 2,2z");
        assert_eq!(
            path.commands,
            vec![
                PathCommand::move_to(Point::new(10.0, 20.0)),
                PathCommand::line_to(Point::new(15.0, 15.0)),
                PathCommand::line_to(Point::new(25.0, 15.0)),
                PathCommand::line_to(Point::new(25.0, 25.0)),
                PathCommand::cubic_to(
                    Point::new(26.0, 27.0),
                    Point::new(28.0, 29.0),
                    Point::new(30.0, 31.0)
                ),
                PathCommand::cubic_to(
                    Point::new(32.0, 33.0),
                    Point::new(31.0, 32.0),
                    Point::new(32.0, 33.0)
                ),
                PathCommand::Close,
            ]
        );
    }

    #[test]
    fn render_error_is_public_next_to_render_diagram() {
        let scene = crate::render_error("a failure", &Config::default());
        assert_eq!(scene.view_box.width, VIEWPORT_WIDTH);
    }
}
