//! Source-to-[`Scene`] rendering for state diagrams: the Rust side of
//! `stateRenderer-v3-unified.ts` `draw`.
//!
//! `draw` calls `diag.db.extract(...)` (already applied by [`super::parse`]), asks
//! `getData()` for the layout nodes and edges, copies the configuration `draw` applies
//! (`type`, `layoutAlgorithm`, `nodeSpacing`, `rankSpacing`, `markers`, `diagramId`), runs
//! the unified pipeline, and sets the viewport with `setupViewPortForSVG` (`padding` `8`,
//! `state.useMaxWidth`). Here those are [`super::parse`], [`layout_data`],
//! [`crate::render::pipeline::render`] (which includes the viewport fitted with the layout
//! data's padding and `useMaxWidth`).
//!
//! Where the port stops short of upstream, the difference is in the painter-facing scene the
//! pipeline already documents: HTML labels (`foreignObject`) are plain text runs, the
//! clickable links `draw` wraps nodes in are not inserted (the scene carries the diagram, not
//! the page around it), and the SVG title is not inserted.

use crate::config::Config;
use crate::error::ParseError;
use crate::measure::{FallbackMeasure, TextMeasure};
use crate::render::layout_dagre::LayoutError;
use crate::render::pipeline;
use crate::render::pipeline::LayoutData;
use crate::scene::Scene;

use super::ast::Ast;
use super::layout_data::{DEFAULT_DIAGRAM_ID, layout_data};

/// Renders a state diagram body (the text [`super::parse`] accepts) to a [`Scene`].
///
/// `config` is the diagram configuration after the source's frontmatter and directives have
/// been applied, as `getConfig()` returns it to `draw`.
pub fn draw(
    body: &str,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, StateRenderError> {
    let ast = super::parse(body)?;
    draw_ast(&ast, config, measure)
}

/// Renders an already parsed state diagram.
pub fn draw_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, StateRenderError> {
    draw_layout_ast(ast, config, measure).map(|(_, scene)| scene)
}

/// Renders an already parsed state diagram and keeps the laid-out [`LayoutData`], whose nodes
/// and edges carry the positions and points the scene was painted from.
pub fn draw_layout_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<(LayoutData, Scene), StateRenderError> {
    let mut data = layout_data(ast, config, DEFAULT_DIAGRAM_ID);
    let scene = pipeline::render(&mut data, measure).map_err(StateRenderError::Layout)?;
    Ok((data, scene))
}

/// Renders a state diagram body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, StateRenderError> {
    draw(body, config, &FallbackMeasure)
}

/// What can stop a state render.
#[derive(Clone, Debug, PartialEq)]
pub enum StateRenderError {
    Parse(ParseError),
    Layout(LayoutError),
}

impl std::fmt::Display for StateRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(f),
            Self::Layout(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for StateRenderError {}

impl From<ParseError> for StateRenderError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<LayoutError> for StateRenderError {
    fn from(error: LayoutError) -> Self {
        Self::Layout(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_a_rendered_scene_with_view_box() {
        let scene = render(
            "stateDiagram-v2\n  [*] --> A\n  A --> [*]\n",
            &Config::default(),
        )
        .expect("render state diagram");
        assert!(scene.view_box.width > 0.0);
        assert!(scene.view_box.height > 0.0);
        assert!(!scene.items.is_empty());
    }

    #[test]
    fn rejects_a_body_without_a_state_header() {
        let error = render("not a state diagram", &Config::default()).unwrap_err();
        assert!(matches!(error, StateRenderError::Parse(_)));
    }
}
