//! `SegmentedControl`: a recessed track (the field face with the control drop and the strong
//! edge, as the filter bar's track is drawn) holding one round segment per option. The picked
//! segment wears the neutral texture face of a secondary `Button`; the others are flat, their
//! muted ink brightening on hover.
//!
//! It behaves like gpui-kit's segmented `ButtonGroup` in single-select mode: the caller owns
//! the selection and gets the clicked index back; clicking the picked segment again reports
//! nothing. Segments track hover and press as controls do.

use gpui::{App, ElementId, FontWeight, IntoElement, SharedString, Window, div, prelude::*, px};

use super::material::{self, Edge, face};
use super::pointer::{track, use_pointer};
use crate::ValueHandler;
use crate::theme::{Theme, mix};

/// The track's height; each segment sits 2px in.
const HEIGHT: f32 = 28.0;
const INSET: f32 = 2.0;

#[derive(IntoElement)]
pub struct SegmentedControl {
    id: SharedString,
    options: Vec<SharedString>,
    selected: usize,
    on_select: Option<ValueHandler<usize>>,
}

impl SegmentedControl {
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            options: Vec::new(),
            selected: 0,
            on_select: None,
        }
    }

    pub fn option(mut self, label: impl Into<SharedString>) -> Self {
        self.options.push(label.into());
        self
    }

    pub fn options<L: Into<SharedString>>(mut self, labels: impl IntoIterator<Item = L>) -> Self {
        self.options.extend(labels.into_iter().map(Into::into));
        self
    }

    /// The picked option's index.
    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    /// Called with the index of a segment clicked while it was not the picked one.
    pub fn on_select(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(std::rc::Rc::new(handler));
        self
    }
}

impl RenderOnce for SegmentedControl {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let radius = HEIGHT / 2.0;
        let inner = radius - INSET;
        let mut track_row = div()
            .id(ElementId::Name(self.id.clone()))
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .h(px(HEIGHT))
            .p(px(INSET))
            .gap(px(INSET))
            .rounded(px(radius))
            .bg(face(theme.field_fill_top, theme.field_fill_bottom))
            .shadow(material::control_drop(&theme));
        for (index, label) in self.options.into_iter().enumerate() {
            let id = ElementId::NamedInteger(self.id.clone(), index as u64);
            let pointer = use_pointer(id.clone(), window, cx);
            let picked = index == self.selected;
            let (hovered, hover_color) = {
                let state = pointer.read(cx);
                (state.hovered(), state.hover_color())
            };
            let mut segment = div()
                .id(id)
                .relative()
                .flex()
                .items_center()
                .justify_center()
                .h_full()
                .px(px(10.0))
                .rounded(px(inner))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .font_weight(FontWeight::MEDIUM)
                .whitespace_nowrap();
            segment = if picked {
                let faces = theme.face_neutral;
                let (top, bottom) = if hovered {
                    (faces.hover_top, faces.hover_bottom)
                } else {
                    (faces.top, faces.bottom)
                };
                segment
                    .bg(material::face(top, bottom))
                    .shadow(material::control_drop(&theme))
                    .text_color(faces.ink)
                    .child(label)
                    .child(material::edge(Edge::Strong, inner, &theme))
            } else {
                let ink = mix(
                    theme.color_text_muted,
                    theme.color_text_primary,
                    hover_color,
                );
                let segment = segment.cursor_pointer().text_color(ink).child(label);
                match self.on_select.clone() {
                    Some(handler) => {
                        segment.on_click(move |_, window, cx| handler(index, window, cx))
                    }
                    None => segment,
                }
            };
            track_row = track_row.child(track(segment, &pointer, !picked));
        }
        track_row.child(material::edge(Edge::Strong, radius, &theme))
    }
}
