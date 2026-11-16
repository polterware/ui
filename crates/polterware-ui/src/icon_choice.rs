//! `IconChoice`: a horizontal row of selectable ~64px square image tiles with labels under
//! them, built for choosing among variants such as an app's icon.
//!
//! Visually it follows the library's choice and tile controls (`radio_group.rs`,
//! `segmented.rs`, `OptionTile` in `icon_tile.rs`, and `Theme` tokens): unselected tiles sit in
//! the recessed field fill (`field_fill_top` to `field_fill_bottom`) with the quiet inset well
//! edge (`Edge::Well`) and brighten their border toward `color_border_strong` on hover; the
//! selected tile wears the neutral control face (`face_neutral`) with `control_drop` and a 2px
//! `color_accent` ring on `radius_card`. Labels sit centered under each tile in `font_size_sm`
//! secondary ink, brightening to `color_text_primary` on hover and selection. Hover and press
//! ease through `PointerState` (`PointerTiming::control`). GPUI `img` receives `radius_item`
//! inside a `space_1`-padded `radius_card` tile so square corners stay inside the ring; each
//! option's `Arc<gpui::Image>` is built once and cached in keyed state across frames.
//!
//! Behaviorally it follows gpui-kit's horizontal `RadioGroup` in single-select controlled mode:
//! the caller owns the selected key and hears changes through [`IconChoice::on_change`];
//! clicking the already-selected option or interacting while disabled emits nothing; when
//! focused, Left/Right/Up/Down (wrapping) and Home/End move the selection.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    App, Context, ElementId, FocusHandle, FontWeight, Image, ImageFormat, IntoElement,
    KeyDownEvent, ObjectFit, Rgba, SharedString, Window, div, img, prelude::*, px,
};

use crate::ValueHandler;
use crate::material::{self, Edge};
use crate::pointer::{track, use_pointer};
use crate::theme::{Theme, fade, mix};
use crate::window_drag::WindowDragRegion;

/// Image side inside each tile; with `theme.space_1` padding on all sides, the tile is 72px.
const IMAGE_SIZE: f32 = 64.0;
const RING_WIDTH: f32 = 2.0;

struct IconChoiceOption {
    key: SharedString,
    label: SharedString,
    png: &'static [u8],
}

/// Keyed state kept across frames: focus handle and one-time decoded `Arc<Image>` cache.
struct IconChoiceState {
    focus: FocusHandle,
    images: HashMap<(usize, usize), Arc<Image>>,
}

impl IconChoiceState {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            images: HashMap::new(),
        }
    }

    /// Returns the cached `Arc<Image>` for `png`, building it only on first sight.
    fn image(&mut self, png: &'static [u8]) -> Arc<Image> {
        let key = (png.as_ptr() as usize, png.len());
        self.images
            .entry(key)
            .or_insert_with(|| Arc::new(Image::from_bytes(ImageFormat::Png, png.to_vec())))
            .clone()
    }
}

/// A horizontal row of selectable square image tiles with labels underneath.
#[derive(IntoElement)]
pub struct IconChoice {
    id: SharedString,
    options: Vec<IconChoiceOption>,
    selected: Option<SharedString>,
    disabled: bool,
    on_change: Option<ValueHandler<SharedString>>,
}

impl IconChoice {
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            options: Vec::new(),
            selected: None,
            disabled: false,
            on_change: None,
        }
    }

    /// Appends one selectable option with its key, label, and static PNG bytes.
    pub fn option(
        mut self,
        key: impl Into<SharedString>,
        label: impl Into<SharedString>,
        png: &'static [u8],
    ) -> Self {
        self.options.push(IconChoiceOption {
            key: key.into(),
            label: label.into(),
            png,
        });
        self
    }

    /// Sets the currently selected option key.
    pub fn selected(mut self, key: impl Into<SharedString>) -> Self {
        self.selected = Some(key.into());
        self
    }

    /// Disables pointer and keyboard selection.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Called with the newly chosen option key when the user clicks or navigates with arrows.
    pub fn on_change(
        mut self,
        handler: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

fn next_index(key: &str, current: Option<usize>, len: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    match key {
        "right" | "down" => Some(match current {
            Some(index) => (index + 1) % len,
            None => 0,
        }),
        "left" | "up" => Some(match current {
            Some(index) => (index + len - 1) % len,
            None => len - 1,
        }),
        "home" => Some(0),
        "end" => Some(len - 1),
        _ => None,
    }
}

fn tile_face_colors(
    selected: bool,
    disabled: bool,
    hover: f32,
    press: f32,
    theme: &Theme,
) -> (Rgba, Rgba) {
    if disabled {
        return (theme.face_disabled.top, theme.face_disabled.bottom);
    }
    let (rest_top, rest_bottom, hover_top, hover_bottom) = if selected {
        (
            theme.face_neutral.top,
            theme.face_neutral.bottom,
            theme.face_neutral.hover_top,
            theme.face_neutral.hover_bottom,
        )
    } else {
        (
            theme.field_fill_top,
            theme.field_fill_bottom,
            theme.field_fill_hover_top,
            theme.field_fill_hover_bottom,
        )
    };
    let top = mix(
        mix(rest_top, hover_top, hover),
        theme.face_neutral.active_top,
        press,
    );
    let bottom = mix(
        mix(rest_bottom, hover_bottom, hover),
        theme.face_neutral.active_bottom,
        press,
    );
    (top, bottom)
}

fn render_tile(
    image: Arc<Image>,
    selected: bool,
    disabled: bool,
    hover: f32,
    press: f32,
    theme: &Theme,
) -> gpui::Div {
    let tile_size = IMAGE_SIZE + 2.0 * theme.space_1;
    let radius = theme.radius_card;
    let (top, bottom) = tile_face_colors(selected, disabled, hover, press, theme);
    let inset = px(press);

    let face = div()
        .absolute()
        .top(inset)
        .left(inset)
        .right(inset)
        .bottom(inset)
        .rounded(px(radius))
        .p(px(theme.space_1))
        .flex()
        .items_center()
        .justify_center()
        .bg(material::face(top, bottom))
        .child(
            img(image)
                .size_full()
                .object_fit(ObjectFit::Contain)
                .rounded(px(theme.radius_item))
                .when(disabled, |image| image.opacity(0.5)),
        );

    let mut tile = div()
        .relative()
        .flex_none()
        .size(px(tile_size))
        .rounded(px(radius));
    if selected && !disabled {
        tile = tile.shadow(material::control_drop(theme));
    }
    tile = tile.child(face);

    if selected {
        let ring = if disabled {
            fade(theme.color_accent, 0.5)
        } else {
            theme.color_accent
        };
        tile.child(
            div()
                .absolute()
                .inset_0()
                .rounded(px(radius))
                .border(px(RING_WIDTH))
                .border_color(ring),
        )
    } else {
        tile.child(material::edge(Edge::Well, radius, theme)).when(
            hover > 0.0 && !disabled,
            |tile| {
                tile.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(px(radius))
                        .border_1()
                        .border_color(fade(theme.color_border_strong, hover)),
                )
            },
        )
    }
}

fn state_id(id: &SharedString) -> ElementId {
    ElementId::Name(format!("{id}-state").into())
}

impl RenderOnce for IconChoice {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let state =
            window.use_keyed_state(state_id(&self.id), cx, |_, cx| IconChoiceState::new(cx));
        let focus = state.read(cx).focus.clone();

        let keys_list: Vec<SharedString> = self
            .options
            .iter()
            .map(|option| option.key.clone())
            .collect();
        let selected_key = self.selected.clone();
        let on_change = self.on_change.clone();
        let handle_keys = move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
            if keys_list.is_empty() || event.keystroke.modifiers.modified() {
                return;
            }
            let current = selected_key
                .as_ref()
                .and_then(|key| keys_list.iter().position(|candidate| candidate == key));
            let Some(next) = next_index(event.keystroke.key.as_str(), current, keys_list.len())
            else {
                return;
            };
            cx.stop_propagation();
            let target = keys_list[next].clone();
            if selected_key.as_ref() != Some(&target)
                && let Some(on_change) = &on_change
            {
                on_change(target, window, cx);
            }
        };

        let tile_size = IMAGE_SIZE + 2.0 * theme.space_1;
        let mut row = div()
            .id(ElementId::Name(self.id.clone()))
            .debug_selector({
                let id = self.id.clone();
                move || id.to_string()
            })
            .track_focus(&focus.clone().tab_stop(!self.disabled))
            .flex()
            .flex_wrap()
            .items_start()
            .gap(px(theme.space_3))
            .when(!self.disabled, |row| row.on_key_down(handle_keys));

        for (index, option) in self.options.into_iter().enumerate() {
            let option_id = ElementId::NamedInteger(self.id.clone(), index as u64);
            let pointer = use_pointer(option_id.clone(), window, cx);
            let (hover, hover_color, press) = if self.disabled {
                (0.0, 0.0, 0.0)
            } else {
                let state = pointer.read(cx);
                (state.hover(), state.hover_color(), state.press())
            };
            let selected = self.selected.as_ref() == Some(&option.key);
            let image = state.update(cx, |state, _| state.image(option.png));
            let tile = render_tile(image, selected, self.disabled, hover, press, &theme);

            let label_color = if self.disabled {
                theme.color_text_disabled
            } else if selected {
                theme.color_text_primary
            } else {
                mix(
                    theme.color_text_secondary,
                    theme.color_text_primary,
                    hover_color,
                )
            };
            let label = div()
                .text_size(px(theme.font_size_sm))
                .line_height(px(theme.font_size_sm * theme.line_height))
                .font_weight(if selected {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                })
                .text_color(label_color)
                .whitespace_nowrap()
                .child(option.label);

            let selector = format!("{}-{}", self.id, option.key);
            let mut column = div()
                .id(option_id)
                .debug_selector(move || selector.clone())
                .window_drag_control()
                .min_w(px(tile_size))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(theme.space_1))
                .child(tile)
                .child(label);
            if self.disabled {
                column = column.cursor_not_allowed();
            } else {
                let focus = focus.clone();
                let key = option.key;
                let on_change = self.on_change.clone();
                column = column.cursor_pointer().on_click(move |_, window, cx| {
                    window.focus(&focus);
                    if !selected && let Some(on_change) = &on_change {
                        on_change(key.clone(), window, cx);
                    }
                });
            }
            row = row.child(track(column, &pointer, !self.disabled));
        }

        row
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    use gpui::{Modifiers, SharedString, TestAppContext, prelude::*, size};

    use super::{IconChoice, IconChoiceState, next_index};
    use crate::test_support::{draw, open};

    const PNG_DEFAULT: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 48, 48, 48, 248, 15,
        0, 2, 180, 1, 144, 230, 97, 53, 9, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    const PNG_ICE: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 168, 155, 127, 227,
        63, 0, 6, 137, 2, 245, 118, 214, 217, 125, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    const PNG_FIRE: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 120, 146, 150, 246,
        31, 0, 6, 146, 2, 176, 61, 116, 145, 244, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];

    fn sample_choice(selected: SharedString) -> IconChoice {
        IconChoice::new("app-icon")
            .option("default", "Default", PNG_DEFAULT)
            .option("ice", "Ice", PNG_ICE)
            .option("fire", "Fire", PNG_FIRE)
            .selected(selected)
    }

    #[test]
    fn navigation_index_handles_empty_unselected_and_wrapping() {
        assert_eq!(next_index("right", None, 0), None);
        assert_eq!(next_index("right", None, 3), Some(0));
        assert_eq!(next_index("down", None, 3), Some(0));
        assert_eq!(next_index("home", None, 3), Some(0));
        assert_eq!(next_index("left", None, 3), Some(2));
        assert_eq!(next_index("up", None, 3), Some(2));
        assert_eq!(next_index("end", None, 3), Some(2));
        assert_eq!(next_index("right", Some(2), 3), Some(0));
        assert_eq!(next_index("left", Some(0), 3), Some(2));
        assert_eq!(next_index("enter", Some(0), 3), None);
    }

    #[gpui::test]
    fn clicking_another_option_selects_it_and_clicking_selected_reports_nothing(
        cx: &mut TestAppContext,
    ) {
        let current: Rc<RefCell<SharedString>> = Rc::new(RefCell::new("default".into()));
        let events: Rc<RefCell<Vec<SharedString>>> = Rc::default();
        let current_ref = current.clone();
        let events_ref = events.clone();

        let mut cx = open(cx, size(420.0, 180.0), move |_, _| {
            let current = current_ref.clone();
            let events = events_ref.clone();
            let selected = current_ref.borrow().clone();
            sample_choice(selected)
                .on_change(move |key, window, _| {
                    *current.borrow_mut() = key.clone();
                    events.borrow_mut().push(key);
                    window.refresh();
                })
                .into_any_element()
        });
        draw(&mut cx);

        let default = cx.debug_bounds("app-icon-default").expect("Default draws");
        let ice = cx.debug_bounds("app-icon-ice").expect("Ice draws");
        let fire = cx.debug_bounds("app-icon-fire").expect("Fire draws");
        assert!(ice.left() > default.right());
        assert!(fire.left() > ice.right());

        cx.simulate_click(ice.center(), Modifiers::none());
        draw(&mut cx);
        cx.simulate_click(ice.center(), Modifiers::none());
        draw(&mut cx);
        cx.simulate_click(fire.center(), Modifiers::none());
        draw(&mut cx);

        assert_eq!(*events.borrow(), vec!["ice", "fire"]);
        assert_eq!(*current.borrow(), "fire");
    }

    #[gpui::test]
    fn arrow_keys_move_and_wrap_selection_when_focused(cx: &mut TestAppContext) {
        let current: Rc<RefCell<SharedString>> = Rc::new(RefCell::new("default".into()));
        let events: Rc<RefCell<Vec<SharedString>>> = Rc::default();
        let current_ref = current.clone();
        let events_ref = events.clone();

        let mut cx = open(cx, size(420.0, 180.0), move |_, _| {
            let current = current_ref.clone();
            let events = events_ref.clone();
            let selected = current_ref.borrow().clone();
            sample_choice(selected)
                .on_change(move |key, window, _| {
                    *current.borrow_mut() = key.clone();
                    events.borrow_mut().push(key);
                    window.refresh();
                })
                .into_any_element()
        });
        draw(&mut cx);

        // Clicking the already-selected option focuses the group without emitting a change.
        let default = cx.debug_bounds("app-icon-default").expect("Default draws");
        cx.simulate_click(default.center(), Modifiers::none());
        draw(&mut cx);
        assert!(events.borrow().is_empty());

        for (keystroke, expected) in [
            ("right", "ice"),
            ("right", "fire"),
            ("right", "default"),
            ("left", "fire"),
            ("up", "ice"),
            ("down", "fire"),
            ("home", "default"),
            ("end", "fire"),
        ] {
            cx.simulate_keystrokes(keystroke);
            draw(&mut cx);
            assert_eq!(current.borrow().as_ref(), expected);
        }
        assert_eq!(
            *events.borrow(),
            vec![
                "ice", "fire", "default", "fire", "ice", "fire", "default", "fire"
            ]
        );
    }

    #[gpui::test]
    fn disabled_ignores_clicks_and_keys(cx: &mut TestAppContext) {
        let events: Rc<RefCell<Vec<SharedString>>> = Rc::default();
        let events_ref = events.clone();

        let mut cx = open(cx, size(420.0, 180.0), move |_, _| {
            let events = events_ref.clone();
            sample_choice("default".into())
                .disabled(true)
                .on_change(move |key, _, _| events.borrow_mut().push(key))
                .into_any_element()
        });
        draw(&mut cx);

        let ice = cx.debug_bounds("app-icon-ice").expect("Ice draws");
        cx.simulate_click(ice.center(), Modifiers::none());
        cx.simulate_keystrokes("right");
        draw(&mut cx);

        assert!(events.borrow().is_empty());
    }

    #[gpui::test]
    fn images_are_built_once_and_reused_across_frames(cx: &mut TestAppContext) {
        let state = cx.update(|cx| cx.new(IconChoiceState::new));
        let first = state.update(cx, |state, _| {
            [PNG_DEFAULT, PNG_ICE, PNG_FIRE].map(|png| state.image(png))
        });
        let second = state.update(cx, |state, _| {
            [PNG_DEFAULT, PNG_ICE, PNG_FIRE].map(|png| state.image(png))
        });

        assert_eq!(state.read_with(cx, |state, _| state.images.len()), 3);
        for (before, after) in first.iter().zip(second.iter()) {
            assert!(Arc::ptr_eq(before, after));
        }
    }

    #[gpui::test]
    fn empty_icon_choice_renders_without_panicking(cx: &mut TestAppContext) {
        let mut cx = open(cx, size(320.0, 120.0), |_, _| {
            IconChoice::new("empty-icon-choice").into_any_element()
        });
        draw(&mut cx);
        assert!(cx.debug_bounds("empty-icon-choice").is_some());
    }
}
