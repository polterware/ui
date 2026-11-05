//! `A11yDialog` with its default chrome (`dialog.tsx`, `dialog.css`): a form or a question
//! in a small dialog, over a [`modal_layer`](super::modal_layer).
//!
//! It looks like the design system's: the `surface-panel-float` frame at its size's width
//! (`sm` 30rem, `md` 36rem, `lg` 44rem, never past the window less 2.5rem) and at most
//! `min(90vh, 640px)` tall (`720px` from `md`; `lg` `min(85vh, 760px)`), on `--radius-dialog`.
//! The header (`px-7 pt-6 pb-2`) holds the 16px semibold title, its optional 12px muted
//! description `mt-1` under it, and Close at its end: an `h-8` round button with a 16px muted
//! X, pulled `-mt-1 -mr-2`, that fills with `--color-surface-raised` in primary ink under the
//! pointer. The body (`px-7 py-4`, 13px secondary text) scrolls once the dialog reaches its
//! height; the footer (`px-7 pt-3 pb-6`) sets its buttons at its end, `gap-2` apart.
//!
//! [`form_stack`] is `FormStack` (a column 14px apart) and [`Field`] is `Field`: an 11px
//! medium muted label (`line-height: 1.3`) over its control 6px lower, then an optional
//! hint in muted ink or error in danger ink (11px, `line-height: 1.35`).
//!
//! [`DialogPresence`] is `dialog.css`'s motion: the panel fades in and its frame scales from
//! 0.97 to 1 on the slow spring (0.24s, bounce 0.12; the fade and the scrim on
//! `--ease-standard` over the same time), and it leaves on the tier's quicker tween (0.16s,
//! `--ease-standard`), fading and shrinking back to 0.97. GPUI cannot scale an element, so the
//! frame (its face, drop, and lit edge) is drawn at the scale around the content, which keeps
//! its layout; while the frame is under 1 the content is nearly transparent, so the two read
//! as one panel. Hand [`DialogPresence::frame`]'s [`DialogPose`] to [`Dialog::pose`] and its
//! opacity to [`modal_layer`](super::modal_layer)'s scrim.
//!
//! It behaves like gpui-kit's `Dialog`: the app owns whether it is open and what Close and
//! the buttons do; the dialog only draws and reports clicks.

use std::time::Instant;

use gpui::{
    AnyElement, App, Div, FontWeight, IntoElement, MouseButton, ParentElement, RenderOnce,
    SharedString, Window, div, prelude::*, px, relative,
};

use crate::Handler;
use crate::icons::{Icon, icon_colored};
use crate::material::{Edge, drop, edge, float_drop};
use crate::motion::{Curve, SpringTier, Tween};
use crate::theme::{Ease, Theme, rem, tw};

/// The scale a dialog's frame opens from and closes to (`pw-dialog-content-in`).
pub const DIALOG_SCALE_FROM: f32 = 0.97;
/// `--ease-standard`, the fade's and the exit's curve.
const EASE_STANDARD: Ease = Ease(0.2, 0.0, 0.0, 1.0);

/// How far in a dialog is drawn: its opacity (the scrim's too) and its frame's scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DialogPose {
    pub opacity: f32,
    pub scale: f32,
}

impl DialogPose {
    pub const SHOWN: Self = Self {
        opacity: 1.0,
        scale: 1.0,
    };
    pub const HIDDEN: Self = Self {
        opacity: 0.0,
        scale: DIALOG_SCALE_FROM,
    };
}

/// A dialog's open and close motion: see the module's docs. It starts closed, at
/// [`DialogPose::HIDDEN`].
#[derive(Clone, Copy, Debug)]
pub struct DialogPresence {
    open: bool,
    opacity: Tween,
    /// 0 at the hidden scale, 1 at full size; the spring takes it a hair past 1.
    grow: Tween,
}

impl Default for DialogPresence {
    fn default() -> Self {
        Self::new()
    }
}

impl DialogPresence {
    pub fn new() -> Self {
        Self {
            open: false,
            opacity: Tween::new(0.0),
            grow: Tween::new(0.0),
        }
    }

    /// Open and at rest, with no motion: a capture's dialog.
    pub fn shown() -> Self {
        Self {
            open: true,
            opacity: Tween::new(1.0),
            grow: Tween::new(1.0),
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Opens or closes it from wherever it is; returns whether that changed anything.
    pub fn set_open(&mut self, open: bool) -> bool {
        if self.open == open {
            return false;
        }
        self.open = open;
        let slow = SpringTier::Slow;
        if open {
            self.opacity
                .animate_to(1.0, slow.duration(), Curve::Ease(EASE_STANDARD));
            self.grow.animate_to(1.0, slow.duration(), slow.curve());
        } else {
            let leave = Curve::Ease(EASE_STANDARD);
            self.opacity.animate_to(0.0, slow.exit_duration(), leave);
            self.grow.animate_to(0.0, slow.exit_duration(), leave);
        }
        true
    }

    /// Whether it must be drawn: open, or still leaving.
    pub fn is_present(&self) -> bool {
        self.open || self.is_animating() || self.opacity.value() > 0.001
    }

    pub fn is_animating(&self) -> bool {
        self.opacity.is_animating() || self.grow.is_animating()
    }

    /// The pose at `now`.
    pub fn pose_at(&self, now: Instant) -> DialogPose {
        let grow = self.grow.value_at(now);
        DialogPose {
            opacity: self.opacity.value_at(now).clamp(0.0, 1.0),
            scale: DIALOG_SCALE_FROM + (1.0 - DIALOG_SCALE_FROM) * grow,
        }
    }

    /// The pose now, asking for the next frame while it moves.
    pub fn frame(&self, window: &mut Window) -> DialogPose {
        if self.is_animating() {
            window.request_animation_frame();
        }
        self.pose_at(Instant::now())
    }
}

/// The frame of a floating panel drawn at `scale` around its box: `fill`'s face, drop, and lit
/// edge, under the content.
pub(crate) fn scaled_frame(scale: f32, fill: Div) -> Div {
    let inset = relative((1.0 - scale) / 2.0);
    #[cfg(test)]
    let fill = fill.debug_selector(|| "scaled-frame".into());
    fill.absolute()
        .top(inset)
        .bottom(inset)
        .left(inset)
        .right(inset)
}

/// What the dialog keeps clear of the window's edges (`calc(100vw - 2.5rem)`).
fn window_inset() -> f32 {
    2.5 * rem()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogSize {
    Sm,
    Md,
    Lg,
}

impl DialogSize {
    /// Its width when the window has room.
    pub fn width(self) -> f32 {
        rem()
            * match self {
                DialogSize::Sm => 30.0,
                DialogSize::Md => 36.0,
                DialogSize::Lg => 44.0,
            }
    }

    /// Its tallest, in a window `viewport_height` tall.
    fn max_height(self, viewport_height: f32) -> f32 {
        match self {
            DialogSize::Sm => (viewport_height * 0.9).min(640.0),
            DialogSize::Md => (viewport_height * 0.9).min(720.0),
            DialogSize::Lg => (viewport_height * 0.85).min(760.0),
        }
    }
}

/// A dialog titled `title`, `presence` (0 to 1) of the way in.
pub fn dialog(title: impl Into<SharedString>, presence: f32) -> Dialog {
    Dialog {
        title: title.into(),
        title_leading: None,
        description: None,
        size: DialogSize::Md,
        presence,
        on_close: None,
        pose: None,
        footer: Vec::new(),
        body: Vec::new(),
        overlay: Vec::new(),
        flush: false,
    }
}

#[derive(IntoElement)]
pub struct Dialog {
    title: SharedString,
    title_leading: Option<AnyElement>,
    description: Option<SharedString>,
    size: DialogSize,
    presence: f32,
    pose: Option<DialogPose>,
    on_close: Option<Handler>,
    footer: Vec<AnyElement>,
    body: Vec<AnyElement>,
    overlay: Vec<AnyElement>,
    flush: bool,
}

impl Dialog {
    pub fn size(mut self, size: DialogSize) -> Self {
        self.size = size;
        self
    }

    /// `titleLeading`: drawn before the title, 10px from it, such as the topic's icon tile.
    pub fn title_leading(mut self, leading: impl IntoElement) -> Self {
        self.title_leading = Some(leading.into_any_element());
        self
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Drawn at `pose` (from [`DialogPresence::frame`]) instead of `presence`, which only
    /// fades it and lifts it 8px.
    pub fn pose(mut self, pose: DialogPose) -> Self {
        self.pose = Some(pose);
        self
    }

    /// Close at the header's end.
    pub fn on_close(mut self, handler: Handler) -> Self {
        self.on_close = Some(handler);
        self
    }

    /// `chrome="flush"`: no header, and a body with no padding of its own, so the content lays
    /// out its own title (a [`Peek`](super::Peek)); the title stays the dialog's name.
    pub fn flush(mut self) -> Self {
        self.flush = true;
        self
    }

    /// The footer's buttons, in order; the last is the main one.
    pub fn footer(mut self, buttons: impl IntoIterator<Item = AnyElement>) -> Self {
        self.footer.extend(buttons);
        self
    }

    /// Drawn over the dialog, outside the body's scroll and clip: the panel a control in the
    /// body opens, such as a select's menu.
    pub fn overlay(mut self, overlay: impl IntoElement) -> Self {
        self.overlay.push(overlay.into_any_element());
        self
    }
}

impl ParentElement for Dialog {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.body.extend(elements);
    }
}

impl RenderOnce for Dialog {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let viewport = window.viewport_size();
        let (viewport_w, viewport_h) = (f32::from(viewport.width), f32::from(viewport.height));
        let width = self
            .size
            .width()
            .min(viewport_w - window_inset())
            .max(280.0);
        let max_h = self.size.max_height(viewport_h).max(200.0);
        let radius = theme.radius_dialog;
        let mut shadow = float_drop(&theme);
        shadow.push(drop(20.0, 56.0, theme.color_shadow_float));
        let t = self.presence.clamp(0.0, 1.0);
        let (opacity, scale, lift) = match self.pose {
            Some(pose) => (pose.opacity, pose.scale, 0.0),
            None => (t, 1.0, (1.0 - t) * 8.0),
        };

        let close = self.on_close.map(|handler| {
            div()
                .id("dialog-close")
                .flex_none()
                .size(px(tw(8.0)))
                .mt(px(-tw(1.0)))
                .mr(px(-tw(2.0)))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .text_color(theme.color_text_muted)
                .hover(move |close| {
                    close
                        .bg(theme.color_surface_raised)
                        .text_color(theme.color_text_primary)
                })
                .on_click(move |_, window, cx| handler(window, cx))
                .child(icon_colored(Icon::Close, 16.0, theme.color_text_muted))
        });
        let header = div()
            .flex_none()
            .flex()
            .items_start()
            .justify_between()
            .gap(px(tw(3.0)))
            .px(px(tw(7.0)))
            .pt(px(tw(6.0)))
            .pb(px(tw(2.0)))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(tw(2.5)))
                            .min_w(px(0.0))
                            .children(
                                self.title_leading
                                    .map(|leading| div().flex_none().child(leading)),
                            )
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .text_size(px(16.0))
                                    .line_height(px(24.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.color_text_primary)
                                    .child(self.title),
                            ),
                    )
                    .children(self.description.map(|description| {
                        div()
                            .mt(px(tw(1.0)))
                            .text_size(px(12.0))
                            .text_color(theme.color_text_muted)
                            .child(description)
                    })),
            )
            .children(close);
        let footer = (!self.footer.is_empty()).then(|| {
            div()
                .flex_none()
                .flex()
                .items_center()
                .justify_end()
                .gap(px(tw(2.0)))
                .px(px(tw(7.0)))
                .pt(px(tw(3.0)))
                .pb(px(tw(6.0)))
                .children(self.footer)
        });
        let frame = scaled_frame(
            scale,
            div()
                .rounded(px(radius))
                .bg(theme.color_surface_subtle)
                .shadow(shadow)
                .child(edge(Edge::Quiet, radius, &theme)),
        );
        div()
            .occlude()
            .relative()
            .w(px(width))
            .max_h(px(max_h))
            .flex()
            .flex_col()
            .font_family(theme.font_sans)
            .text_color(theme.color_text_primary)
            .mt(px(lift))
            .opacity(opacity)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(frame)
            .when(!self.flush, |dialog| dialog.child(header))
            .child(
                div()
                    .id("dialog-body")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .when(!self.flush, |body| body.px(px(tw(7.0))).py(px(tw(4.0))))
                    .flex()
                    .flex_col()
                    .text_size(px(13.0))
                    .text_color(theme.color_text_secondary)
                    .children(self.body),
            )
            .children(footer)
            .children(self.overlay)
    }
}

/// `FormStack`: `children` in a column 14px apart.
pub fn form_stack(children: impl IntoIterator<Item = AnyElement>) -> gpui::Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .children(children)
}

/// `Field`: see the module's docs.
#[derive(IntoElement)]
pub struct Field {
    label: SharedString,
    control: AnyElement,
    hint: Option<SharedString>,
    error: Option<SharedString>,
}

/// `control` under the label `label`.
pub fn field(label: impl Into<SharedString>, control: impl IntoElement) -> Field {
    Field {
        label: label.into(),
        control: control.into_any_element(),
        hint: None,
        error: None,
    }
}

impl Field {
    pub fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// What is wrong with the value, in place of the hint.
    pub fn error(mut self, error: Option<impl Into<SharedString>>) -> Self {
        self.error = error.map(Into::into);
        self
    }
}

impl RenderOnce for Field {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = Theme::global(cx);
        let note = self
            .error
            .map(|error| (error, theme.color_danger))
            .or(self.hint.map(|hint| (hint, theme.color_text_muted)));
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .line_height(px(11.0 * 1.3))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.color_text_muted)
                    .child(self.label),
            )
            .child(self.control)
            .children(note.map(|(text, color)| {
                div()
                    .text_size(px(11.0))
                    .line_height(px(11.0 * 1.35))
                    .text_color(color)
                    .child(text)
            }))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use gpui::{TestAppContext, div, prelude::*, px, size};

    use super::{
        DIALOG_SCALE_FROM, DialogPose, DialogPresence, DialogSize, dialog, field, form_stack,
    };
    use crate::test_support::{draw, open};
    use crate::theme::{rem, tw};

    #[test]
    fn a_dialog_springs_in_and_tweens_out() {
        let mut presence = DialogPresence::new();
        assert!(!presence.is_present());
        assert_eq!(presence.pose_at(Instant::now()), DialogPose::HIDDEN);
        assert!(presence.set_open(true));
        assert!(
            !presence.set_open(true),
            "opening an open dialog changes nothing"
        );
        let start = Instant::now();
        let at = |presence: &DialogPresence, ms: u64| {
            presence.pose_at(start + Duration::from_millis(ms))
        };
        let first = at(&presence, 0);
        assert!(first.opacity < 0.01 && (first.scale - DIALOG_SCALE_FROM).abs() < 1e-3);
        // A quarter of the slow spring's 0.24s: two thirds of the way, the fade further on.
        let quarter = at(&presence, 60);
        let grown = (quarter.scale - DIALOG_SCALE_FROM) / (1.0 - DIALOG_SCALE_FROM);
        assert!((grown - 0.68).abs() < 0.03, "{grown}");
        assert!(quarter.opacity > 0.5 && quarter.opacity < 1.0);
        // Bounce 0.12 carries the frame a hair past full size before it lands.
        assert!(at(&presence, 180).scale > 1.0);
        assert_eq!(at(&presence, 240), DialogPose::SHOWN);
        assert!(presence.is_present());

        // Closing starts from where it is, so it leaves from a dialog at rest.
        let mut presence = DialogPresence::shown();
        assert!(presence.set_open(false));
        let start = Instant::now();
        let leaving = presence.pose_at(start + Duration::from_millis(80));
        assert!(leaving.opacity > 0.0 && leaving.opacity < 0.5);
        assert!(leaving.scale < 1.0 && leaving.scale > DIALOG_SCALE_FROM);
        assert!(presence.is_present(), "it stays drawn while it leaves");
        assert_eq!(
            presence.pose_at(start + Duration::from_millis(160)),
            DialogPose::HIDDEN
        );
        assert_eq!(DialogPresence::shown().pose_at(start), DialogPose::SHOWN);
    }

    #[gpui::test]
    fn the_frame_is_drawn_at_the_pose_s_scale(cx: &mut TestAppContext) {
        let mut cx = open(cx, size(1200.0, 800.0), |_, _| {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div().debug_selector(|| "dialog".into()).child(
                        dialog("Connect", 1.0)
                            .size(DialogSize::Sm)
                            .pose(DialogPose {
                                opacity: 0.5,
                                scale: 0.97,
                            })
                            .child(div().h(px(300.0))),
                    ),
                )
                .into_any_element()
        });
        draw(&mut cx);
        let panel = cx.debug_bounds("dialog").expect("the dialog draws");
        let frame = cx.debug_bounds("scaled-frame").expect("the frame draws");
        // The content keeps the dialog's width; the frame is 97% of it, centered.
        assert_eq!(panel.size.width, px(30.0 * rem()));
        let width = f32::from(frame.size.width);
        assert!((width - 0.97 * 30.0 * rem()).abs() < 0.5, "{width}");
        let left = f32::from(frame.origin.x - panel.origin.x);
        assert!((left - 0.015 * 30.0 * rem()).abs() < 0.5, "{left}");
        let height = f32::from(frame.size.height) / f32::from(panel.size.height);
        assert!((height - 0.97).abs() < 0.005, "{height}");
    }

    #[gpui::test]
    fn a_small_dialog_sets_its_fields_under_the_title(cx: &mut TestAppContext) {
        let mut cx = open(cx, size(1200.0, 800.0), |_, _| {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div().debug_selector(|| "dialog".into()).child(
                        dialog("Connect", 1.0)
                            .size(DialogSize::Sm)
                            .child(form_stack([div()
                                .debug_selector(|| "first-field".into())
                                .child(field("Host", div().h(px(32.0))))
                                .into_any_element()])),
                    ),
                )
                .into_any_element()
        });
        draw(&mut cx);
        let frame = cx.debug_bounds("dialog").expect("the dialog draws");
        let first = cx.debug_bounds("first-field").expect("the field draws");
        assert_eq!(frame.size.width, px(30.0 * rem()));
        assert_eq!(first.origin.x - frame.origin.x, px(tw(7.0)));
        // `pt-6`, the 24px title, `pb-2`, then the body's `py-4`.
        let top = tw(6.0) + 24.0 + tw(2.0) + tw(4.0);
        assert!((f32::from(first.origin.y - frame.origin.y) - top).abs() <= 1.0);
        // The 14.3px label, 6px, then the control.
        assert!((f32::from(first.size.height) - (14.3 + 6.0 + 32.0)).abs() <= 1.0);
    }
}
