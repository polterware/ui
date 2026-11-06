//! `display-options.tsx`/`display-options.css`: 12px padding and row gaps,
//! 148px selects, a direction button and property chips spaced 6px apart.
//! The host owns and remembers the values; selections use the shared gpui-kit-like
//! Menu keyboard/focus behavior. Used at the end of FilterPopover's root menu.
use crate::{
    button::{Button, ButtonKind, ButtonSize},
    icons::Icon,
    menu::{Menu, MenuItem},
    select::select_trigger,
    theme::Theme,
};
use gpui::{
    AnyElement, App, Bounds, Context, Entity, EventEmitter, IntoElement, Pixels, Point, Render,
    SharedString, Window, canvas, deferred, div, prelude::*, px,
};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayChoice {
    pub value: SharedString,
    pub label: SharedString,
}
impl DisplayChoice {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisplaySettings {
    pub grouping: SharedString,
    pub ordering: SharedString,
    pub descending: bool,
    pub properties: Vec<SharedString>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayChanged(pub DisplaySettings);

pub struct DisplayOptions {
    groupings: Vec<DisplayChoice>,
    orderings: Vec<DisplayChoice>,
    properties: Vec<DisplayChoice>,
    settings: DisplaySettings,
    menu: Entity<Menu>,
    anchors: [Rc<Cell<Option<Bounds<Pixels>>>>; 2],
    anchor: Option<Bounds<Pixels>>,
}
impl EventEmitter<DisplayChanged> for DisplayOptions {}
impl DisplayOptions {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let menu = cx.new(Menu::new);
        menu.update(cx, |menu, _| menu.set_width(148.0));
        cx.observe(&menu, |_, _, cx| cx.notify()).detach();
        Self {
            groupings: vec![],
            orderings: vec![],
            properties: vec![],
            settings: DisplaySettings::default(),
            menu,
            anchors: std::array::from_fn(|_| Rc::new(Cell::new(None))),
            anchor: None,
        }
    }
    pub fn set_choices(
        &mut self,
        groupings: Vec<DisplayChoice>,
        orderings: Vec<DisplayChoice>,
        properties: Vec<DisplayChoice>,
    ) {
        self.groupings = groupings;
        self.orderings = orderings;
        self.properties = properties;
    }
    pub fn set_settings(&mut self, settings: DisplaySettings, cx: &mut Context<Self>) {
        if self.settings != settings {
            self.settings = settings;
            cx.notify();
        }
    }
    pub fn settings(&self) -> &DisplaySettings {
        &self.settings
    }
    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.menu.update(cx, |menu, cx| {
            menu.set_open(false);
            cx.notify();
        });
    }
    pub fn contains_menu(&self, at: Point<Pixels>, cx: &App) -> bool {
        self.menu
            .read(cx)
            .bounds()
            .is_some_and(|bounds| bounds.contains(&at))
    }
    pub fn overlay(&self) -> AnyElement {
        match self.anchor {
            Some(anchor) => deferred(crate::patterns::window_anchor(anchor, self.menu.clone()))
                .with_priority(15)
                .into_any_element(),
            None => div().into_any_element(),
        }
    }
    fn changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(DisplayChanged(self.settings.clone()));
        cx.notify();
    }
    fn toggle_property(&mut self, value: &SharedString, cx: &mut Context<Self>) {
        if self.settings.properties.contains(value) {
            self.settings.properties.retain(|p| p != value);
        } else {
            self.settings.properties = self
                .properties
                .iter()
                .map(|p| p.value.clone())
                .filter(|p| p == value || self.settings.properties.contains(p))
                .collect();
        }
        self.changed(cx);
    }
    fn select(&mut self, index: usize, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(bounds) = self.anchors[index].get() else {
            return;
        };
        let choices = if index == 0 {
            &self.groupings
        } else {
            &self.orderings
        };
        let selected = if index == 0 {
            &self.settings.grouping
        } else {
            &self.settings.ordering
        };
        let items = choices
            .iter()
            .map(|choice| {
                let value = choice.value.clone();
                let entity = cx.entity();
                MenuItem::Radio {
                    label: choice.label.clone(),
                    checked: selected == &value,
                    on_select: Rc::new(move |_, cx| {
                        entity.update(cx, |this, cx| {
                            if index == 0 {
                                this.settings.grouping = value.clone();
                            } else {
                                this.settings.ordering = value.clone();
                            }
                            this.changed(cx);
                        })
                    }),
                }
            })
            .collect::<Vec<_>>();
        self.anchor = Some(Bounds {
            origin: bounds.origin + gpui::point(px(0.0), px(4.0)),
            size: bounds.size,
        });
        self.menu.update(cx, |menu, cx| {
            menu.set_menu(items);
            menu.set_open(true);
            cx.notify();
        });
        // Menu hangs from the recorded trigger; placement is resolved by its own Hang.
    }
}
impl Render for DisplayOptions {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let mut panel = div()
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .text_size(px(13.0))
            .text_color(theme.color_text_secondary);
        for (index, (title, choices, selected)) in [
            ("Grouping", &self.groupings, &self.settings.grouping),
            ("Ordering", &self.orderings, &self.settings.ordering),
        ]
        .into_iter()
        .enumerate()
        {
            if choices.is_empty() {
                continue;
            }
            let label = choices
                .iter()
                .find(|choice| &choice.value == selected)
                .map(|c| c.label.clone())
                .unwrap_or_default();
            let entity = cx.entity();
            let bounds = self.anchors[index].clone();
            let trigger = div()
                .id(("display-select-anchor", index))
                .debug_selector(move || format!("display-select-{index}"))
                .relative()
                .w(px(148.0))
                .child(select_trigger(
                    SharedString::from(format!("display-select-{index}")),
                    label,
                    Some(title),
                    Rc::new(move |window, cx| {
                        entity.update(cx, |this, cx| this.select(index, window, cx))
                    }),
                    cx,
                ))
                .child(
                    canvas(move |rect, _, _| bounds.set(Some(rect)), |_, _, _, _| {})
                        .absolute()
                        .inset_0(),
                );
            let controls = div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(trigger)
                .when(index == 1, |row| {
                    row.child(
                        Button::new("display-direction", ButtonKind::Ghost)
                            .size(ButtonSize::Icon)
                            .icon(if self.settings.descending {
                                Icon::ArrowDown
                            } else {
                                Icon::ArrowUp
                            })
                            .tooltip(if self.settings.descending {
                                "Descending"
                            } else {
                                "Ascending"
                            })
                            .on_click({
                                let entity = cx.entity();
                                move |_, cx| {
                                    entity.update(cx, |this, cx| {
                                        this.settings.descending = !this.settings.descending;
                                        this.changed(cx);
                                    })
                                }
                            }),
                    )
                });
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(title)
                    .child(controls),
            );
        }
        if !self.properties.is_empty() {
            let chips = self.properties.iter().map(|choice| {
                let value = choice.value.clone();
                let entity = cx.entity();
                Button::new(
                    SharedString::from(format!("display-property-{}", choice.value)),
                    if self.settings.properties.contains(&choice.value) {
                        ButtonKind::Secondary
                    } else {
                        ButtonKind::Ghost
                    },
                )
                .size(ButtonSize::Sm)
                .height(26.0)
                .radius(11.0)
                .pad_x(12.0)
                .label(choice.label.clone())
                .selected(self.settings.properties.contains(&choice.value))
                .on_click(move |_, cx| {
                    entity.update(cx, |this, cx| this.toggle_property(&value, cx))
                })
            });
            panel = panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child("Display properties")
                    .child(div().flex().flex_wrap().gap(px(6.0)).children(chips)),
            );
        }
        panel
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    #[gpui::test]
    fn selects_use_keyboard_and_changes_leave_other_values_intact(cx: &mut TestAppContext) {
        let display = cx.new(DisplayOptions::new);
        display.update(cx, |this, cx| {
            this.set_choices(
                vec![
                    DisplayChoice::new("none", "No grouping"),
                    DisplayChoice::new("stage", "Stage"),
                    DisplayChoice::new("milestone", "Milestone"),
                ],
                vec![
                    DisplayChoice::new("updated", "Updated"),
                    DisplayChoice::new("title", "Title"),
                ],
                vec![DisplayChoice::new("slug", "Slug")],
            );
            this.set_settings(
                DisplaySettings {
                    grouping: "stage".into(),
                    ordering: "updated".into(),
                    descending: true,
                    properties: vec!["slug".into()],
                },
                cx,
            );
        });
        let host = display.clone();
        let mut visual = crate::test_support::open(cx, gpui::size(650.0, 550.0), move |_, cx| {
            let overlay = host.read(cx).overlay();
            div()
                .w(px(320.0))
                .child(host.clone())
                .child(overlay)
                .into_any_element()
        });
        visual.update(|window, cx| display.update(cx, |display, cx| display.select(0, window, cx)));
        crate::test_support::settle(&mut visual);
        visual.simulate_keystrokes("down");
        visual.simulate_keystrokes("down");
        visual.simulate_keystrokes("down");
        visual.simulate_keystrokes("enter");
        display.read_with(&visual, |display, _| {
            assert_eq!(display.settings.grouping, "milestone");
            assert_eq!(display.settings.ordering, "updated");
            assert!(display.settings.descending);
            assert_eq!(
                display.settings.properties,
                vec![SharedString::from("slug")]
            );
        });
    }

    #[gpui::test]
    fn properties_toggle_in_option_order_and_allow_none(cx: &mut TestAppContext) {
        let display = cx.new(DisplayOptions::new);
        display.update(cx, |this, cx| {
            this.set_choices(
                vec![],
                vec![],
                vec![DisplayChoice::new("a", "A"), DisplayChoice::new("b", "B")],
            );
            this.toggle_property(&"b".into(), cx);
            this.toggle_property(&"a".into(), cx);
            assert_eq!(
                this.settings.properties,
                vec![SharedString::from("a"), "b".into()]
            );
            this.toggle_property(&"a".into(), cx);
            this.toggle_property(&"b".into(), cx);
            assert!(this.settings.properties.is_empty());
        });
    }
}
