//! Classic port of upstream `shapes/usecaseActorIcon.ts`.
//!
//! Look: a 52 × 52 rounded frame with the actor's icon centred inside it, above the same
//! label stack as `usecaseActor`.
//! Behavior: upstream `usecaseActorIcon.ts` delegates to `renderUsecaseActor`; so does this
//! port, with [`ActorVariant::Icon`](super::usecase_actor::ActorVariant::Icon). The icon is a
//! [`crate::scene::ImageRef`] the painter resolves, which replaces upstream's inline iconify
//! svg, its `.html()` insertion and its unknown-icon fallback.
//!
//! `handDrawn` is not rendered: rough.js jitter stays a renderer TODO.

use super::usecase_actor::{self, ActorOptions, ActorVariant};
use super::{Node, RenderedNode, ShapeContext, ShapeError};

// handDrawn: rough.js (later)

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    draw_with_icon(node, ctx, None)
}

/// Draws the icon actor with the named asset inside its frame.
pub fn draw_with_icon(
    node: &Node,
    ctx: &ShapeContext<'_>,
    icon: Option<&str>,
) -> Result<RenderedNode, ShapeError> {
    usecase_actor::draw_with(
        node,
        ctx,
        ActorVariant::Icon,
        &ActorOptions {
            icon,
            ..ActorOptions::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::FallbackMeasure;
    use crate::scene::Item;
    use crate::theme::ThemeVariables;

    #[test]
    fn icon_asset_sits_inside_the_frame() {
        let node = Node::new("a", "Actor", "usecaseActorIcon");
        let rendered = draw_with_icon(
            &node,
            &ShapeContext {
                measure: &FallbackMeasure,
                theme: &ThemeVariables::default(),
            },
            Some("fa:fa-user"),
        )
        .unwrap();
        match rendered.group.items.last() {
            Some(Item::Group(glyph)) => {
                assert_eq!(
                    glyph.class.as_deref(),
                    Some("usecase-actor-glyph usecase-actor-icon")
                );
                match &glyph.items[1] {
                    Item::Image(image) => {
                        assert_eq!(image.name, "fa:fa-user");
                        assert_eq!(
                            image.rect,
                            crate::scene::Rect::new(-21.0, -23.0, 42.0, 42.0)
                        );
                    }
                    _ => panic!("the icon follows the frame"),
                }
            }
            _ => panic!("the glyph group is last"),
        }
    }

    #[test]
    fn matches_upstream_fixtures() {
        use crate::render::shapes::usecase_ellipse::difftest;
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../testdata/shapes/usecaseActorIcon.json"))
                .expect("fixture parses");
        let theme = ThemeVariables::default();
        for case in fixture["cases"].as_array().expect("cases") {
            let measure = difftest::FixedMeasure::from_case(case);
            let node = difftest::node_for(case, "usecaseActorIcon");
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
