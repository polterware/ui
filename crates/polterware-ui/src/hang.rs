//! Where a floating panel hangs from the control that opens it.
//!
//! The panel is laid out at the top-left corner of the box it is rendered in, which stands for
//! the trigger: `trigger_height` tall from there. It goes under the trigger with their left
//! edges aligned, above it when it does not fit under and does fit above, and is kept
//! `WINDOW_MARGIN` inside the window. The side is picked in the frame that lays the panel out,
//! from its own size, so it never draws on the wrong side first. `slide` moves it toward the
//! trigger along the side it took, for the spring in.
//!
//! Hosts draw some panels in a deferred layer already and gpui cannot nest one in another, so
//! this only places the panel; it does not lift it out of its ancestors.

use gpui::{
    AnyElement, App, Bounds, Element, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    Pixels, Position, Style, Window, point, px,
};

/// Distance kept between a panel and the window's edge.
pub const WINDOW_MARGIN: f32 = 8.0;
/// How far a panel slides toward its trigger as it springs in.
pub const SLIDE: f32 = 6.0;

/// The side of the trigger a panel took.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Below,
    Above,
}

pub struct Hang {
    child: AnyElement,
    trigger_height: Pixels,
    slide: Pixels,
    on_side: Option<Box<dyn Fn(Side)>>,
}

/// `child` hung from a trigger `trigger_height` tall, `slide` away from where it rests.
pub fn hang(trigger_height: f32, slide: f32, child: impl IntoElement) -> Hang {
    Hang {
        child: child.into_any_element(),
        trigger_height: px(trigger_height),
        slide: px(slide),
        on_side: None,
    }
}

impl Hang {
    /// Tells `callback` the side the panel took, each frame.
    pub fn on_side(mut self, callback: impl Fn(Side) + 'static) -> Self {
        self.on_side = Some(Box::new(callback));
        self
    }
}

/// The panel's origin for a `size` panel hung from a trigger whose top-left corner is
/// `trigger`, in a `viewport`.
fn place(
    trigger: gpui::Point<Pixels>,
    trigger_height: Pixels,
    size: gpui::Size<Pixels>,
    viewport: gpui::Size<Pixels>,
) -> (gpui::Point<Pixels>, Side) {
    let margin = px(WINDOW_MARGIN);
    let below = trigger.y + trigger_height;
    let above = trigger.y - size.height;
    let fits_below = below + size.height <= viewport.height - margin;
    let fits_above = above >= margin;
    let (mut y, side) = if fits_below || !fits_above {
        (below, Side::Below)
    } else {
        (above, Side::Above)
    };
    if side == Side::Below && !fits_below {
        y = (viewport.height - margin - size.height).max(margin);
    }
    let mut x = trigger.x;
    if x + size.width > viewport.width - margin {
        x = (viewport.width - margin - size.width).max(margin);
    }
    (point(x, y), side)
}

impl Element for Hang {
    type RequestLayoutState = LayoutId;
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, LayoutId) {
        let child = self.child.request_layout(window, cx);
        let mut style = Style {
            position: Position::Absolute,
            ..Style::default()
        };
        style.inset.top = px(0.0).into();
        style.inset.left = px(0.0).into();
        (window.request_layout(style, [child], cx), child)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut LayoutId,
        window: &mut Window,
        cx: &mut App,
    ) {
        let size = window.layout_bounds(*child).size;
        let (origin, side) = place(
            bounds.origin,
            self.trigger_height,
            size,
            window.viewport_size(),
        );
        if let Some(callback) = &self.on_side {
            callback(side);
        }
        let toward = match side {
            Side::Below => self.slide,
            Side::Above => -self.slide,
        };
        let offset = point(
            origin.x - bounds.origin.x,
            origin.y + toward - bounds.origin.y,
        );
        let offset = point(offset.x.round(), offset.y.round());
        window.with_element_offset(offset, |window| self.child.prepaint(window, cx));
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _child: &mut LayoutId,
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

impl IntoElement for Hang {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

#[cfg(test)]
mod tests {
    use gpui::{point, px, size};

    use super::{Side, place};

    const VIEWPORT: (f32, f32) = (600.0, 400.0);

    fn at(x: f32, y: f32) -> (gpui::Point<gpui::Pixels>, Side) {
        place(
            point(px(x), px(y)),
            px(32.0),
            size(px(200.0), px(150.0)),
            size(px(VIEWPORT.0), px(VIEWPORT.1)),
        )
    }

    #[test]
    fn it_hangs_under_the_trigger_from_its_left_edge() {
        assert_eq!(at(40.0, 40.0), (point(px(40.0), px(72.0)), Side::Below));
    }

    #[test]
    fn near_the_bottom_it_goes_above_the_trigger() {
        assert_eq!(at(40.0, 300.0), (point(px(40.0), px(150.0)), Side::Above));
    }

    #[test]
    fn near_the_right_edge_it_keeps_the_margin() {
        let (origin, _) = at(520.0, 40.0);
        assert_eq!(origin.x, px(VIEWPORT.0 - 8.0 - 200.0));
    }

    #[test]
    fn with_room_on_neither_side_it_stays_under_inside_the_window() {
        let (origin, side) = place(
            point(px(40.0), px(100.0)),
            px(32.0),
            size(px(200.0), px(300.0)),
            size(px(VIEWPORT.0), px(VIEWPORT.1)),
        );
        assert_eq!(side, Side::Below);
        assert_eq!(origin.y, px(VIEWPORT.1 - 8.0 - 300.0));
    }
}
