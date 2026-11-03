//! `SidebarItem` (`.pw-sidebar-item`): a 28px row (padding 0 9px 0 8px, radius 8, gap 8)
//! with a 14px icon and 13px/1.2 text in the muted color. Hover fills it with
//! `--color-surface-hover`, the press with `--color-surface-active`, and the open page with
//! `--color-surface-selected`; text turns primary on hover, press, and the open page. Color
//! and fill ease over `--duration-fast` on `--ease-standard`.
//!
//! `SidebarSectionLabel` is `SidebarSection`'s toggle label.

use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, ElementId, FontWeight, IntoElement, MouseButton, MouseDownEvent,
    SharedString, Window, div, prelude::*, px, relative,
};

use crate::ellipsis::ellipsis;
use crate::icons::{Icon, icon_colored, icon_rotated};
use crate::motion::{Curve, Tween};
use crate::pointer::{PointerTiming, track_with_hover, use_pointer_timed};
use crate::theme::{Theme, fade, mix};
use crate::{Handler, ValueHandler};

type ClickHandler = Handler;
type HoverHandler = ValueHandler<bool>;
type RightClickHandler = Rc<dyn Fn(&MouseDownEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct SidebarItem {
    id: ElementId,
    label: SharedString,
    icon: Option<Icon>,
    /// A custom leading element and the size of the box it sits in (`.pw-sidebar-item-icon`).
    leading: Option<(AnyElement, f32)>,
    /// Replaces the one-line label (`.pw-sidebar-item-label`).
    content: Option<AnyElement>,
    trailing: Option<AnyElement>,
    min_height: f32,
    pad_y: f32,
    gap: f32,
    active: bool,
    footer: bool,
    on_click: Option<ClickHandler>,
    on_hover: Option<HoverHandler>,
    on_right_click: Option<RightClickHandler>,
}

impl SidebarItem {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            leading: None,
            content: None,
            trailing: None,
            min_height: 28.0,
            pad_y: 0.0,
            gap: 8.0,
            active: false,
            footer: false,
            on_click: None,
            on_hover: None,
            on_right_click: None,
        }
    }

    /// `.dispatch-run-item`: min-height 52px, 6px block padding, 12px gap.
    pub fn run_row(mut self) -> Self {
        self.min_height = 52.0;
        self.pad_y = 6.0;
        self.gap = 12.0;
        self
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// A leading element centered in a `box_size` square, which it may overflow.
    pub fn leading(mut self, element: impl IntoElement, box_size: f32) -> Self {
        self.leading = Some((element.into_any_element(), box_size));
        self
    }

    /// The label's place, for content richer than one line of text.
    pub fn content(mut self, element: impl IntoElement) -> Self {
        self.content = Some(element.into_any_element());
        self
    }

    /// An element after the label, such as a `Kbd`.
    pub fn trailing(mut self, element: impl IntoElement) -> Self {
        self.trailing = Some(element.into_any_element());
        self
    }

    /// The open page (`aria-current="page"`).
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// `.pw-sidebar-item--footer`: grows to fill the footer row.
    pub fn footer(mut self) -> Self {
        self.footer = true;
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    pub fn on_hover(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_hover = Some(Rc::new(handler));
        self
    }

    pub fn on_right_click(
        mut self,
        handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_right_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for SidebarItem {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let pointer = use_pointer_timed(self.id.clone(), PointerTiming::item(&theme), window, cx);
        let (hover, press) = {
            let state = pointer.read(cx);
            (state.hover(), state.press())
        };
        let rest = if self.active {
            theme.color_surface_selected
        } else {
            fade(theme.color_surface_hover, 0.0)
        };
        let fill = mix(
            mix(rest, theme.color_surface_hover, hover),
            theme.color_surface_active,
            press,
        );
        let engaged = if self.active { 1.0 } else { hover.max(press) };
        let ink = mix(theme.color_text_muted, theme.color_text_primary, engaged);

        let mut row = div()
            .id(self.id.clone())
            .flex()
            .items_center()
            .gap(px(self.gap))
            .min_h(px(self.min_height))
            .py(px(self.pad_y))
            .pl(px(theme.space_2))
            .pr(px(9.0))
            .rounded(px(theme.radius_sm))
            .bg(fill)
            .text_color(ink)
            .text_size(px(13.0))
            .line_height(relative(1.2))
            .cursor_pointer();
        row = if self.footer {
            row.flex_1().min_w(px(0.0))
        } else {
            row.w_full()
        };
        if let Some(name) = self.icon {
            row = row.child(icon_colored(name, 14.0, ink));
        }
        if let Some((element, box_size)) = self.leading {
            row = row.child(
                div()
                    .size(px(box_size))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(element),
            );
        }
        row = match self.content {
            Some(content) => row.child(div().flex_1().min_w(px(0.0)).child(content)),
            None => row.child(div().flex_1().min_w(px(0.0)).child(ellipsis(self.label))),
        };
        if let Some(trailing) = self.trailing {
            row = row.child(trailing);
        }
        if let Some(handler) = self.on_right_click {
            row = row.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                cx.stop_propagation();
                handler(event, window, cx);
            });
        }
        let row = track_with_hover(row, &pointer, true, self.on_hover);
        match self.on_click {
            Some(handler) => row.on_click(move |event, window, cx| {
                if event.standard_click() {
                    handler(window, cx);
                }
            }),
            None => row,
        }
    }
}

/// `SidebarSection`'s toggle (`.pw-sidebar-group-label.pw-sidebar-group-toggle`): a 28px row
/// with 8px sides, 12px/500 muted text and a 12px chevron right after it, 4px apart. Hover
/// turns the text primary over `--duration-fast` and shows the chevron, which also shows
/// while the group is collapsed, turned -90° over `--duration-fast`. The text clips without
/// an ellipsis, as the button's anonymous text box does.
#[derive(IntoElement)]
pub struct SidebarSectionLabel {
    id: ElementId,
    label: SharedString,
    collapsed: bool,
    pad_left: f32,
    pad_right: f32,
    on_toggle: Option<ClickHandler>,
    on_right_click: Option<RightClickHandler>,
}

impl SidebarSectionLabel {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>, collapsed: bool) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            collapsed,
            pad_left: 8.0,
            pad_right: 8.0,
            on_toggle: None,
            on_right_click: None,
        }
    }

    pub fn padding(mut self, left: f32, right: f32) -> Self {
        self.pad_left = left;
        self.pad_right = right;
        self
    }

    pub fn on_toggle(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(handler));
        self
    }

    pub fn on_right_click(
        mut self,
        handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_right_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for SidebarSectionLabel {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let pointer = use_pointer_timed(self.id.clone(), PointerTiming::item(&theme), window, cx);
        let (hovered, hover_color) = {
            let state = pointer.read(cx);
            (state.hovered(), state.hover_color())
        };
        let turn_target = if self.collapsed { -90.0 } else { 0.0 };
        let turn = window.use_keyed_state(
            ElementId::from((self.id.clone(), SharedString::from("chevron"))),
            cx,
            |_, _: &mut Context<Tween>| Tween::new(turn_target),
        );
        let degrees = turn.update(cx, |tween, _| {
            if tween.target() != turn_target {
                tween.animate_to(
                    turn_target,
                    theme.duration_fast,
                    Curve::Ease(theme.ease_standard),
                );
            }
            tween.value()
        });
        if turn.read(cx).is_animating() {
            window.request_animation_frame();
        }
        let ink = mix(
            theme.color_text_muted,
            theme.color_text_primary,
            hover_color,
        );
        let chevron_shown = hovered || self.collapsed;

        let mut row = div()
            .id(self.id.clone())
            .w_full()
            .min_h(px(theme.sidebar_row_height))
            .flex()
            .items_center()
            .gap(px(theme.space_1))
            .pl(px(self.pad_left))
            .pr(px(self.pad_right))
            .overflow_hidden()
            .whitespace_nowrap()
            .rounded(px(theme.radius_sm))
            .text_size(px(theme.font_size_sm))
            .font_weight(FontWeight::MEDIUM)
            .text_color(ink)
            .cursor_pointer()
            .child(div().flex_none().child(self.label))
            .child(
                div()
                    .size(px(12.0))
                    .flex_none()
                    .when(chevron_shown, |slot| {
                        slot.child(icon_rotated(
                            Icon::ChevronDown,
                            12.0,
                            ink,
                            degrees.to_radians(),
                        ))
                    }),
            );
        if let Some(handler) = self.on_right_click {
            row = row.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                cx.stop_propagation();
                handler(event, window, cx);
            });
        }
        let row = track_with_hover(row, &pointer, true, None);
        match self.on_toggle {
            Some(handler) => row.on_click(move |event, window, cx| {
                if event.standard_click() {
                    handler(window, cx);
                }
            }),
            None => row,
        }
    }
}
