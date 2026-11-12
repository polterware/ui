//! Port of upstream `shapes/bpmnDataObject.ts`: the 36 × 50 page with a folded corner, its
//! optional data-direction arrow and collection marks, and the caption below.
//!
//! Classic look only; upstream never calls rough.js here. Upstream reads
//! `node.metadata.dataDirection` and `.isCollection`, which the shared [`Node`] does not carry:
//! [`draw`] renders the plain page and [`draw_with_extras`] carries the full upstream logic.
//! The shared intersection model cannot express `faceProjectIntersect`; [`intersect`] is the
//! exact port, while the node carries a rectangle for the drawn page.

use super::bpmn_shape_core::{
    BpmnLabel, LABEL_GAP, bpmn_group, bpmn_label, classed_path, face_project_intersect,
    label_wrap_width, position_label_below, reserve_bounds,
};
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError};
use crate::scene::{Group, Item, Path, Point, SourceRef};

pub const DATA_WIDTH: f64 = 36.0;
pub const DATA_HEIGHT: f64 = 50.0;
const FOLD: f64 = 10.0;

/// `dataDirectionOf`: the arrow a data object carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataDirection {
    Input,
    Output,
}

impl DataDirection {
    fn as_str(self) -> &'static str {
        match self {
            DataDirection::Input => "input",
            DataDirection::Output => "output",
        }
    }
}

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    draw_with_extras(node, ctx, None, false)
}

/// The full upstream handler, including the direction arrow and collection marks.
pub fn draw_with_extras(
    node: &Node,
    ctx: &ShapeContext<'_>,
    direction: Option<DataDirection>,
    collection: bool,
) -> Result<RenderedNode, ShapeError> {
    let mut node = node.clone();

    let BpmnLabel {
        group: label,
        width: label_width,
        height: label_height,
    } = bpmn_label(&node, ctx, label_wrap_width(&node))?;

    let mut group = bpmn_group(&node, "bpmn-data-object");
    let mut body = Group::new().with_class("bpmn-data-body");

    let left = -DATA_WIDTH / 2.0;
    let top = -DATA_HEIGHT / 2.0;
    let right = DATA_WIDTH / 2.0;
    let bottom = DATA_HEIGHT / 2.0;

    let mut page = Path::new();
    page.move_to(Point::new(left, top))
        .line_to(Point::new(right - FOLD, top))
        .line_to(Point::new(right, top + FOLD))
        .line_to(Point::new(right, bottom))
        .line_to(Point::new(left, bottom))
        .close();
    body.push(classed_path(
        "bpmn-data-page",
        page,
        SourceRef::node(node.id.clone()),
    ));

    let mut fold = Path::new();
    fold.move_to(Point::new(right - FOLD, top))
        .line_to(Point::new(right - FOLD, top + FOLD))
        .line_to(Point::new(right, top + FOLD));
    body.push(classed_path(
        "bpmn-data-fold",
        fold,
        SourceRef::node(node.id.clone()),
    ));

    if let Some(direction) = direction {
        let mut arrow = Path::new();
        arrow
            .move_to(Point::new(left + 4.0, top + 9.0))
            .line_to(Point::new(left + 11.0, top + 9.0))
            .line_to(Point::new(left + 11.0, top + 6.0))
            .line_to(Point::new(left + 16.0, top + 10.5))
            .line_to(Point::new(left + 11.0, top + 15.0))
            .line_to(Point::new(left + 11.0, top + 12.0))
            .line_to(Point::new(left + 4.0, top + 12.0))
            .close();
        body.push(classed_path(
            &format!("bpmn-data-arrow bpmn-data-arrow-{}", direction.as_str()),
            arrow,
            SourceRef::node(node.id.clone()),
        ));
    }

    if collection {
        let mut marks = Path::new();
        for x in [-4.0, 0.0, 4.0] {
            marks
                .move_to(Point::new(x, bottom - 11.0))
                .line_to(Point::new(x, bottom - 2.0));
        }
        body.push(classed_path(
            "bpmn-data-collection",
            marks,
            SourceRef::node(node.id.clone()),
        ));
    }

    group.push(body);

    let has_label = label_height > 0.0 && !node.label.is_empty();
    let caption_band = if has_label {
        LABEL_GAP + label_height
    } else {
        0.0
    };
    let total_height = DATA_HEIGHT + 2.0 * caption_band;
    let total_width = DATA_WIDTH.max(if has_label { label_width } else { 0.0 });

    let mut label = label;
    if has_label {
        position_label_below(
            &mut label,
            label_width,
            label_height,
            DATA_HEIGHT / 2.0 + LABEL_GAP + label_height / 2.0,
        );
    }
    group.push(Item::Group(label));

    group.push(reserve_bounds(&mut node, total_width, total_height));
    // `reserveBounds` inserts the rect first; keep the scene order equal to the SVG order.
    group.items.rotate_right(1);

    node.intersect = IntersectKind::FaceProject {
        width: DATA_WIDTH,
        height: DATA_HEIGHT,
        face: None,
    };
    Ok(RenderedNode { node, group })
}

/// The exact `faceProjectIntersect` attachment for a data object centred at zero.
pub fn intersect(point: Point) -> Point {
    face_project_intersect(Point::ZERO, DATA_WIDTH, DATA_HEIGHT, point)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::shapes::bpmn_shape_core::fixture_support as fixture;
    use crate::theme::ThemeVariables;

    #[test]
    fn matches_upstream_data_object_geometry() {
        let fixture = fixture::load(include_str!("../testdata/shapes/bpmn-data.json"));
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
            .expect("data object renders");
            fixture::assert_node(case, &rendered);
            fixture::assert_label(case, &rendered.group);
            fixture::assert_elements(case, &rendered.group);
            fixture::assert_intersects(case, intersect);
        }
    }

    #[test]
    fn extras_add_the_direction_arrow_and_collection_marks() {
        let node = Node::new("n", "Data", "bpmn-data");
        let measure = fixture::FixedMeasure {
            char_width: 7.0,
            line_height: 16.0,
        };
        let theme = ThemeVariables::default();
        let rendered = draw_with_extras(
            &node,
            &ShapeContext {
                measure: &measure,
                theme: &theme,
            },
            Some(DataDirection::Output),
            true,
        )
        .expect("data object renders");
        fn collect_classes(group: &Group, out: &mut Vec<String>) {
            for item in &group.items {
                if let Item::Group(group) = item {
                    if let Some(class) = &group.class {
                        out.push(class.clone());
                    }
                    collect_classes(group, out);
                }
            }
        }
        let mut found = Vec::new();
        collect_classes(&rendered.group, &mut found);
        assert!(
            found
                .iter()
                .any(|class| class == "bpmn-data-arrow bpmn-data-arrow-output")
        );
        assert!(found.iter().any(|class| class == "bpmn-data-collection"));
    }
}
