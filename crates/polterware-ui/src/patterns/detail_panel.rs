//! `DetailPanel` (`detail-panel.css`, `side-panel.tsx`): the floating panel a list opens its
//! active record in, beside the list rather than over a scrim. The frame is
//! `surface-panel-float` (`--card-fill`, the quiet edge, the float drop and a 0 20px 56px
//! `--color-shadow-float` shadow, `--radius-surface`), 360px wide, 8px in from the page's right and top (below a toolbar
//! over the page). It is as tall as its content, up to 8px above the page's bottom; past
//! that the body and title scroll inside while Close and the footer stay. Its optional 12px invisible handle
//! on the left edge resizes it with the column-resize cursor and follows the pointer directly.
//! While it is open it takes the presses and the wheel over its face, so the list under it
//! keeps neither. While it is leaving it keeps neither, and the list under it gets them.
//!
//! It comes and goes like the docs rail: it slides 16px in from the right and fades on
//! `spring.slow`, and leaves on that tier's exit tween, still showing the last record, then
//! unmounts. Swapping records while it is open replaces the content and does not replay the
//! slide. A presence created already open is at rest on the first paint, as the rail is.
//! The host holds one [`DetailPanelPresence`] and hands it the body each frame.
//!
//! The panel has no title bar: the scrolling body starts with the title as its first
//! section, a heading like every [`detail_section`]'s (the [`DetailPanel::leading`] mark in a
//! 24px slot, else the [`DetailPanel::title_section`] icon tile, then its label, Summary by
//! default, and the chips) over the title in [`detail_text`]. A title longer than three lines
//! shows them as one paragraph cut with an ellipsis, and a muted 12px Show more under it
//! opens every line (Show less closes them). Close details stays
//! 12px from the frame's top and right, clear of the title. The body pads 20px at the sides
//! and 24px at the bottom; its sections ([`detail_section`]) stand 24px apart, each a 24px icon tile and a
//! 13px title 8px after it, then its content 10px under them. The rail's sections
//! ([`DetailPanel::rail`], `DetailPanel.Rail`) follow the main ones 12px under them, the
//! record's gap, since the panel stacks the two. [`detail_properties`] lists label and value rows: a
//! 110px muted 12px label, 12px apart from a 12.5px primary value at the end, 28px tall and
//! 8px apart. The footer, when there is one, pads 12px 20px 20px and holds its actions 6px
//! apart.
//!
//! As `dismissOnOutsideClick` asks, a click outside the frame tells the host
//! ([`DetailPanel::on_outside_click`]) after it has done what it does there, so the host can
//! close the panel unless that click opened a record in it, as a row (an opener) does.

use std::{cell::Cell, rc::Rc};

use gpui::{
    AnyElement, App, Bounds, ContentMask, CursorStyle, DispatchPhase, Element, ElementId,
    FontWeight, GlobalElementId, InspectorElementId, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, SharedString, Window, canvas, div, prelude::*, px,
};

use super::rail_page::RailPresence;

use crate::Handler;
use crate::data_table::{sans, tabular};
use crate::icon_button::icon_button;
use crate::icon_tile::IconTile;
use crate::icons::Icon;
use crate::material::{drop, floating_panel};
use crate::scroll::scroll_area;
use crate::theme::Theme;

/// `--detail-panel-width`.
pub const DETAIL_PANEL_WIDTH: f32 = 360.0;
/// The narrowest width of a resizable detail panel.
pub const DETAIL_PANEL_MIN_WIDTH: f32 = 320.0;
/// The widest width of a resizable detail panel.
pub const DETAIL_PANEL_MAX_WIDTH: f32 = 720.0;
/// The frame's inset from the page's edges.
pub const DETAIL_PANEL_INSET: f32 = 8.0;
/// How far the panel slides as it comes and goes, the rail's travel.
const TRAVEL: f32 = 16.0;

/// The body a host rebuilds while the panel is open. Kept, and not replaced, while it leaves.
pub type DetailPanelBody = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// The detail panel's presence, as [`RailPresence`] is the rail's: one progress from 0
/// (gone) to 1 (in place) on `spring.slow`, and the last body, kept while it leaves.
///
/// A host holds one in an `Entity`. Each frame it calls [`set`](Self::set) with the body
/// while a record is open and with `None` once it has closed, then [`element`](Self::element)
/// for what to draw. A new body while it is already open swaps the record and does not replay
/// the slide. Created open, the first paint is already at rest.
pub struct DetailPanelPresence {
    motion: RailPresence,
    body: Option<DetailPanelBody>,
}

impl DetailPanelPresence {
    /// At rest, open or not, so a page created with the panel open does not slide on first paint.
    pub fn new(open: bool) -> Self {
        Self {
            motion: RailPresence::new(open),
            body: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.motion.is_open()
    }

    /// Whether the panel draws: open with a body, or still leaving with the last one.
    pub fn is_present(&self) -> bool {
        self.body.is_some() && self.motion.is_present()
    }

    pub(crate) fn value(&self) -> f32 {
        self.motion.value()
    }

    /// `Some` replaces the body and opens. `None` starts the exit and keeps the last body
    /// until the tween ends.
    pub fn set(&mut self, body: Option<DetailPanelBody>) {
        let open = body.is_some();
        if let Some(body) = body {
            self.body = Some(body);
        }
        self.motion.set_open(open);
        if !self.motion.is_present() {
            self.body = None;
        }
    }

    /// The panel for this frame, slid and faded. `None` once it has gone.
    ///
    /// While it is leaving, the panel draws but takes no pointer and no wheel.
    /// Call this from the page's render: an in-flight tween asks that page for the next frame.
    pub fn element(&mut self, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        if !self.motion.is_present() {
            self.body = None;
            return None;
        }
        let body = self.body.clone()?;
        if self.motion.is_animating() {
            window.request_animation_frame();
        }
        let progress = self.motion.value();
        let offset = TRAVEL * (1.0 - progress);
        let opacity = progress.clamp(0.0, 1.0);
        // Shift both edges by the travel so the panel's own 8px inset rides with it, the way
        // the rail's drawer sets `right: 8px - offset`.
        let layer = div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(offset))
            .right(px(-offset))
            .opacity(opacity)
            .child(body(window, cx));
        if self.motion.is_open() {
            Some(layer.into_any_element())
        } else {
            Some(unhit(layer).into_any_element())
        }
    }
}

/// Draws `child` and leaves it out of hit testing, so presses and the wheel fall through.
struct Unhit {
    child: AnyElement,
}

fn unhit(child: impl IntoElement) -> Unhit {
    Unhit {
        child: child.into_any_element(),
    }
}

impl IntoElement for Unhit {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Unhit {
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
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        (self.child.request_layout(window, cx), ())
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
        // Hit testing intersects each hitbox with the mask stored here. An empty mask misses,
        // so the occluding face, Close, and the wheel never see the pointer. Paint stays
        // unmasked, so the panel is still on screen.
        let mask = ContentMask {
            bounds: Bounds::default(),
        };
        window.with_content_mask(Some(mask), |window| {
            self.child.prepaint(window, cx);
        });
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

const SECTION_GAP: f32 = 24.0;
/// `.pw-detail-panel__record`'s gap, between the main column and the rail stacked under it.
const RECORD_GAP: f32 = 12.0;
const PROPERTY_LABEL_WIDTH: f32 = 110.0;
/// How many lines a long title shows before Show more.
const TITLE_LINES: usize = 3;
/// The body's side padding.
const BODY_PAD_X: f32 = 20.0;
/// The body's top padding, which puts the title section's 20px heading level with Close.
const BODY_PAD_TOP: f32 = 16.0;
/// What the title's heading leaves on its right for Close (28px and its 12px inset, less
/// the body's 20px).
const CLOSE_ROOM: f32 = 20.0;

/// How many lines `title` takes at `width` in the title section's 12.5px text.
fn title_lines(title: &SharedString, width: f32, window: &Window) -> usize {
    if width <= 0.0 {
        return 1;
    }
    let run = window.text_style().to_run(title.len());
    window
        .text_system()
        .shape_text(title.clone(), px(12.5), &[run], Some(px(width)), None)
        .map(|lines| {
            lines
                .iter()
                .map(|line| line.wrap_boundaries().len() + 1)
                .sum()
        })
        .unwrap_or(1)
}

/// A short key for a title, so each record's Show more starts closed.
fn title_key(title: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    title.hash(&mut hasher);
    hasher.finish()
}
const DETAIL_HANDLE: f32 = 12.0;

/// A cheap, cloneable width shared by a page and its [`DetailPanel`].
///
/// Create it once per page and pass a clone to [`DetailPanel::width`]. The page can reserve
/// `get() + DETAIL_PANEL_INSET` beside its table. The effective value is limited to 320–720px
/// and to the viewport width minus 360px.
#[derive(Clone)]
pub struct DetailPanelWidth(Rc<DetailPanelWidthState>);

struct DetailPanelWidthState {
    width: Cell<f32>,
    drag: Cell<Option<DetailDrag>>,
}

impl Default for DetailPanelWidth {
    fn default() -> Self {
        Self::new()
    }
}

impl DetailPanelWidth {
    pub fn new() -> Self {
        Self(Rc::new(DetailPanelWidthState {
            width: Cell::new(DETAIL_PANEL_WIDTH),
            drag: Cell::new(None),
        }))
    }

    /// The current panel width in pixels.
    pub fn get(&self) -> f32 {
        self.0.width.get()
    }

    fn set(&self, width: f32) {
        self.0.width.set(width);
    }
}

#[derive(Clone, Copy, Debug)]
struct DetailDrag {
    from_x: f32,
    from_width: f32,
}

fn clamp_detail_width(width: f32, viewport_width: f32) -> f32 {
    let max = (viewport_width - 360.0).clamp(0.0, DETAIL_PANEL_MAX_WIDTH);
    width.clamp(
        if max >= DETAIL_PANEL_MIN_WIDTH {
            DETAIL_PANEL_MIN_WIDTH
        } else {
            0.0
        },
        max,
    )
}

fn dragged_detail_width(drag: DetailDrag, x: f32, viewport_width: f32) -> f32 {
    clamp_detail_width(drag.from_width - (x - drag.from_x), viewport_width)
}

#[derive(IntoElement)]
pub struct DetailPanel {
    id: ElementId,
    title: SharedString,
    title_label: SharedString,
    title_icon: Icon,
    leading: Option<AnyElement>,
    aside: Vec<AnyElement>,
    sections: Vec<AnyElement>,
    rail: Vec<AnyElement>,
    footer: Option<AnyElement>,
    on_close: Option<Handler>,
    on_outside: Option<Handler>,
    top: f32,
    width: Option<DetailPanelWidth>,
}

impl DetailPanel {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            title_label: "Summary".into(),
            title_icon: Icon::FileText,
            leading: None,
            aside: Vec::new(),
            sections: Vec::new(),
            rail: Vec::new(),
            footer: None,
            on_close: None,
            on_outside: None,
            top: 0.0,
            width: None,
        }
    }

    /// The heading of the title's section (default: Summary, with the file icon).
    pub fn title_section(mut self, label: impl Into<SharedString>, icon: Icon) -> Self {
        self.title_label = label.into();
        self.title_icon = icon;
        self
    }

    /// `DetailPanel.Title`'s icon: a mark before the title, such as the actor's bot, 8px
    /// from it.
    pub fn leading(mut self, element: impl IntoElement) -> Self {
        self.leading = Some(element.into_any_element());
        self
    }

    /// An element after the title, such as a `StatusChip`.
    pub fn aside(mut self, element: impl IntoElement) -> Self {
        self.aside.push(element.into_any_element());
        self
    }

    /// A body section, in order: usually a [`detail_section`].
    pub fn section(mut self, section: impl IntoElement) -> Self {
        self.sections.push(section.into_any_element());
        self
    }

    /// `DetailPanel.Footer`: the record's actions, under the body.
    /// A section of the rail (`DetailPanel.Rail`), such as Properties, under the main ones.
    pub fn rail(mut self, section: impl IntoElement) -> Self {
        self.rail.push(section.into_any_element());
        self
    }

    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    /// Close details: the header's last control.
    pub fn on_close(mut self, handler: Handler) -> Self {
        self.on_close = Some(handler);
        self
    }

    /// Called once a click outside the frame has run its own action: the host closes the
    /// panel unless that click opened a record, such as a row of the list.
    pub fn on_outside_click(mut self, handler: Handler) -> Self {
        self.on_outside = Some(handler);
        self
    }

    /// `--pw-toolbar-overlay`: how far a toolbar floating over the page reaches into it, so
    /// the frame starts under its controls.
    pub fn top(mut self, top: f32) -> Self {
        self.top = top;
        self
    }

    /// Uses a page-shared width, which the host can also read to reserve space beside its table.
    pub fn width(mut self, width: DetailPanelWidth) -> Self {
        self.width = Some(width);
        self
    }
}

impl RenderOnce for DetailPanel {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let viewport_width = f32::from(window.viewport_size().width);
        let panel_width = self.width.as_ref().map_or(DETAIL_PANEL_WIDTH, |width| {
            let clamped = clamp_detail_width(width.get(), viewport_width);
            width.set(clamped);
            clamped
        });
        // The title is the body's first section: its heading (the leading mark, or the
        // title's icon tile, the label, and the chips) level with Close, and its text cut to
        // three lines while it runs longer, with Show more under it.
        let text_width = panel_width - 2.0 * BODY_PAD_X;
        let long = title_lines(&self.title, text_width, window) > TITLE_LINES;
        let open = window.use_keyed_state(
            ElementId::Name(format!("{:?}-title-{}", self.id, title_key(&self.title)).into()),
            cx,
            |_, _| false,
        );
        let expanded = long && *open.read(cx);
        let toggle = long.then(|| {
            let open = open.clone();
            div()
                .id((self.id.clone(), "title-toggle"))
                .debug_selector(|| "detail-title-toggle".into())
                .mt(px(6.0))
                .text_size(px(12.0))
                .line_height(px(18.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.color_text_muted)
                .cursor_pointer()
                .hover(|toggle| toggle.text_color(theme.color_text_primary))
                .child(if expanded { "Show less" } else { "Show more" })
                .on_click(move |_, window, cx| {
                    open.update(cx, |open, cx| {
                        *open = !*open;
                        cx.notify();
                    });
                    window.refresh();
                })
        });
        let mark = match self.leading {
            Some(leading) => div()
                .flex_none()
                .size(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .child(leading)
                .into_any_element(),
            None => IconTile::new(self.title_icon)
                .size(24.0, 14.0)
                .into_any_element(),
        };
        let heading = div()
            .min_w(px(0.0))
            .min_h(px(20.0))
            // Clear of Close, which stands level with it at the frame's top right.
            .pr(px(CLOSE_ROOM))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .child(mark)
            .child(
                div()
                    .text_size(px(13.0))
                    .line_height(px(13.0 * 1.35))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.color_text_primary)
                    .child(self.title_label),
            )
            .children(self.aside);
        let text = detail_text(
            if long && !expanded {
                // Closed, its lines run on as one paragraph, which the clamp cuts.
                SharedString::from(self.title.split_whitespace().collect::<Vec<_>>().join(" "))
            } else {
                self.title
            },
            cx,
        )
        .debug_selector(|| "detail-title".into())
        .when(long && !expanded, |text| {
            text.line_clamp(TITLE_LINES).text_ellipsis()
        });
        let title = div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .min_w(px(0.0))
            .child(heading)
            .child(div().flex().flex_col().child(text).children(toggle));
        let close = self.on_close.map(|on_close| {
            icon_button(
                (self.id.clone(), "close"),
                Icon::Close,
                "Close details",
                on_close,
                cx,
            )
        });
        let mut main = div().w_full().flex().flex_col().pb(px(24.0));
        let mut sections = div()
            .w_full()
            .flex()
            .flex_col()
            .pt(px(BODY_PAD_TOP))
            .px(px(BODY_PAD_X))
            .child(title);
        let has_main = !self.sections.is_empty();
        for section in self.sections {
            sections = sections.child(div().mt(px(SECTION_GAP)).child(section));
        }
        for (index, section) in self.rail.into_iter().enumerate() {
            let gap = if index > 0 || !has_main {
                SECTION_GAP
            } else {
                RECORD_GAP
            };
            sections = sections.child(div().mt(px(gap)).child(section));
        }
        main = main.child(sections);
        // The body keeps its content's height and gives it up first when the frame reaches
        // the page's height, scrolling inside while Close and the footer stay put.
        let body = div().flex_initial().min_h(px(0.0)).child(scroll_area(
            (self.id.clone(), "body"),
            main,
            window,
            cx,
        ));
        let footer = self.footer.map(|footer| {
            div()
                .flex_none()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.0))
                .pt(px(12.0))
                .px(px(20.0))
                .pb(px(20.0))
                .child(footer)
        });
        let frame = div()
            .id(self.id.clone())
            .relative()
            .w_full()
            .max_h_full()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .bg(theme.color_surface_subtle)
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(body)
            .children(footer)
            .children(
                close.map(|close| div().absolute().top(px(12.0)).right(px(12.0)).child(close)),
            );
        let frame = match self.on_outside {
            Some(on_outside) => {
                // Only a click counts: a press outside released within 3px of where it began.
                let press =
                    window.use_keyed_state((self.id.clone(), "outside-press"), cx, |_, _| {
                        None::<Point<Pixels>>
                    });
                let down = press.clone();
                frame
                    .on_mouse_down_out(move |event, _, cx| {
                        if event.button == MouseButton::Left {
                            down.update(cx, |press, _| *press = Some(event.position));
                        }
                    })
                    .on_mouse_up_out(MouseButton::Left, move |event, window, cx| {
                        let Some(start) = press.update(cx, |press, _| press.take()) else {
                            return;
                        };
                        let travel = event.position - start;
                        if f32::from(travel.x).hypot(f32::from(travel.y)) >= 3.0 {
                            return;
                        }
                        let on_outside = on_outside.clone();
                        // After the click's own listeners, which may open another record.
                        window.defer(cx, move |window, cx| on_outside(window, cx));
                    })
            }
            None => frame,
        };
        let mut frame = floating_panel(frame, theme.radius_surface, &theme);
        let mut shadows = crate::material::float_drop(&theme);
        shadows.push(drop(20.0, 56.0, theme.color_shadow_float));
        frame = frame.shadow(shadows);
        // The room the frame may take: from under the toolbar to the page's bottom inset.
        let room = div()
            .absolute()
            .top(px(DETAIL_PANEL_INSET + self.top))
            .right(px(DETAIL_PANEL_INSET))
            .bottom(px(DETAIL_PANEL_INSET))
            .w(px(panel_width))
            .flex()
            .flex_col();
        // As tall as its content, up to that room; the resize handle runs along it.
        let mut panel = div()
            .relative()
            .w_full()
            .max_h_full()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(frame);
        if let Some(width) = self.width {
            let handle_width = width.clone();
            let handle_drag = width.clone();
            let handle = div()
                .id((self.id, "resize-handle"))
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .ml(px(-DETAIL_HANDLE / 2.0))
                .w(px(DETAIL_HANDLE))
                .cursor(CursorStyle::ResizeLeftRight)
                .on_mouse_down(
                    MouseButton::Left,
                    move |event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        handle_drag.0.drag.set(Some(DetailDrag {
                            from_x: f32::from(event.position.x),
                            from_width: handle_width.get(),
                        }));
                        window.refresh();
                    },
                );
            let move_width = width.clone();
            let move_drag = width.clone();
            let up_drag = width.clone();
            let tracker = canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    if move_drag.0.drag.get().is_none() {
                        return;
                    }
                    window.on_mouse_event({
                        let width = move_width.clone();
                        let drag = move_drag.clone();
                        move |event: &MouseMoveEvent, phase, window, _| {
                            if phase != DispatchPhase::Bubble
                                || event.pressed_button != Some(MouseButton::Left)
                            {
                                return;
                            }
                            let Some(drag) = drag.0.drag.get() else {
                                return;
                            };
                            width.set(dragged_detail_width(
                                drag,
                                f32::from(event.position.x),
                                f32::from(window.viewport_size().width),
                            ));
                            window.refresh();
                        }
                    });
                    window.on_mouse_event({
                        let drag = up_drag.clone();
                        move |event: &MouseUpEvent, phase, window, _| {
                            if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                                drag.0.drag.set(None);
                                window.refresh();
                            }
                        }
                    });
                },
            )
            .absolute()
            .size_0();
            panel = panel.child(handle).child(tracker);
        }
        room.child(panel)
    }
}

/// `DetailPanel.Section`: a 24px neutral icon tile and the title in 13px primary, 8px apart,
/// then `content` 10px under them.
pub fn detail_section(
    title: impl Into<SharedString>,
    icon: Icon,
    content: impl IntoElement,
    cx: &App,
) -> gpui::Div {
    section(title.into(), icon, None, content.into_any_element(), cx)
}

/// [`detail_section`] with `aside` at the end of its title row (the section's `aside`), such
/// as an icon button.
pub fn detail_section_with_aside(
    title: impl Into<SharedString>,
    icon: Icon,
    aside: impl IntoElement,
    content: impl IntoElement,
    cx: &App,
) -> gpui::Div {
    section(
        title.into(),
        icon,
        Some(aside.into_any_element()),
        content.into_any_element(),
        cx,
    )
}

fn section(
    title: SharedString,
    icon: Icon,
    aside: Option<AnyElement>,
    content: AnyElement,
    cx: &App,
) -> gpui::Div {
    let theme = Theme::global(cx);
    // `.pw-detail-panel__section-heading`: at least 20px, the label and the aside apart.
    let label = div()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(IconTile::new(icon).size(24.0, 14.0))
        .child(
            div()
                .text_size(px(13.0))
                .line_height(px(13.0 * 1.35))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.color_text_primary)
                .child(title),
        );
    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .min_w(px(0.0))
        .child(
            div()
                .min_w(px(0.0))
                .min_h(px(20.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(label)
                .children(aside),
        )
        .child(content)
}

/// `DetailPanel.Properties`: the rows 8px apart.
pub fn detail_properties(rows: impl IntoIterator<Item = AnyElement>) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .min_w(px(0.0))
        .children(rows)
}

/// `DetailPanel.Property`: the label in a 110px muted 12px column, then the value at the end
/// in 12.5px primary, wrapping anywhere.
pub fn detail_property(
    label: impl Into<SharedString>,
    value: impl IntoElement,
    cx: &App,
) -> AnyElement {
    let theme = Theme::global(cx);
    div()
        .min_h(px(28.0))
        .flex()
        .items_center()
        .gap(px(12.0))
        .min_w(px(0.0))
        .child(
            div()
                .flex_none()
                .w(px(PROPERTY_LABEL_WIDTH))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(px(12.0))
                .text_color(theme.color_text_muted)
                .child(label.into()),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_wrap()
                .justify_end()
                .items_center()
                .gap(px(4.0))
                .text_size(px(12.5))
                .text_color(theme.color_text_primary)
                .child(value),
        )
        .into_any_element()
}

/// `.pw-detail-panel__text`: a paragraph in 12.5px secondary on a 1.55 line.
pub fn detail_text(text: impl Into<SharedString>, cx: &App) -> gpui::Div {
    let theme = Theme::global(cx);
    div()
        .text_size(px(12.5))
        .line_height(px(12.5 * 1.55))
        .text_color(theme.color_text_secondary)
        .child(text.into())
}

/// A numbered list, as a record's options read: each item's two-digit number in muted tabular
/// figures, 8px before it, the items 8px apart.
pub fn detail_numbered_list<S: Into<SharedString>>(
    items: impl IntoIterator<Item = S>,
    cx: &App,
) -> gpui::Div {
    let theme = Theme::global(cx);
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .text_size(px(12.5))
        .line_height(px(18.0))
        .text_color(theme.color_text_primary)
        .children(items.into_iter().enumerate().map(|(index, item)| {
            div()
                .flex()
                .items_start()
                .gap(px(8.0))
                .min_w(px(0.0))
                .child(
                    div()
                        .flex_none()
                        .font(tabular(sans(theme, FontWeight::NORMAL)))
                        .text_color(theme.color_text_muted)
                        .child(SharedString::from(format!("{:02}", index + 1))),
                )
                .child(div().flex_1().min_w(px(0.0)).child(item.into()))
        }))
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, OnceCell};
    use std::rc::Rc;
    use std::time::Duration;

    use gpui::{
        Modifiers, MouseButton, ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase, point,
        prelude::*, px, size,
    };

    use super::{
        DETAIL_PANEL_MAX_WIDTH, DETAIL_PANEL_MIN_WIDTH, DETAIL_PANEL_WIDTH, DetailDrag,
        DetailPanel, DetailPanelBody, DetailPanelPresence, DetailPanelWidth, clamp_detail_width,
        dragged_detail_width,
    };
    use crate::test_support::{draw, open, settle};

    #[test]
    fn width_clamps_to_limits_and_available_viewport() {
        assert_eq!(DetailPanelWidth::default().get(), DETAIL_PANEL_WIDTH);
        assert_eq!(clamp_detail_width(100.0, 1200.0), DETAIL_PANEL_MIN_WIDTH);
        assert_eq!(clamp_detail_width(900.0, 1200.0), DETAIL_PANEL_MAX_WIDTH);
        assert_eq!(clamp_detail_width(700.0, 900.0), 540.0);
        assert_eq!(clamp_detail_width(500.0, 600.0), 240.0);
    }

    #[test]
    fn dragging_the_left_edge_tracks_pointer_and_clamps() {
        let drag = DetailDrag {
            from_x: 600.0,
            from_width: 360.0,
        };
        assert_eq!(dragged_detail_width(drag, 560.0, 1200.0), 400.0);
        assert_eq!(
            dragged_detail_width(drag, 660.0, 1200.0),
            DETAIL_PANEL_MIN_WIDTH
        );
        assert_eq!(dragged_detail_width(drag, 0.0, 900.0), 540.0);
    }

    #[gpui::test]
    fn the_panel_floats_8px_in_from_the_right_and_closes(cx: &mut TestAppContext) {
        let closed = Rc::new(Cell::new(0));
        let count = closed.clone();
        let mut cx = open(cx, size(1000.0, 600.0), move |_, _| {
            let count = count.clone();
            gpui::div()
                .relative()
                .size_full()
                .child(
                    DetailPanel::new("panel", "Rename export snapshot")
                        .on_close(Rc::new(move |_, _| count.set(count.get() + 1))),
                )
                .into_any_element()
        });
        draw(&mut cx);
        let close = cx.debug_bounds("panel-close").expect("Close details draws");
        // Close stays 12px from the frame's top and right.
        assert_eq!(f32::from(close.right()), 1000.0 - 8.0 - 12.0);
        assert_eq!(f32::from(close.top()), 8.0 + 12.0);
        cx.simulate_click(close.center(), Modifiers::none());
        assert_eq!(closed.get(), 1);
    }

    fn panel_with_body(height: f32) -> gpui::AnyElement {
        gpui::div()
            .relative()
            .size_full()
            .child(
                DetailPanel::new("panel", "Which screens first?")
                    .on_close(Rc::new(|_, _| {}))
                    .section(
                        gpui::div()
                            .debug_selector(|| "body".into())
                            .h(gpui::px(height)),
                    )
                    .footer(
                        gpui::div()
                            .debug_selector(|| "actions".into())
                            .h(gpui::px(32.0))
                            .w(gpui::px(100.0)),
                    ),
            )
            .into_any_element()
    }

    #[gpui::test]
    fn a_short_record_keeps_the_panel_as_tall_as_its_content(cx: &mut TestAppContext) {
        let mut cx = open(cx, size(1000.0, 600.0), |_, _| panel_with_body(100.0));
        draw(&mut cx);
        let body = cx.debug_bounds("body").expect("the section draws");
        let actions = cx.debug_bounds("actions").expect("the footer draws");
        // The footer follows the body's 24px bottom padding and its own 12px top, far above
        // the page's bottom.
        assert_eq!(
            f32::from(actions.top()),
            f32::from(body.bottom()) + 24.0 + 12.0
        );
        assert!(f32::from(actions.bottom()) < 600.0 - 8.0 - 20.0 - 100.0);
    }

    #[gpui::test]
    fn a_long_record_scrolls_its_body_and_keeps_the_footer_down(cx: &mut TestAppContext) {
        let mut cx = open(cx, size(1000.0, 600.0), |_, _| panel_with_body(2000.0));
        draw(&mut cx);
        let actions = cx.debug_bounds("actions").expect("the footer draws");
        // The frame stops 8px above the page's bottom; the footer pads 20px under its actions.
        assert_eq!(f32::from(actions.bottom()), 600.0 - 8.0 - 20.0);
        let body = cx.debug_bounds("body").expect("the section draws");
        assert!(
            f32::from(body.bottom()) > f32::from(actions.top()),
            "the body runs on, scrolled"
        );
    }

    #[gpui::test]
    fn a_long_title_scrolls_while_close_and_footer_stay(cx: &mut TestAppContext) {
        let mut cx = open(cx, size(1000.0, 400.0), |_, _| {
            gpui::div()
                .relative()
                .size_full()
                .child(
                    DetailPanel::new("panel", ("A long prompt line\n").repeat(20))
                        .on_close(Rc::new(|_, _| {}))
                        .footer(
                            gpui::div()
                                .debug_selector(|| "actions".into())
                                .h(gpui::px(32.0)),
                        ),
                )
                .into_any_element()
        });
        draw(&mut cx);
        let toggle = cx
            .debug_bounds("detail-title-toggle")
            .expect("Show more draws");
        cx.simulate_click(toggle.center(), Modifiers::none());
        draw(&mut cx);
        let close = cx.debug_bounds("panel-close").expect("Close details draws");
        let actions = cx.debug_bounds("actions").expect("the footer draws");
        assert_eq!(f32::from(close.top()), 8.0 + 12.0);
        assert_eq!(f32::from(close.right()), 1000.0 - 8.0 - 12.0);
        assert_eq!(f32::from(actions.bottom()), 400.0 - 8.0 - 20.0);
    }

    #[gpui::test]
    fn a_long_title_shows_three_lines_until_show_more(cx: &mut TestAppContext) {
        let mut cx = open(cx, size(1000.0, 800.0), |_, _| {
            gpui::div()
                .relative()
                .size_full()
                .child(
                    DetailPanel::new("panel", ("A long prompt line\n").repeat(20))
                        .on_close(Rc::new(|_, _| {})),
                )
                .into_any_element()
        });
        draw(&mut cx);
        let line = 12.5 * 1.55;
        let title = cx.debug_bounds("detail-title").expect("the title draws");
        assert!(
            (f32::from(title.size.height) - 3.0 * line).abs() < 1.0,
            "three lines, not {}",
            f32::from(title.size.height)
        );
        let toggle = cx
            .debug_bounds("detail-title-toggle")
            .expect("Show more draws");
        cx.simulate_click(toggle.center(), Modifiers::none());
        draw(&mut cx);
        let title = cx.debug_bounds("detail-title").expect("the title draws");
        assert!(
            f32::from(title.size.height) > 19.0 * line,
            "every line shows"
        );
    }

    #[gpui::test]
    fn a_short_title_has_no_show_more(cx: &mut TestAppContext) {
        let mut cx = open(cx, size(1000.0, 600.0), |_, _| {
            gpui::div()
                .relative()
                .size_full()
                .child(DetailPanel::new("panel", "Rename export snapshot"))
                .into_any_element()
        });
        draw(&mut cx);
        assert!(cx.debug_bounds("detail-title-toggle").is_none());
    }

    #[gpui::test]
    fn chips_put_the_title_on_its_30px_line(cx: &mut TestAppContext) {
        let mut cx = open(cx, size(1000.0, 600.0), |_, _| {
            gpui::div()
                .relative()
                .size_full()
                .child(
                    DetailPanel::new("panel", "Rename export snapshot")
                        .aside(gpui::div().id("chip").w(gpui::px(40.0)).h(gpui::px(20.0)))
                        .on_close(Rc::new(|_, _| {})),
                )
                .into_any_element()
        });
        draw(&mut cx);
        let close = cx.debug_bounds("panel-close").expect("Close details draws");
        // Close keeps its position even with chips.
        assert_eq!(f32::from(close.top()), 8.0 + 12.0);
    }

    #[gpui::test]
    fn a_section_aside_ends_its_title_row(cx: &mut TestAppContext) {
        let mut cx = open(cx, size(300.0, 200.0), |_, cx| {
            gpui::div()
                .w(gpui::px(300.0))
                .child(super::detail_section_with_aside(
                    "Run summary",
                    crate::icons::Icon::FileText,
                    gpui::div()
                        .debug_selector(|| "aside".into())
                        .size(gpui::px(26.0)),
                    gpui::div()
                        .debug_selector(|| "body".into())
                        .h(gpui::px(10.0)),
                    cx,
                ))
                .into_any_element()
        });
        draw(&mut cx);
        let aside = cx.debug_bounds("aside").expect("the aside draws");
        assert_eq!(f32::from(aside.right()), 300.0);
        assert_eq!(f32::from(aside.top()), 0.0);
        // The row grows past its 20px to the aside; the body stays 10px under it.
        let body = cx.debug_bounds("body").expect("the body draws");
        assert_eq!(f32::from(body.top()), 26.0 + 10.0);
    }

    fn body(label: &'static str) -> DetailPanelBody {
        Rc::new(move |_: &mut gpui::Window, _: &mut gpui::App| {
            gpui::div().child(label).into_any_element()
        })
    }

    #[test]
    fn the_panel_keeps_its_place_across_records_and_leaves_before_it_unmounts() {
        let ready = DetailPanelPresence::new(true);
        assert!(ready.is_open());
        assert!((ready.value() - 1.0).abs() < f32::EPSILON);
        assert!(!ready.is_present());

        let mut presence = DetailPanelPresence::new(false);
        assert!(!presence.is_present());
        presence.set(Some(body("one")));
        assert!(presence.is_open());
        assert!(presence.is_present());
        std::thread::sleep(Duration::from_millis(50));
        let mid = presence.value();
        assert!(mid > 0.05, "the slow spring has left the start, was {mid}");
        presence.set(Some(body("two")));
        let swapped = presence.value();
        assert!(
            swapped + 0.02 >= mid,
            "a new record does not replay the slide ({mid} -> {swapped})"
        );
        assert!(presence.is_open());
        presence.set(None);
        assert!(!presence.is_open());
        assert!(
            presence.is_present(),
            "the last record stays mounted while it leaves"
        );
        std::thread::sleep(Duration::from_millis(200));
        assert!(!presence.is_present());
    }

    #[gpui::test]
    fn the_panel_slides_in_and_lets_the_list_take_presses_while_it_leaves(
        cx: &mut TestAppContext,
    ) {
        let hits = Rc::new(Cell::new(0));
        let wheels = Rc::new(Cell::new(0));
        let closes = Rc::new(Cell::new(0));
        let shown = Rc::new(Cell::new(true));
        let slot: Rc<OnceCell<gpui::Entity<DetailPanelPresence>>> = Rc::new(OnceCell::new());

        let hits_b = hits.clone();
        let wheels_b = wheels.clone();
        let closes_b = closes.clone();
        let shown_b = shown.clone();
        let slot_b = slot.clone();
        let mut cx = open(cx, size(1000.0, 600.0), move |window, cx| {
            let presence = slot_b
                .get_or_init(|| cx.new(|_| DetailPanelPresence::new(false)))
                .clone();
            let closes = closes_b.clone();
            let panel = shown_b.get().then(|| {
                let closes = closes.clone();
                Rc::new(move |_: &mut gpui::Window, _: &mut gpui::App| {
                    DetailPanel::new("panel", "Rename export snapshot")
                        .on_close(Rc::new({
                            let closes = closes.clone();
                            move |_, _| closes.set(closes.get() + 1)
                        }))
                        .into_any_element()
                }) as DetailPanelBody
            });
            presence.update(cx, |presence, _| presence.set(panel));
            let panel = presence.update(cx, |presence, cx| presence.element(window, cx));
            let hits = hits_b.clone();
            let wheels = wheels_b.clone();
            gpui::div()
                .relative()
                .size_full()
                .child(
                    gpui::div()
                        .size_full()
                        .on_mouse_down(MouseButton::Left, move |_, _, _| hits.set(hits.get() + 1))
                        .on_scroll_wheel(move |_, _, _| wheels.set(wheels.get() + 1)),
                )
                .children(panel)
                .into_any_element()
        });

        let close = cx.debug_bounds("panel-close").expect("Close draws on the way in");
        assert!(
            f32::from(close.right()) > 980.0 + 4.0,
            "it starts 16px to the right of rest, right edge {}",
            f32::from(close.right())
        );
        settle(&mut cx);
        let close = cx.debug_bounds("panel-close").expect("Close draws at rest");
        assert_eq!(f32::from(close.right()), 1000.0 - 8.0 - 12.0);

        let wheel = |at: gpui::Point<gpui::Pixels>| ScrollWheelEvent {
            position: at,
            delta: ScrollDelta::Pixels(point(px(0.0), px(-40.0))),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
        };
        cx.simulate_event(wheel(close.center()));
        assert_eq!(wheels.get(), 0, "an open panel takes the wheel");
        cx.simulate_click(close.center(), Modifiers::none());
        assert_eq!(closes.get(), 1);
        assert_eq!(hits.get(), 0, "an open panel takes the press");

        shown.set(false);
        draw(&mut cx);
        let (present, open) = cx.read(|app| {
            let presence = slot.get().unwrap().read(app);
            (presence.is_present(), presence.is_open())
        });
        assert!(present, "the last record stays mounted while it leaves");
        assert!(!open);
        let close = cx
            .debug_bounds("panel-close")
            .expect("Close stays through the exit");
        cx.simulate_event(wheel(close.center()));
        cx.simulate_click(close.center(), Modifiers::none());
        assert_eq!(closes.get(), 1, "Close takes nothing while the panel leaves");
        assert_eq!(wheels.get(), 1, "the list takes the wheel while the panel leaves");
        assert_eq!(hits.get(), 1, "the list takes the press while the panel leaves");

        // debug_bounds keeps a selector after the element is gone, so the presence is the
        // record of whether this frame still drew it.
        std::thread::sleep(Duration::from_millis(200));
        draw(&mut cx);
        let (value, present) = cx.read(|app| {
            let presence = slot.get().unwrap().read(app);
            (presence.value(), presence.is_present())
        });
        assert!(!present, "it unmounts when the exit tween ends");
        assert!(value < 0.01, "the exit tween has arrived, was {value}");
    }
}
