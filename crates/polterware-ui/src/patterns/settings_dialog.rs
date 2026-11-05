//! The settings dialog every app shares, over the page it was opened from. It opens on a
//! [`modal_layer`](super::modal_layer).
//!
//! It looks like the Solid design system's `SettingsDialog`: the `A11yDialog` frame (55rem by
//! `min(78vh, 760px)`, narrower than the web's `xl` 72rem, which read as too wide beside the
//! window; `--radius-dialog`, on `--color-app-background`) holding,
//! 8px in and 8px apart, the sections' rail (208px, 144 in a narrow window, 8px in, rows 4px
//! apart) and the active section on an opaque panel (`surface-panel`, the dialog's radius
//! less the inset). A rail row is `h-8` (26px), `px-2.5` in at `--radius-control`, its 14px
//! icon `gap-2` before its 12.5px label, both secondary, primary on
//! `--color-surface-selected` when it is the open section; a group's quiet heading (11px
//! medium muted) leads the rows that share it. With [`SettingsDialog::search`], the rail
//! opens on the search field (the small pill `Input`, `mb-2` over the rows), and a query
//! swaps the content for the settings it matches (see [`search_settings`]): each a row with
//! its label in 12.5px primary ink over its path in 11.5px muted ink, `px-2.5 py-2` in, the
//! hover fill under the pointer, or a muted "No settings match" line. The panel draws Close
//! (the secondary icon button, on the control material) 20px in from its top right, the
//! section's 16px semibold heading (and its actions) `mb-6` above the content, its groups
//! `gap-6` apart, and scrolls the content `24px 32px 32px` in; scrolled, the content fades
//! out under Close. A panel a control in the content opens (a menu, a picker) goes in
//! [`SettingsDialog::overlay`], drawn over the whole dialog and outside the content's scroll
//! and clip.
//!
//! It behaves like gpui-kit's `Settings` (its `SettingPage`s in a sidebar): the app owns
//! which section is open and hears a pick through [`SettingsDialog::on_select`]; the panel's
//! scroll is its own and starts at the top for each section the app opens.

use std::rc::Rc;

use gpui::{
    AnyElement, App, CursorStyle, FontWeight, IntoElement, MouseButton, ParentElement, RenderOnce,
    SharedString, Window, div, point, prelude::*, px,
};

use crate::Handler;
use crate::button::{Button, ButtonKind, ButtonSize};
use crate::icons::{Icon, icon_colored};
use crate::material::{Edge, drop, edge, float_drop};
use crate::scroll::{GUTTER, ScrollAxis, ScrollGeometry, scrollbar, use_scroll};
use crate::theme::{Theme, tw};

use super::dialog::{DialogPose, scaled_frame};
use super::scroll_fade::toolbar_trail;

/// The dialog's width (55rem) and height when the window has room for them.
pub const SETTINGS_WIDTH: f32 = 880.0;
pub const SETTINGS_HEIGHT: f32 = 760.0;
/// The sections' rail, and its width once the dialog is under 720px.
const NAV_WIDTH: f32 = 208.0;
const NAV_NARROW: f32 = 144.0;
/// The inset around and between the rail and the panel.
const INSET: f32 = 8.0;
/// What the dialog keeps clear of the window's edges.
const WINDOW_INSET: f32 = 40.0;
/// The trail over the panel's top once its content scrolls: Close's row and a little more.
const TRAIL: f32 = 56.0;

/// One section in the rail.
#[derive(Clone)]
pub struct SettingsNavItem {
    id: SharedString,
    label: SharedString,
    icon: Option<Icon>,
    group: Option<SharedString>,
}

impl SettingsNavItem {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            group: None,
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Items that share a group are listed under one quiet heading, in order.
    pub fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.group = Some(group.into());
        self
    }
}

/// Called with the id of the section picked in the rail.
pub type SettingsSelect = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

/// One setting the search finds (`SettingsSearchEntry`): the rail item it opens, the group
/// it sits in, and the words it matches on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsSearchEntry {
    pub item: SharedString,
    pub section: Option<SharedString>,
    pub label: SharedString,
    pub description: Option<SharedString>,
    pub keywords: Option<SharedString>,
}

impl SettingsSearchEntry {
    /// The setting `label`, in the rail item `item`.
    pub fn new(item: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            item: item.into(),
            section: None,
            label: label.into(),
            description: None,
            keywords: None,
        }
    }

    /// The group's title inside the item.
    pub fn section(mut self, section: impl Into<SharedString>) -> Self {
        self.section = Some(section.into());
        self
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Words it also matches on, which it does not show.
    pub fn keywords(mut self, keywords: impl Into<SharedString>) -> Self {
        self.keywords = Some(keywords.into());
        self
    }
}

/// The entries whose label, description, section, or keywords hold every word of `query`,
/// ignoring case (`searchSettings`); none for a blank query.
pub fn search_settings(entries: &[SettingsSearchEntry], query: &str) -> Vec<SettingsSearchEntry> {
    let query = query.trim().to_lowercase();
    let terms: Vec<&str> = query.split_whitespace().collect();
    if terms.is_empty() {
        return Vec::new();
    }
    entries
        .iter()
        .filter(|entry| {
            let haystack = [
                Some(&entry.label),
                entry.description.as_ref(),
                entry.section.as_ref(),
                entry.keywords.as_ref(),
            ]
            .into_iter()
            .flatten()
            .map(|text| text.as_ref())
            .collect::<Vec<&str>>()
            .join(" ")
            .to_lowercase();
            terms.iter().all(|term| haystack.contains(term))
        })
        .cloned()
        .collect()
}

/// Called with the search result picked.
pub type SettingsSearchOpen = Rc<dyn Fn(&SettingsSearchEntry, &mut Window, &mut App)>;

/// The rail's search: the field the app owns, what it holds, the settings to search, and
/// what a picked result does (open its item, and clear the field).
struct SettingsSearch {
    field: AnyElement,
    query: SharedString,
    entries: Vec<SettingsSearchEntry>,
    on_open: SettingsSearchOpen,
}

/// The dialog, `presence` (0 to 1) of the way in.
pub fn settings_dialog(presence: f32) -> SettingsDialog {
    SettingsDialog {
        presence,
        pose: None,
        items: Vec::new(),
        active: SharedString::default(),
        on_select: None,
        heading: None,
        actions: None,
        on_close: None,
        search: None,
        content: Vec::new(),
        overlay: Vec::new(),
    }
}

#[derive(IntoElement)]
pub struct SettingsDialog {
    presence: f32,
    pose: Option<DialogPose>,
    items: Vec<SettingsNavItem>,
    active: SharedString,
    on_select: Option<SettingsSelect>,
    heading: Option<SharedString>,
    actions: Option<AnyElement>,
    on_close: Option<Handler>,
    search: Option<SettingsSearch>,
    content: Vec<AnyElement>,
    overlay: Vec<AnyElement>,
}

impl SettingsDialog {
    pub fn items(mut self, items: Vec<SettingsNavItem>) -> Self {
        self.items = items;
        self
    }

    /// The open section's id.
    pub fn active(mut self, id: impl Into<SharedString>) -> Self {
        self.active = id.into();
        self
    }

    pub fn on_select(mut self, handler: SettingsSelect) -> Self {
        self.on_select = Some(handler);
        self
    }

    /// The panel's heading; the open section's label by default.
    pub fn heading(mut self, heading: impl Into<SharedString>) -> Self {
        self.heading = Some(heading.into());
        self
    }

    /// Controls beside the heading, such as an Add button.
    pub fn actions(mut self, actions: impl IntoElement) -> Self {
        self.actions = Some(actions.into_any_element());
        self
    }

    /// Drawn at `pose` (from [`DialogPresence::frame`](super::DialogPresence::frame)), the
    /// `A11yDialog` motion, instead of `presence`, which only fades it and lifts it 8px.
    pub fn pose(mut self, pose: DialogPose) -> Self {
        self.pose = Some(pose);
        self
    }

    /// Close at the panel's top right.
    pub fn on_close(mut self, handler: Handler) -> Self {
        self.on_close = Some(handler);
        self
    }

    /// The rail's search: `field` (a small pill [`TextInput`](crate::text_input::TextInput)
    /// with "Search settings" for a placeholder) holding `query`, over `entries`; the rail's
    /// items are searched by their label and group too. While `query` holds a word, the
    /// content is its results, and `on_open` hears the one picked.
    pub fn search(
        mut self,
        field: impl IntoElement,
        query: impl Into<SharedString>,
        entries: Vec<SettingsSearchEntry>,
        on_open: SettingsSearchOpen,
    ) -> Self {
        self.search = Some(SettingsSearch {
            field: field.into_any_element(),
            query: query.into(),
            entries,
            on_open,
        });
        self
    }

    /// Drawn over the dialog, after the content and outside its scroll and clip: the panel a
    /// control in the content opens, hung from it in a
    /// [`window_anchor`](super::window_anchor), so the rows after the control never paint
    /// over it and the panel's edge is the window's, not the content's.
    pub fn overlay(mut self, overlay: impl IntoElement) -> Self {
        self.overlay.push(overlay.into_any_element());
        self
    }
}

impl ParentElement for SettingsDialog {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.content.extend(elements);
    }
}

/// The dialog's size in a `viewport`: [`SETTINGS_WIDTH`] by [`SETTINGS_HEIGHT`] (at most 78%
/// of the window's height), smaller in a small window, never under 480 by 320.
pub fn settings_size(window: &Window) -> (f32, f32) {
    let viewport = window.viewport_size();
    let (width, height) = (f32::from(viewport.width), f32::from(viewport.height));
    (
        SETTINGS_WIDTH.min(width - WINDOW_INSET).max(480.0),
        SETTINGS_HEIGHT
            .min(height * 0.78)
            .min(height - WINDOW_INSET)
            .max(320.0),
    )
}

impl RenderOnce for SettingsDialog {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let t = self.presence.clamp(0.0, 1.0);
        let (width, height) = settings_size(window);
        let nav_width = if width < 720.0 { NAV_NARROW } else { NAV_WIDTH };
        let radius = theme.radius_dialog;
        let mut shadow = float_drop(&theme);
        shadow.push(drop(20.0, 56.0, theme.color_shadow_float));

        // The panel scrolls back to its top when another section opens.
        let scroll = use_scroll("settings-content", window, cx);
        let handle = scroll.read(cx).handle().clone();
        let seen = window.use_keyed_state("settings-content-section", cx, |_, _| {
            SharedString::default()
        });
        if *seen.read(cx) != self.active {
            handle.set_offset(point(px(0.0), px(0.0)));
            let active = self.active.clone();
            seen.update(cx, |seen, _| *seen = active);
        }
        let max = f32::from(handle.max_offset().height);
        let view_h = f32::from(handle.bounds().size.height);
        // Scrolled away from the top, the content goes soft under Close instead of being cut
        // at the panel's edge: the toolbar's trail, as tall as the scroll has gone, up to 56.
        let scrolled = (-f32::from(handle.offset().y)).clamp(0.0, TRAIL);
        let bar = (max > 0.5).then(|| {
            scrollbar(
                "settings-content-bar",
                &handle,
                ScrollAxis::Vertical,
                ScrollGeometry {
                    track: view_h,
                    viewport: view_h,
                    content: view_h + max,
                },
                window,
                cx,
            )
        });

        let mut nav = div()
            .id("settings-nav")
            .w(px(nav_width))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .p(px(INSET))
            .flex()
            .flex_col()
            .gap(px(4.0));
        let searching = self
            .search
            .as_ref()
            .is_some_and(|search| !search.query.trim().is_empty());
        let mut results = None;
        if let Some(search) = self.search {
            nav = nav.child(div().flex_none().mb(px(tw(2.0))).child(search.field));
            if searching {
                let label_of = |id: &SharedString| {
                    self.items
                        .iter()
                        .find(|item| &item.id == id)
                        .map(|item| item.label.clone())
                };
                let mut entries: Vec<SettingsSearchEntry> = self
                    .items
                    .iter()
                    .map(|item| {
                        let mut entry =
                            SettingsSearchEntry::new(item.id.clone(), item.label.clone());
                        entry.keywords = item.group.clone();
                        entry
                    })
                    .collect();
                entries.extend(search.entries.iter().map(|entry| {
                    let mut entry = entry.clone();
                    let keywords = [entry.keywords.clone(), label_of(&entry.item)]
                        .into_iter()
                        .flatten()
                        .map(|text| text.to_string())
                        .collect::<Vec<_>>()
                        .join(" ");
                    entry.keywords = Some(keywords.into());
                    entry
                }));
                let found = search_settings(&entries, &search.query);
                let path = |entry: &SettingsSearchEntry| -> SharedString {
                    let item = self.items.iter().find(|item| item.id == entry.item);
                    let path = match &entry.section {
                        Some(section) => format!(
                            "{} / {section}",
                            item.map(|item| item.label.as_ref()).unwrap_or("")
                        ),
                        None => item
                            .and_then(|item| item.group.clone().or(Some(item.label.clone())))
                            .map(|text| text.to_string())
                            .unwrap_or_default(),
                    };
                    match &entry.description {
                        Some(description) => format!("{path} — {description}").into(),
                        None => path.into(),
                    }
                };
                results = Some(if found.is_empty() {
                    div()
                        .px(px(tw(1.0)))
                        .text_size(px(12.5))
                        .text_color(theme.color_text_muted)
                        .child(SharedString::from(format!(
                            "No settings match \u{201c}{}\u{201d}.",
                            search.query.trim()
                        )))
                        .into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(tw(1.0)))
                        .children(found.into_iter().enumerate().map(|(index, entry)| {
                            let open = search.on_open.clone();
                            let shown = path(&entry);
                            div()
                                .id(("settings-result", index))
                                .w_full()
                                .flex()
                                .flex_col()
                                .items_start()
                                .gap(px(tw(0.5)))
                                .px(px(tw(2.5)))
                                .py(px(tw(2.0)))
                                .rounded(px(theme.radius_control))
                                .cursor_pointer()
                                .hover(move |row| row.bg(theme.color_surface_hover))
                                .child(
                                    div()
                                        .text_size(px(12.5))
                                        .text_color(theme.color_text_primary)
                                        .child(entry.label.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.5))
                                        .text_color(theme.color_text_muted)
                                        .child(shown),
                                )
                                .on_click(move |_, window, cx| open(&entry, window, cx))
                        }))
                        .into_any_element()
                });
            }
        }
        let mut last_group: Option<SharedString> = None;
        for (index, item) in self.items.iter().enumerate() {
            if item.group.is_some() && item.group != last_group {
                nav = nav.child(
                    div()
                        .px(px(tw(2.5)))
                        .pb(px(tw(0.5)))
                        .when(index > 0, |heading| heading.pt(px(tw(3.0))))
                        .text_size(px(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.color_text_muted)
                        .child(item.group.clone().unwrap_or_default()),
                );
            }
            last_group = item.group.clone();
            let on = !searching && item.id == self.active;
            let ink = if on {
                theme.color_text_primary
            } else {
                theme.color_text_secondary
            };
            let select = self.on_select.clone();
            let id = item.id.clone();
            nav = nav.child(
                div()
                    .id(("settings-nav-item", index))
                    .h(px(tw(8.0)))
                    .w_full()
                    .flex_none()
                    .px(px(tw(2.5)))
                    .flex()
                    .items_center()
                    .gap(px(tw(2.0)))
                    .rounded(px(theme.radius_control))
                    .cursor_pointer()
                    .text_size(px(12.5))
                    .text_color(ink)
                    .map(|row| {
                        if on {
                            row.bg(theme.color_surface_selected)
                        } else {
                            row.hover(move |row| {
                                row.bg(theme.color_surface_hover)
                                    .text_color(theme.color_text_primary)
                            })
                        }
                    })
                    .on_click(move |_, window, cx| {
                        if let Some(select) = &select {
                            select(&id, window, cx);
                        }
                    })
                    .children(item.icon.map(|name| icon_colored(name, 14.0, ink)))
                    .child(div().truncate().child(item.label.clone())),
            );
        }

        let heading = if searching {
            Some(SharedString::from("Search results"))
        } else {
            self.heading.clone()
        };
        let heading = heading.or_else(|| {
            self.items
                .iter()
                .find(|item| item.id == self.active)
                .map(|item| item.label.clone())
        });
        let panel_radius = radius - INSET;
        let close = self.on_close.map(|handler| {
            div()
                .absolute()
                .top(px(20.0))
                .right(px(20.0))
                .cursor(CursorStyle::PointingHand)
                // The control material, as the toolbars' icon buttons wear it, so Close reads
                // as a button over the panel and not as a bare glyph.
                .child(
                    Button::new("settings-close", ButtonKind::Secondary)
                        .size(ButtonSize::Icon)
                        .icon(Icon::Close)
                        .tooltip("Close settings")
                        .on_click_rc(handler),
                )
        });
        let panel = div()
            .relative()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .rounded(px(panel_radius))
            .bg(theme.color_surface)
            .overflow_hidden()
            .child(
                div()
                    .id("settings-content")
                    // Tests scroll the section over it (a no-op outside gpui's test support).
                    .debug_selector(|| "settings-content".to_string())
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&handle)
                    .child(
                        div()
                            .w_full()
                            .pt(px(24.0))
                            .px(px(32.0))
                            .pb(px(32.0))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .mb(px(tw(6.0)))
                                    .pr(px(40.0))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(12.0))
                                    .child(
                                        div()
                                            .text_size(px(16.0))
                                            .line_height(px(24.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.color_text_primary)
                                            .child(heading.unwrap_or_default()),
                                    )
                                    .children(self.actions.filter(|_| !searching)),
                            )
                            .children(results)
                            .when(!searching, |content| {
                                content.child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(tw(6.0)))
                                        .children(self.content),
                                )
                            }),
                    ),
            )
            .when(scrolled > 0.5, |panel| {
                panel.child(
                    div()
                        .absolute()
                        .top(px(0.0))
                        // Clear of the rounded corners and of the scrollbar.
                        .left(px(panel_radius))
                        .right(px(GUTTER + panel_radius))
                        .child(toolbar_trail(theme.color_surface, scrolled)),
                )
            })
            .children(bar.map(|bar| {
                div()
                    .absolute()
                    .top(px(0.0))
                    .right(px(0.0))
                    .bottom(px(0.0))
                    .w(px(GUTTER))
                    .child(bar)
            }))
            .children(close);

        let (opacity, scale, lift) = match self.pose {
            Some(pose) => (pose.opacity, pose.scale, 0.0),
            None => (t, 1.0, (1.0 - t) * 8.0),
        };
        let frame = scaled_frame(
            scale,
            div()
                .rounded(px(radius))
                .bg(theme.color_app_background)
                .shadow(shadow)
                .child(edge(Edge::Quiet, radius, &theme)),
        );
        div()
            .occlude()
            .relative()
            .w(px(width))
            .h(px(height))
            .mt(px(lift))
            .opacity(opacity)
            .flex()
            .gap(px(INSET))
            .p(px(INSET))
            .text_color(theme.color_text_primary)
            .font_family(theme.font_sans)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(frame)
            .child(nav)
            .child(panel)
            .children(self.overlay)
    }
}

#[cfg(test)]
mod tests {
    use super::{SettingsSearchEntry, search_settings};

    #[test]
    fn a_search_needs_every_word_in_any_field() {
        let entries = vec![
            SettingsSearchEntry::new("general", "Network unit")
                .section("Units")
                .description("Bytes or bits per second.")
                .keywords("bandwidth rate"),
            SettingsSearchEntry::new("connections", "Reconnect delay").keywords("seconds retry"),
        ];
        let labels = |query: &str| -> Vec<String> {
            search_settings(&entries, query)
                .into_iter()
                .map(|entry| entry.label.to_string())
                .collect()
        };
        assert!(labels("  ").is_empty());
        assert_eq!(labels("BITS"), vec!["Network unit"]);
        assert_eq!(labels("units bandwidth"), vec!["Network unit"]);
        assert_eq!(labels("retry"), vec!["Reconnect delay"]);
        assert!(labels("retry bits").is_empty());
    }
}
