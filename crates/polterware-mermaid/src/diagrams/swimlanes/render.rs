//! Source-to-[`Scene`] rendering for swimlanes: the Rust side of `swimlanesDiagram.ts`.
//!
//! Upstream builds the diagram with flowchart's exported `createFlowDiagram({ styles:
//! swimlanesStyles })`: the parser, the database, and the renderer are the flowchart's, and
//! only two things change — the layout engine (`swimlane.layout`, default `swimlane`) and the
//! lane stylesheet, which suppresses the generic cluster border so a lane draws its own
//! border and title band. Here [`super::parse`] is the flowchart parser (it accepts the
//! `swimlane-beta` header), [`layout_data`] is the flowchart `getData` plus `draw`'s
//! configuration, and [`pipeline::render`] runs the swimlanes layout the diagram registers.
//!
//! The lane styles are the painted lane cluster: [`prepare_swimlane_layout`] marks every
//! top-level subgraph `shape: "swimlane"`, and the pipeline's cluster painter draws the title
//! band and the frame for that shape, so no separate CSS layer is needed. The scene carries
//! no page-level stylesheet (the Polterware theme paints the scene), and the layer differences
//! are the ones the pipeline documents: HTML labels are plain text runs and the SVG title is
//! not inserted.
//!
//! [`prepare_swimlane_layout`]: crate::layout::swimlanes::index::prepare_swimlane_layout

use crate::config::Config;
use crate::error::ParseError;
use crate::measure::{FallbackMeasure, TextMeasure};
use crate::render::layout_dagre::LayoutError;
use crate::render::pipeline;
use crate::render::pipeline::LayoutData;
use crate::render::pipeline::get_registered_layout_algorithm;
use crate::scene::Scene;

use super::Ast;
use crate::diagrams::flowchart::layout_data::{DEFAULT_DIAGRAM_ID, layout_data};

/// The diagram type upstream registers the swimlane-beta plugin under.
pub const DIAGRAM_TYPE: &str = "swimlane";

/// The layout a swimlanes diagram resolves to, in the order the configuration does it: an
/// explicit top-level `layout` wins, then the diagram's own `swimlane.layout`, then the
/// `swimlane` schema default the section carries.
fn layout_algorithm(config: &Config) -> String {
    let configured = if config.layout != Config::default().layout {
        config.layout.as_str()
    } else {
        config.swimlane.layout.as_str()
    };
    get_registered_layout_algorithm(configured, None)
}

/// Renders a swimlanes body (the text [`super::parse`] accepts) to a [`Scene`].
pub fn draw(
    body: &str,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, SwimlaneRenderError> {
    let ast = super::parse(body)?;
    draw_ast(&ast, config, measure)
}

/// Renders an already parsed swimlanes diagram.
pub fn draw_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<Scene, SwimlaneRenderError> {
    draw_layout_ast(ast, config, measure).map(|(_, scene)| scene)
}

/// Renders an already parsed swimlanes diagram and keeps the laid-out [`LayoutData`], whose
/// nodes and edges carry the positions, lane boxes, and routed points the scene was painted
/// from.
pub fn draw_layout_ast(
    ast: &Ast,
    config: &Config,
    measure: &dyn TextMeasure,
) -> Result<(LayoutData, Scene), SwimlaneRenderError> {
    let mut data = layout_data(ast, config, DEFAULT_DIAGRAM_ID);
    data.r#type = Some(DIAGRAM_TYPE.to_string());
    data.layout_algorithm = Some(layout_algorithm(config));
    let scene = pipeline::render(&mut data, measure).map_err(SwimlaneRenderError::Layout)?;
    Ok((data, scene))
}

/// Renders a swimlanes body with the crate's monospace fallback measurement.
pub fn render(body: &str, config: &Config) -> Result<Scene, SwimlaneRenderError> {
    draw(body, config, &FallbackMeasure)
}

/// What can stop a swimlanes render.
#[derive(Clone, Debug, PartialEq)]
pub enum SwimlaneRenderError {
    Parse(ParseError),
    Layout(LayoutError),
}

impl std::fmt::Display for SwimlaneRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => error.fmt(f),
            Self::Layout(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for SwimlaneRenderError {}

impl From<ParseError> for SwimlaneRenderError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<LayoutError> for SwimlaneRenderError {
    fn from(error: LayoutError) -> Self {
        Self::Layout(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_the_swimlane_layout() {
        assert_eq!(layout_algorithm(&Config::default()), "swimlane");
    }

    #[test]
    fn keeps_a_global_layout_override() {
        let mut config = Config::default();
        config.layout = "dagre".to_string();
        assert_eq!(layout_algorithm(&config), "dagre");
    }

    #[test]
    fn keeps_a_diagram_scoped_layout_override() {
        let mut config = Config::default();
        config.swimlane.layout = "dagre".to_string();
        assert_eq!(layout_algorithm(&config), "dagre");
    }

    #[test]
    fn draws_a_swimlane_body_with_lanes() {
        let scene = render(
            "swimlane-beta TB\n  subgraph lane1[Sprint 1]\n    A[Start] --> B[Build]\n  end\n",
            &Config::default(),
        )
        .expect("render swimlanes");
        assert!(scene.view_box.width > 0.0);
        assert!(scene.view_box.height > 0.0);
        assert!(!scene.items.is_empty());
    }
}
