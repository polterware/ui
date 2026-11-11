//! Upstream `shapes/bpmnShapeCore.ts`.
use super::{Node, RenderedNode, ShapeContext, ShapeError};
use crate::scene::Point;

pub const EVENT_DIAMETER: f64 = 36.0;
pub const GATEWAY_SIZE: f64 = 50.0;
pub const ACTIVITY_WIDTH: f64 = 100.0;
pub const ACTIVITY_HEIGHT: f64 = 80.0;
pub const LABEL_GAP: f64 = 6.0;
pub const ICON_SIZE: f64 = 16.0;
pub const BPMN_ICON_PREFIX: &str = "bpmn";
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EventRings {
    pub rings: u8,
    pub stroke_width: f64,
}
pub fn event_rings(position: &str) -> Option<EventRings> {
    match position {
        "start" => Some(EventRings {
            rings: 1,
            stroke_width: 1.6,
        }),
        "intermediate" | "boundary" => Some(EventRings {
            rings: 2,
            stroke_width: 1.4,
        }),
        "end" => Some(EventRings {
            rings: 1,
            stroke_width: 3.4,
        }),
        _ => None,
    }
}

pub fn face_project_intersect(center: Point, width: f64, height: f64, point: Point) -> Point {
    let dx = point.x - center.x;
    let dy = point.y - center.y;
    let hw = width / 2.0;
    let hh = height / 2.0;
    let reach = |half: f64| (half - (8.0_f64).min(half / 2.0)).max(0.0);
    if dx.abs() * hh > dy.abs() * hw {
        Point::new(
            center.x + dx.signum() * hw,
            point.y.clamp(center.y - reach(hh), center.y + reach(hh)),
        )
    } else {
        Point::new(
            point.x.clamp(center.x - reach(hw), center.x + reach(hw)),
            center.y + if dy >= 0.0 { hh } else { -hh },
        )
    }
}

// Compatibility entry for the current registry; this upstream module is a helper, not a shape.
pub fn draw(node: &Node, _ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    Err(ShapeError {
        message: format!("{} is a rendering helper, not a shape", node.shape),
    })
}
