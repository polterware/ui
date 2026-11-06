//! `date-picker.tsx`, `calendar.tsx`, `calendar.css`, and `picker-primitives.css`:
//! the 276px floating surface, 252px calendar, 32px days, and Today/Clear actions.
//! State entity, single-date Change event, adjacent-month days, month/year selection,
//! focus restoration and dismissal follow gpui-kit's DatePicker/Calendar (gpui-base
//! `date_picker.rs` and `calendar.rs`). Arrow keys move the focused date, Home/End
//! move within its week, PageUp/PageDown move months (Shift moves years).
//! The host draws `overlay` outside clipping ancestors, like PropertyPills.

use crate::motion::{Curve, SpringTier, Tween};
use crate::{
    button::{Button, ButtonKind, ButtonSize},
    icons::Icon,
    material,
    theme::Theme,
};
use chrono::{Datelike, Duration, Local, NaiveDate};
use gpui::{
    AnyElement, App, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    KeyDownEvent, Pixels, Render, SharedString, Window, canvas, div, prelude::*, px,
};
use std::{cell::Cell, rc::Rc};

pub use chrono::NaiveDate as Date;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateChanged(pub Option<Date>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    Day,
    Month,
    Year,
}

pub struct DatePicker {
    value: Option<Date>,
    focused: Date,
    today: Date,
    view: View,
    open: bool,
    disabled: bool,
    label: SharedString,
    focus: FocusHandle,
    restore: Option<FocusHandle>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    alpha: Tween,
    slide: Tween,
}
impl EventEmitter<DateChanged> for DatePicker {}
impl Focusable for DatePicker {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl DatePicker {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let today = Local::now().date_naive();
        Self {
            value: None,
            focused: today,
            today,
            view: View::Day,
            open: false,
            disabled: false,
            label: "Choose date".into(),
            focus: cx.focus_handle(),
            restore: None,
            bounds: Rc::new(Cell::new(None)),
            alpha: Tween::new(0.0),
            slide: Tween::new(0.0),
        }
    }
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn value(&self) -> Option<Date> {
        self.value
    }
    pub fn parse(value: &str) -> Option<Date> {
        if value.len() != 10 {
            return None;
        }
        let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
        (date.year() > 0 && date.to_string() == value).then_some(date)
    }
    pub fn set_value(&mut self, value: Option<Date>, cx: &mut Context<Self>) {
        self.value = value;
        self.focused = value.unwrap_or(self.today);
        cx.notify();
    }
    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled || self.open {
            return;
        }
        self.today = Local::now().date_naive();
        self.focused = self.value.unwrap_or(self.today);
        self.view = View::Day;
        self.restore = window.focused(cx);
        self.open = true;
        self.alpha.animate_to(
            1.0,
            SpringTier::Fast.duration(),
            Curve::Ease(crate::theme::Ease(0.0, 0.0, 0.58, 1.0)),
        );
        self.slide
            .animate_to(1.0, SpringTier::Fast.duration(), SpringTier::Fast.curve());
        self.focus.focus(window);
        cx.notify();
    }
    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        self.open = false;
        self.alpha.animate_to(
            0.0,
            SpringTier::Fast.exit_duration(),
            Curve::Ease(crate::theme::Ease(0.42, 0.0, 1.0, 1.0)),
        );
        self.slide.animate_to(
            0.0,
            SpringTier::Fast.exit_duration(),
            Curve::Ease(crate::theme::Ease(0.42, 0.0, 1.0, 1.0)),
        );
        if self.focus.contains_focused(window, cx)
            && let Some(focus) = self.restore.take()
        {
            focus.focus(window);
        }
        cx.notify();
    }
    fn choose(&mut self, value: Option<Date>, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled || value.is_some_and(|date| !(1..=9999).contains(&date.year())) {
            return;
        }
        self.value = value;
        if let Some(value) = value {
            self.focused = value;
        }
        self.close(window, cx);
        cx.emit(DateChanged(value));
        cx.notify();
    }
    fn shift_month(&mut self, offset: i32) {
        let index = self.focused.year() * 12 + self.focused.month0() as i32 + offset;
        let year = index.div_euclid(12).clamp(1, 9999);
        let month = index.rem_euclid(12) as u32 + 1;
        self.focused = date_clamped(year, month, self.focused.day());
    }
    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        if key.modifiers.platform || key.modifiers.control {
            return;
        }
        if !self.open {
            if matches!(key.key.as_str(), "enter" | "space" | "down") {
                self.open(window, cx);
                cx.stop_propagation();
            }
            return;
        }
        let columns = if self.view == View::Day { 7 } else { 3 };
        let offset = match key.key.as_str() {
            "left" => -1,
            "right" => 1,
            "up" => -columns,
            "down" => columns,
            _ => 0,
        };
        if offset != 0 {
            match self.view {
                View::Day => {
                    if let Some(date) = self
                        .focused
                        .checked_add_signed(Duration::days(offset as i64))
                        .filter(|d| (1..=9999).contains(&d.year()))
                    {
                        self.focused = date;
                    }
                }
                View::Month => self.shift_month(offset),
                View::Year => self.shift_month(offset * 12),
            }
        } else {
            match key.key.as_str() {
                "escape" => self.close(window, cx),
                "enter" | "space" => match self.view {
                    View::Day => self.choose(Some(self.focused), window, cx),
                    View::Month => self.view = View::Day,
                    View::Year => self.view = View::Month,
                },
                "pageup" | "pagedown" => {
                    let step = if key.modifiers.shift || self.view != View::Day {
                        12
                    } else {
                        1
                    };
                    self.shift_month(if key.key == "pageup" { -step } else { step });
                }
                "home" | "end" => {
                    let weekday = self.focused.weekday().num_days_from_sunday() as i64;
                    self.focused += Duration::days(if key.key == "home" {
                        -weekday
                    } else {
                        6 - weekday
                    });
                }
                "tab" => {
                    self.close(window, cx);
                    return;
                }
                _ => return,
            }
        }
        cx.stop_propagation();
        cx.notify();
    }
    /// A window-level floating calendar. Call after the picker trigger is laid out.
    pub fn overlay(entity: &Entity<Self>, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        entity.update(cx, |this, cx| this.panel(window, cx))
    }
    fn panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let visible = self.open || self.alpha.is_animating() || self.alpha.value() > 0.001;
        if self.alpha.is_animating() || self.slide.is_animating() {
            window.request_animation_frame();
        }
        if !visible {
            return None;
        }
        let bounds = self.bounds.get()?;
        let theme = *Theme::global(cx);
        let t = self.alpha.value().clamp(0.0, 1.0);
        let title = match self.view {
            View::Day => self.focused.format("%B %Y").to_string(),
            View::Month => self.focused.year().to_string(),
            View::Year => {
                let start = self.focused.year().div_euclid(12) * 12;
                format!("{} – {}", start.max(1), (start + 11).min(9999))
            }
        };
        let heading = Button::new("calendar-heading", ButtonKind::Ghost)
            .size(ButtonSize::Sm)
            .label(title)
            .radius(theme.radius_control)
            .pad_x(8.0)
            .on_click({
                let entity = cx.entity();
                move |_, cx| {
                    entity.update(cx, |this, cx| {
                        this.view = if this.view == View::Day {
                            View::Month
                        } else {
                            View::Year
                        };
                        cx.notify();
                    })
                }
            });
        let nav = |id: &'static str, icon, offset, cx: &Context<Self>| {
            Button::new(id, ButtonKind::Ghost)
                .size(ButtonSize::Icon)
                .height(theme.control_height_sm)
                .radius(theme.radius_control)
                .icon(icon)
                .tooltip(match (self.view, offset < 0) {
                    (View::Day, true) => "Previous month",
                    (View::Day, false) => "Next month",
                    (View::Month, true) => "Previous year",
                    (View::Month, false) => "Next year",
                    (View::Year, true) => "Previous years",
                    (View::Year, false) => "Next years",
                })
                .on_click({
                    let entity = cx.entity();
                    move |_, cx| {
                        entity.update(cx, |this, cx| {
                            let step = match this.view {
                                View::Day => 1,
                                View::Month => 12,
                                View::Year => 144,
                            };
                            this.shift_month(offset * step);
                            cx.notify();
                        })
                    }
                })
        };
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .mb(px(8.0))
            .child(heading)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(nav("calendar-prev", Icon::ChevronLeft, -1, cx))
                    .when(self.view == View::Day, |row| {
                        row.child(
                            Button::new("calendar-today", ButtonKind::Ghost)
                                .size(ButtonSize::Sm)
                                .label("Today")
                                .radius(theme.radius_control)
                                .pad_x(8.0)
                                .on_click({
                                    let entity = cx.entity();
                                    move |window, cx| {
                                        entity.update(cx, |this, cx| {
                                            this.choose(Some(Local::now().date_naive()), window, cx)
                                        })
                                    }
                                }),
                        )
                    })
                    .child(nav("calendar-next", Icon::ChevronRight, 1, cx)),
            );
        let mut body = div().flex().flex_col().gap(px(2.0)).child(header);
        if self.view == View::Day {
            body = body.child(
                div()
                    .flex()
                    .children(["S", "M", "T", "W", "T", "F", "S"].map(|day| {
                        div()
                            .w(px(36.0))
                            .h(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(11.0))
                            .text_color(theme.color_text_muted)
                            .child(day)
                    })),
            );
            for week in month_days(self.focused).chunks(7) {
                body = body.child(div().flex().children(week.iter().map(|date| {
                    let date = *date;
                    let selected = self.value == Some(date);
                    div().w(px(36.0)).flex().justify_center().child(
                        div()
                            .id(SharedString::from(format!("calendar-{date}")))
                            .debug_selector(move || format!("calendar-{date}"))
                            .w(px(32.0))
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(theme.radius_item))
                            .cursor_pointer()
                            .text_color(if date.month() != self.focused.month() {
                                theme.color_text_muted
                            } else if date == self.today {
                                theme.color_brand
                            } else {
                                theme.color_text_primary
                            })
                            .when(selected, |day| {
                                day.bg(theme.color_surface_selected)
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                            })
                            .when(date == self.focused, |day| {
                                day.border_1().border_color(theme.color_border_strong)
                            })
                            .hover(|day| day.bg(theme.color_surface_hover))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.choose(Some(date), window, cx)
                            }))
                            .child(date.day().to_string()),
                    )
                })));
            }
        } else {
            let year_view = self.view == View::Year;
            let start = self.focused.year().div_euclid(12) * 12;
            for row in 0..4 {
                body = body.child(div().flex().children((0..3).map(|column| {
                    let index = row * 3 + column;
                    let year = if year_view {
                        (start + index).clamp(1, 9999)
                    } else {
                        self.focused.year()
                    };
                    let month = if year_view {
                        self.focused.month()
                    } else {
                        index as u32 + 1
                    };
                    let date = date_clamped(year, month, self.focused.day());
                    let label = if year_view {
                        year.to_string()
                    } else {
                        date.format("%b").to_string()
                    };
                    div()
                        .id(("calendar-period", index as usize))
                        .w(px(84.0))
                        .h(px(36.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .rounded(px(theme.radius_item))
                        .hover(|s| s.bg(theme.color_surface_hover))
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.focused = date;
                            this.view = if year_view { View::Month } else { View::Day };
                            cx.notify();
                        }))
                })));
            }
        }
        let panel = div()
            .id("date-picker-panel")
            .debug_selector(|| "date-picker-panel".into())
            .relative()
            .occlude()
            .key_context("DatePicker")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &crate::Dismiss, window, cx| this.close(window, cx)))
            .on_key_down(cx.listener(Self::key))
            .on_mouse_down_out(cx.listener(
                move |this, event: &gpui::MouseDownEvent, window, cx| {
                    if !bounds.contains(&event.position) {
                        this.close(window, cx);
                    }
                },
            ))
            .w(px(276.0))
            .p(px(12.0))
            .bg(theme.color_surface_subtle)
            .text_size(px(12.0))
            .text_color(theme.color_text_secondary)
            .opacity(t)
            .child(body);
        let mut shadows = material::float_drop(&theme);
        shadows.push(material::drop(20.0, 56.0, theme.color_shadow_float));
        let panel = material::floating_panel(panel, theme.radius_surface, &theme).shadow(shadows);
        Some(crate::patterns::window_anchor(
            bounds,
            crate::hang::hang(
                f32::from(bounds.size.height) + 4.0,
                (1.0 - self.slide.value()) * 4.0,
                panel,
            ),
        ))
    }
}
impl Render for DatePicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let bounds = self.bounds.clone();
        let label = self
            .value
            .map(|date| date.format("%m/%d/%Y").to_string())
            .unwrap_or_else(|| self.label.to_string());
        div()
            .relative()
            .flex()
            .items_center()
            .gap(px(8.0))
            .track_focus(&self.focus.clone().tab_stop(!self.disabled))
            .on_key_down(cx.listener(Self::key))
            .child(
                div()
                    .id("date-picker-value")
                    .flex_1()
                    .min_w(px(90.0))
                    .h(px(theme.field_height_sm))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme.radius_control))
                    .bg(material::face(
                        theme.field_fill_top,
                        theme.field_fill_bottom,
                    ))
                    .text_size(px(12.0))
                    .text_color(if self.disabled {
                        theme.color_text_disabled
                    } else if self.value.is_none() {
                        theme.color_text_muted
                    } else {
                        theme.color_text_primary
                    })
                    .child(label)
                    .on_click(cx.listener(|this, _, window, cx| this.open(window, cx))),
            )
            .child(
                Button::new("date-picker-trigger", ButtonKind::Secondary)
                    .size(ButtonSize::Icon)
                    .icon(Icon::CalendarClock)
                    .tooltip("Choose date")
                    .disabled(self.disabled)
                    .on_click({
                        let entity = cx.entity();
                        move |window, cx| {
                            entity.update(cx, |this, cx| {
                                if this.open {
                                    this.close(window, cx)
                                } else {
                                    this.open(window, cx)
                                }
                            })
                        }
                    }),
            )
            .child(
                Button::new("date-picker-clear", ButtonKind::Secondary)
                    .size(ButtonSize::Sm)
                    .label("Clear")
                    .tooltip("Clear date")
                    .disabled(self.disabled || self.value.is_none())
                    .on_click({
                        let entity = cx.entity();
                        move |window, cx| {
                            entity.update(cx, |this, cx| this.choose(None, window, cx))
                        }
                    }),
            )
            .child(
                canvas(move |rect, _, _| bounds.set(Some(rect)), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            .text_color(theme.color_text_primary)
    }
}
fn date_clamped(year: i32, month: u32, day: u32) -> Date {
    (1..=day.min(31))
        .rev()
        .find_map(|day| Date::from_ymd_opt(year, month, day))
        .expect("valid year and month")
}
fn month_days(date: Date) -> Vec<Date> {
    let first = date.with_day(1).expect("first day");
    let start = first - Duration::days(first.weekday().num_days_from_sunday() as i64);
    let last = date_clamped(date.year(), date.month(), 31);
    let count = (last - start).num_days() + 1;
    let count = ((count + 6) / 7) * 7;
    (0..count).map(|day| start + Duration::days(day)).collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    fn fixture(cx: &mut TestAppContext) -> (Entity<DatePicker>, gpui::VisualTestContext) {
        let picker = cx.new(DatePicker::new);
        picker.update(cx, |picker, cx| {
            picker.set_value(DatePicker::parse("2024-01-31"), cx)
        });
        let host = picker.clone();
        let mut visual =
            crate::test_support::open(cx, gpui::size(650.0, 550.0), move |window, cx| {
                let overlay = DatePicker::overlay(&host, window, cx);
                div()
                    .p(px(20.0))
                    .child(host.clone())
                    .children(overlay)
                    .into_any_element()
            });
        visual.update(|window, cx| picker.update(cx, |picker, cx| picker.open(window, cx)));
        crate::test_support::settle(&mut visual);
        (picker, visual)
    }

    #[gpui::test]
    fn keyboard_moves_across_months_then_commits_once(cx: &mut TestAppContext) {
        let (picker, mut visual) = fixture(cx);
        let changes = Rc::new(std::cell::RefCell::new(Vec::new()));
        let heard = changes.clone();
        visual.update(|_, cx| {
            cx.subscribe(&picker, move |_, event: &DateChanged, _| {
                heard.borrow_mut().push(event.0)
            })
            .detach()
        });
        visual.simulate_keystrokes("right");
        picker.read_with(&visual, |picker, _| {
            assert_eq!(picker.focused.to_string(), "2024-02-01");
            assert_eq!(picker.value.unwrap().to_string(), "2024-01-31");
        });
        visual.simulate_keystrokes("down");
        visual.simulate_keystrokes("home");
        picker.read_with(&visual, |picker, _| {
            assert_eq!(picker.focused.to_string(), "2024-02-04")
        });
        visual.simulate_keystrokes("end");
        visual.simulate_keystrokes("enter");
        picker.read_with(&visual, |picker, _| {
            assert_eq!(picker.value.unwrap().to_string(), "2024-02-10");
            assert!(!picker.open);
        });
        assert_eq!(changes.borrow().len(), 1);
    }

    #[gpui::test]
    fn escape_does_not_commit_and_reopen_restores_selection(cx: &mut TestAppContext) {
        let (picker, mut visual) = fixture(cx);
        visual.simulate_keystrokes("pagedown");
        visual.simulate_keystrokes("shift-pagedown");
        picker.read_with(&visual, |picker, _| {
            assert_eq!(picker.focused.to_string(), "2025-02-28")
        });
        visual.simulate_keystrokes("escape");
        visual.update(|window, cx| {
            picker.update(cx, |picker, cx| {
                assert!(!picker.open);
                picker.open(window, cx);
                assert_eq!(picker.focused.to_string(), "2024-01-31");
            })
        });
        visual.simulate_click(gpui::point(px(600.0), px(500.0)), gpui::Modifiers::none());
        picker.read_with(&visual, |picker, _| assert!(!picker.open));
    }

    #[gpui::test]
    fn today_and_clear_emit_optional_dates(cx: &mut TestAppContext) {
        let (picker, mut visual) = fixture(cx);
        let changes = Rc::new(std::cell::RefCell::new(Vec::new()));
        let heard = changes.clone();
        visual.update(|_, cx| {
            cx.subscribe(&picker, move |_, event: &DateChanged, _| {
                heard.borrow_mut().push(event.0)
            })
            .detach()
        });
        let today = Local::now().date_naive();
        let button = visual.debug_bounds("calendar-today").unwrap();
        visual.simulate_click(button.center(), gpui::Modifiers::none());
        picker.read_with(&visual, |picker, _| {
            assert_eq!(picker.value, Some(today));
            assert!(!picker.open);
        });
        crate::test_support::settle(&mut visual);
        let clear = visual.debug_bounds("date-picker-clear").unwrap();
        visual.simulate_click(clear.center(), gpui::Modifiers::none());
        assert_eq!(*changes.borrow(), vec![Some(today), None]);
        visual.update(|window, cx| {
            picker.update(cx, |picker, cx| {
                assert_eq!(picker.value, None);
                picker.disabled = true;
                picker.open(window, cx);
                assert!(!picker.open);
                picker.choose(Some(today), window, cx);
                assert_eq!(picker.value, None);
            })
        });
    }

    #[gpui::test]
    fn calendar_flips_above_a_low_trigger_and_stops_animating(cx: &mut TestAppContext) {
        let picker = cx.new(DatePicker::new);
        let host = picker.clone();
        let mut visual =
            crate::test_support::open(cx, gpui::size(650.0, 550.0), move |window, cx| {
                let overlay = DatePicker::overlay(&host, window, cx);
                div()
                    .relative()
                    .size_full()
                    .child(
                        div()
                            .absolute()
                            .right(px(20.0))
                            .bottom(px(20.0))
                            .w(px(300.0))
                            .child(host.clone()),
                    )
                    .children(overlay)
                    .into_any_element()
            });
        visual.update(|window, cx| picker.update(cx, |picker, cx| picker.open(window, cx)));
        crate::test_support::settle(&mut visual);
        let panel = visual
            .debug_bounds("date-picker-panel")
            .expect("calendar visible");
        picker.read_with(&visual, |picker, _| {
            let trigger = picker.bounds.get().unwrap();
            assert!(panel.bottom() <= trigger.top());
            assert!(!picker.alpha.is_animating());
            assert!(!picker.slide.is_animating());
        });
    }

    #[test]
    fn strict_dates_and_six_week_months() {
        assert!(DatePicker::parse("2024-02-29").is_some());
        for bad in ["2025-02-29", "2026-04-31", "2026-1-01", "", "0000-01-01"] {
            assert!(DatePicker::parse(bad).is_none(), "{bad}");
        }
        let days = month_days(DatePicker::parse("2026-08-01").unwrap());
        assert_eq!(days.len(), 42);
        assert_eq!(days[0].to_string(), "2026-07-26");
        assert_eq!(days[41].to_string(), "2026-09-05");
    }
    #[gpui::test]
    fn month_moves_clamp_and_do_not_select(cx: &mut TestAppContext) {
        let picker = cx.new(DatePicker::new);
        picker.update(cx, |picker, cx| {
            picker.set_value(DatePicker::parse("2024-01-31"), cx);
            picker.shift_month(1);
            assert_eq!(picker.focused.to_string(), "2024-02-29");
            assert_eq!(picker.value.unwrap().to_string(), "2024-01-31");
            picker.shift_month(12);
            assert_eq!(picker.focused.to_string(), "2025-02-28");
        });
    }
}
