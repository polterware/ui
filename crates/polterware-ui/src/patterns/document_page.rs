//! `RailPage` as a document opens in it (`rail-page.tsx`, `rail-page.css`, with no `wide`,
//! `splitScroll`, or `railCard`), as Polterware's Knowledge decisions and docs and its Work
//! cards are pages: the 44px [`rail_page_header`] with Back (a texture icon button) at the
//! start and, at the end, a quiet status, the page's actions, and the rail's toggle, pressed
//! while the rail shows. Under it the page scrolls as one, padded `0 62px 64px`: a column at
//! most 654px wide, centered in the room the rail leaves, then 48px, then the 376px rail of
//! properties, 48px down. The web rail is sticky at the scroll's top, which keeps it where it
//! starts, so it is drawn over the scroll there and the column keeps its room; a rail taller
//! than the window is cut at its bottom.
//!
//! The rail holds [`document_group`]s, the `RailGroup` that does not fold: a 24px neutral
//! icon tile and the 13px/500 title, then the content 6px under, the group padded
//! `0 0 10px 4px` and 20px above the next. Its rows are [`rail_property`]s in
//! [`rail_properties`](super::rail_properties); [`document_footer`] ends the rail with its
//! actions under a hairline. [`document_title`] is the column's 24px title, read-only.
//! [`rail_select`] is a `Select` in the rail, a ghost pill instead of a field.
//!
//! It behaves like the Dispatch run's [`RailPage`](super::rail_page): the host holds a
//! [`RailState`] that the toggle flips. The rail comes and goes as the web's does: it slides
//! 16px and fades on `spring.slow` (a tween on its exit), and its room in the row closes with
//! it, so the column glides to the middle instead of jumping; its content keeps its full width
//! and leaves past the page's right edge, where the web also fades it with a mask GPUI does
//! not have. Below 900px the rail leaves the row and opens as a drawer over the text, closed
//! until its toggle opens it: on the floating material at the card radius, 52px from the top
//! and 8px in from the other edges, at most 376px wide, padded `16px 8px`, scrolling on its
//! own and taking the pointer and the wheel from the text under it.

use gpui::{
    AnyElement, App, Div, ElementId, Entity, FontWeight, IntoElement, RenderOnce, SharedString,
    Window, div, prelude::*, px,
};

use crate::Handler;
use crate::button::{Button, ButtonKind, ButtonSize};
use crate::icon_tile::{IconTile, TileTone};
use crate::icons::{Icon, icon_colored};
use crate::scroll::scroll_area;
use crate::theme::Theme;

use super::rail_page::{RAIL_WIDTH, RailState, rail_page_header, rail_toggle_label};
use crate::material::{Edge, edge, float_drop};

/// `.pw-document__column`'s measure.
pub const DOCUMENT_COLUMN: f32 = 654.0;

/// A rail button is `variant="texture" size="sm"`, which keeps `rounded-[6px]` where the
/// toolbars' buttons are round-ended.
pub const RAIL_BUTTON_RADIUS: f32 = 6.0;
/// The scroll row's gap between the column and the rail.
const RAIL_GAP: f32 = 48.0;
/// The scroll's side padding.
const PAGE_PADDING: f32 = 62.0;
/// The scroll's bottom padding.
const PAGE_BOTTOM: f32 = 64.0;
/// `.pw-document__rail`'s padding-top.
const RAIL_TOP: f32 = 48.0;
/// How far the rail slides as it comes and goes.
const TRAVEL: f32 = 16.0;
/// The narrow drawer: 52px from the page's top, 8px in from its other edges.
const DRAWER_TOP: f32 = 52.0;
const DRAWER_INSET: f32 = 8.0;

#[derive(IntoElement)]
pub struct DocumentPage {
    id: ElementId,
    on_back: Option<Handler>,
    status: Option<AnyElement>,
    actions: Vec<AnyElement>,
    rail: Vec<AnyElement>,
    rail_state: Option<Entity<RailState>>,
    children: Vec<AnyElement>,
}

impl DocumentPage {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            on_back: None,
            status: None,
            actions: Vec::new(),
            rail: Vec::new(),
            rail_state: None,
            children: Vec::new(),
        }
    }

    /// Back to the list.
    pub fn on_back(mut self, handler: Handler) -> Self {
        self.on_back = Some(handler);
        self
    }

    /// A quiet live status in the header, such as "Saving…".
    pub fn status(mut self, status: impl Into<SharedString>) -> Self {
        let status = status.into();
        if !status.is_empty() {
            self.status = Some(status.into_any_element());
        }
        self
    }

    /// A header control before the rail's toggle.
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }

    /// A group of the rail; a page with none has no rail and no toggle.
    pub fn rail(mut self, group: impl IntoElement) -> Self {
        self.rail.push(group.into_any_element());
        self
    }

    /// What the rail's toggle flips; without one the rail shows, at rest, and the toggle does
    /// nothing.
    pub fn rail_state(mut self, state: &Entity<RailState>) -> Self {
        self.rail_state = Some(state.clone());
        self
    }

    /// The column's content, in order.
    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.children
            .extend(children.into_iter().map(IntoElement::into_any_element));
        self
    }
}

impl RenderOnce for DocumentPage {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let has_rail = !self.rail.is_empty();
        // Where the rail is: whether it shows, how far in (0 gone, 1 in place), whether it
        // still draws, and whether the page is narrow and how wide.
        let (visible, progress, present, narrow, width) = match &self.rail_state {
            Some(state) => state.update(cx, |state, _| {
                let presence = state.presence();
                let (progress, present, animating) = (
                    presence.value(),
                    presence.is_present(),
                    presence.is_animating(),
                );
                if animating {
                    window.request_animation_frame();
                }
                (
                    state.is_visible(),
                    progress,
                    present,
                    state.page().is_narrow(),
                    state.page().width(),
                )
            }),
            None => (true, 1.0, true, false, 0.0),
        };
        let rail_shown = has_rail && visible;
        let present = has_rail && present;
        let back = self.on_back.map(|on_back| {
            Button::new((self.id.clone(), "back"), ButtonKind::Secondary)
                .size(ButtonSize::Icon)
                .icon(Icon::ChevronLeft)
                .tooltip("Back")
                .on_click_rc(on_back)
        });
        let toggle = has_rail.then(|| {
            let button = Button::new((self.id.clone(), "rail"), ButtonKind::Secondary)
                .size(ButtonSize::Icon)
                .icon(Icon::SidebarRightIcon)
                .tooltip(rail_toggle_label(rail_shown))
                .selected(rail_shown);
            match self.rail_state.clone() {
                Some(state) => button.on_click(move |window, cx| {
                    state.update(cx, |state, _| state.toggle());
                    // The page is the host's view: draw it again for the rail's new place.
                    window.refresh();
                }),
                None => button,
            }
        });
        let end = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .children(self.status.map(|status| {
                div()
                    .text_size(px(12.0))
                    .text_color(theme.color_text_muted)
                    .child(status)
            }))
            .children(self.actions)
            .children(toggle);
        let header = rail_page_header(div().children(back), end, cx);
        // The column, centered in the room the rail leaves, and that room after it.
        let row = div()
            .w_full()
            .flex()
            .items_start()
            .px(px(PAGE_PADDING))
            .pb(px(PAGE_BOTTOM))
            .child(
                div().flex_1().min_w(px(0.0)).flex().justify_center().child(
                    div()
                        .w_full()
                        .max_w(px(DOCUMENT_COLUMN))
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .children(self.children),
                ),
            )
            // Beside the text the rail's room closes with it, the gap before it too.
            .when(present && !narrow, |row| {
                row.child(
                    div()
                        .flex_none()
                        .w(px((RAIL_GAP + RAIL_WIDTH) * progress.max(0.0))),
                )
            });
        let scroll = scroll_area((self.id.clone(), "scroll"), row, window, cx);
        let offset = TRAVEL * (1.0 - progress);
        let opacity = progress.clamp(0.0, 1.0);
        let rail = present.then(|| {
            if narrow {
                // Over the text, the drawer takes the pointer and the wheel.
                return div()
                    .absolute()
                    .occlude()
                    .top(px(DRAWER_TOP))
                    .right(px(DRAWER_INSET - offset))
                    .bottom(px(DRAWER_INSET))
                    .w(px(RAIL_WIDTH.min(width - 2.0 * DRAWER_INSET).max(0.0)))
                    .opacity(opacity)
                    .rounded(px(theme.radius_card))
                    .bg(theme.color_surface_subtle)
                    .shadow(float_drop(&theme))
                    .child(
                        div()
                            .id((self.id.clone(), "drawer"))
                            .size_full()
                            .overflow_y_scroll()
                            .py(px(16.0))
                            .px(px(8.0))
                            .child(div().w_full().flex().flex_col().children(self.rail)),
                    )
                    .child(edge(Edge::Quiet, theme.radius_card, &theme));
            }
            // Its content keeps its full width and leaves past the page's right edge as the
            // room closes.
            let room = (RAIL_WIDTH * progress).clamp(0.0, RAIL_WIDTH);
            div()
                .absolute()
                .top(px(theme.page_header_height + RAIL_TOP))
                .right(px(PAGE_PADDING - (RAIL_WIDTH - room) - offset))
                .bottom(px(0.0))
                .w(px(RAIL_WIDTH))
                .opacity(opacity)
                .overflow_hidden()
                .flex()
                .flex_col()
                .children(self.rail)
        });
        let probe = self
            .rail_state
            .as_ref()
            .map(|state| state.read(cx).page().probe());
        div()
            .id(self.id)
            .relative()
            .size_full()
            .overflow_hidden()
            .flex()
            .flex_col()
            .children(probe)
            .child(header)
            .child(div().flex_1().min_h(px(0.0)).child(scroll))
            .children(rail)
    }
}

/// `RailGroup` that does not fold: the tiled heading over `content`.
pub fn document_group(
    title: impl Into<SharedString>,
    icon: Icon,
    content: impl IntoElement,
    cx: &App,
) -> Div {
    document_group_with(title, icon, None, false, content, cx)
}

/// [`document_group`] with its `aside` at the heading's end (`RailGroup`'s `aside`, such as a
/// count chip or a button); `in_column` sets the content 12px under the heading, as a group in
/// the text column sits (`.pw-card-work > .pw-document__group`), where the rail's is 6px.
pub fn document_group_with(
    title: impl Into<SharedString>,
    icon: Icon,
    aside: Option<AnyElement>,
    in_column: bool,
    content: impl IntoElement,
    cx: &App,
) -> Div {
    let theme = Theme::global(cx);
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(if in_column { 12.0 } else { 6.0 }))
        .when(!in_column, |group| group.mb(px(20.0)))
        .pl(px(4.0))
        .pb(px(10.0))
        .child(
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(4.0))
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .child(IconTile::new(icon).size(24.0, 14.0).tone(TileTone::Neutral)),
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
                        .child(title.into()),
                )
                .children(aside.map(|aside| div().flex_none().flex().items_center().child(aside))),
        )
        .child(content)
}

/// `Select` in the rail (`[data-variant="rail"] dd .control-field`): the field's chrome gives
/// way to a ghost pill as wide as its label and 14px muted chevron (0.5rem apart), 28px tall,
/// padded `0 10px 0 6px` and pulled 6px back to the value's edge, filling with the hover
/// surface under the pointer. It is `relative()`, for a popover's anchor probe.
pub fn rail_select(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on_click: Handler,
    cx: &App,
) -> gpui::Stateful<Div> {
    let theme = Theme::global(cx);
    let hover = theme.color_surface_hover;
    div()
        .id(id.into())
        .relative()
        .flex_none()
        .max_w_full()
        .h(px(28.0))
        .ml(px(-6.0))
        .pl(px(6.0))
        .pr(px(10.0))
        .flex()
        .items_center()
        .gap(px(6.5))
        .rounded_full()
        .cursor_pointer()
        .text_size(px(13.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.color_text_primary)
        .hover(move |style| style.bg(hover))
        .on_click(move |_, window, cx| on_click(window, cx))
        .child(
            div()
                .min_w(px(0.0))
                .overflow_hidden()
                .whitespace_nowrap()
                .child(label.into()),
        )
        .child(icon_colored(
            Icon::ChevronDown,
            14.0,
            theme.color_text_muted,
        ))
}

/// `.pw-document__group-body`: a group's controls in a wrapping row, 6px apart and 4px in
/// from the group's edges, as Labels holds its tags.
pub fn document_group_body(content: impl IntoElement) -> Div {
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(6.0))
        .px(px(4.0))
        .child(content)
}

/// `.pw-document__rail-footer`: the rail's last actions, 6px apart, padded `12px 8px 0`
/// under a 0.5px hairline.
pub fn document_footer(actions: impl IntoIterator<Item = AnyElement>, cx: &App) -> Div {
    let theme = Theme::global(cx);
    div()
        .w_full()
        .flex()
        .flex_wrap()
        .gap(px(6.0))
        .pt(px(12.0))
        .px(px(8.0))
        .border_t(px(0.5))
        .border_color(theme.color_border_default)
        .children(actions)
}

/// `.pw-document__title`: 24px/600 on a 32px line, 32px under the header and 8px over what
/// follows.
pub fn document_title(title: impl Into<SharedString>, cx: &App) -> Div {
    let theme = Theme::global(cx);
    div()
        .w_full()
        .mt(px(32.0))
        .mb(px(8.0))
        .text_size(px(24.0))
        .line_height(px(32.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.color_text_primary)
        .child(crate::wrap::wrap_text(title.into()))
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use gpui::{TestAppContext, size};

    use super::*;
    use crate::patterns::rail_page::{rail_properties, rail_property};
    use crate::test_support::{draw, open};

    #[gpui::test]
    fn the_column_centers_in_the_room_the_rail_leaves(cx: &mut TestAppContext) {
        let mut cx = open(cx, size(1400.0, 800.0), |_, cx| {
            DocumentPage::new("page")
                .on_back(Rc::new(|_, _| {}))
                .rail(document_group(
                    "Properties",
                    Icon::InformationCircle,
                    rail_properties().child(rail_property(
                        "Status",
                        div().debug_selector(|| "value".into()).child("Active"),
                        false,
                        cx,
                    )),
                    cx,
                ))
                .child(div().debug_selector(|| "body".into()).h(px(20.0)))
                .into_any_element()
        });
        draw(&mut cx);
        let body = cx.debug_bounds("body").expect("the column draws");
        // 1400 less both 62px paddings and the rail's 48 + 376 leaves 852px: the 654px column
        // sits in its middle.
        let room = 1400.0 - 2.0 * PAGE_PADDING - RAIL_GAP - RAIL_WIDTH;
        assert_eq!(f32::from(body.size.width), DOCUMENT_COLUMN);
        assert_eq!(
            f32::from(body.left()),
            PAGE_PADDING + (room - DOCUMENT_COLUMN) / 2.0
        );
        // The rail ends 62px in from the right; a value starts past the group's 4px, the
        // 88px label, the 4px gap, and its own 6px.
        let value = cx.debug_bounds("value").expect("the property draws");
        let rail_left = 1400.0 - PAGE_PADDING - RAIL_WIDTH;
        assert_eq!(f32::from(value.left()), rail_left + 4.0 + 88.0 + 4.0 + 6.0);
    }

    #[gpui::test]
    fn a_hidden_rail_gives_the_column_the_page(cx: &mut TestAppContext) {
        let state = cx.update(|cx| cx.new(|_| RailState::new(false)));
        let held = state.clone();
        let mut cx = open(cx, size(900.0, 600.0), move |_, _| {
            DocumentPage::new("page")
                .rail_state(&held)
                .rail(div().debug_selector(|| "rail".into()).child("Rail"))
                .child(div().debug_selector(|| "body".into()).h(px(20.0)))
                .into_any_element()
        });
        draw(&mut cx);
        assert!(cx.debug_bounds("rail").is_none());
        let body = cx.debug_bounds("body").expect("the column draws");
        assert_eq!(f32::from(body.size.width), DOCUMENT_COLUMN);
        assert_eq!(f32::from(body.left()), (900.0 - DOCUMENT_COLUMN) / 2.0);
        assert!(!state.read_with(&cx, |state, _| state.is_visible()));
    }

    #[gpui::test]
    fn a_narrow_page_opens_the_rail_as_a_drawer_over_the_text(cx: &mut TestAppContext) {
        let state = cx.update(|cx| cx.new(|_| RailState::new(true)));
        let held = state.clone();
        let mut cx = open(cx, size(800.0, 600.0), move |_, _| {
            DocumentPage::new("page")
                .rail_state(&held)
                .rail(div().debug_selector(|| "rail".into()).child("Rail"))
                .child(div().debug_selector(|| "body".into()).h(px(20.0)))
                .into_any_element()
        });
        draw(&mut cx);
        draw(&mut cx);
        // Under 900px the drawer starts closed, and the column takes the page.
        assert!(state.read_with(&cx, |state, _| state.page().is_narrow()));
        assert!(!state.read_with(&cx, |state, _| state.is_visible()));
        let body = cx.debug_bounds("body").expect("the column draws");
        assert_eq!(f32::from(body.left()), (800.0 - DOCUMENT_COLUMN) / 2.0);
        // The toggle opens the drawer; the choice beside the text stays as it was.
        state.update(&mut cx, |state, _| state.toggle());
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
        draw(&mut cx);
        assert!(state.read_with(&cx, |state, _| state.is_visible() && state.is_open()));
        // 376px less its 8px sides; it slides in from 16px further right.
        let rail = cx.debug_bounds("rail").expect("the drawer draws");
        assert_eq!(f32::from(rail.size.width), RAIL_WIDTH - 16.0);
        let end = 800.0 - DRAWER_INSET - 8.0;
        assert!((end..=end + TRAVEL).contains(&f32::from(rail.right())));
        let body = cx.debug_bounds("body").expect("the column stays");
        assert_eq!(f32::from(body.left()), (800.0 - DOCUMENT_COLUMN) / 2.0);
    }
}
