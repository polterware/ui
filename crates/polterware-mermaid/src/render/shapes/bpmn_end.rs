//! Port of upstream `shapes/bpmnEnd.ts`: an end event is [`super::bpmn_event`] with the `end`
//! ring set (one heavier ring, 3.4 weight).

use super::{Node, RenderedNode, ShapeContext, ShapeError};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    super::bpmn_event::render(node, ctx, "end")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::shapes::bpmn_shape_core::{
        EVENT_DIAMETER, fixture_support as fixture, ring_intersect,
    };
    use crate::scene::Point;
    use crate::theme::ThemeVariables;

    #[test]
    fn matches_upstream_end_event_geometry() {
        let fixture = fixture::load(include_str!("../testdata/shapes/bpmn-end.json"));
        let measure = fixture.measure();
        let theme = ThemeVariables::default();
        for case in &fixture.cases {
            let rendered = draw(
                &fixture.node(case),
                &ShapeContext {
                    measure: &measure,
                    theme: &theme,
                },
            )
            .expect("end event renders");
            fixture::assert_node(case, &rendered);
            fixture::assert_label(case, &rendered.group);
            fixture::assert_elements(case, &rendered.group);
            fixture::assert_intersects(case, |point| {
                ring_intersect(Point::ZERO, EVENT_DIAMETER, point)
            });
        }
    }
}
