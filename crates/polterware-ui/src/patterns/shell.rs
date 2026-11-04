//! The app frame's surfaces: the main surface inset over the shell, and the layer a modal
//! (Commander, a Composer) opens on.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, Div, Element, ElementId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, MouseButton, Pixels, Position, Style, Window, canvas, deferred, div,
    point, prelude::*, px,
};

use crate::Handler;
use crate::material::{Edge, edge, raised_drop};
use crate::theme::Theme;

/// `MainSurfaceFrame`: the inset card over the shell with the raised drop, its quiet lit edge
/// drawn above the page. The web face is `linear-gradient(to bottom, --color-surface,
/// #1c1c1c)`, a single 8-bit step that GPUI draws undithered as a hard seam across the middle,
/// so the card is filled flat with `--color-surface`. GPUI clips to rectangles, so the page
/// keeps its corners clear itself.
pub fn main_surface(page: impl IntoElement, theme: &Theme) -> Div {
    div()
        .relative()
        .size_full()
        .rounded(px(theme.radius_surface))
        .bg(theme.color_surface)
        .shadow(raised_drop(theme))
        .child(
            div()
                .size_full()
                .rounded(px(theme.radius_surface))
                .overflow_hidden()
                .child(page),
        )
        .child(edge(Edge::Quiet, theme.radius_surface, theme))
}

/// Where modals draw among the deferred layers: over menus, popovers, and hover cards.
const MODAL_PRIORITY: usize = 100;

/// A full-window layer with the overlay scrim at `presence` (0 to 1) and `content` centered
/// on it, as the Solid `Dialog` portals to the body: wherever a view declares it (the app's
/// root, or a page inside the main surface), it covers the whole window, sidebar included,
/// and draws over everything else. While `interactive`, the layer takes the pointer and a
/// press on the scrim calls `on_dismiss`; a press on the content stays on the content.
pub fn modal_layer(
    presence: f32,
    interactive: bool,
    on_dismiss: Handler,
    content: impl IntoElement,
    theme: &Theme,
) -> AnyElement {
    let mut scrim = theme.color_overlay;
    scrim.a *= presence.clamp(0.0, 1.0);
    let layer = div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .absolute()
                .size_full()
                .bg(scrim)
                .when(interactive, |backdrop| {
                    backdrop
                        .occlude()
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            cx.stop_propagation();
                            on_dismiss(window, cx);
                        })
                }),
        )
        .child(
            div()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(content),
        );
    deferred(WindowLayer {
        child: layer.into_any_element(),
    })
    .with_priority(MODAL_PRIORITY)
    .into_any_element()
}

/// Lays its child out at the window's size and paints it at the window's origin, whatever
/// box declared it. Deferred draws keep no clip of their own, so the child is not clipped by
/// the main surface either.
/// `child` laid out at the window's size and painted from the window's origin, wherever it
/// is declared: an overlay positioned in window coordinates, such as a picker hung from a
/// control inside a scrolling body. Declare it outside any clipping box (a sibling of the
/// scroll, not a child), since an ancestor's clip still applies.
pub fn window_layer(child: impl IntoElement) -> AnyElement {
    WindowLayer {
        child: child.into_any_element(),
    }
    .into_any_element()
}

/// `child` in a box at `anchor`, a trigger's bounds in window coordinates, through
/// [`window_layer`]: a panel that hangs from the trigger (a [`Popover`](crate::Popover), a
/// [`Menu`](crate::Menu)) drawn outside the scrolling or clipping box that holds the trigger,
/// so it neither scrolls away under the rows after it nor is cut by that box's edge.
pub fn window_anchor(anchor: Bounds<Pixels>, child: impl IntoElement) -> AnyElement {
    window_layer(
        div().size_full().child(
            div()
                .absolute()
                .left(anchor.origin.x)
                .top(anchor.origin.y)
                .w(anchor.size.width)
                .h(anchor.size.height)
                .child(child),
        ),
    )
}

/// Where a trigger was laid out, for [`window_anchor`]: the bounds the last frame gave the
/// box an [`AnchorSlot::probe`] fills. A host builds the overlay from the last frame's bounds,
/// so when they change (the trigger first drawn, or moved by a scroll) the probe asks for one
/// more frame, which hangs the panel where the trigger is now.
#[derive(Clone, Default)]
pub struct AnchorSlot(Rc<Cell<Option<Bounds<Pixels>>>>);

impl AnchorSlot {
    /// The bounds, once a frame has laid the probe out.
    pub fn get(&self) -> Option<Bounds<Pixels>> {
        self.0.get()
    }

    /// Forgets the bounds, for a slot that moves to another trigger: its panel waits for the
    /// new one instead of showing a frame at the old.
    pub fn clear(&self) {
        self.0.set(None);
    }

    /// A probe that fills its `relative()` parent and records its bounds each frame.
    pub fn probe(&self) -> impl IntoElement {
        let slot = self.0.clone();
        canvas(
            move |bounds, window, _| {
                if slot.get() != Some(bounds) {
                    slot.set(Some(bounds));
                    // gpui ignores a refresh asked for while the window draws, so the redraw
                    // that moves the panel to the new bounds is asked for from the next frame.
                    window.on_next_frame(|window, _| window.refresh());
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .size_full()
    }
}

struct WindowLayer {
    child: AnyElement,
}

impl IntoElement for WindowLayer {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for WindowLayer {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
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
    ) -> (LayoutId, Self::RequestLayoutState) {
        let viewport = window.viewport_size();
        let mut style = Style {
            position: Position::Absolute,
            ..Default::default()
        };
        style.size.width = viewport.width.into();
        style.size.height = viewport.height.into();
        let child = self.child.request_layout(window, cx);
        (window.request_layout(style, [child], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let to_origin = point(px(0.0), px(0.0)) - bounds.origin;
        window.with_element_offset(to_origin, |window| self.child.prepaint(window, cx));
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}
