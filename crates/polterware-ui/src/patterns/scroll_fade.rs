//! Scroll fades: a list that fades out at an end that has more to show, and the fade that
//! trails a toolbar over the content scrolling under it.
//!
//! The web design system masks the scroller (`.pw-scroll-fade`): each end fades over the
//! first 24px of scroll away from it (`--scroll-fade-size`), so an end at rest is crisp.
//! GPUI cannot mask, so [`fade_scroll`] lays a gradient of the surface's color over each end
//! instead, as tall as the scroll has moved away from that end, up to 24px. A translucent
//! surface (the sidebar's glass over the window material) has no one color to fade into, so
//! there the scroll draws no bands ([`FadeScroll::bands`] off) and each row fades itself
//! ([`fade_item`]): its opacity follows where it sits in the fading stretch, which reads as
//! the mask does, a row at a time.
//!
//! [`toolbar_trail`] is `--material-fade-edge-toolbar-trail`: the surface at the toolbar's
//! edge, 52% of it at 30%, 18% at 65%, and nothing at the end, laid under a toolbar that
//! floats over its page so the content goes soft as it passes beneath.

use gpui::{
    AnyElement, App, Bounds, Div, ElementId, IntoElement, ParentElement, Pixels, RenderOnce, Rgba,
    ScrollHandle, SharedString, Window, canvas, div, linear_color_stop, linear_gradient,
    prelude::*, px,
};

/// `--scroll-fade-size`.
pub const SCROLL_FADE: f32 = 24.0;

fn with_alpha(color: Rgba, alpha: f32) -> Rgba {
    Rgba {
        a: color.a * alpha,
        ..color
    }
}

/// A `height` band from `color` at `from` of its alpha to `to`, top to bottom.
fn band(color: Rgba, from: f32, to: f32, height: f32) -> Div {
    div().w_full().h(px(height)).flex_none().bg(linear_gradient(
        180.0,
        linear_color_stop(with_alpha(color, from), 0.0),
        linear_color_stop(with_alpha(color, to), 1.0),
    ))
}

/// The fade at one end of a scroll, `height` tall: clear into `color` at the edge, the top's
/// running down from it and the bottom's up to it. Lay it over the end, absolutely.
pub(crate) fn end_fade(color: Rgba, height: f32, at_top: bool) -> Div {
    if at_top {
        band(color, 1.0, 0.0, height)
    } else {
        band(color, 0.0, 1.0, height)
    }
}

/// `--material-fade-edge-toolbar-trail`, `height` tall: `color` at the top, clear at the
/// bottom. Position it under the toolbar.
pub fn toolbar_trail(color: Rgba, height: f32) -> Div {
    div()
        .w_full()
        .h(px(height))
        .flex()
        .flex_col()
        .child(band(color, 1.0, 0.52, height * 0.30))
        .child(band(color, 0.52, 0.18, height * 0.35))
        .child(band(color, 0.18, 0.0, height * 0.35))
}

/// `toolbar` floating over the content that scrolls under it, on a [`toolbar_trail`] as
/// tall as the toolbar: the surface only at its very top, so what passes beneath goes soft
/// instead of meeting a band and a cut. The trail stays `inset` in from each side: GPUI
/// clips to rectangles, so a trail across a rounded surface's top would paint its square
/// corners over the round ones. Pass the surface's radius; nothing scrolls that close to
/// the sides.
pub fn trailing_toolbar(toolbar: impl IntoElement, color: Rgba, height: f32, inset: f32) -> Div {
    div()
        .relative()
        .w_full()
        .child(
            div()
                .absolute()
                .top_0()
                .left(px(inset))
                .right(px(inset))
                .child(toolbar_trail(color, height)),
        )
        .child(toolbar)
}

/// A vertical scroll over `handle` that fades into `color` at each end with more to show.
pub fn fade_scroll(id: impl Into<SharedString>, handle: &ScrollHandle, color: Rgba) -> FadeScroll {
    FadeScroll {
        id: id.into(),
        handle: handle.clone(),
        color,
        size: SCROLL_FADE,
        inset_x: 0.0,
        bands: true,
        content: Vec::new(),
        style: div(),
    }
}

#[derive(IntoElement)]
pub struct FadeScroll {
    id: SharedString,
    handle: ScrollHandle,
    color: Rgba,
    size: f32,
    inset_x: f32,
    bands: bool,
    content: Vec<AnyElement>,
    /// The scrolling column, as the caller styles it (gap, padding).
    style: Div,
}

impl FadeScroll {
    /// How tall a fade grows. Default: 24px.
    pub fn fade_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Keeps the fades `inset` in from both sides, such as inside a rounded frame.
    pub fn inset_x(mut self, inset: f32) -> Self {
        self.inset_x = inset;
        self
    }

    /// Off over a translucent surface: no bands, the rows fade themselves ([`fade_item`]).
    pub fn bands(mut self, bands: bool) -> Self {
        self.bands = bands;
        self
    }

    /// Styles the column the content scrolls in (its gap and padding).
    pub fn column(mut self, style: impl FnOnce(Div) -> Div) -> Self {
        self.style = style(self.style);
        self
    }
}

impl ParentElement for FadeScroll {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.content.extend(elements);
    }
}

/// How far `handle` has scrolled from its top, as layout will place it. A wheel adds its
/// delta to the offset unclamped and layout clamps it into `0..=max` only when it lays the
/// scroll out, so a fast scroll past an end reads, until then, as further than the rows will
/// ever go: everything that follows the scroll reads it through here.
fn scrolled(handle: &ScrollHandle) -> (f32, f32) {
    let max = f32::from(handle.max_offset().height).max(0.0);
    let offset = (-f32::from(handle.offset().y)).clamp(0.0, max);
    (offset, max)
}

impl RenderOnce for FadeScroll {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (offset, max) = scrolled(&self.handle);
        let top = offset.clamp(0.0, self.size);
        let bottom = (max - offset).clamp(0.0, self.size);
        // The fades read the scroll of the frame before: draw again once it moves.
        let seen = window.use_keyed_state(
            ElementId::Name(format!("{}-fade", self.id).into()),
            cx,
            |_, _| (0.0_f32, 0.0_f32),
        );
        let watch = self.handle.clone();
        // A cached view draws again only when notified, so the watcher notifies it.
        let view = window.current_view();
        let watcher = canvas(
            move |_, _, cx| {
                let now = scrolled(&watch);
                let last = *seen.read(cx);
                if (now.0 - last.0).abs() > 0.5 || (now.1 - last.1).abs() > 0.5 {
                    seen.update(cx, |seen, _| *seen = now);
                    cx.notify(view);
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_0();
        let edge = |height: f32, at_top: bool| {
            let fade = end_fade(self.color, height, at_top);
            div()
                .absolute()
                .left(px(self.inset_x))
                .right(px(self.inset_x))
                .h(px(height))
                .map(|edge| {
                    if at_top {
                        edge.top_0()
                    } else {
                        edge.bottom_0()
                    }
                })
                .child(fade)
        };
        let column = self
            .style
            .id(ElementId::Name(self.id.clone()))
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.handle)
            .children(self.content);
        div()
            .relative()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .child(column)
            .when(self.bands && top > 0.5, |area| area.child(edge(top, true)))
            .when(self.bands && bottom > 0.5, |area| {
                area.child(edge(bottom, false))
            })
            .child(watcher)
    }
}

/// Where an item was laid out, and the scroll's offset then.
type Seen = Option<(Bounds<Pixels>, f32)>;

/// Where the rows of a [`fade_scroll`] with bands off fade, at each end of its viewport: a
/// row is gone once its middle is `inset` from that edge and whole once its middle is `size`
/// past that. A scroll that runs under chrome (a titlebar, a pinned footer) sets the inset to
/// the chrome, so a row is faded out before the chrome covers it and nothing is ever cut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FadeEdges {
    pub top_inset: f32,
    pub top: f32,
    pub bottom_inset: f32,
    pub bottom: f32,
}

impl FadeEdges {
    /// `top` and `bottom` long stretches at the viewport's very edges.
    pub fn new(top: f32, bottom: f32) -> Self {
        Self {
            top_inset: 0.0,
            top,
            bottom_inset: 0.0,
            bottom,
        }
    }

    /// Starts each stretch `top` and `bottom` in from the viewport's edges.
    pub fn insets(mut self, top: f32, bottom: f32) -> Self {
        self.top_inset = top;
        self.bottom_inset = bottom;
        self
    }
}

impl Default for FadeEdges {
    fn default() -> Self {
        Self::new(SCROLL_FADE, SCROLL_FADE)
    }
}

/// How visible a row whose middle is `center` is in a viewport `view_top..view_bottom` that
/// has scrolled `offset` from its top with `left` more to go. Each end fades in as the scroll
/// moves away from it, so an end at rest is crisp, and the ramp eases out (squared) so the
/// row nearly vanishes before the edge reaches it.
fn edge_alpha(
    center: f32,
    view_top: f32,
    view_bottom: f32,
    offset: f32,
    left: f32,
    edges: FadeEdges,
) -> f32 {
    let ramp = |distance: f32, inset: f32, size: f32, room: f32| {
        if size <= 0.5 {
            return 1.0;
        }
        let shown = ((distance - inset) / size).clamp(0.0, 1.0);
        let engaged = (room / size).clamp(0.0, 1.0);
        1.0 - (1.0 - shown * shown) * engaged
    };
    let from_top = ramp(center - view_top, edges.top_inset, edges.top, offset);
    let from_bottom = ramp(view_bottom - center, edges.bottom_inset, edges.bottom, left);
    from_top.min(from_bottom)
}

/// `child`, a row inside a [`fade_scroll`] over `handle` with bands off, fading over `edges`
/// as it nears an end of the scroll with more beyond it. `id` names it across frames; the
/// fade follows the scroll in the frame it moves, from where the row was laid out and how
/// far the scroll has gone since.
pub fn fade_item(
    id: impl Into<SharedString>,
    handle: &ScrollHandle,
    edges: FadeEdges,
    child: impl IntoElement,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id: SharedString = id.into();
    let seen = window.use_keyed_state(
        ElementId::Name(format!("{id}-fade-item").into()),
        cx,
        |_, _| -> Seen { None },
    );
    let (offset, max) = scrolled(handle);
    let view = handle.bounds();
    let alpha = match *seen.read(cx) {
        Some((bounds, then)) if f32::from(view.size.height) > 1.0 => {
            let moved = offset - then;
            let center = f32::from(bounds.top() + bounds.bottom()) / 2.0 - moved;
            edge_alpha(
                center,
                f32::from(view.top()),
                f32::from(view.bottom()),
                offset,
                max - offset,
                edges,
            )
        }
        _ => 1.0,
    };
    let watch = handle.clone();
    div()
        .relative()
        .w_full()
        .opacity(alpha)
        .child(child)
        .child(
            canvas(
                move |bounds, _, cx| {
                    // Laid out by now, so the offset is the one these bounds were placed at.
                    let then = scrolled(&watch).0;
                    seen.update(cx, |seen, _| *seen = Some((bounds, then)));
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use gpui::{ScrollHandle, point, px};

    use super::{FadeEdges, edge_alpha, scrolled};

    #[test]
    fn a_wheel_past_an_end_reads_as_the_end() {
        // A wheel left the offset past both ends before layout clamped it.
        let handle = ScrollHandle::new();
        handle.set_offset(point(px(0.0), px(-500.0)));
        assert_eq!(scrolled(&handle), (0.0, 0.0));
        handle.set_offset(point(px(0.0), px(80.0)));
        assert_eq!(scrolled(&handle), (0.0, 0.0));
    }

    #[test]
    fn an_end_at_rest_is_crisp_and_one_with_more_fades() {
        let edges = FadeEdges::new(24.0, 24.0);
        // At the top, nothing scrolled: a row at the very top shows in full.
        assert_eq!(edge_alpha(5.0, 0.0, 400.0, 0.0, 300.0, edges), 1.0);
        // Scrolled 100px: the same place fades, and a row past the stretch does not.
        assert!(edge_alpha(5.0, 0.0, 400.0, 100.0, 200.0, edges) < 0.1);
        assert_eq!(edge_alpha(50.0, 0.0, 400.0, 100.0, 200.0, edges), 1.0);
        // More below: the last row fades; at the end it does not.
        assert!(edge_alpha(395.0, 0.0, 400.0, 100.0, 200.0, edges) < 0.1);
        assert_eq!(edge_alpha(395.0, 0.0, 400.0, 300.0, 0.0, edges), 1.0);
    }

    #[test]
    fn a_row_under_chrome_is_gone_before_the_chrome() {
        // A 48px footer: a row whose middle is behind it shows nothing.
        let edges = FadeEdges::new(24.0, 40.0).insets(0.0, 48.0);
        assert_eq!(edge_alpha(370.0, 0.0, 400.0, 100.0, 200.0, edges), 0.0);
        assert!(edge_alpha(330.0, 0.0, 400.0, 100.0, 200.0, edges) < 0.5);
        assert_eq!(edge_alpha(300.0, 0.0, 400.0, 100.0, 200.0, edges), 1.0);
    }
}
