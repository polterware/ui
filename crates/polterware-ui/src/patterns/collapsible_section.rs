//! `CollapsibleSection` (`detail-panel.tsx`): a titled block whose heading folds its body.
//!
//! The heading is a toggle, `py-1` with 6px gaps: an optional icon in the secondary color, the
//! title (12px/500 muted at line-height 1.35, primary while the toggle is hovered), an optional
//! aside (12px muted figures, such as a count), and a 14px muted chevron right after it that turns −90° when closed over `--duration-control` on
//! `--ease-standard`. While open the heading has `pb-4` under it. The body, padded 10px on top,
//! is a flex column with 10px gaps inside a [`SectionClip`].
//!
//! Actions, when set, sit at the heading's end beside the toggle, so they never fold it. A
//! document's section ([`CollapsibleSection::set_large_title`]) sets its title 16px/600 in
//! primary ink, 12px in, with 6px under the heading while open.
//!
//! The body is a `'static` builder because a section cannot update another entity while it
//! is rendering. Read other entities inside the builder.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, FontWeight, IntoElement, Render, SharedString, Window, canvas, div,
    prelude::*, px,
};

use crate::icon_tile::{IconTile, TileTone};
use crate::icons::{Icon, icon_colored, icon_rotated};
use crate::motion::{Curve, SpringTier, Tween};
use crate::theme::{Theme, tw};
use crate::{sans, tabular};

pub type SectionBody = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// `DetailPanelSectionBody`: the clip a section's body folds into. Closing tweens its height
/// from where it is to 0 on the moderate tier's exit; opening springs from where it is to the
/// body's measured height on the moderate spring, so a body that was fully closed (and
/// hidden, which the browser measures already open) appears at once. While it moves, its
/// opacity is the height over the larger end; at rest it is the body's own height, or nothing.
/// The first paint snaps.
pub struct SectionClip {
    open: bool,
    height: Tween,
    /// The larger end of the move in progress, which the opacity is measured against.
    span: f32,
    /// The body's height, as the last frame laid it out.
    measured: Rc<Cell<f32>>,
}

impl SectionClip {
    pub fn new(open: bool) -> Self {
        Self {
            open,
            height: Tween::new(0.0),
            span: 1.0,
            measured: Rc::new(Cell::new(0.0)),
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Whether the body draws this frame: open, or still folding.
    pub fn shows_body(&self) -> bool {
        self.open || self.height.is_animating()
    }

    pub fn set_open(&mut self, open: bool) {
        if open == self.open {
            return;
        }
        let moving = self.height.is_animating();
        let from = if moving {
            self.height.value()
        } else if self.open {
            self.measured.get()
        } else {
            0.0
        };
        self.open = open;
        if open {
            if !moving {
                // From hidden the browser reads the clip already at its full height.
                return;
            }
            let to = self.measured.get();
            self.span = to.max(1.0);
            self.height.snap(from);
            self.height.animate_to(
                to,
                SpringTier::Moderate.duration(),
                SpringTier::Moderate.curve(),
            );
        } else {
            self.span = from.max(1.0);
            self.height.snap(from);
            self.height.animate_to(
                0.0,
                SpringTier::Moderate.exit_duration(),
                SpringTier::Moderate.exit_curve(),
            );
        }
    }

    /// The clip around `body`, which is `None` when [`Self::shows_body`] is false.
    pub fn render(&mut self, body: Option<AnyElement>, window: &mut Window) -> AnyElement {
        let Some(body) = body.filter(|_| self.shows_body()) else {
            return div().into_any_element();
        };
        let measured = self.measured.clone();
        let probe = canvas(
            move |bounds, _, _| measured.set(f32::from(bounds.size.height)),
            |_, _, _, _| {},
        )
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .size_full();
        // A column, so the body takes the section's width: in a row it would size to its own
        // content, and a body that sizes itself to its width (a table) would never grow back
        // from the nothing it had while folded.
        let inner = div()
            .relative()
            .w_full()
            .flex()
            .flex_col()
            .child(body)
            .child(probe);
        let clip = div().w_full().min_w(px(0.0));
        if !self.height.is_animating() {
            return clip.child(inner).into_any_element();
        }
        window.request_animation_frame();
        let value = self.height.value().max(0.0);
        clip.h(px(value))
            .overflow_hidden()
            .opacity((value / self.span).clamp(0.0, 1.0))
            .child(inner)
            .into_any_element()
    }
}

/// What leads a section's title: an `IconTile`, as Stats passes one, or a bare mark.
#[derive(Clone, Copy)]
enum SectionIcon {
    Tile(Icon, TileTone),
    Plain(Icon),
}

pub struct CollapsibleSection {
    title: SharedString,
    icon: Option<SectionIcon>,
    aside: Option<SharedString>,
    clip: SectionClip,
    /// The chevron's turn, 0 open and −90° closed.
    chevron: Tween,
    body: SectionBody,
    /// Controls at the heading's end, outside the toggle: a card's own filters.
    actions: Option<SectionBody>,
    /// A document's section heading, set larger than its rows.
    large: bool,
    /// A canvas side panel's section, set tighter.
    compact: bool,
}

impl CollapsibleSection {
    /// An open section, as `defaultOpen` leaves it.
    pub fn new(title: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        let _ = cx;
        Self {
            title: title.into(),
            icon: None,
            aside: None,
            clip: SectionClip::new(true),
            chevron: Tween::new(0.0),
            body: Rc::new(|_, _| div().into_any_element()),
            actions: None,
            large: false,
            compact: false,
        }
    }

    /// A section of a document, as Structure's record sections are set
    /// (`.pw-structure__section`): the title 16px/600 in primary ink, the heading 12px in on
    /// each side, in line with the rows' text, and 6px over the body while open.
    pub fn set_large_title(&mut self, large: bool) {
        self.large = large;
    }

    /// A section of a canvas side panel (`.pw-canvas-panel-body`): its toggle 6px over and
    /// under, and its body's rows 8px apart.
    pub fn set_compact(&mut self, compact: bool) {
        self.compact = compact;
    }

    pub fn set_title(&mut self, title: impl Into<SharedString>) {
        self.title = title.into();
    }

    /// A 24px `IconTile` with a 12px mark before the title.
    pub fn set_icon(&mut self, icon: Icon, tone: TileTone) {
        self.icon = Some(SectionIcon::Tile(icon, tone));
    }

    /// A bare 14px mark in the secondary color before the title (`.pw-detail-panel__section-icon`).
    pub fn set_plain_icon(&mut self, icon: Icon) {
        self.icon = Some(SectionIcon::Plain(icon));
    }

    /// Muted text between the title and the chevron, such as a count.
    pub fn set_aside(&mut self, aside: Option<SharedString>) {
        self.aside = aside;
    }

    pub fn set_body(&mut self, body: SectionBody) {
        self.body = body;
    }

    /// Controls drawn at the heading's end, outside the toggle, so a click on them does not
    /// fold the section: the section's own selects or buttons.
    pub fn set_actions(&mut self, actions: Option<SectionBody>) {
        self.actions = actions;
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

impl Render for CollapsibleSection {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let open = self.clip.is_open();
        if self.chevron.is_animating() {
            window.request_animation_frame();
        }
        let group: SharedString = format!("section-toggle-{}", cx.entity_id()).into();
        let toggle = div()
            .id(("section-toggle", cx.entity_id()))
            .group(group.clone())
            .flex()
            .flex_1()
            .min_w(px(0.0))
            .items_center()
            .gap(px(6.0))
            .py(px(if self.compact { 6.0 } else { tw(1.0) }))
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                let open = !this.is_open();
                this.set_open(open, cx);
                cx.notify();
            }))
            .when_some(self.icon, |toggle, icon| {
                let mark = match icon {
                    SectionIcon::Tile(icon, tone) => IconTile::new(icon)
                        .size(24.0, 12.0)
                        .tone(tone)
                        .into_any_element(),
                    SectionIcon::Plain(icon) => {
                        icon_colored(icon, 14.0, theme.color_text_secondary).into_any_element()
                    }
                };
                toggle.child(div().flex().flex_none().child(mark))
            })
            .child(
                div()
                    .flex_shrink()
                    .min_w(px(0.0))
                    // One line: a title free to wrap here collapsed to a letter per line in
                    // GPUI's flex layout, where the browser keeps a short title whole.
                    .whitespace_nowrap()
                    .map(|title| {
                        if self.large {
                            title
                                .text_size(px(16.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .line_height(px(16.0 * 1.35))
                                .text_color(theme.color_text_primary)
                        } else {
                            title
                                .text_size(px(12.0))
                                .font_weight(FontWeight::MEDIUM)
                                .line_height(px(12.0 * 1.35))
                                .text_color(theme.color_text_muted)
                                .group_hover(group, |style| {
                                    style.text_color(theme.color_text_primary)
                                })
                        }
                    })
                    .child(self.title.clone()),
            )
            .when_some(self.aside.clone(), |toggle, aside| {
                toggle.child(
                    div()
                        .flex_none()
                        .font(tabular(sans(&theme, FontWeight::NORMAL)))
                        .text_size(px(12.0))
                        .text_color(theme.color_text_muted)
                        .child(aside),
                )
            })
            .child(icon_rotated(
                Icon::ChevronDown,
                14.0,
                theme.color_text_muted,
                self.chevron.value(),
            ));
        let body = self.clip.shows_body().then(|| {
            div()
                .w_full()
                .pt(px(10.0))
                .flex()
                .flex_col()
                .gap(px(if self.compact { 8.0 } else { 10.0 }))
                .min_w(px(0.0))
                .child((self.body)(window, cx))
                .into_any_element()
        });
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .min_w(px(0.0))
                    .items_center()
                    .gap(px(4.0))
                    .when(self.large, |heading| heading.px(px(12.0)))
                    .when(open, |heading| {
                        heading.pb(px(if self.large { 6.0 } else { tw(4.0) }))
                    })
                    .child(toggle)
                    .when_some(self.actions.clone(), |heading, actions| {
                        heading.child(div().flex_none().child(actions(window, cx)))
                    }),
            )
            .child(self.clip.render(body, window))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use gpui::{Bounds, Entity, Pixels, TestAppContext, size};

    use super::*;
    use crate::data_table::{Column, DataTable};
    use crate::test_support::{draw, open, settle};

    type Painted = Rc<RefCell<Vec<(usize, Bounds<Pixels>)>>>;

    /// A section whose body is a headless record table, as Structure's are, each row noting
    /// where it was laid out.
    fn record_section(
        cx: &mut TestAppContext,
        painted: Painted,
    ) -> (Entity<CollapsibleSection>, gpui::VisualTestContext) {
        let slot: Rc<RefCell<Option<Entity<CollapsibleSection>>>> = Rc::default();
        let made = slot.clone();
        let mut vcx = open(cx, size(600.0, 600.0), move |_, cx| {
            let section = made
                .borrow_mut()
                .get_or_insert_with(|| {
                    let painted = painted.clone();
                    cx.new(|cx| {
                        let mut section = CollapsibleSection::new("Rules", cx);
                        section.set_large_title(true);
                        section.set_body(Rc::new(move |_, _| {
                            let painted = painted.clone();
                            div()
                                .w_full()
                                .h(px(3.0 * 44.0 + 8.0))
                                .child(
                                    DataTable::new(
                                        "records",
                                        vec![Column::new("").shrinks()],
                                        3,
                                        move |row, _, _| {
                                            let painted = painted.clone();
                                            let index = row.index;
                                            vec![
                                                div()
                                                    .h(px(20.0))
                                                    .w_full()
                                                    .child(
                                                        canvas(
                                                            move |bounds, _, _| {
                                                                painted
                                                                    .borrow_mut()
                                                                    .push((index, bounds))
                                                            },
                                                            |_, _, _, _| {},
                                                        )
                                                        .size_full(),
                                                    )
                                                    .into_any_element(),
                                            ]
                                        },
                                    )
                                    .header(0.0, 11.0)
                                    .row_height(40.0)
                                    .gutters(0.0),
                                )
                                .into_any_element()
                        }));
                        section
                    })
                })
                .clone();
            div().size_full().child(section).into_any_element()
        });
        settle(&mut vcx);
        let section = slot.borrow().clone().expect("the section was built");
        (section, vcx)
    }

    fn shown_rows(painted: &Painted) -> Vec<usize> {
        let mut rows: Vec<usize> = painted
            .borrow()
            .iter()
            .filter(|(_, bounds)| bounds.size.width > px(300.0) && bounds.size.height > px(1.0))
            .map(|(row, _)| *row)
            .collect();
        rows.sort();
        rows.dedup();
        rows
    }

    #[gpui::test]
    fn a_table_body_shows_its_rows_again_after_closing_and_opening(cx: &mut TestAppContext) {
        let painted: Painted = Rc::default();
        let (section, mut cx) = record_section(cx, painted.clone());
        painted.borrow_mut().clear();
        draw(&mut cx);
        assert_eq!(shown_rows(&painted), vec![0, 1, 2], "rows show at first");

        for _ in 0..2 {
            cx.update(|_, cx| section.update(cx, |s, cx| s.set_open(false, cx)));
            settle(&mut cx);
            cx.update(|_, cx| section.update(cx, |s, cx| s.set_open(true, cx)));
            settle(&mut cx);
            painted.borrow_mut().clear();
            draw(&mut cx);
            assert_eq!(
                shown_rows(&painted),
                vec![0, 1, 2],
                "rows show after reopening"
            );
        }
    }
}
