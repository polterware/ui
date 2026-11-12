//! The swimlanes layout in the unified pipeline.
//!
//! Upstream registers the layout as `swimlane` (`rendering-util/layout-algorithms/swimlanes/
//! index.ts`) and the swimlane-beta diagram declares it in the configuration schema. This
//! module exposes it under that name and under the module's own name, `swimlanes`, and runs
//! the full common renderer: prepare, measure, lay out, paint, and the line-hop pass. The
//! renderer itself lives beside the layout, in
//! [`crate::layout::swimlanes::index::render`], so the layout-utils callers and the pipeline
//! share one implementation.

use crate::layout::swimlanes::index;
use crate::measure::TextMeasure;
use crate::render::layout_dagre::LayoutError;
use crate::scene::Scene;

use super::types::LayoutData;

/// Render function for the swimlanes layout algorithm.
pub fn swimlanes_render(
    data4_layout: &mut LayoutData,
    measure: &dyn TextMeasure,
) -> Result<Scene, LayoutError> {
    index::render(data4_layout, measure).map_err(|error| LayoutError::Dagre(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::render::pipeline::render;

    #[test]
    fn swimlanes_is_a_registered_layout_algorithm() {
        for name in ["swimlane", "swimlanes"] {
            assert_eq!(
                super::super::get_registered_layout_algorithm(name, None),
                name
            );
            assert!(super::super::get_layout_loader_definition(name).is_ok());
        }
    }

    #[test]
    fn the_pipeline_runs_the_swimlanes_algorithm() {
        let mut data = LayoutData {
            layout_algorithm: Some("swimlanes".to_string()),
            nodes: vec![
                crate::render::pipeline::types::Node::new("a", "A", "rect"),
                crate::render::pipeline::types::Node::new("b", "B", "rect"),
            ],
            edges: vec![crate::render::pipeline::types::Edge::new("ab", "a", "b")],
            ..Default::default()
        };

        let scene = render(&mut data, &FallbackMeasure).expect("render");

        assert!(scene.view_box.width > 0.0);
        assert!(scene.view_box.height > 0.0);
        for node in &data.nodes {
            assert!(node.x.is_some_and(f64::is_finite));
            assert!(node.y.is_some_and(f64::is_finite));
        }
    }
}
