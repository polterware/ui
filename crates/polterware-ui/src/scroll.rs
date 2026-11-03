//! A vertical scroll with Chromium's classic macOS scrollbar, which the Electron app shows on
//! every `overflow: auto` box that overflows: a 12px gutter taken from the box's width at its
//! right edge (its bottom edge, sideways), holding a 6px fully rounded thumb (#373737, #8f8f8f
//! under the pointer) 3px in from the gutter's sides and from the ends of its box, which is as
//! long as the whole track times the visible share and travels the track's length (so at the
//! ends the thumb stands 3px off them). Dragging the thumb scrolls; the track has no arrows and a
//! press on it does not page. [`scrollbar`] draws one for any [`ScrollSource`]: a
//! `ScrollHandle`, or a `gpui::list`'s state through [`ListScroll`].
//!
//! [`page_content`] is `PageContent` under a `PageToolbar`: the scroll runs the page's full
//! height and the toolbar overlays its top 44px, so the first child sits 44px down.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, DispatchPhase, ElementId, Entity, IntoElement, ListState,
    MouseButton, MouseMoveEvent, MouseUpEvent, ScrollHandle, Window, canvas, div, point,
    prelude::*, px,
};

use crate::theme::Theme;

pub const GUTTER: f32 = 12.0;
const THUMB: f32 = 6.0;
/// The thumb's inset from the gutter's sides and from the track's ends.
const THUMB_INSET: f32 = 3.0;
const MIN_THUMB: f32 = 18.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAxis {
    Vertical,
    Horizontal,
}

impl ScrollAxis {
    fn of(self, point: gpui::Point<gpui::Pixels>) -> f32 {
        match self {
            ScrollAxis::Vertical => f32::from(point.y),
            ScrollAxis::Horizontal => f32::from(point.x),
        }
    }

    fn of_size(self, size: gpui::Size<gpui::Pixels>) -> f32 {
        match self {
            ScrollAxis::Vertical => f32::from(size.height),
            ScrollAxis::Horizontal => f32::from(size.width),
        }
    }
}

/// What a scrollbar reads and moves, as distances along its axis from the start.
pub trait ScrollSource: Clone + 'static {
    fn scrolled(&self, axis: ScrollAxis) -> f32;
    fn max_scroll(&self, axis: ScrollAxis) -> f32;
    fn scroll_to(&self, axis: ScrollAxis, scrolled: f32);
}

impl ScrollSource for ScrollHandle {
    fn scrolled(&self, axis: ScrollAxis) -> f32 {
        -axis.of(self.offset())
    }

    fn max_scroll(&self, axis: ScrollAxis) -> f32 {
        axis.of_size(self.max_offset())
    }

    fn scroll_to(&self, axis: ScrollAxis, scrolled: f32) {
        let current = self.offset();
        self.set_offset(match axis {
            ScrollAxis::Vertical => point(current.x, px(-scrolled)),
            ScrollAxis::Horizontal => point(px(-scrolled), current.y),
        });
    }
}

/// A vertical `gpui::list`, whose furthest scroll the caller reads with its padding, as the
/// list's own reading leaves the padding out.
#[derive(Clone)]
pub struct ListScroll {
    pub state: ListState,
    pub max: f32,
}

impl ScrollSource for ListScroll {
    fn scrolled(&self, _: ScrollAxis) -> f32 {
        -f32::from(self.state.scroll_px_offset_for_scrollbar().y)
    }

    fn max_scroll(&self, _: ScrollAxis) -> f32 {
        self.max
    }

    fn scroll_to(&self, _: ScrollAxis, scrolled: f32) {
        self.state
            .set_offset_from_scrollbar(point(px(0.0), px(scrolled)));
    }
}

/// One scrollbar's thumb: under the pointer, and the drag in progress.
pub struct ScrollbarState {
    thumb_hovered: bool,
    /// The pointer's position and the scroll offset along the axis when a drag began.
    drag: Option<(f32, f32)>,
    /// The scroll per pixel of thumb travel, from the last frame.
    ratio: Rc<Cell<f32>>,
}

impl ScrollbarState {
    fn new() -> Self {
        Self {
            thumb_hovered: false,
            drag: None,
            ratio: Rc::new(Cell::new(1.0)),
        }
    }
}

/// How long the scrollbar's track runs, and the visible and whole lengths it stands for.
#[derive(Clone, Copy, Debug)]
pub struct ScrollGeometry {
    pub track: f32,
    pub viewport: f32,
    pub content: f32,
}

/// The classic scrollbar of `handle` along `axis`, filling the 12px gutter its parent gives
/// it. The thumb's box is the visible share of the track and moves with `handle`'s offset; the
/// thumb is drawn 3px inside the box's ends.
pub fn scrollbar<S: ScrollSource>(
    id: impl Into<ElementId>,
    handle: &S,
    axis: ScrollAxis,
    geometry: ScrollGeometry,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id = id.into();
    let theme = *Theme::global(cx);
    let state = window.use_keyed_state(id.clone(), cx, |_, _: &mut Context<ScrollbarState>| {
        ScrollbarState::new()
    });
    let (hovered, dragging, ratio) = {
        let state = state.read(cx);
        (
            state.thumb_hovered,
            state.drag.is_some(),
            state.ratio.clone(),
        )
    };
    // Chromium sizes the thumb's box against the whole track and insets the drawn thumb
    // within it: 740px showing 8000px gives a 62.5px thumb, not the 67.9px of an inset track.
    let track = geometry.track.max(0.0);
    let span = if geometry.content > 0.0 {
        (track * geometry.viewport / geometry.content)
            .clamp((MIN_THUMB + 2.0 * THUMB_INSET).min(track), track)
    } else {
        track
    };
    let length = (span - 2.0 * THUMB_INSET).max(0.0);
    let travel = (track - span).max(0.0);
    let max = handle.max_scroll(axis);
    let offset = handle.scrolled(axis);
    let start = THUMB_INSET
        + if max > 0.0 {
            (offset / max).clamp(0.0, 1.0) * travel
        } else {
            0.0
        };
    ratio.set(if travel > 0.0 { max / travel } else { 0.0 });

    let hover_state = state.clone();
    let down_state = state.clone();
    let down_handle = handle.clone();
    let mut thumb = div()
        .id((id.clone(), "thumb"))
        .absolute()
        .rounded(px(THUMB / 2.0))
        .bg(if hovered || dragging {
            theme.scrollbar_thumb_hover
        } else {
            theme.scrollbar_thumb
        })
        .on_hover(move |hovered, _, cx| {
            hover_state.update(cx, |state, cx| {
                state.thumb_hovered = *hovered;
                cx.notify();
            });
        })
        .on_mouse_down(MouseButton::Left, move |event, _, cx| {
            cx.stop_propagation();
            let at = axis.of(event.position);
            let from = down_handle.scrolled(axis);
            down_state.update(cx, |state, cx| {
                state.drag = Some((at, from));
                cx.notify();
            });
        });
    thumb = match axis {
        ScrollAxis::Vertical => thumb
            .top(px(start))
            .left(px(THUMB_INSET))
            .w(px(THUMB))
            .h(px(length)),
        ScrollAxis::Horizontal => thumb
            .left(px(start))
            .top(px(THUMB_INSET))
            .h(px(THUMB))
            .w(px(length)),
    };
    let mut gutter = div().relative().size_full().child(thumb);
    if dragging {
        let move_state = state.clone();
        let up_state = state.clone();
        let move_handle = handle.clone();
        gutter = gutter.child(
            canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    window.on_mouse_event({
                        let state = move_state.clone();
                        let handle = move_handle.clone();
                        let ratio = ratio.clone();
                        move |event: &MouseMoveEvent, phase, _, cx| {
                            if phase != DispatchPhase::Bubble {
                                return;
                            }
                            let Some((from_at, from_offset)) = state.read(cx).drag else {
                                return;
                            };
                            let max = handle.max_scroll(axis);
                            let delta = (axis.of(event.position) - from_at) * ratio.get();
                            handle.scroll_to(axis, (from_offset + delta).clamp(0.0, max));
                            state.update(cx, |_, cx| cx.notify());
                        }
                    });
                    window.on_mouse_event({
                        let state = up_state.clone();
                        move |event: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                                state.update(cx, |state, cx| {
                                    state.drag = None;
                                    cx.notify();
                                });
                            }
                        }
                    });
                },
            )
            .absolute()
            .size_0(),
        );
    }
    gutter.into_any_element()
}

pub struct ScrollState {
    handle: ScrollHandle,
    /// The viewport height and the overflow the last frame laid out.
    seen: Rc<Cell<(f32, f32)>>,
}

impl ScrollState {
    fn new() -> Self {
        Self {
            handle: ScrollHandle::new(),
            seen: Rc::new(Cell::new((0.0, 0.0))),
        }
    }

    pub fn handle(&self) -> &ScrollHandle {
        &self.handle
    }

    /// The visible height, as the last frame laid it out.
    pub fn viewport(&self) -> f32 {
        f32::from(self.handle.bounds().size.height)
    }

    fn overflows(&self) -> bool {
        f32::from(self.handle.max_offset().height) > 0.5
    }
}

pub fn use_scroll(
    id: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<ScrollState> {
    window.use_keyed_state(id, cx, |_, _: &mut Context<ScrollState>| ScrollState::new())
}

/// The vertical scroll `id` around `content`, with the classic scrollbar once it overflows.
pub fn scroll_area(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id = id.into();
    let state = use_scroll(id.clone(), window, cx);
    let (handle, overflows, seen) = {
        let state = state.read(cx);
        (state.handle.clone(), state.overflows(), state.seen.clone())
    };
    let scroller = div()
        .id(id.clone())
        .size_full()
        .overflow_y_scroll()
        .track_scroll(&handle)
        .child(
            div()
                .w_full()
                .when(overflows, |content| content.pr(px(GUTTER)))
                .child(content),
        );
    // The gutter and `page_inner_height` read the layout of the frame before, so a new size
    // or overflow draws again once it is known.
    let watch = handle.clone();
    let watcher = canvas(
        move |_, window, _| {
            let now = (
                f32::from(watch.bounds().size.height),
                f32::from(watch.max_offset().height),
            );
            let last = seen.get();
            if (now.0 - last.0).abs() > 0.5 || (now.1 > 0.5) != (last.1 > 0.5) {
                seen.set(now);
                window.refresh();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_0();
    let area = div()
        .relative()
        .size_full()
        .min_h(px(0.0))
        .child(scroller)
        .child(watcher);
    if !overflows {
        return area.into_any_element();
    }
    let viewport = f32::from(handle.bounds().size.height);
    let max = f32::from(handle.max_offset().height);
    let bar = scrollbar(
        (id.clone(), "bar"),
        &handle,
        ScrollAxis::Vertical,
        ScrollGeometry {
            track: viewport,
            viewport,
            content: viewport + max,
        },
        window,
        cx,
    );
    area.child(
        div()
            .absolute()
            .top(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .w(px(GUTTER))
            .child(bar),
    )
    .into_any_element()
}

/// The height `PageContent` gives its children, inside `padding` and under the 44px toolbar,
/// as the last frame laid it out: `calc(100% - 44px)` of the padded box.
pub fn page_inner_height(
    id: impl Into<ElementId>,
    padding: f32,
    window: &mut Window,
    cx: &mut App,
) -> f32 {
    let theme = Theme::global(cx);
    let toolbar = theme.page_header_height;
    let viewport = use_scroll(id, window, cx).read(cx).viewport();
    (viewport - 2.0 * padding - toolbar).max(0.0)
}

/// `PageContent` (`overflow-auto`, padded) with its `PageToolbar` over the top.
pub fn page_content(
    id: impl Into<ElementId>,
    toolbar: impl IntoElement,
    padding: f32,
    content: impl IntoElement,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = Theme::global(cx);
    let top = theme.page_header_height + padding;
    let body = div()
        .w_full()
        .pt(px(top))
        .px(px(padding))
        .pb(px(padding))
        .child(content);
    div()
        .relative()
        .flex_1()
        .min_h(px(0.0))
        .w_full()
        .child(scroll_area(id, body, window, cx))
        .child(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .child(toolbar),
        )
        .into_any_element()
}

/// `PageContent` whose content fills it, as a page with a scroller of its own lays it out
/// (`flex min-h-0 flex-1 flex-col`): the box does not scroll, its toolbar overlays the top
/// 44px, and `content` gets the padded height below it.
pub fn page_fill(
    toolbar: impl IntoElement,
    padding: f32,
    content: impl IntoElement,
    cx: &App,
) -> AnyElement {
    page_fill_bar(toolbar, None, padding, content, cx)
}

/// [`page_fill`] with the `ActiveFilterBar` between the toolbar and the page's content.
pub fn page_fill_bar(
    toolbar: impl IntoElement,
    bar: Option<AnyElement>,
    padding: f32,
    content: impl IntoElement,
    cx: &App,
) -> AnyElement {
    let theme = Theme::global(cx);
    div()
        .relative()
        .flex_1()
        .min_h(px(0.0))
        .w_full()
        .child(
            div()
                .size_full()
                .flex()
                .flex_col()
                .pt(px(theme.page_header_height))
                .children(bar)
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.0))
                        .flex()
                        .flex_col()
                        .p(px(padding))
                        .child(content),
                ),
        )
        .child(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .child(toolbar),
        )
        .into_any_element()
}
