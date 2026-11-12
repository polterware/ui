//! Classic port of upstream `shapes/usecaseActorAwesome.ts`.
//!
//! Look: the filled head-and-shoulders silhouette of the web design system's business
//! actor, inside the same 56 × 72 frame and label stack as `usecaseActor`.
//! Behavior: upstream `usecaseActorAwesome.ts` delegates to `renderUsecaseActor`; so does
//! this port, with [`ActorVariant::Awesome`](super::usecase_actor::ActorVariant::Awesome).
//!
//! `handDrawn` is not rendered: rough.js jitter stays a renderer TODO.

use super::usecase_actor::{self, ActorOptions, ActorVariant};
use super::{Node, RenderedNode, ShapeContext, ShapeError};

// handDrawn: rough.js (later)

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    usecase_actor::draw_with(node, ctx, ActorVariant::Awesome, &ActorOptions::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::scene::Item;
    use crate::theme::ThemeVariables;

    #[test]
    fn silhouette_is_a_single_filled_path() {
        let node = Node::new("a", "Actor", "usecaseActorAwesome");
        let rendered = draw(
            &node,
            &ShapeContext {
                measure: &FallbackMeasure,
                theme: &ThemeVariables::default(),
            },
        )
        .unwrap();
        match rendered.group.items.last() {
            Some(Item::Group(glyph)) => {
                assert_eq!(
                    glyph.class.as_deref(),
                    Some("usecase-actor-glyph usecase-actor-awesome")
                );
                assert_eq!(glyph.items.len(), 1);
                assert!(matches!(&glyph.items[0], Item::Path(path) if path.style.fill.is_some()));
            }
            _ => panic!("the glyph group is last"),
        }
    }

    #[test]
    fn matches_upstream_fixtures() {
        use crate::render::shapes::usecase_ellipse::difftest;
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../testdata/shapes/usecase_actor_awesome.json"
        ))
        .expect("fixture parses");
        let theme = ThemeVariables::default();
        for case in fixture["cases"].as_array().expect("cases") {
            let measure = difftest::FixedMeasure::from_case(case);
            let node = difftest::node_for(case, "usecaseActorAwesome");
            let rendered = draw(
                &node,
                &ShapeContext {
                    measure: &measure,
                    theme: &theme,
                },
            )
            .expect("shape renders");
            difftest::assert_case(&rendered, case);
        }
    }
}
