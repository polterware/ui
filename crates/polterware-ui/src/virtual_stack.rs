//! A vertical `gpui::list` with the chrome of [`scroll_area_under`](crate::scroll::scroll_area_under).
//!
//! Callers flatten a grouped stack into items (a collapsible header, then its cards) and
//! keep a [`ListState`] on the view. Only the items in view, plus the list's overdraw, are
//! built each frame. `top` rides on the first item, so at rest the stack starts below a
//! floating toolbar and can scroll up under it; the scrollbar starts there, and the end
//! fades into the page surface while more remains below.

use gpui::{
    AnyElement, App, ElementId, IntoElement, ListOffset, ListState, ScrollWheelEvent, Window,
    canvas, div, list, prelude::*, px,
};

use crate::patterns::scroll_fade::{FadeEdges, SCROLL_FADE, end_fade, list_item_alpha};
use crate::scroll::{GUTTER, ListScroll, ScrollAxis, ScrollGeometry, reserved_gutter, scrollbar};
use crate::theme::Theme;

/// The list's overdraw: a couple of tall cards past the viewport, so a fast scroll still
/// finds the next one measured.
pub const STACK_OVERDRAW: f32 = 480.0;

/// Bring `list` to `count` items.
///
/// A new count, or `remeasure`, replaces the items so cached heights do not stay stuck to
/// the old indexes, then measures them once on the next layout. The scrollbar spans the
/// whole stack, and the scroll stays on the same item when that index still exists.
/// `remeasure` is for a new row structure or a width change: gpui drops cached heights when
/// the width changes, and without a full measure only the rows in view would get a height,
/// so the scrollbar would shrink to them.
pub fn sync_virtual_list(list: &ListState, count: usize, remeasure: bool) {
    let old = list.item_count();
    if old != count || remeasure {
        let anchor = list.logical_scroll_top();
        list.splice(0..old, count);
        let item_ix = if count == 0 {
            0
        } else {
            anchor.item_ix.min(count - 1)
        };
        let offset_in_item = if item_ix == anchor.item_ix {
            anchor.offset_in_item
        } else {
            px(0.0)
        };
        list.scroll_to(ListOffset {
            item_ix,
            offset_in_item,
        });
    }
    if remeasure || old != count {
        let _ = list.clone().measure_all();
    }
}

/// The stack of `list`, filling its parent. `render_item` builds item `index`. The first
/// item keeps `top` clear and the last keeps `bottom`, both scrolling with the items.
#[allow(clippy::too_many_arguments)]
pub fn virtual_stack(
    id: impl Into<ElementId>,
    list_state: &ListState,
    top: f32,
    bottom: f32,
    render_item: impl Fn(usize, &mut Window, &mut App) -> AnyElement + 'static,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id = id.into();
    let list_state = list_state.clone();
    let width = f32::from(list_state.viewport_bounds().size.width);
    let seen_width = window.use_keyed_state((id.clone(), "width"), cx, |_, _| 0.0_f32);
    let last_width = *seen_width.read(cx);
    if width > 1.0 && (width - last_width).abs() > 0.5 {
        seen_width.update(cx, |seen, _| *seen = width);
        let _ = list_state.clone().measure_all();
    }

    let extent = stack_extent(&list_state);
    let overflows = extent.is_some_and(|(content, viewport, _)| content > viewport + 0.5);
    let gutter = if overflows { reserved_gutter() } else { 0.0 };
    let count = list_state.item_count();
    let state = list_state.clone();
    let mut lines = list(state, move |index, window, cx| {
        let item = render_item(index, window, cx);
        div()
            .w_full()
            .when(index == 0 && top > 0.0, |row| row.pt(px(top)))
            .when(count > 0 && index + 1 == count && bottom > 0.0, |row| {
                row.pb(px(bottom))
            })
            .child(item)
            .into_any_element()
    })
    .size_full();
    lines.style().restrict_scroll_to_axis = Some(true);

    let (content, viewport, scrolled) = extent.unwrap_or((0.0, 0.0, 0.0));
    let surface = Theme::global(cx).color_surface;
    let watched = list_state.clone();
    let view = window.current_view();
    // The fade was drawn for `scrolled`. A scroll since this frame draws again.
    let watcher = canvas(
        move |_, window, cx| {
            let Some((_, _, at)) = stack_extent(&watched) else {
                return;
            };
            if (at - scrolled).abs() > 0.5 {
                cx.notify(view);
                crate::frame::draw_again(window);
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_0();

    let mut area = div()
        .id(id.clone())
        .relative()
        .size_full()
        .min_h(px(0.0))
        .child(
            div()
                .size_full()
                .min_h(px(0.0))
                .when(gutter > 0.0, |slot| slot.pr(px(gutter)))
                .child(lines),
        )
        .child(watcher);

    let left = (content - viewport - scrolled).max(0.0);
    let fade = left.clamp(0.0, SCROLL_FADE);
    if overflows && fade > 0.5 {
        area = area.child(
            div()
                .absolute()
                .left(px(0.0))
                .right(px(gutter))
                .bottom(px(0.0))
                .child(end_fade(surface, fade, false)),
        );
    }
    if overflows {
        let bar = scrollbar(
            (id.clone(), "bar"),
            &ListScroll {
                state: list_state.clone(),
                max: (content - viewport).max(0.0),
            },
            ScrollAxis::Vertical,
            ScrollGeometry {
                track: (viewport - top).max(0.0),
                viewport,
                content,
            },
            window,
            cx,
        );
        let wheeled = list_state.clone();
        let view = window.current_view();
        area = area.child(
            div()
                .id((id, "gutter"))
                .absolute()
                .top(px(top))
                .right(px(0.0))
                .bottom(px(0.0))
                .w(px(GUTTER))
                .on_scroll_wheel(move |event: &ScrollWheelEvent, window, cx| {
                    let delta = event.delta.pixel_delta(window.line_height());
                    wheeled.scroll_by(-delta.y);
                    cx.notify(view);
                })
                .child(bar),
        );
    }
    area.into_any_element()
}

/// A [`ListState`] stack over a translucent surface (the sidebar's glass over the window
/// material). It looks as `.pw-scroll-fade` with its mask read a row at a time
/// ([`fade_item`](crate::patterns::fade_item)): the Web CSS fades a scroll's ends by masking
/// them, which GPUI cannot do over a surface with no one color, so each item fades itself as
/// it nears an end, and the list draws no band or scrollbar gutter. It behaves as gpui's
/// `list` (gpui-kit's `VirtualList`): only the items in view are built. The first item keeps
/// `top` clear and the last `bottom`, both scrolling with the items.
#[allow(clippy::too_many_arguments)]
pub fn virtual_fade_list(
    id: impl Into<ElementId>,
    list_state: &ListState,
    top: f32,
    bottom: f32,
    edges: FadeEdges,
    render_item: impl Fn(usize, &mut Window, &mut App) -> AnyElement + 'static,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id = id.into();
    let state = list_state.clone();
    let width = f32::from(state.viewport_bounds().size.width);
    let seen_width = window.use_keyed_state((id.clone(), "width"), cx, |_, _| 0.0_f32);
    let last_width = *seen_width.read(cx);
    if width > 1.0 && (width - last_width).abs() > 0.5 {
        seen_width.update(cx, |seen, _| *seen = width);
        let _ = state.clone().measure_all();
    }

    let count = state.item_count();
    // Read before the list borrows its state to lay the items out: where each item in view
    // sits for the scroll this frame holds. Items not measured yet show whole, and the ones
    // past the viewport are never drawn, so only those up to its end are asked for.
    let mut alphas = vec![1.0_f32; count];
    let viewport = state.viewport_bounds();
    let mut unmeasured = false;
    if f32::from(viewport.size.height) > 1.0 {
        let first = state.logical_scroll_top().item_ix.min(count);
        for index in first..count {
            let Some(bounds) = state.bounds_for_item(index) else {
                unmeasured = true;
                break;
            };
            // The stack's first and last keep its chrome inside them.
            let inset_top = if index == 0 { top } else { 0.0 };
            let inset_bottom = if index + 1 == count { bottom } else { 0.0 };
            alphas[index] = list_item_alpha(&state, index, edges, inset_top, inset_bottom);
            if bounds.top() >= viewport.bottom() {
                break;
            }
        }
    }
    if unmeasured {
        // A splice leaves its items unmeasured until the layout that follows this read, so
        // the fade just drawn is one frame behind: draw again where they now sit.
        crate::frame::draw_again(window);
    }
    let mut lines = list(state.clone(), move |index, window, cx| {
        let item = render_item(index, window, cx);
        let alpha = alphas.get(index).copied().unwrap_or(1.0);
        div()
            .w_full()
            .when(index == 0 && top > 0.0, |row| row.pt(px(top)))
            .when(count > 0 && index + 1 == count && bottom > 0.0, |row| {
                row.pb(px(bottom))
            })
            .opacity(alpha)
            .child(item)
            .into_any_element()
    })
    .size_full();
    lines.style().restrict_scroll_to_axis = Some(true);

    // The fade was drawn for the scroll this frame read: draw again once it moves.
    let watched = state.clone();
    let seen = window.use_keyed_state((id.clone(), "fade"), cx, |_, _| 0.0_f32);
    let view = window.current_view();
    let watcher = canvas(
        move |_, window, cx| {
            let now = list_scrolled(&watched);
            if (now - *seen.read(cx)).abs() > 0.5 {
                seen.update(cx, |seen, _| *seen = now);
                cx.notify(view);
                crate::frame::draw_again(window);
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_0();

    div()
        .id(id)
        .relative()
        .flex_1()
        .w_full()
        .min_h(px(0.0))
        .child(lines)
        .child(watcher)
        .into_any_element()
}

/// How far a list has scrolled from its top, as the fade reads it.
fn list_scrolled(list: &ListState) -> f32 {
    (-f32::from(list.scroll_px_offset_for_scrollbar().y)).max(0.0)
}

/// Content height, viewport height, and how far the list has scrolled, from the last layout.
fn stack_extent(list: &ListState) -> Option<(f32, f32, f32)> {
    let bounds = list.viewport_bounds();
    let viewport = f32::from(bounds.size.height);
    let count = list.item_count();
    if viewport <= 0.0 || count == 0 {
        return None;
    }
    let scrolled = -f32::from(list.scroll_px_offset_for_scrollbar().y);
    let content = match list.bounds_for_item(count - 1) {
        Some(last) => f32::from(last.bottom() - bounds.top()) + scrolled,
        None => f32::from(list.max_offset_for_scrollbar().height) + viewport,
    };
    Some((content, viewport, scrolled))
}

#[cfg(test)]
mod tests {
    use super::sync_virtual_list;
    use gpui::{ListAlignment, ListState, px};

    #[test]
    fn sync_keeps_the_item_the_scroll_was_on() {
        let list = ListState::new(10, ListAlignment::Top, px(20.0));
        list.scroll_to(gpui::ListOffset {
            item_ix: 4,
            offset_in_item: px(3.0),
        });
        sync_virtual_list(&list, 12, false);
        let top = list.logical_scroll_top();
        assert_eq!(list.item_count(), 12);
        assert_eq!(top.item_ix, 4);
        assert_eq!(f32::from(top.offset_in_item), 3.0);
    }

    #[test]
    fn sync_remeasure_keeps_the_item_when_the_count_stays() {
        let list = ListState::new(10, ListAlignment::Top, px(20.0));
        list.scroll_to(gpui::ListOffset {
            item_ix: 4,
            offset_in_item: px(3.0),
        });
        sync_virtual_list(&list, 10, true);
        let top = list.logical_scroll_top();
        assert_eq!(list.item_count(), 10);
        assert_eq!(top.item_ix, 4);
        assert_eq!(f32::from(top.offset_in_item), 3.0);
    }

    #[test]
    fn sync_clamps_the_scroll_when_the_list_shrinks_past_it() {
        let list = ListState::new(10, ListAlignment::Top, px(20.0));
        list.scroll_to(gpui::ListOffset {
            item_ix: 8,
            offset_in_item: px(4.0),
        });
        sync_virtual_list(&list, 3, false);
        let top = list.logical_scroll_top();
        assert_eq!(list.item_count(), 3);
        assert_eq!(top.item_ix, 2);
        assert_eq!(f32::from(top.offset_in_item), 0.0);
    }
}
