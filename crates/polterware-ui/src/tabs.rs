//! `Tabs` (`tabs.tsx`, `tabs.css`): section tabs, such as a page toolbar's All, Human, and
//! Agent. A recessed track (`.pw-tabs`: the field face, the 0.5px well ring, 14px round, 2px
//! in, the tabs 2px apart) holds one pill per tab (`.pw-tab`: 26px, `2px 12px` in, 11px round,
//! 12px text). The active tab sits on the lit face of a small control (`--material-face-*`,
//! the control drop and edge) that glides between tabs on the moderate spring; the others are
//! muted, secondary under the pointer on a hover pill. A label takes the semibold weight while
//! active, in the width its semibold copy reserves, so its neighbours never move.
//!
//! Chromium draws the labels from the variable Inter at wght 450 (medium) and 550 (semibold),
//! 0.12px tighter per letter (`letter-spacing: -0.01em`); the library has the static 500 and
//! 600 faces and no letter spacing, so the reserved width is the 550 copy's: halfway between
//! the two faces, less the tracking.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    App, Bounds, ElementId, FontWeight, IntoElement, Pixels, SharedString, Window, canvas, div,
    prelude::*, px,
};

use crate::ValueHandler;
use crate::data_table::{sans, text_width};
use crate::material::{self, Edge, face};
use crate::motion::{SpringTier, SpringValue};
use crate::pointer::{track, use_pointer};
use crate::theme::{Theme, mix};

/// `calc(var(--control-height-md) - 6px)`.
const TAB_HEIGHT: f32 = 26.0;
const TRACK_PAD: f32 = 2.0;
const TRACK_RADIUS: f32 = 14.0;
const TAB_RADIUS: f32 = 11.0;
const TEXT_SIZE: f32 = 12.0;
/// `letter-spacing: -0.01em` at 12px.
const TRACKING: f32 = -0.12;

/// The track's state: where each tab laid out, and the glide.
struct Glide {
    tabs: Rc<RefCell<Vec<Option<Bounds<Pixels>>>>>,
    x: SpringValue,
    w: SpringValue,
    placed: bool,
    last: std::time::Instant,
}

#[derive(IntoElement)]
pub struct Tabs {
    id: SharedString,
    labels: Vec<SharedString>,
    selected: usize,
    on_select: Option<ValueHandler<usize>>,
}

impl Tabs {
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            labels: Vec::new(),
            selected: 0,
            on_select: None,
        }
    }

    pub fn tab(mut self, label: impl Into<SharedString>) -> Self {
        self.labels.push(label.into());
        self
    }

    /// The active tab's index.
    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    /// Called with the index of a tab clicked while it was not the active one.
    pub fn on_select(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Tabs {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let count = self.labels.len();
        let glide = window.use_keyed_state(
            ElementId::Name(format!("{}-glide", self.id).into()),
            cx,
            |_, _| Glide {
                tabs: Rc::new(RefCell::new(Vec::new())),
                x: SpringValue::new(0.0, SpringTier::Moderate),
                w: SpringValue::new(0.0, SpringTier::Moderate),
                placed: false,
                last: std::time::Instant::now(),
            },
        );
        // The pill's target: the active tab as the last frame laid it out, from the track's
        // padding box.
        let (pill, moving, tabs) = glide.update(cx, |glide, _| {
            glide.tabs.borrow_mut().resize(count, None);
            let tabs = glide.tabs.clone();
            let origin = tabs
                .borrow()
                .first()
                .copied()
                .flatten()
                .map(|first| first.origin.x);
            let target = tabs
                .borrow()
                .get(self.selected)
                .copied()
                .flatten()
                .zip(origin)
                .map(|(bounds, origin)| {
                    (
                        f32::from(bounds.origin.x - origin),
                        f32::from(bounds.size.width),
                    )
                });
            let now = std::time::Instant::now();
            let dt = (now - glide.last).as_secs_f32().min(1.0 / 20.0);
            glide.last = now;
            let mut moving = false;
            if let Some((x, w)) = target {
                if glide.placed {
                    glide.x.set_target(x);
                    glide.w.set_target(w);
                    moving |= glide.x.step(dt);
                    moving |= glide.w.step(dt);
                } else {
                    glide.x.snap(x);
                    glide.w.snap(w);
                    glide.placed = true;
                }
            }
            (
                (glide.placed).then(|| (glide.x.value(), glide.w.value())),
                moving,
                tabs,
            )
        });
        // A frame more while it glides, or until the tabs have laid out once.
        if moving || (pill.is_none() && count > 0) {
            window.request_animation_frame();
        }

        let mut row = div().relative().flex().items_center().gap(px(TRACK_PAD));
        if let Some((x, w)) = pill {
            row = row.child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(x))
                    .w(px(w))
                    .h(px(TAB_HEIGHT))
                    .rounded(px(TAB_RADIUS))
                    .bg(face(theme.material_face_top, theme.material_face_bottom))
                    .shadow(material::control_drop(&theme))
                    .child(material::edge(Edge::Strong, TAB_RADIUS, &theme)),
            );
        }
        for (index, label) in self.labels.into_iter().enumerate() {
            let id = ElementId::NamedInteger(self.id.clone(), index as u64);
            let pointer = use_pointer(id.clone(), window, cx);
            let active = index == self.selected;
            let (hover, hover_color) = {
                let state = pointer.read(cx);
                (state.hover(), state.hover_color())
            };
            let ink = if active {
                theme.color_text_primary
            } else {
                mix(
                    theme.color_text_muted,
                    theme.color_text_secondary,
                    hover_color,
                )
            };
            let mut hover_fill = theme.color_surface_hover;
            hover_fill.a *= if active { 0.0 } else { hover };
            let slots = tabs.clone();
            let medium = text_width(&label, sans(&theme, FontWeight::MEDIUM), TEXT_SIZE, window);
            let semibold = text_width(
                &label,
                sans(&theme, FontWeight::SEMIBOLD),
                TEXT_SIZE,
                window,
            );
            let reserved = (medium + semibold) / 2.0 + TRACKING * label.chars().count() as f32;
            let selector = format!("{}-{index}", self.id);
            let mut tab = div()
                .id(id)
                // Tests find a tab as `<id>-<index>` (a no-op outside gpui's test support).
                .debug_selector(move || selector.clone())
                .relative()
                .flex()
                .items_center()
                .justify_center()
                .h(px(TAB_HEIGHT))
                .px(px(13.0))
                .rounded(px(TAB_RADIUS))
                .bg(hover_fill)
                .text_size(px(TEXT_SIZE))
                .line_height(px(14.4))
                .text_color(ink)
                .whitespace_nowrap()
                // The semibold copy's width holds the label, which shows centred over it.
                .child(
                    div().relative().w(px(reserved)).h(px(14.4)).child(
                        div()
                            .absolute()
                            .top_0()
                            .left(px(-TAB_RADIUS))
                            .right(px(-TAB_RADIUS))
                            .flex()
                            .justify_center()
                            .font_weight(if active {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::MEDIUM
                            })
                            .child(label),
                    ),
                )
                .child(
                    canvas(
                        move |bounds, _, _| {
                            if let Some(slot) = slots.borrow_mut().get_mut(index) {
                                *slot = Some(bounds);
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                );
            if !active {
                tab = tab.cursor_pointer();
                if let Some(handler) = self.on_select.clone() {
                    tab = tab.on_click(move |_, window, cx| handler(index, window, cx));
                }
            }
            row = row.child(track(tab, &pointer, !active));
        }
        div()
            .id(ElementId::Name(self.id))
            .relative()
            .flex_none()
            .p(px(TRACK_PAD))
            .rounded(px(TRACK_RADIUS))
            .bg(face(theme.field_fill_top, theme.field_fill_bottom))
            .child(row)
            .child(material::edge(Edge::Well, TRACK_RADIUS, &theme))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{Modifiers, TestAppContext, prelude::*, size};

    use super::Tabs;
    use crate::test_support::{draw, open};

    #[gpui::test]
    fn a_click_on_another_tab_selects_it_and_the_active_one_keeps(cx: &mut TestAppContext) {
        let picked: Rc<RefCell<Vec<usize>>> = Rc::default();
        let sink = picked.clone();
        let mut cx = open(cx, size(600.0, 200.0), move |_, _| {
            let sink = sink.clone();
            Tabs::new("actors")
                .tab("All")
                .tab("Human")
                .tab("Agent")
                .selected(0)
                .on_select(move |index, _, _| sink.borrow_mut().push(index))
                .into_any_element()
        });
        draw(&mut cx);
        let all = cx.debug_bounds("actors-0").expect("All draws");
        let human = cx.debug_bounds("actors-1").expect("Human draws");
        let agent = cx.debug_bounds("actors-2").expect("Agent draws");
        // 26px pills 2px apart, in order.
        assert_eq!(f32::from(human.size.height), 26.0);
        assert_eq!(f32::from(human.left() - all.right()), 2.0);
        assert!(agent.left() > human.right());
        cx.simulate_click(agent.center(), Modifiers::none());
        cx.simulate_click(all.center(), Modifiers::none());
        assert_eq!(*picked.borrow(), vec![2]);
    }
}
