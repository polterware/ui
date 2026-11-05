//! `ArchivedList` (`archived-list.tsx`): the archive of one surface, where an archived record
//! waits until it is restored or deleted for good.
//!
//! It looks like the Solid one on the [`DataTable`]: Title (the record's 16px icon on a
//! neutral 24px tile, then its title in medium ink), Type (a short detail such as its tags,
//! "—" without one) and Archived (its date), both muted, then each row's actions at the end:
//! a 28px round "…" (a 14px muted mark, filling under the pointer) that opens a menu of
//! Restore and Delete permanently (in danger ink), placed `bottom-end` under it. With nothing
//! archived it is the sleeping "Nothing archived" page.
//!
//! It behaves like gpui-kit's `Table` with a row menu: the headers sort the rows by their
//! text, the menu is a [`ContextMenu`] opened under the row's button, and choosing an item
//! tells the host with [`ArchivedListEvent`]. While the host writes, [`ArchivedList::set_busy`]
//! holds every row's button.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AnyElement, App, AppContext as _, Bounds, Context, Entity, EventEmitter, FontWeight,
    IntoElement, Pixels, Render, SharedString, Window, canvas, deferred, div, prelude::*, px,
};

use super::context_menu::ContextMenu;
use crate::button::{Button, ButtonKind, ButtonSize};
use crate::data_table::{Column, DataTable, primary_cell, sans, text_width};
use crate::icon_tile::{IconTile, TileTone};
use crate::icons::Icon;
use crate::menu::MenuItem;
use crate::sort::{SortDirection, SortValue, next_sort, sort_rows};
use crate::theme::Theme;

/// A default-density row: the 24px tile in the cell's 5px padding.
const ROW_HEIGHT: f32 = 34.0;
/// The row menu's width (`min-w-44` holding "Delete permanently" and its mark).
const MENU_WIDTH: f32 = 176.0;
/// Where a deferred menu draws among the layers: over the table, under a modal.
const MENU_PRIORITY: usize = 50;

/// One archived record (`ArchivedListItem`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArchivedItem {
    pub id: SharedString,
    pub title: SharedString,
    /// Short context, such as its tags or path; empty shows "—".
    pub detail: SharedString,
    /// The date it was archived, as the host formats it.
    pub archived_at: SharedString,
}

/// What a row's menu asks the host for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArchivedListEvent {
    Restore(SharedString),
    Delete(SharedString),
}

pub struct ArchivedList {
    id: SharedString,
    icon: Icon,
    items: Vec<ArchivedItem>,
    busy: bool,
    gutter: f32,
    sort: Option<(usize, SortDirection)>,
    menu: Entity<ContextMenu>,
    /// Each shown row's "…", as the last frame laid it out.
    anchors: Rc<RefCell<Vec<Option<Bounds<Pixels>>>>>,
}

impl EventEmitter<ArchivedListEvent> for ArchivedList {}

impl ArchivedList {
    /// The archive named `id`, its rows marked with `icon` (`Archive` on the web unless a
    /// surface gives its own, such as a doc's).
    pub fn new(id: impl Into<SharedString>, icon: Icon, cx: &mut Context<Self>) -> Self {
        let menu = cx.new(|cx| {
            let mut menu = ContextMenu::new(cx);
            menu.set_width(MENU_WIDTH);
            menu
        });
        cx.observe(&menu, |_, _, cx| cx.notify()).detach();
        Self {
            id: id.into(),
            icon,
            items: Vec::new(),
            busy: false,
            gutter: 0.0,
            sort: None,
            menu,
            anchors: Rc::default(),
        }
    }

    /// The table's inline padding, as the page's other tables have it.
    pub fn gutters(mut self, inline: f32) -> Self {
        self.gutter = inline;
        self
    }

    pub fn set_items(&mut self, items: Vec<ArchivedItem>, cx: &mut Context<Self>) {
        if self.items != items {
            self.items = items;
            cx.notify();
        }
    }

    pub fn items(&self) -> &[ArchivedItem] {
        &self.items
    }

    /// Holds every row's button while the host writes.
    pub fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        if self.busy != busy {
            self.busy = busy;
            if busy {
                self.menu.update(cx, |menu, cx| menu.close(cx));
            }
            cx.notify();
        }
    }

    /// The rows in the order the sort asks.
    fn rows(&self) -> Vec<ArchivedItem> {
        match self.sort {
            Some((column, direction)) => sort_rows(&self.items, direction, |item| {
                let text = match column {
                    0 => &item.title,
                    1 => &item.detail,
                    _ => &item.archived_at,
                };
                SortValue::Text(text.to_string())
            }),
            None => self.items.clone(),
        }
    }

    /// Opens the menu of `item` under its row's button.
    fn open_menu(
        &mut self,
        slot: usize,
        item: ArchivedItem,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(anchor) = self.anchors.borrow().get(slot).copied().flatten() else {
            return;
        };
        let view = cx.entity().downgrade();
        let pick = move |event: ArchivedListEvent| -> crate::Handler {
            let view = view.clone();
            Rc::new(move |_, cx: &mut App| {
                let _ = view.update(cx, |_, cx| cx.emit(event.clone()));
            })
        };
        let items = vec![
            MenuItem::action(
                "Restore",
                Some(Icon::Undo2),
                pick(ArchivedListEvent::Restore(item.id.clone())),
            ),
            MenuItem::danger(
                "Delete permanently",
                Some(Icon::Delete),
                pick(ArchivedListEvent::Delete(item.id)),
            ),
        ];
        self.menu
            .update(cx, |menu, cx| menu.open_under(anchor, items, window, cx));
    }
}

/// The muted text of a Type or Archived cell.
fn muted(text: SharedString, cx: &App) -> AnyElement {
    let theme = Theme::global(cx);
    div()
        .text_size(px(12.5))
        .text_color(theme.color_text_muted)
        .whitespace_nowrap()
        .child(text)
        .into_any_element()
}

impl Render for ArchivedList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.items.is_empty() {
            return empty(cx);
        }
        let rows = Rc::new(self.rows());
        self.anchors.borrow_mut().resize(rows.len(), None);
        let columns = vec![
            Column::new("Title").sortable().shrinks(),
            Column::new("Type").sortable(),
            Column::new("Archived").sortable(),
            Column::new("").end().fixed().content(28.0).resizable(false),
        ];
        let cells = rows.clone();
        let measured = rows.clone();
        let view = cx.entity().downgrade();
        let anchors = self.anchors.clone();
        let (icon, busy) = (self.icon, self.busy);
        let id = self.id.clone();
        let table = DataTable::new(
            gpui::ElementId::Name(format!("{}-table", self.id).into()),
            columns,
            rows.len(),
            move |row, _, cx| {
                let item = &cells[row.index];
                let theme = Theme::global(cx);
                let title = primary_cell(
                    IconTile::new(icon).size(24.0, 16.0).tone(TileTone::Neutral),
                    item.title.clone(),
                    sans(theme, FontWeight::MEDIUM),
                    None,
                    cx,
                );
                let detail = if item.detail.is_empty() {
                    SharedString::from("\u{2014}")
                } else {
                    item.detail.clone()
                };
                let slot = row.index;
                let open = {
                    let view = view.clone();
                    let item = item.clone();
                    move |window: &mut Window, cx: &mut App| {
                        let _ = view.update(cx, |this, cx| {
                            this.open_menu(slot, item.clone(), window, cx);
                        });
                    }
                };
                let anchors = anchors.clone();
                let actions = div()
                    .relative()
                    .child(
                        Button::new((id.clone(), slot), ButtonKind::Ghost)
                            .size(ButtonSize::Icon)
                            .icon(Icon::MoreHorizontal)
                            .tooltip(format!("Actions for {}", item.title))
                            .disabled(busy)
                            .on_click(open),
                    )
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                if let Some(anchor) = anchors.borrow_mut().get_mut(slot) {
                                    *anchor = Some(bounds);
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    )
                    .into_any_element();
                vec![
                    title,
                    muted(detail, cx),
                    muted(item.archived_at.clone(), cx),
                    actions,
                ]
            },
        )
        .measure(move |range, window, cx| {
            let theme = *Theme::global(cx);
            let normal = sans(&theme, FontWeight::NORMAL);
            let medium = sans(&theme, FontWeight::MEDIUM);
            let mut widest = [0.0_f32, 0.0, 0.0, 28.0];
            for item in measured.get(range).unwrap_or_default() {
                let detail = if item.detail.is_empty() {
                    "\u{2014}"
                } else {
                    &item.detail
                };
                let widths = [
                    24.0 + 10.0 + text_width(&item.title, medium.clone(), 12.5, window),
                    text_width(detail, normal.clone(), 12.5, window),
                    text_width(&item.archived_at, normal.clone(), 12.5, window),
                ];
                for (slot, width) in widest.iter_mut().zip(widths) {
                    *slot = slot.max(width);
                }
            }
            widest.to_vec()
        })
        .row_height(ROW_HEIGHT)
        .sort(self.sort, {
            let view = cx.entity().downgrade();
            move |column, _, cx| {
                let _ = view.update(cx, |this, cx| {
                    this.sort = next_sort(this.sort, column);
                    cx.notify();
                });
            }
        })
        .gutters(self.gutter)
        .measure_every_row()
        .freeze_once();
        div()
            .relative()
            .size_full()
            .child(table)
            .child(deferred(self.menu.clone()).with_priority(MENU_PRIORITY))
            .into_any_element()
    }
}

/// `PageEmpty title="Nothing archived" mood="sleeping"`.
#[cfg(feature = "bots")]
fn empty(cx: &mut App) -> AnyElement {
    super::page_empty("Nothing archived", super::MascotMood::Sleeping, cx).into_any_element()
}

/// Without the mascots, the empty archive says so on its own.
#[cfg(not(feature = "bots"))]
fn empty(cx: &mut App) -> AnyElement {
    let theme = Theme::global(cx);
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(13.0))
        .text_color(theme.color_text_muted)
        .child("Nothing archived")
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{SharedString, TestAppContext};

    use super::{ArchivedItem, ArchivedList, ArchivedListEvent};
    use crate::icons::Icon;
    use crate::sort::SortDirection;
    use crate::test_support::draw;

    fn item(id: &str, title: &str, at: &str) -> ArchivedItem {
        ArchivedItem {
            id: SharedString::from(id.to_string()),
            title: SharedString::from(title.to_string()),
            detail: SharedString::default(),
            archived_at: SharedString::from(at.to_string()),
        }
    }

    #[gpui::test]
    fn a_row_menu_restores_or_deletes_its_record(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (list, cx) =
            cx.add_window_view(|_, cx| ArchivedList::new("archived-docs", Icon::FileText, cx));
        let seen: Rc<RefCell<Vec<ArchivedListEvent>>> = Rc::default();
        let sink = seen.clone();
        cx.update(|_, cx| {
            cx.subscribe(&list, move |_, event: &ArchivedListEvent, _| {
                sink.borrow_mut().push(event.clone());
            })
            .detach();
            list.update(cx, |list, cx| {
                list.set_items(
                    vec![
                        item("d1", "Runbook", "Nov 5, 2026"),
                        item("d2", "Audit", "Nov 4, 2026"),
                    ],
                    cx,
                );
            });
        });
        draw(cx);
        // The menu hangs from the button the last frame laid out.
        cx.update(|window, cx| {
            list.update(cx, |list, cx| {
                list.open_menu(0, list.items()[0].clone(), window, cx);
            });
        });
        assert!(cx.update(|_, cx| list.read(cx).menu.read(cx).is_open(cx)));
        cx.update(|_, cx| {
            let event = ArchivedListEvent::Restore("d1".into());
            list.update(cx, |_, cx| cx.emit(event));
        });
        assert_eq!(
            seen.borrow().as_slice(),
            [ArchivedListEvent::Restore("d1".into())]
        );
        // Busy, the menu closes and stays shut.
        cx.update(|_, cx| list.update(cx, |list, cx| list.set_busy(true, cx)));
        assert!(!cx.update(|_, cx| list.read(cx).menu.read(cx).is_open(cx)));
    }

    #[gpui::test]
    fn the_headers_sort_the_rows_by_their_text(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (list, cx) =
            cx.add_window_view(|_, cx| ArchivedList::new("archived-docs", Icon::FileText, cx));
        cx.update(|_, cx| {
            list.update(cx, |list, cx| {
                list.set_items(
                    vec![item("d1", "Runbook", "b"), item("d2", "Audit", "a")],
                    cx,
                );
                list.sort = Some((0, SortDirection::Ascending));
            });
        });
        let titles: Vec<String> = cx.update(|_, cx| {
            list.read(cx)
                .rows()
                .iter()
                .map(|item| item.title.to_string())
                .collect()
        });
        assert_eq!(titles, ["Audit", "Runbook"]);
    }
}
