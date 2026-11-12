//! `deferred`, safe to nest. GPUI's own `deferred` panics ("cannot call defer_draw during
//! deferred drawing") when a deferred element sits inside another one's deferred draw: a
//! morph or a menu opened inside a dialog, a picker inside Commander. This one defers only
//! when nothing above it already did; inside a deferred draw it draws in place, which is
//! already above the page, in the order its parent overlay gives it.

use std::cell::Cell;

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Window,
};

thread_local! {
    /// Whether a deferred draw of ours is prepainting or painting right now.
    static DEFERRING: Cell<bool> = const { Cell::new(false) };
}

/// Runs `f` with the flag set, restoring what was there.
fn deferring<R>(f: impl FnOnce() -> R) -> R {
    let was = DEFERRING.with(|flag| flag.replace(true));
    let result = f();
    DEFERRING.with(|flag| flag.set(was));
    result
}

/// Draws `child` after the rest of the window, above it, like GPUI's `deferred`, but never
/// defers twice: inside another deferred draw it draws in place.
pub fn deferred(child: impl IntoElement) -> Deferred {
    Deferred {
        // Wrapped now, so the element the window later draws is the one laid out.
        child: Some(
            Guard {
                child: child.into_any_element(),
            }
            .into_any_element(),
        ),
        priority: 0,
    }
}

/// See [`deferred`].
pub struct Deferred {
    child: Option<AnyElement>,
    priority: usize,
}

impl Deferred {
    /// Sets the priority among deferred draws: a higher one draws later, on top.
    pub fn with_priority(mut self, priority: usize) -> Self {
        self.priority = priority;
        self
    }
}

impl IntoElement for Deferred {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Deferred {
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
        let child = self.child.as_mut().expect("deferred child");
        (child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if DEFERRING.with(Cell::get) {
            if let Some(child) = self.child.as_mut() {
                child.prepaint(window, cx);
            }
            return;
        }
        let Some(child) = self.child.take() else {
            return;
        };
        let offset = window.element_offset();
        window.defer_draw(child, offset, self.priority);
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
        // Only an in-place draw keeps its child; a deferred one handed it to the window.
        if let Some(child) = self.child.as_mut() {
            child.paint(window, cx);
        }
    }
}

/// The deferred child, drawn with the flag set so the overlays inside it draw in place.
struct Guard {
    child: AnyElement,
}

impl IntoElement for Guard {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Guard {
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
        let child = deferring(|| self.child.request_layout(window, cx));
        (child, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        deferring(|| self.child.prepaint(window, cx));
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
        deferring(|| self.child.paint(window, cx));
    }
}
