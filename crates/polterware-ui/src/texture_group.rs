//! `TextureButtonGroup` (`texture-button-group.css`): one texture pill split into icon
//! segments, such as a title bar's Go back and Go forward. The group is the shell
//! (`--tx-outer-*`, #474747), with no frame or drop of its own; each segment is a 28px face
//! filling it edge to edge, 1px apart, so the gap shows the shell's lighter tone. The first
//! segment is round on its leading side and square on the other, the last the mirror: the CSS
//! asks for 3px there, but its 9999px ends overflow the box, so the browser scales every
//! radius down by the same factor and the 3px ones vanish. Where the faces meet the shell's
//! edge the browser lets a hairline of the shell through; the faces sit a quarter pixel in to
//! draw the same rim. A segment
//! swaps its face at once on hover and press, as CSS cannot ease a gradient; a disabled one
//! takes the disabled face (#262626 to #2e2e2e) and the disabled ink. Each has the shared
//! tooltip with its label.

use std::rc::Rc;

use gpui::{App, Corners, ElementId, IntoElement, SharedString, Window, div, prelude::*, px};

use crate::Handler;
use crate::icons::{Icon, icon_colored};
use crate::material::face;
use crate::pointer::{track, use_pointer};
use crate::theme::{Theme, hex};
use crate::tooltip::tooltip;
use crate::window_drag::WindowDragRegion;

/// The face's palette (`.pw-texture-group`, `materials.css`).
const SHELL: u32 = 0x474747;
const FACE: (u32, u32) = (0x303030, 0x2b2b2b);
const FACE_HOVER: (u32, u32) = (0x383838, 0x333333);
const FACE_ACTIVE: (u32, u32) = (0x2b2b2b, 0x2b2b2b);
const FACE_DISABLED: (u32, u32) = (0x262626, 0x2e2e2e);
/// How far in from the shell's edge the faces sit, which leaves its rim showing.
const RIM: f32 = 0.25;

/// One segment: its label (the tooltip and accessible name), its icon, and its click.
pub struct TextureGroupItem {
    pub label: SharedString,
    pub icon: Icon,
    pub disabled: bool,
    pub on_click: Option<Handler>,
}

impl TextureGroupItem {
    pub fn new(label: impl Into<SharedString>, icon: Icon) -> Self {
        Self {
            label: label.into(),
            icon,
            disabled: false,
            on_click: None,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

#[derive(IntoElement)]
pub struct TextureButtonGroup {
    id: ElementId,
    items: Vec<TextureGroupItem>,
    icon_size: f32,
}

impl TextureButtonGroup {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            items: Vec::new(),
            icon_size: 14.0,
        }
    }

    pub fn item(mut self, item: TextureGroupItem) -> Self {
        self.items.push(item);
        self
    }

    /// The marks' size: the CSS draws every segment's svg at 14px.
    pub fn icon_size(mut self, size: f32) -> Self {
        self.icon_size = size;
        self
    }
}

/// Segment `index` of `count`'s corners: the leading one round on its start, the trailing one
/// on its end, square where two segments meet (the CSS's 3px, scaled away).
pub fn segment_corners(index: usize, count: usize, height: f32) -> Corners<f32> {
    let round = height / 2.0 - RIM;
    let first = index == 0;
    let last = index + 1 == count;
    let start = if first { round } else { 0.0 };
    let end = if last { round } else { 0.0 };
    Corners {
        top_left: start,
        bottom_left: start,
        top_right: end,
        bottom_right: end,
    }
}

impl RenderOnce for TextureButtonGroup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let height = theme.control_height_sm;
        let count = self.items.len();
        let mut group = div()
            .id(self.id.clone())
            .flex()
            .flex_none()
            .gap(px(1.0))
            .h(px(height))
            .p(px(RIM))
            .rounded_full()
            .bg(hex(SHELL));
        for (index, item) in self.items.into_iter().enumerate() {
            let segment_id =
                ElementId::NamedInteger(format!("{}-segment", self.id).into(), index as u64);
            let pointer = use_pointer(segment_id.clone(), window, cx);
            let (hovered, press) = {
                let state = pointer.read(cx);
                (state.hovered(), state.press())
            };
            let enabled = !item.disabled;
            let ((top, bottom), ink) = if !enabled {
                (FACE_DISABLED, theme.color_text_disabled)
            } else if press > 0.5 {
                (FACE_ACTIVE, theme.color_text_primary)
            } else if hovered {
                (FACE_HOVER, theme.color_text_primary)
            } else {
                (FACE, theme.color_text_primary)
            };
            let corners = segment_corners(index, count, height);
            let label = item.label.clone();
            let mut segment = div()
                .id(segment_id.clone())
                .debug_selector({
                    let id = segment_id.clone();
                    move || id.to_string()
                })
                .window_drag_control()
                .w(px(height - RIM))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .rounded_tl(px(corners.top_left))
                .rounded_bl(px(corners.bottom_left))
                .rounded_tr(px(corners.top_right))
                .rounded_br(px(corners.bottom_right))
                .bg(face(hex(top), hex(bottom)))
                .child(icon_colored(item.icon, self.icon_size, ink));
            if enabled {
                segment = segment.cursor_pointer().tooltip(tooltip(label));
            } else {
                segment = segment.cursor_not_allowed();
            }
            let segment = track(segment, &pointer, enabled);
            let segment = match item.on_click.filter(|_| enabled) {
                Some(handler) => segment.on_click(move |_, window, cx| handler(window, cx)),
                None => segment,
            };
            group = group.child(segment);
        }
        group
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::{TestAppContext, div, prelude::*, size};

    use super::{TextureButtonGroup, TextureGroupItem, segment_corners};
    use crate::icons::Icon;
    use crate::test_support::{draw, open};

    #[test]
    fn the_ends_are_round_and_the_seam_is_square() {
        let first = segment_corners(0, 2, 28.0);
        assert_eq!((first.top_left, first.top_right), (13.75, 0.0));
        let last = segment_corners(1, 2, 28.0);
        assert_eq!((last.top_left, last.bottom_right), (0.0, 13.75));
        let only = segment_corners(0, 1, 28.0);
        assert_eq!((only.top_left, only.top_right), (13.75, 13.75));
    }

    #[gpui::test]
    fn a_disabled_segment_takes_no_click(cx: &mut TestAppContext) {
        let back = Rc::new(Cell::new(0));
        let forward = Rc::new(Cell::new(0));
        let (b, f) = (back.clone(), forward.clone());
        let mut cx = open(cx, size(200.0, 60.0), move |_, _| {
            let (b, f) = (b.clone(), f.clone());
            div()
                .child(
                    TextureButtonGroup::new("history")
                        .item(
                            TextureGroupItem::new("Go back", Icon::ChevronLeft)
                                .on_click(move |_, _| b.set(b.get() + 1)),
                        )
                        .item(
                            TextureGroupItem::new("Go forward", Icon::ChevronRight)
                                .disabled(true)
                                .on_click(move |_, _| f.set(f.get() + 1)),
                        ),
                )
                .into_any_element()
        });
        draw(&mut cx);
        cx.simulate_click(
            gpui::point(gpui::px(14.0), gpui::px(14.0)),
            gpui::Modifiers::none(),
        );
        cx.simulate_click(
            gpui::point(gpui::px(43.0), gpui::px(14.0)),
            gpui::Modifiers::none(),
        );
        assert_eq!(back.get(), 1);
        assert_eq!(forward.get(), 0);
    }
}
