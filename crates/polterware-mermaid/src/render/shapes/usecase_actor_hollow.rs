//! Classic port of upstream `shapes/usecaseActorHollow.ts`.
//!
//! Look: the outlined head circle above a blocky body of the web design system's hollow
//! actor, inside the same 56 × 72 frame and label stack as `usecaseActor`.
//! Behavior: upstream `usecaseActorHollow.ts` delegates to `renderUsecaseActor`; so does
//! this port, with [`ActorVariant::Hollow`](super::usecase_actor::ActorVariant::Hollow).
//!
//! `handDrawn` is not rendered: rough.js jitter stays a renderer TODO.

use super::usecase_actor::{self, ActorOptions, ActorVariant};
use super::{Node, RenderedNode, ShapeContext, ShapeError};

// handDrawn: rough.js (later)

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    usecase_actor::draw_with(node, ctx, ActorVariant::Hollow, &ActorOptions::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::scene::Item;
    use crate::theme::ThemeVariables;

    #[test]
    fn head_circle_and_body_are_unfilled() {
        let node = Node::new("a", "Actor", "usecaseActorHollow");
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
                assert_eq!(glyph.items.len(), 2);
                match &glyph.items[0] {
                    Item::Ellipse(head) => {
                        assert_eq!(head.center, crate::scene::Point::new(0.0, -23.0));
                        assert_eq!(head.radius_x, 9.0);
                        assert!(head.style.fill.is_none());
                    }
                    _ => panic!("the head circle is first"),
                }
                assert!(matches!(&glyph.items[1], Item::Path(path) if path.style.fill.is_none()));
            }
            _ => panic!("the glyph group is last"),
        }
    }

    #[test]
    fn matches_upstream_fixtures() {
        use crate::render::shapes::usecase_ellipse::difftest;
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../testdata/shapes/usecaseActorHollow.json"))
                .expect("fixture parses");
        let theme = ThemeVariables::default();
        for case in fixture["cases"].as_array().expect("cases") {
            let measure = difftest::FixedMeasure::from_case(case);
            let node = difftest::node_for(case, "usecaseActorHollow");
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
