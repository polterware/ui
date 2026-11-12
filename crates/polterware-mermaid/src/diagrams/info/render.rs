//! Source-to-[`Scene`] rendering for info diagrams: the Rust side of `infoRenderer.ts`
//! `draw`.
//!
//! `draw` sizes the SVG to a 400-wide, 100-high box with `useMaxWidth`, appends one group,
//! and draws one `<text x="100" y="40" class="version" font-size="32" text-anchor="middle">`
//! whose content is `v{version}`. The version is the build's Mermaid version, which upstream
//! passes as the renderer's third argument; this crate ports Mermaid 12.3.0, so [`VERSION`] is
//! that version.
//!
//! The scene carries the group's text run and the 400x100 view box the renderer configures.
//! Upstream's SVG has no `viewBox` attribute for an info diagram (`configureSvgSize` only sets
//! `width` and `max-width`), so the view box here is the canvas it sizes to, not a fitted
//! content box. The text is the theme's `textColor` over the SVG's inherited `fill`, and the
//! font family is the configured one; no requirement or diagram stylesheet changes it.

use crate::config::Config;
use crate::error::ParseError;
use crate::measure::{FallbackMeasure, Font, TextMeasure, plain_text_block};
use crate::scene::{Color, Group, Item, Point, Rect, Scene, TextAnchor, TextRun};

/// The Mermaid version this crate ports, and therefore the version `info` draws.
pub const VERSION: &str = "12.3.0";

/// `configureSvgSize(svg, 100, 400, true)`'s width.
pub const VIEWPORT_WIDTH: f64 = 400.0;
/// `configureSvgSize(svg, 100, 400, true)`'s height.
pub const VIEWPORT_HEIGHT: f64 = 100.0;
/// `font-size` on the version text.
pub const VERSION_FONT_SIZE: f64 = 32.0;
/// The text's `x` attribute; `text-anchor: middle` centres on it.
pub const VERSION_X: f64 = 100.0;
/// The text's `y` attribute, its baseline.
pub const VERSION_Y: f64 = 40.0;

/// Renders an info body (the text [`super::parse`] accepts) to a [`Scene`].
pub fn draw(
    body: &str,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, InfoRenderError> {
    super::parse(body)?;
    Ok(draw_version(config, measure))
}

/// Draws the version text, the only content `infoRenderer.ts` produces. The parsed info
/// text's title and accessible fields do not reach the diagram.
pub fn draw_version(config: &Config, measure: &dyn TextMeasure) -> Scene {
    let family = config
        .theme_variables
        .get("fontFamily")
        .and_then(|value| value.as_str())
        .unwrap_or("trebuchet ms")
        .to_string();
    let fill = config
        .theme_variables
        .get("textColor")
        .and_then(|value| value.as_str())
        .map(Color::new)
        .unwrap_or_else(|| Color::new("#333"));
    let font = Font {
        family,
        size: VERSION_FONT_SIZE,
        weight: crate::scene::NORMAL_WEIGHT,
        italic: false,
    };
    let text = format!("v{VERSION}");
    let block = plain_text_block(&text, &font, measure);
    let mut run = TextRun::new(Point::new(VERSION_X, VERSION_Y), font, block);
    run.anchor = TextAnchor::Middle;
    run.fill = Some(fill);

    let mut group = Group::new().with_class("version");
    group.push(Item::Text(run));

    let mut scene = Scene::new(Rect::new(0.0, 0.0, VIEWPORT_WIDTH, VIEWPORT_HEIGHT));
    scene.push(Item::Group(group));
    scene
}

/// Renders an info diagram with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, InfoRenderError> {
    draw(body, config, &FallbackMeasure)
}

/// What can stop an info render.
#[derive(Clone, Debug, PartialEq)]
pub enum InfoRenderError {
    Parse(ParseError),
}

impl std::fmt::Display for InfoRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for InfoRenderError {}

impl From<ParseError> for InfoRenderError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
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
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/diagrams/info/tests/render");
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

    #[test]
    fn renders_recorded_info_fixtures() {
        let fixtures = fixtures();
        assert!(!fixtures.is_empty(), "expected info fixtures");

        for (file, fixture) in fixtures {
            let source = fixture["source"].as_str().expect("fixture source");
            let body = crate::preprocess::preprocess(source).body;
            let scene = render(&body, &Config::default())
                .unwrap_or_else(|error| panic!("{file}: render: {error}"));
            let expected = &fixture["expected"];

            // The SVG has no viewBox; this is the 400x100 canvas `configureSvgSize` sizes.
            let expected_canvas = &expected["canvas"];
            close(
                scene.view_box.x,
                expected_canvas["x"].as_f64().expect("canvas x"),
                "viewBox x",
                &file,
            );
            close(
                scene.view_box.y,
                expected_canvas["y"].as_f64().expect("canvas y"),
                "viewBox y",
                &file,
            );
            close(
                scene.view_box.width,
                expected_canvas["width"].as_f64().expect("canvas width"),
                "viewBox width",
                &file,
            );
            close(
                scene.view_box.height,
                expected_canvas["height"].as_f64().expect("canvas height"),
                "viewBox height",
                &file,
            );

            let actual: Vec<&TextRun> = scene
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Group(group) if group.class.as_deref() == Some("version") => {
                        group.items.iter().find_map(|item| match item {
                            Item::Text(text) => Some(text),
                            _ => None,
                        })
                    }
                    _ => None,
                })
                .collect();
            let expected_texts = expected["texts"].as_array().expect("expected texts");
            assert_eq!(actual.len(), expected_texts.len(), "{file}: text count");
            for (text, expected_text) in actual.iter().zip(expected_texts) {
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
    fn draws_the_version_at_the_renderer_position() {
        let scene = render("info", &Config::default()).expect("render info");
        let run = scene
            .items
            .iter()
            .find_map(|item| match item {
                Item::Group(group) => group.items.iter().find_map(|item| match item {
                    Item::Text(text) => Some(text),
                    _ => None,
                }),
                _ => None,
            })
            .expect("version text");
        assert_eq!(
            run.lines.iter().map(|line| line.text()).collect::<String>(),
            format!("v{VERSION}")
        );
        assert_eq!(run.position, Point::new(100.0, 40.0));
        assert_eq!(run.font.size, 32.0);
        assert_eq!(run.anchor, TextAnchor::Middle);
    }

    #[test]
    fn render_diagram_dispatches_info() {
        let scene = crate::render_diagram("info", &Config::default()).expect("dispatch info");
        assert_eq!(scene.items.len(), 1);
    }

    #[test]
    fn rejects_text_that_is_not_an_info_diagram() {
        let error = render("requirementDiagram", &Config::default()).unwrap_err();
        assert!(matches!(error, InfoRenderError::Parse(_)));
    }
}
