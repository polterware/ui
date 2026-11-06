//! A vertical scroll with Chromium's classic macOS scrollbar, which the Electron app shows on
//! every `overflow: auto` box that overflows: a 12px gutter taken from the box's width at its
//! right edge (its bottom edge, sideways), holding a 6px fully rounded thumb (#373737, #8f8f8f
//! under the pointer) 3px in from the gutter's sides and from the ends of its box, which is as
//! long as the whole track times the visible share and travels the track's length (so at the
//! ends the thumb stands 3px off them). When the system draws overlay scrollers (a trackpad
//! and "Show scroll bars: automatically"), as Chromium then does, the gutter takes no room
//! ([`reserved_gutter`] is 0): the thumb floats over the content's edge only while it scrolls,
//! under the pointer, or dragged, and fades out a second after. Dragging the thumb scrolls; the track has no arrows and a
//! press on it does not page. [`scrollbar`] draws one for any [`ScrollSource`]: a
//! `ScrollHandle`, or a `gpui::list`'s state through [`ListScroll`].
//!
//! [`page_content`] is `PageContent` under a `PageToolbar`: the scroll runs the page's full
//! height and the toolbar overlays its top 44px, so the first child sits 44px down. What
//! scrolls up goes soft under the toolbar's trail, and the end fades into the surface while
//! there is more below. [`page_under`] does the same for a page whose content scrolls itself
//! (a `DataTable`), with the `ActiveFilterBar` floating under the toolbar.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, DispatchPhase, ElementId, Entity, IntoElement, ListState,
    MouseButton, MouseMoveEvent, MouseUpEvent, Rgba, ScrollHandle, Window, canvas, div, point,
    prelude::*, px,
};

use crate::patterns::scroll_fade::{SCROLL_FADE, end_fade, toolbar_trail};
use crate::theme::Theme;

pub const GUTTER: f32 = 12.0;
/// How long an overlay thumb stays after the last scroll, then how long it fades.
const OVERLAY_HOLD: f32 = 1.0;
const OVERLAY_FADE: f32 = 0.3;

/// The width a scroll box gives up to its scrollbar: the 12px gutter, or none while the
/// system's scrollers overlay the content.
pub fn reserved_gutter() -> f32 {
    if crate::window::scroller::overlay_scrollers() {
        0.0
    } else {
        GUTTER
    }
}
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
    /// An overlay thumb's last offset and when it last changed.
    seen_offset: Option<f32>,
    moved_at: Option<std::time::Instant>,
}

impl ScrollbarState {
    fn new() -> Self {
        Self {
            thumb_hovered: false,
            drag: None,
            ratio: Rc::new(Cell::new(1.0)),
            seen_offset: None,
            moved_at: None,
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
    // An overlay thumb shows while the offset moves, and stays while it is under the pointer
    // or dragged, then a second more; at rest it is gone and takes no events.
    let shown = if crate::window::scroller::overlay_scrollers() {
        let since = state.update(cx, |state, _| {
            let moved = state
                .seen_offset
                .is_some_and(|seen| (seen - offset).abs() > 0.5);
            state.seen_offset = Some(offset);
            if moved || hovered || dragging {
                state.moved_at = Some(std::time::Instant::now());
            }
            state.moved_at.map(|at| at.elapsed().as_secs_f32())
        });
        let shown = match since {
            Some(since) if since < OVERLAY_HOLD => 1.0,
            Some(since) => (1.0 - (since - OVERLAY_HOLD) / OVERLAY_FADE).clamp(0.0, 1.0),
            None => 0.0,
        };
        if shown <= 0.0 {
            return div().into_any_element();
        }
        window.request_animation_frame();
        shown
    } else {
        1.0
    };

    let hover_state = state.clone();
    let down_state = state.clone();
    let down_handle = handle.clone();
    let mut thumb = div()
        .id((id.clone(), "thumb"))
        .absolute()
        .opacity(shown)
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
    scroll_area_below(id, content, 0.0, None, false, window, cx)
}

/// `markdown-editor.tsx`'s frozen ancestor scroll: preserve its offset and gutter while
/// hiding overflow and disabling its thumb. The overlay's own list keeps scrolling.
pub fn scroll_area_locked(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    locked: bool,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    scroll_area_below(id, content, 0.0, None, locked, window, cx)
}

/// A `white-space: pre; overflow-x: auto` viewport, as Markdown code in
/// `markdown-editor.css`: hard lines retain their width and the shared scrollbar moves them.
/// `content` supplies its natural width; `handle` also lets an editor reveal its caret.
pub fn horizontal_scroll_area(
    id: impl Into<ElementId>,
    handle: &ScrollHandle,
    content: impl IntoElement,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id = id.into();
    let viewport = f32::from(handle.bounds().size.width);
    let max = f32::from(handle.max_offset().width);
    let mut scroller = div()
        .id(id.clone())
        .w_full()
        .min_w(px(0.0))
        .flex()
        .overflow_x_scroll()
        .track_scroll(handle)
        .child(content);
    scroller.style().restrict_scroll_to_axis = Some(true);
    let watch = handle.clone();
    let view = window.current_view();
    let watcher = canvas(
        move |_, _, cx| {
            if (f32::from(watch.bounds().size.width) - viewport).abs() > 0.5
                || (f32::from(watch.max_offset().width) - max).abs() > 0.5
            {
                cx.notify(view);
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_0();
    let mut area = div()
        .relative()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .child(scroller)
        .child(watcher);
    if max > 0.5 {
        let bar = scrollbar(
            (id, "bar"),
            handle,
            ScrollAxis::Horizontal,
            ScrollGeometry {
                track: viewport,
                viewport,
                content: viewport + max,
            },
            window,
            cx,
        );
        area = area.child(
            div()
                .relative()
                .w_full()
                .h(px(reserved_gutter()))
                .flex_none()
                .child(
                    div()
                        .absolute()
                        .bottom_0()
                        .w_full()
                        .h(px(GUTTER))
                        .child(bar),
                ),
        );
    }
    area.into_any_element()
}

/// [`scroll_area`] for [`page_under`]'s content: `top` kept clear above `content` at rest, the
/// scrollbar starting there, under the chrome, and the end fading into the page's surface
/// while there is more below.
pub fn scroll_area_under(
    id: impl Into<ElementId>,
    top: f32,
    content: impl IntoElement,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let surface = Theme::global(cx).color_surface;
    let body = div().w_full().pt(px(top)).child(content);
    scroll_area_below(id, body, top, Some(surface), false, window, cx)
}

/// [`scroll_area`] whose scrollbar starts `top` down, under what floats over the area's top
/// (a page's toolbar), so the bar and the toolbar's controls never meet. With `fade`, the end
/// fades into that color, over as far as there is left to scroll, up to 24px.
#[allow(clippy::too_many_arguments)]
fn scroll_area_below(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    top: f32,
    fade: Option<Rgba>,
    locked: bool,
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
        .map(|scroller| {
            if locked {
                scroller.overflow_y_hidden()
            } else {
                scroller.overflow_y_scroll()
            }
        })
        .track_scroll(&handle)
        .child(
            div()
                .w_full()
                .when(overflows, |content| content.pr(px(reserved_gutter())))
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
    let mut area = div()
        .relative()
        .size_full()
        .min_h(px(0.0))
        .child(scroller)
        .child(watcher);
    if !overflows || locked {
        return area.into_any_element();
    }
    let viewport = f32::from(handle.bounds().size.height);
    let max = f32::from(handle.max_offset().height);
    if let Some(color) = fade {
        let offset = (-f32::from(handle.offset().y)).clamp(0.0, max);
        let left = (max - offset).clamp(0.0, SCROLL_FADE);
        if left > 0.5 {
            area = area.child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .right(px(reserved_gutter()))
                    .bottom(px(0.0))
                    .child(end_fade(color, left, false)),
            );
        }
        // The fade reads the offset this frame rendered with: a scroll since draws again (a
        // cached view draws only when notified).
        let watch = handle.clone();
        let view = window.current_view();
        let rendered = f32::from(handle.offset().y);
        area = area.child(
            canvas(
                move |_, _, cx| {
                    if (f32::from(watch.offset().y) - rendered).abs() > 0.5 {
                        cx.notify(view);
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_0(),
        );
    }
    let bar = scrollbar(
        (id.clone(), "bar"),
        &handle,
        ScrollAxis::Vertical,
        ScrollGeometry {
            track: (viewport - top).max(0.0),
            viewport,
            content: viewport + max,
        },
        window,
        cx,
    );
    area.child(
        div()
            .absolute()
            .top(px(top))
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
    let theme = *Theme::global(cx);
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
        // The bar starts under the toolbar, so the toolbar spans the page's full width and
        // its end sits as far in as its start.
        .child(scroll_area_below(
            id,
            body,
            theme.page_header_height,
            Some(theme.color_surface),
            false,
            window,
            cx,
        ))
        .child(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                // What scrolls up goes soft under the toolbar instead of being cut.
                .child(crate::patterns::trailing_toolbar(
                    toolbar,
                    theme.color_surface,
                    theme.page_header_height,
                    theme.radius_surface,
                )),
        )
        .into_any_element()
}

/// The room [`page_under`]'s chrome takes from the top of the page `id`, plus `padding`: its
/// content keeps it clear at rest. `bar` says whether the `ActiveFilterBar` shows, whose
/// height is the last frame's.
pub fn page_under_inset(
    id: impl Into<ElementId>,
    bar: bool,
    padding: f32,
    window: &mut Window,
    cx: &mut App,
) -> f32 {
    let measured = chrome_state(id, window, cx);
    chrome_height(&measured, bar, cx) + padding
}

fn chrome_state(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Entity<f32> {
    let id: ElementId = id.into();
    window.use_keyed_state(
        ElementId::NamedChild(Box::new(id), "chrome".into()),
        cx,
        |_, _: &mut Context<f32>| 0.0_f32,
    )
}

/// The chrome's height as the last frame laid it out: the toolbar's alone until then.
fn chrome_height(measured: &Entity<f32>, bar: bool, cx: &App) -> f32 {
    let toolbar = Theme::global(cx).page_header_height;
    if bar {
        measured.read(cx).max(toolbar)
    } else {
        toolbar
    }
}

/// `PageContent` whose content scrolls itself and runs up under the chrome, as the run page's
/// transcript runs under its header: the `PageToolbar`, and the `ActiveFilterBar` under it,
/// float over the top on the toolbar's trail
/// ([`trailing_toolbar`](crate::patterns::trailing_toolbar)), so what scrolls up goes soft
/// beneath them instead of meeting a cut. `content` keeps [`page_under_inset`] clear at its
/// top at rest: hand that to [`DataTable::inset_top`](crate::DataTable::inset_top) (with
/// [`DataTable::fades`](crate::DataTable::fades) for the bottom) or [`scroll_area_under`].
/// The content fills the page inside `padding` at the sides and the bottom.
pub fn page_under(
    id: impl Into<ElementId>,
    toolbar: impl IntoElement,
    bar: Option<AnyElement>,
    padding: f32,
    content: impl IntoElement,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = *Theme::global(cx);
    let has_bar = bar.is_some();
    let measured = chrome_state(id, window, cx);
    let chrome = chrome_height(&measured, has_bar, cx);
    let view = window.current_view();
    let probe = {
        let measured = measured.clone();
        canvas(
            move |bounds, _, cx| {
                let height = f32::from(bounds.size.height);
                let changed = measured.update(cx, |seen, _| {
                    let changed = (*seen - height).abs() > 0.5;
                    *seen = height;
                    changed
                });
                if changed && has_bar {
                    cx.notify(view);
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
    };
    let body = div()
        .absolute()
        .inset_0()
        .px(px(padding))
        .pb(px(padding))
        .flex()
        .flex_col()
        .child(content);
    // The trail stays the surface's radius in from each side, clear of its round corners.
    let chrome = div()
        .relative()
        .w_full()
        .child(
            div()
                .absolute()
                .top_0()
                .left(px(theme.radius_surface))
                .right(px(theme.radius_surface))
                .child(toolbar_trail(theme.color_surface, chrome)),
        )
        .child(
            div()
                .relative()
                .w_full()
                .flex()
                .flex_col()
                .child(toolbar)
                .children(bar)
                .child(probe),
        );
    div()
        .relative()
        .flex_1()
        .min_h(px(0.0))
        .w_full()
        .child(body)
        .child(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .child(chrome),
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

#[cfg(test)]
mod lock_tests {
    use super::*;
    use crate::test_support::{draw, open};
    use gpui::{Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase, size};

    #[gpui::test]
    fn locking_preserves_offset_and_gutter_and_unlocking_restores_wheel(cx: &mut TestAppContext) {
        let locked = Rc::new(Cell::new(false));
        let flag = locked.clone();
        let captured = Rc::new(std::cell::RefCell::new(None));
        let capture = captured.clone();
        let mut cx = open(cx, size(300.0, 200.0), move |window, cx| {
            *capture.borrow_mut() = Some(
                use_scroll("locked-scroll", window, cx)
                    .read(cx)
                    .handle()
                    .clone(),
            );
            scroll_area_locked(
                "locked-scroll",
                div().h(px(1000.0)).child(
                    div()
                        .debug_selector(|| "scroll-content".into())
                        .w_full()
                        .h(px(30.0)),
                ),
                flag.get(),
                window,
                cx,
            )
        });
        let handle = captured.borrow().clone().unwrap();
        handle.set_offset(point(px(0.0), px(-100.0)));
        draw(&mut cx);
        let width = cx.debug_bounds("scroll-content").unwrap().size.width;
        locked.set(true);
        draw(&mut cx);
        assert_eq!(handle.offset().y, px(-100.0));
        assert_eq!(cx.debug_bounds("scroll-content").unwrap().size.width, width);
        let wheel = || ScrollWheelEvent {
            position: point(px(100.0), px(100.0)),
            delta: ScrollDelta::Pixels(point(px(0.0), px(-60.0))),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
        };
        cx.simulate_event(wheel());
        draw(&mut cx);
        assert_eq!(handle.offset().y, px(-100.0));
        locked.set(false);
        draw(&mut cx);
        cx.simulate_event(wheel());
        draw(&mut cx);
        assert!(handle.offset().y < px(-100.0));
    }
}
