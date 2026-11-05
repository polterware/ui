//! The message composer: the round card under a chat or a log where the reader writes the
//! next message.
//!
//! It looks like the design system's `Card` (`.card-panel`: `--color-surface-subtle`, the
//! quiet edge, and the raised drop), rounder ([`MESSAGE_COMPOSER_RADIUS`]), with the field on
//! top and a footer row under it: an optional start slot and the send button, the primary
//! icon button with the up arrow, at its end. It behaves like gpui-kit's `Input` with
//! `auto_grow`: the card grows with each line typed until the field reaches its
//! [`MessageComposer::max_height`], and then the field scrolls inside the card, fading at an
//! end that has more to show (the design system's `.pw-scroll-fade`, as [`fade_scroll`]
//! draws it), while the caret stays in view as it moves.
//!
//! The app holds the state: a [`TextInput`] made with [`TextInput::chat`] (Enter sends,
//! Shift+Enter breaks the line) and a [`ScrollHandle`] for the field's scroll, and it decides
//! what sending does. [`message_input`] makes the field in the composer's text.
//!
//! [`fade_scroll`]: super::fade_scroll

use gpui::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, FontWeight, IntoElement,
    ParentElement, Pixels, RenderOnce, ScrollHandle, SharedString, Window, canvas, div, prelude::*,
    px,
};

use gpui::Focusable as _;

use super::scroll_fade::{SCROLL_FADE, end_fade};
use crate::Handler;
use crate::button::{Button, ButtonKind, ButtonSize};
use crate::icons::Icon;
use crate::material::raised_panel;
use crate::text_input::TextInput;
use crate::theme::Theme;

/// The card's corner radius.
pub const MESSAGE_COMPOSER_RADIUS: f32 = 20.0;
/// The tallest the field grows before it scrolls inside the card.
pub const MESSAGE_COMPOSER_MAX_HEIGHT: f32 = 200.0;
/// Inside the card, around the field and the footer.
const PAD_X: f32 = 16.0;
const PAD_TOP: f32 = 14.0;
const PAD_BOTTOM: f32 = 10.0;
/// Between the field and the footer.
const GAP: f32 = 8.0;
/// The field's text and line height.
const TEXT_SIZE: f32 = 13.5;
const LINE_HEIGHT: f32 = 20.0;

/// A field for [`message_composer`]: [`TextInput::chat`] in the composer's text, with
/// `placeholder`, sending its text through `on_send`.
pub fn message_input(
    placeholder: impl Into<SharedString>,
    on_send: impl Fn(&str, &mut Window, &mut App) + 'static,
    cx: &mut App,
) -> Entity<TextInput> {
    let placeholder = placeholder.into();
    cx.new(|cx: &mut Context<TextInput>| {
        TextInput::new(cx)
            .bare()
            .prose(TEXT_SIZE, LINE_HEIGHT, FontWeight::NORMAL)
            .chat()
            .placeholder(placeholder)
            .on_submit(on_send)
    })
}

/// The composer `id` around `input`, whose scroll `scroll` holds. Set what the send button
/// does with [`MessageComposer::on_send`].
pub fn message_composer(
    id: impl Into<SharedString>,
    input: &Entity<TextInput>,
    scroll: &ScrollHandle,
) -> MessageComposer {
    MessageComposer {
        id: id.into(),
        input: input.clone(),
        scroll: scroll.clone(),
        max_height: MESSAGE_COMPOSER_MAX_HEIGHT,
        send_label: SharedString::from("Send"),
        on_send: None,
        can_send: None,
        busy: false,
        footer_start: None,
    }
}

#[derive(IntoElement)]
pub struct MessageComposer {
    id: SharedString,
    input: Entity<TextInput>,
    scroll: ScrollHandle,
    max_height: f32,
    send_label: SharedString,
    on_send: Option<Handler>,
    can_send: Option<bool>,
    busy: bool,
    footer_start: Option<AnyElement>,
}

impl MessageComposer {
    /// The tallest the field grows before it scrolls. Default: [`MESSAGE_COMPOSER_MAX_HEIGHT`].
    pub fn max_height(mut self, height: f32) -> Self {
        self.max_height = height;
        self
    }

    /// What the send button does: send the field's text, as its Enter does.
    pub fn on_send(mut self, handler: Handler) -> Self {
        self.on_send = Some(handler);
        self
    }

    /// The send button's tooltip. Default: Send.
    pub fn send_label(mut self, label: impl Into<SharedString>) -> Self {
        self.send_label = label.into();
        self
    }

    /// Whether the send button takes a press. Default: while the field holds more than
    /// white space.
    pub fn can_send(mut self, can: bool) -> Self {
        self.can_send = Some(can);
        self
    }

    /// A message is on its way: the send button is off.
    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }

    /// What the footer holds at its start, such as a note on where the message goes.
    pub fn footer_start(mut self, element: impl IntoElement) -> Self {
        self.footer_start = Some(element.into_any_element());
        self
    }
}

/// How far `handle` has scrolled from its top, and how far it can, as layout will place it.
fn scrolled(handle: &ScrollHandle) -> (f32, f32) {
    let max = f32::from(handle.max_offset().height).max(0.0);
    let offset = (-f32::from(handle.offset().y)).clamp(0.0, max);
    (offset, max)
}

/// The offset from the top that keeps a caret `caret_top..caret_bottom` (in the content's
/// coordinates) inside a view `view` tall that is now `offset` down, or `None` when it is
/// already in view.
pub(crate) fn offset_for_caret(
    caret_top: f32,
    caret_bottom: f32,
    offset: f32,
    view: f32,
) -> Option<f32> {
    if caret_top < offset {
        Some(caret_top.max(0.0))
    } else if caret_bottom > offset + view {
        Some(caret_bottom - view)
    } else {
        None
    }
}

impl RenderOnce for MessageComposer {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let surface = theme.color_surface_subtle;
        let (offset, max) = scrolled(&self.scroll);
        let top = offset.clamp(0.0, SCROLL_FADE);
        let bottom = (max - offset).clamp(0.0, SCROLL_FADE);
        let has_text = !self.input.read(cx).text().trim().is_empty();
        let can_send = self.can_send.unwrap_or(has_text) && !self.busy;

        // What the last frame saw: the scroll, so the fades draw again once it moves, and the
        // caret, so it is followed only when it moves (the reader may scroll away from it).
        let seen = window.use_keyed_state(
            ElementId::Name(format!("{}-seen", self.id).into()),
            cx,
            |_, _| (0.0_f32, 0.0_f32, usize::MAX),
        );
        let watch = self.scroll.clone();
        let input = self.input.clone();
        let view = window.current_view();
        let watcher = canvas(
            move |_, window, cx| {
                let now = scrolled(&watch);
                let (last_offset, last_max, last_caret) = *seen.read(cx);
                let (caret_now, caret_bounds) = {
                    let field = input.read(cx);
                    let at = field.cursor();
                    (at, field.range_bounds(at..at))
                };
                let mut changed =
                    (now.0 - last_offset).abs() > 0.5 || (now.1 - last_max).abs() > 0.5;
                let viewport = watch.bounds();
                let focused = input.read(cx).focus_handle(cx).is_focused(window);
                if caret_now != last_caret
                    && focused
                    && let Some(bounds) = caret_bounds
                    && viewport.size.height > Pixels::ZERO
                {
                    // The caret was drawn at this offset: in the content it sits that far down.
                    let top = f32::from(bounds.top() - viewport.top()) + now.0;
                    let bottom = f32::from(bounds.bottom() - viewport.top()) + now.0;
                    let height = f32::from(viewport.size.height);
                    if let Some(next) = offset_for_caret(top, bottom, now.0, height) {
                        let next = next.clamp(0.0, now.1);
                        if (next - now.0).abs() > 0.5 {
                            watch.set_offset(gpui::point(px(0.0), px(-next)));
                            changed = true;
                        }
                    }
                }
                if changed || caret_now != last_caret {
                    seen.update(cx, |seen, _| *seen = (now.0, now.1, caret_now));
                }
                if changed {
                    cx.notify(view);
                    window.refresh();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_0();

        let focus = self.input.read(cx).focus_handle(cx);
        let field = div()
            .id(ElementId::Name(format!("{}-field", self.id).into()))
            .w_full()
            .max_h(px(self.max_height))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .child(self.input.clone());
        let fade = |height: f32, at_top: bool| {
            div()
                .absolute()
                .left_0()
                .right_0()
                .h(px(height))
                .map(|edge| {
                    if at_top {
                        edge.top_0()
                    } else {
                        edge.bottom_0()
                    }
                })
                .child(end_fade(surface, height, at_top))
        };
        let field = div()
            .relative()
            .w_full()
            .child(field)
            .when(top > 0.5, |area| area.child(fade(top, true)))
            .when(bottom > 0.5, |area| area.child(fade(bottom, false)))
            .child(watcher);

        let send = Button::new(
            ElementId::Name(format!("{}-send", self.id).into()),
            ButtonKind::Primary,
        )
        .size(ButtonSize::Icon)
        .icon(Icon::ArrowUp)
        .icon_size(16.0)
        .tooltip(self.send_label)
        .disabled(!can_send)
        .when_some(self.on_send.filter(|_| can_send), |button, handler| {
            button.on_click_rc(handler)
        });
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(div().flex_1().min_w(px(0.0)).children(self.footer_start))
            .child(send);

        raised_panel(
            div()
                .id(ElementId::Name(self.id.clone()))
                .debug_selector({
                    let id = self.id.to_string();
                    move || id
                })
                .relative()
                .w_full()
                .bg(surface)
                .cursor_text()
                // A press on the card's padding writes in the field, as a press on a
                // `<textarea>`'s frame does.
                .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                    window.focus(&focus);
                    cx.stop_propagation();
                }),
            MESSAGE_COMPOSER_RADIUS,
            &theme,
        )
        .child(
            div()
                .relative()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(GAP))
                .pt(px(PAD_TOP))
                .pb(px(PAD_BOTTOM))
                .px(px(PAD_X))
                .child(field)
                .child(footer),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{draw, open};
    use gpui::{TestAppContext, VisualTestContext, size};
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn the_caret_is_followed_only_out_of_view() {
        // In view: no move.
        assert_eq!(offset_for_caret(40.0, 60.0, 0.0, 200.0), None);
        // Below the view: it scrolls down until the caret's bottom is on the edge.
        assert_eq!(offset_for_caret(200.0, 220.0, 0.0, 200.0), Some(20.0));
        // Above the view: it scrolls up to the caret's top.
        assert_eq!(offset_for_caret(10.0, 30.0, 50.0, 200.0), Some(10.0));
    }

    type Slot = Rc<RefCell<Option<Entity<TextInput>>>>;

    fn composer(
        cx: &mut TestAppContext,
        sent: Rc<RefCell<Vec<String>>>,
    ) -> (Entity<TextInput>, ScrollHandle, VisualTestContext) {
        let slot: Slot = Rc::default();
        let scroll = ScrollHandle::new();
        let held = slot.clone();
        let handle = scroll.clone();
        let mut cx = open(cx, size(480.0, 400.0), move |_, cx| {
            let sink = sent.clone();
            let input = held
                .borrow_mut()
                .get_or_insert_with(|| {
                    message_input(
                        "Message",
                        move |text, _, _| sink.borrow_mut().push(text.to_string()),
                        cx,
                    )
                })
                .clone();
            div()
                .w(px(480.0))
                .child(message_composer("composer", &input, &handle).max_height(100.0))
                .into_any_element()
        });
        let input = slot.borrow().clone().unwrap();
        cx.update(|window, cx| input.focus_handle(cx).focus(window));
        draw(&mut cx);
        (input, scroll, cx)
    }

    fn height(scroll: &ScrollHandle) -> f32 {
        f32::from(scroll.bounds().size.height)
    }

    #[gpui::test]
    fn the_card_grows_to_its_cap_and_then_scrolls_to_the_caret(cx: &mut TestAppContext) {
        let sent: Rc<RefCell<Vec<String>>> = Rc::default();
        let (_input, scroll, mut cx) = composer(cx, sent.clone());
        cx.simulate_input("one");
        draw(&mut cx);
        let one = height(&scroll);
        assert!((one - LINE_HEIGHT).abs() < 1.0, "one line: {one}");
        cx.simulate_keystrokes("shift-enter");
        cx.simulate_input("two");
        draw(&mut cx);
        let two = height(&scroll);
        assert!((two - 2.0 * LINE_HEIGHT).abs() < 1.0, "two lines: {two}");
        // Past the cap the field stays 100px tall and scrolls down to the caret.
        for _ in 0..8 {
            cx.simulate_keystrokes("shift-enter");
            cx.simulate_input("more");
        }
        for _ in 0..4 {
            draw(&mut cx);
        }
        let capped = height(&scroll);
        assert!((capped - 100.0).abs() < 1.0, "capped: {capped}");
        let (offset, max) = scrolled(&scroll);
        assert!(max > 0.0, "the field overflows");
        assert!(
            (offset - max).abs() < 1.0,
            "the caret's line is in view: {offset} of {max}"
        );
        assert!(sent.borrow().is_empty());
        cx.simulate_keystrokes("enter");
        assert_eq!(sent.borrow().len(), 1);
        assert!(sent.borrow()[0].starts_with("one\ntwo\nmore"));
    }
}
