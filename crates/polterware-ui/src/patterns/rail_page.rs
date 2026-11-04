//! `RailPage` (`rail-page.tsx`, `rail-page.css`) as a run opens in it (`wide splitScroll
//! railCard`): a 44px header with Back, the actions, and the rail toggle; then a row padded
//! `0 62px` with 48px gaps, the column (filled by content that scrolls itself, with the
//! column's classic scrollbar at its right edge, ends 64px past its last line, and sets its
//! lines at most [`WIDE_MEASURE`] wide and centered) and the 376px properties rail on the card
//! material, 8px in from the page's right edge and 8px above its bottom, padded `16px 8px 8px`,
//! scrolling on its own.
//! Below 900px the rail leaves the row and opens as a drawer over the text, on the floating
//! material, 52px from the top and 8px in from the other edges.
//!
//! The rail comes and goes like the detail panel: it slides 16px and fades on `spring.slow` (a
//! tween on its exit), and beside the text its width and the gap before it close too, so the
//! column glides instead of jumping. The browser also fades the rail's content out before the
//! page edge with a moving mask; GPUI has no mask, so the content is clipped by the rail.
//!
//! `RailGroup` is one titled group of the rail: a toggle heading (a 24px `IconTile` with a
//! 14px mark, the 13px/500 title, and a 14px muted chevron at the end that turns −90° when
//! closed) over its content, which folds into a [`SectionClip`].

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, Div, FontWeight, IntoElement, Render, SharedString, Window, canvas,
    div, prelude::*, px,
};

use crate::icon_tile::{IconTile, TileTone};
use crate::icons::{Icon, icon_rotated};
use crate::material::{Edge, drop, edge, float_drop};
use crate::motion::{Curve, SpringTier, Tween};
use crate::patterns::collapsible_section::{SectionBody, SectionClip};
use crate::theme::Theme;
use crate::window_drag::WindowDragRegion;

/// Below this page width the rail opens as a drawer.
pub const NARROW_WIDTH: f32 = 900.0;
/// The rail's width and the row's gap before it.
pub const RAIL_WIDTH: f32 = 376.0;
const RAIL_GAP: f32 = 48.0;
/// The row's side padding.
const PAGE_PADDING: f32 = 62.0;
/// The card rail hugs the page edge 8px in, past the row's padding.
const RAIL_INSET: f32 = 8.0;
/// `rail-inner`'s width on the card: the rail less its 8px sides.
const RAIL_INNER: f32 = 360.0;
/// `wide`: a log's measure.
pub const WIDE_MEASURE: f32 = 880.0;
/// How far the rail slides as it comes and goes.
const TRAVEL: f32 = 16.0;

/// `RailPage`'s header: 44px, `0 8px` padding, Back at the start and the actions at the end
/// (8px gaps); the empty part between them drags the window.
pub fn rail_page_header(start: impl IntoElement, end: impl IntoElement, cx: &App) -> Div {
    let theme = Theme::global(cx);
    div()
        .h(px(theme.page_header_height))
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(8.0))
        .child(div().flex().items_center().gap(px(8.0)).child(start))
        .child(div().flex_1().h_full().window_drag_region())
        .child(div().flex().items_center().gap(px(8.0)).child(end))
}

/// The rail's presence: whether the page wants it, and one progress from 0 (gone) to 1
/// (in place) that its slide, fade, and width follow.
pub struct RailPresence {
    open: bool,
    progress: Tween,
}

impl RailPresence {
    /// At rest, open or not, as the first paint snaps.
    pub fn new(open: bool) -> Self {
        Self {
            open,
            progress: Tween::new(if open { 1.0 } else { 0.0 }),
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Whether the rail draws: open, or still leaving.
    pub fn is_present(&self) -> bool {
        self.open || self.progress.is_animating()
    }

    pub fn is_animating(&self) -> bool {
        self.progress.is_animating()
    }

    pub fn set_open(&mut self, open: bool) {
        if open == self.open {
            return;
        }
        self.open = open;
        let from = self.progress.value();
        self.progress.snap(from);
        if open {
            self.progress
                .animate_to(1.0, SpringTier::Slow.duration(), SpringTier::Slow.curve());
        } else {
            self.progress.animate_to(
                0.0,
                SpringTier::Slow.exit_duration(),
                SpringTier::Slow.exit_curve(),
            );
        }
    }

    fn value(&self) -> f32 {
        self.progress.value()
    }
}

/// The page's width as the last frame laid it out, for the narrow switch.
#[derive(Clone, Default)]
pub struct PageWidth(Rc<Cell<f32>>);

impl PageWidth {
    pub fn is_narrow(&self) -> bool {
        let width = self.0.get();
        width > 0.0 && width < NARROW_WIDTH
    }

    /// A probe filling the page that records its width and redraws when it crosses
    /// [`NARROW_WIDTH`]. Add it to a `relative()` page.
    pub fn probe(&self) -> impl IntoElement {
        let width = self.0.clone();
        canvas(
            move |bounds, window, _| {
                let next = f32::from(bounds.size.width);
                let was = width.get();
                width.set(next);
                let narrow = |w: f32| w > 0.0 && w < NARROW_WIDTH;
                if narrow(was) != narrow(next) {
                    window.refresh();
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

/// The scroll row under the header, with the column and, while present, the rail beside it;
/// on a narrow page the rail is instead the drawer returned second, which goes on the page
/// itself (`relative()`), over the header's row and the text.
pub fn rail_page_body(
    column: impl IntoElement,
    rail: AnyElement,
    presence: &RailPresence,
    page: &PageWidth,
    window: &mut Window,
    cx: &App,
) -> (Div, Option<Div>) {
    rail_page_body_under(column, rail, presence, page, 0.0, window, cx)
}

/// [`rail_page_body`] for a page whose header floats over it (see
/// [`trailing_toolbar`](super::trailing_toolbar)): the column runs up under the header,
/// `header` tall, and keeps that much clear at its own top; the rail starts below it.
pub fn rail_page_body_under(
    column: impl IntoElement,
    rail: AnyElement,
    presence: &RailPresence,
    page: &PageWidth,
    header: f32,
    window: &mut Window,
    cx: &App,
) -> (Div, Option<Div>) {
    let theme = Theme::global(cx);
    if presence.is_animating() {
        window.request_animation_frame();
    }
    let progress = presence.value();
    // The column's content fills it and scrolls itself, as the run's transcript is a
    // virtualized list that needs a definite height; a scrolling wrapper would leave it none.
    let column = div()
        .id("rail-page-column")
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .flex()
        .flex_col()
        .child(
            div()
                .flex_1()
                .min_h(px(0.0))
                .w_full()
                .flex()
                .flex_col()
                .child(column),
        );
    let row = div()
        .flex_1()
        .min_h(px(0.0))
        .w_full()
        .flex()
        .flex_row()
        .gap(px(RAIL_GAP))
        .px(px(PAGE_PADDING))
        .overflow_hidden()
        .child(column);
    if !presence.is_present() {
        return (row, None);
    }
    let offset = TRAVEL * (1.0 - progress);
    let opacity = progress.clamp(0.0, 1.0);
    if page.is_narrow() {
        let width = RAIL_WIDTH.min(page.0.get() - 16.0).max(0.0);
        let drawer = div()
            .absolute()
            .top(px(52.0))
            .right(px(8.0 - offset))
            .bottom(px(8.0))
            .w(px(width))
            .opacity(opacity)
            .rounded(px(theme.radius_card))
            .bg(theme.color_surface_subtle)
            .shadow(float_drop(theme))
            .child(
                div()
                    .id("rail-page-rail")
                    .size_full()
                    .overflow_y_scroll()
                    .py(px(16.0))
                    .px(px(8.0))
                    .child(div().w_full().flex().flex_col().child(rail)),
            )
            .child(edge(Edge::Quiet, theme.radius_card, theme));
        return (row, Some(drawer));
    }
    // Beside the text the width closes with the gap before it; past full size it holds.
    let width = (RAIL_WIDTH * progress).clamp(0.0, RAIL_WIDTH);
    let gap_close = -RAIL_GAP * (1.0 - width / RAIL_WIDTH);
    let card = div()
        .relative()
        .flex_none()
        .w(px(width))
        .ml(px(gap_close))
        .mr(px(RAIL_INSET - PAGE_PADDING))
        .mt(px(header))
        .mb(px(8.0))
        .left(px(offset))
        .opacity(opacity)
        .rounded(px(theme.radius_card))
        .bg(theme.color_surface_subtle)
        .shadow(vec![
            drop(2.0, 4.0, theme.color_shadow),
            drop(1.0, 2.0, theme.color_shadow),
        ])
        .child(
            div()
                .id("rail-page-rail")
                .size_full()
                .overflow_x_hidden()
                .overflow_y_scroll()
                .pt(px(16.0))
                .px(px(8.0))
                .pb(px(8.0))
                .child(
                    div()
                        .w_full()
                        .min_w(px(RAIL_INNER))
                        .flex()
                        .flex_col()
                        .child(rail),
                ),
        )
        .child(edge(Edge::Quiet, theme.radius_card, theme));
    (row.child(card), None)
}

/// `RailGroup collapsible`: the heading toggles the content on the shared springs. It starts
/// open.
pub struct RailGroup {
    title: SharedString,
    icon: Icon,
    tone: TileTone,
    clip: SectionClip,
    /// The chevron's turn, 0 open and −90° closed.
    chevron: Tween,
    body: SectionBody,
}

impl RailGroup {
    pub fn new(title: impl Into<SharedString>, icon: Icon, cx: &mut Context<Self>) -> Self {
        let _ = cx;
        Self {
            title: title.into(),
            icon,
            tone: TileTone::Neutral,
            clip: SectionClip::new(true),
            chevron: Tween::new(0.0),
            body: Rc::new(|_, _| div().into_any_element()),
        }
    }

    pub fn set_body(&mut self, body: SectionBody) {
        self.body = body;
    }

    pub fn set_open(&mut self, open: bool, cx: &App) {
        if open == self.clip.is_open() {
            return;
        }
        self.clip.set_open(open);
        let theme = Theme::global(cx);
        self.chevron.animate_to(
            if open {
                0.0
            } else {
                -std::f32::consts::FRAC_PI_2
            },
            theme.duration_control,
            Curve::Ease(theme.ease_standard),
        );
    }

    pub fn is_open(&self) -> bool {
        self.clip.is_open()
    }
}

impl Render for RailGroup {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::global(cx);
        if self.chevron.is_animating() {
            window.request_animation_frame();
        }
        let heading = div()
            .id(("rail-group-toggle", cx.entity_id()))
            .w_full()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(4.0))
            .rounded(px(theme.radius_item))
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                let open = !this.is_open();
                this.set_open(open, cx);
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .child(IconTile::new(self.icon).size(24.0, 14.0).tone(self.tone)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(13.0))
                    .line_height(px(13.0 * 1.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.color_text_primary)
                    .child(self.title.clone()),
            )
            .child(icon_rotated(
                Icon::ChevronDown,
                14.0,
                theme.color_text_muted,
                self.chevron.value(),
            ));
        let shows = self.clip.shows_body();
        let body = shows.then(|| {
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .pt(px(6.0))
                .child((self.body)(window, cx))
                .into_any_element()
        });
        let clip = self.clip.render(body, window);
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .mb(px(20.0))
            .pb(px(10.0))
            .pl(px(4.0))
            .child(heading)
            .when(shows, |group| group.child(clip))
    }
}

/// `DetailPanel.Properties variant="rail"`: rows 2px apart.
pub fn rail_properties() -> Div {
    div().w_full().min_w(px(0.0)).flex().flex_col().gap(px(2.0))
}

/// `DetailPanel.Property` in the rail: an 88px muted label (12px, 8px in) and the value
/// (13px/500 primary, `6px 10px` in, wrapping), at least 30px tall and 4px apart.
pub fn rail_property(label: &str, value: impl IntoElement, mono: bool, cx: &App) -> AnyElement {
    let theme = Theme::global(cx);
    div()
        .w_full()
        .min_w(px(0.0))
        .min_h(px(30.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .child(
            div()
                .w(px(88.0))
                .flex_none()
                .pl(px(8.0))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(12.0))
                .line_height(px(12.0 * 1.5))
                .text_color(theme.color_text_muted)
                .child(label.to_string()),
        )
        // One row, no wrap. The value grows into the column from its own width, or shrinks to
        // it; a text value wraps there when it is a `wrap_text` (gpui's own text can paint
        // one letter a line in this row; see `wrap.rs`).
        .child(
            div()
                .flex_auto()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .pl(px(6.0))
                .pr(px(10.0))
                .text_size(px(13.0))
                .line_height(px(13.0 * 1.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.color_text_primary)
                .when(mono, |value| value.font_family(theme.font_mono))
                .child(value),
        )
        .into_any_element()
}

/// The rail toggle's label, as its tooltip reads.
pub fn rail_toggle_label(visible: bool) -> &'static str {
    if visible {
        "Hide properties"
    } else {
        "Show properties"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rail_leaves_before_it_unmounts_and_snaps_on_first_paint() {
        let mut presence = RailPresence::new(true);
        assert!(presence.is_present());
        assert!(!presence.is_animating());
        presence.set_open(false);
        assert!(!presence.is_open());
        assert!(presence.is_present(), "it stays drawn while it leaves");
        let closed = RailPresence::new(false);
        assert!(!closed.is_present());
    }

    #[test]
    fn the_toggle_names_what_it_does() {
        assert_eq!(rail_toggle_label(true), "Hide properties");
        assert_eq!(rail_toggle_label(false), "Show properties");
    }
}
