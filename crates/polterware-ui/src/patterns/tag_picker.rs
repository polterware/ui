//! `TagPicker` (`tag-picker.tsx`, `tag-picker.css`): the free labels of a doc or a decision as
//! removable badges and an "Add tag" badge. Each badge is a 24px texture pill padded 9px in
//! the neutral face with 12px ink, its × in muted ink (primary under the pointer); Add tag
//! leads with a 10px plus. Opened, Add tag becomes a field in the badge, its frame in the
//! primary color, over a 220px floating list 4px under it: the suggestions the field's text
//! matches that are not picked yet, then Create “text” when no tag is named so, each row 32px
//! tall, padded 10px, in 13px secondary ink (primary on the highlighted row, which fills with
//! the hover surface); with nothing to list it says "Type to create a tag.".
//!
//! It behaves like the design system's and gpui-kit's `Popover` with a filtered list: typing
//! filters and highlights the first row, Enter takes the highlighted row, a click takes any,
//! and Escape or a press outside closes it. [`TagPickerEvent::Changed`] carries the tags after
//! a pick or a removal; the app writes them.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    App, Bounds, Context, Corner, Entity, EventEmitter, Focusable, IntoElement, MouseButton,
    Pixels, Render, SharedString, Subscription, Window, anchored, canvas, deferred, div, point,
    prelude::*, px,
};

use crate::icons::{Icon, icon_colored};
use crate::material::{Edge, edge, face, floating_panel};
use crate::text_input::TextInput;
use crate::theme::Theme;

/// `.pw-tag-badge`: 24px tall, 12px text.
const BADGE_HEIGHT: f32 = 24.0;
const BADGE_PAD: f32 = 9.0;
/// The list's width and its rows.
const LIST_WIDTH: f32 = 220.0;
const ROW_HEIGHT: f32 = 32.0;
/// The field's width in the badge, about the placeholder's.
const FIELD_WIDTH: f32 = 72.0;

/// The tags after a pick or a removal.
#[derive(Clone, Debug, PartialEq)]
pub enum TagPickerEvent {
    Changed(Vec<String>),
}

/// A row of the open list.
#[derive(Clone, Debug, PartialEq)]
pub enum TagRow {
    Pick(String),
    Create(String),
}

/// The rows `query` lists over `suggestions`, none of `picked`: the matches, then Create when
/// no tag is named so.
pub fn tag_rows(suggestions: &[String], picked: &[String], query: &str) -> Vec<TagRow> {
    let needle = query.trim().to_lowercase();
    let mut rows: Vec<TagRow> = suggestions
        .iter()
        .filter(|tag| !picked.contains(tag) && tag.to_lowercase().contains(&needle))
        .map(|tag| TagRow::Pick(tag.clone()))
        .collect();
    let named = suggestions
        .iter()
        .chain(picked)
        .any(|tag| tag.to_lowercase() == needle);
    if !needle.is_empty() && !named {
        rows.push(TagRow::Create(query.trim().to_string()));
    }
    rows
}

/// The picked tags in the order the suggestions list them, then any the suggestions lack, as
/// Electron's picker filters its options by the picked ids.
pub fn shown_tags(suggestions: &[String], picked: &[String]) -> Vec<String> {
    let mut shown: Vec<String> = suggestions
        .iter()
        .filter(|tag| picked.contains(tag))
        .cloned()
        .collect();
    shown.dedup();
    for tag in picked {
        if !shown.contains(tag) {
            shown.push(tag.clone());
        }
    }
    shown
}

pub struct TagPicker {
    id: SharedString,
    tags: Vec<String>,
    suggestions: Vec<String>,
    open: bool,
    field: Entity<TextInput>,
    highlighted: usize,
    anchor: Rc<Cell<Option<Bounds<Pixels>>>>,
    _typed: Subscription,
}

impl EventEmitter<TagPickerEvent> for TagPicker {}

impl TagPicker {
    pub fn new(id: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        let picker = cx.weak_entity();
        let cancel = picker.clone();
        let field = cx.new(|cx| {
            TextInput::new(cx)
                .prose(12.0, 18.0, gpui::FontWeight::NORMAL)
                .placeholder("Add tag")
                // The field calls these inside its own update, so the picker reads it after.
                .on_submit(move |_, window, cx| {
                    let picker = picker.clone();
                    window.defer(cx, move |_, cx| {
                        let _ = picker.update(cx, |this, cx| this.take(None, cx));
                    });
                })
                .on_cancel(move |window, cx| {
                    let cancel = cancel.clone();
                    window.defer(cx, move |_, cx| {
                        let _ = cancel.update(cx, |this, cx| this.close(cx));
                    });
                })
        });
        let typed = cx.observe(&field, |this, _, cx| {
            this.highlighted = 0;
            cx.notify();
        });
        Self {
            id: id.into(),
            tags: Vec::new(),
            suggestions: Vec::new(),
            open: false,
            field,
            highlighted: 0,
            anchor: Rc::new(Cell::new(None)),
            _typed: typed,
        }
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The picked tags, as the record holds them.
    pub fn set_tags(&mut self, tags: Vec<String>, cx: &mut Context<Self>) {
        if tags != self.tags {
            self.tags = tags;
            cx.notify();
        }
    }

    /// Tags offered while adding one, such as every tag the project's docs use.
    pub fn set_suggestions(&mut self, suggestions: Vec<String>) {
        self.suggestions = suggestions;
    }

    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = true;
        self.highlighted = 0;
        self.field.update(cx, |field, cx| field.set_text("", cx));
        self.field.read(cx).focus_handle(cx).focus(window);
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.open {
            self.open = false;
            cx.notify();
        }
    }

    fn rows(&self, cx: &App) -> Vec<TagRow> {
        tag_rows(&self.suggestions, &self.tags, self.field.read(cx).text())
    }

    /// Takes row `index`, or the highlighted one.
    fn take(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        let rows = self.rows(cx);
        let Some(row) = rows.get(index.unwrap_or(self.highlighted)) else {
            return;
        };
        let (TagRow::Pick(tag) | TagRow::Create(tag)) = row;
        if !self.tags.contains(tag) {
            self.tags.push(tag.clone());
            cx.emit(TagPickerEvent::Changed(self.tags.clone()));
        }
        self.close(cx);
    }

    fn remove(&mut self, tag: &str, cx: &mut Context<Self>) {
        let before = self.tags.len();
        self.tags.retain(|picked| picked != tag);
        if self.tags.len() != before {
            cx.emit(TagPickerEvent::Changed(self.tags.clone()));
            cx.notify();
        }
    }
}

/// A texture pill as a tag badge draws it; `ring` frames it in the primary color.
fn badge(id: impl Into<gpui::ElementId>, ring: bool, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    let radius = BADGE_HEIGHT / 2.0;
    let face_set = theme.face_neutral;
    div()
        .id(id)
        .relative()
        .flex_none()
        .h(px(BADGE_HEIGHT))
        .px(px(BADGE_PAD))
        .flex()
        .items_center()
        .gap(px(4.0))
        .rounded(px(radius))
        .bg(face(face_set.top, face_set.bottom))
        .text_size(px(12.0))
        .line_height(px(18.0))
        .text_color(face_set.ink)
        .child(if ring {
            div()
                .absolute()
                .inset_0()
                .rounded(px(radius))
                .border_1()
                .border_color(theme.color_primary)
        } else {
            edge(Edge::Strong, radius, theme)
        })
}

impl Render for TagPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let mut row = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.0));
        for (index, tag) in shown_tags(&self.suggestions, &self.tags)
            .into_iter()
            .enumerate()
        {
            let removed = tag.clone();
            row = row.child(
                badge((self.id.clone(), index), false, &theme)
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(tag.clone()),
                    )
                    .child(
                        div()
                            .id((SharedString::from(format!("{}-remove", self.id)), index))
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .text_color(theme.color_text_muted)
                            .hover(|style| style.text_color(theme.color_text_primary))
                            .child(icon_colored(Icon::X, 10.0, theme.color_text_muted))
                            .on_click(cx.listener(move |this, _, _, cx| this.remove(&removed, cx))),
                    ),
            );
        }
        if !self.open {
            row = row.child(
                badge(
                    SharedString::from(format!("{}-add", self.id)),
                    false,
                    &theme,
                )
                .cursor_pointer()
                .child(icon_colored(Icon::Plus, 10.0, face_ink(&theme)))
                .child("Add tag")
                .on_click(cx.listener(|this, _, window, cx| this.open(window, cx))),
            );
            return row;
        }
        let anchor = self.anchor.clone();
        let editing = badge(
            SharedString::from(format!("{}-editing", self.id)),
            true,
            &theme,
        )
        .child(icon_colored(Icon::Plus, 10.0, face_ink(&theme)))
        .child(div().w(px(FIELD_WIDTH)).child(self.field.clone()))
        .child(
            canvas(
                move |bounds, _, _| anchor.set(Some(bounds)),
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        );
        let rows = self.rows(cx);
        let mut list = div()
            .w(px(LIST_WIDTH))
            .max_h(px(200.0))
            .p(px(6.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .bg(theme.color_surface)
            .text_size(px(13.0));
        if rows.is_empty() {
            list = list.child(
                div()
                    .px(px(10.0))
                    .py(px(8.0))
                    .text_size(px(12.0))
                    .text_color(theme.color_text_muted)
                    .child("Type to create a tag."),
            );
        }
        for (index, entry) in rows.iter().enumerate() {
            let active = index == self.highlighted;
            let (label, create) = match entry {
                TagRow::Pick(tag) => (tag.clone(), false),
                TagRow::Create(tag) => (format!("Create “{tag}”"), true),
            };
            list = list.child(
                div()
                    .id((SharedString::from(format!("{}-option", self.id)), index))
                    .w_full()
                    .min_h(px(ROW_HEIGHT))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(theme.radius_item))
                    .cursor_pointer()
                    .text_color(if active {
                        theme.color_text_primary
                    } else {
                        theme.color_text_secondary
                    })
                    .when(active, |row| row.bg(theme.color_surface_hover))
                    .on_mouse_move(cx.listener(move |this, _, _, cx| {
                        if this.highlighted != index {
                            this.highlighted = index;
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.take(Some(index), cx);
                        }),
                    )
                    .when(create, |row| {
                        row.child(icon_colored(Icon::Plus, 12.0, theme.color_text_secondary))
                    })
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(label),
                    ),
            );
        }
        let at = self
            .anchor
            .get()
            .map(|bounds| point(bounds.origin.x, bounds.bottom() + px(4.0)));
        let popover = at.map(|at| {
            deferred(
                anchored()
                    .position(at)
                    .anchor(Corner::TopLeft)
                    .snap_to_window_with_margin(px(8.0))
                    .child(
                        floating_panel(div().relative(), theme.radius_menu, &theme)
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close(cx)))
                            .child(list),
                    ),
            )
            .with_priority(6)
        });
        row.child(editing).children(popover)
    }
}

/// The ink of a neutral badge.
fn face_ink(theme: &Theme) -> gpui::Rgba {
    theme.face_neutral.ink
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn picked_tags_follow_the_suggestions_then_their_own_order() {
        let suggestions = strings(&["audit", "export", "runbook"]);
        assert_eq!(
            shown_tags(&suggestions, &strings(&["zeta", "export", "audit"])),
            strings(&["audit", "export", "zeta"])
        );
        assert_eq!(shown_tags(&[], &strings(&["b", "a"])), strings(&["b", "a"]));
    }

    #[test]
    fn the_list_offers_unpicked_matches_then_create() {
        let suggestions = strings(&["runbook", "release", "audit"]);
        let picked = strings(&["audit"]);
        assert_eq!(
            tag_rows(&suggestions, &picked, ""),
            [
                TagRow::Pick("runbook".into()),
                TagRow::Pick("release".into())
            ]
        );
        assert_eq!(
            tag_rows(&suggestions, &picked, "RE"),
            [TagRow::Pick("release".into()), TagRow::Create("RE".into())]
        );
        // A tag already named, picked or offered, is not created again.
        assert_eq!(tag_rows(&suggestions, &picked, "Audit "), []);
        assert_eq!(
            tag_rows(&suggestions, &picked, "runbook"),
            [TagRow::Pick("runbook".into())]
        );
    }
}
