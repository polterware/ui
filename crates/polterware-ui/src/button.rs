//! The design system's `Button` as Dispatch draws it: under `TextureFaceProvider` every solid
//! variant is a texture face (`materials.css`, `button.css`), primary in the info tone,
//! secondary neutral, danger red; ghost and icon stay flat with a round hover fill.
//!
//! Geometry follows the CSS: the face fills the border box and pulls 1px in while pressed,
//! showing the frame (or, for the neutral tone, the outer gradient) as a ring. Texture faces
//! swap their gradient at once on hover, as CSS cannot ease a gradient; flat fills ease over
//! the fast spring's duration and the text color over `--duration-control`.

use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, FontWeight, IntoElement, Rgba, SharedString, Window, div,
    prelude::*, px,
};

use super::icons::{Icon, icon_colored};
use super::material::{self, Edge};
use super::pointer::{track, use_pointer};
use super::tooltip::tooltip;
use crate::Handler;
use crate::theme::{FaceSet, Theme, fade, hex, mix, rem, tw};
use crate::window_drag::WindowDragRegion;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Danger,
    Ghost,
    Brand,
    Success,
    Warn,
    Info,
    /// `textureTone="brand"`: the white face with dark ink.
    Inverse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonSize {
    /// 28px, 9.75px text (`text-xs`), round-ended.
    Sm,
    /// 32px, 13px text, round-ended.
    Md,
    /// 28×28, round, a 14px icon.
    Icon,
}

/// The palette of one texture tone: the face per state, the frame, and the outer gradient
/// the pressed face uncovers.
struct Tone {
    face: FaceSet,
    frame: Option<Rgba>,
    outer: (Rgba, Rgba),
}

fn tone(kind: ButtonKind, disabled: bool, theme: &Theme) -> Tone {
    if disabled {
        let face = theme.face_disabled;
        return Tone {
            face,
            frame: Some(hex(0x1c1c1c)),
            outer: (face.top, face.bottom),
        };
    }
    let tinted = |face: FaceSet, frame: u32, top: u32, bottom: u32| Tone {
        face,
        frame: Some(hex(frame)),
        outer: (hex(top), hex(bottom)),
    };
    match kind {
        ButtonKind::Secondary | ButtonKind::Ghost => Tone {
            face: theme.face_neutral,
            frame: None,
            outer: (theme.face_neutral.top, theme.face_neutral.bottom),
        },
        ButtonKind::Primary | ButtonKind::Brand | ButtonKind::Info => {
            tinted(theme.face_primary, 0x1c3560, 0x93c5fd, 0x688db6)
        }
        ButtonKind::Danger => tinted(theme.face_danger, 0x5e2323, 0xfca5a5, 0xef4444),
        ButtonKind::Inverse => tinted(
            FaceSet {
                top: hex(0xfafafa),
                bottom: hex(0xe8e8e8),
                hover_top: hex(0xffffff),
                hover_bottom: hex(0xefefef),
                active_top: hex(0xe2e2e2),
                active_bottom: hex(0xdcdcdc),
                ink: hex(0x1b1b1b),
            },
            0x1a1a1a,
            0xffffff,
            0xd4d4d4,
        ),
        // `color-mix(in oklab, …)` resolved in sRGB, like `face_primary`.
        ButtonKind::Success => tinted(
            FaceSet {
                top: theme.tone_success.top,
                bottom: theme.tone_success.bottom,
                hover_top: hex(0x46875b),
                hover_bottom: hex(0x52a46c),
                active_top: theme.tone_success.top,
                active_bottom: hex(0x5ba872),
                ink: theme.tone_success.ink,
            },
            0x14301d,
            0x6ee7a0,
            0x4da672,
        ),
        ButtonKind::Warn => tinted(
            FaceSet {
                top: theme.tone_warn.top,
                bottom: theme.tone_warn.bottom,
                hover_top: hex(0xb1823a),
                hover_bottom: hex(0xc89642),
                active_top: theme.tone_warn.top,
                active_bottom: hex(0xcb9a4c),
                ink: theme.tone_warn.ink,
            },
            0x5e3f0e,
            0xfcd34d,
            0xb59735,
        ),
    }
}

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    kind: ButtonKind,
    size: ButtonSize,
    label: Option<SharedString>,
    icon: Option<Icon>,
    icon_size: f32,
    leading: Option<AnyElement>,
    trailing: Option<AnyElement>,
    tooltip: Option<SharedString>,
    disabled: bool,
    selected: bool,
    inert: bool,
    full_width: bool,
    pill: bool,
    pad_x: Option<f32>,
    on_click: Option<Handler>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, kind: ButtonKind) -> Self {
        Self {
            id: id.into(),
            kind,
            size: ButtonSize::Md,
            label: None,
            icon: None,
            icon_size: 14.0,
            leading: None,
            trailing: None,
            tooltip: None,
            disabled: false,
            selected: false,
            inert: false,
            full_width: false,
            pill: false,
            pad_x: None,
            on_click: None,
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// A leading 14px icon (or the only content of an icon button).
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// The icon's size when it is not the 14px default, such as a toolbar action's 12px mark.
    pub fn icon_size(mut self, size: f32) -> Self {
        self.icon_size = size;
        self
    }

    /// An element before the label in place of an icon, such as a 14px avatar.
    pub fn leading(mut self, element: impl IntoElement) -> Self {
        self.leading = Some(element.into_any_element());
        self
    }

    pub fn trailing(mut self, element: impl IntoElement) -> Self {
        self.trailing = Some(element.into_any_element());
        self
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// The shared tooltip, which is also the accessible label of an icon-only button.
    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// The button's material as a label: its resting face and ink, with no hover, press,
    /// pointer cursor, or click. For a title drawn like the controls beside it.
    pub fn inert(mut self) -> Self {
        self.inert = true;
        self
    }

    /// `data-selected`: the pressed face, kept.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }

    /// `!rounded-full`. Every button is round-ended now, so this only keeps older call sites
    /// reading as they did.
    pub fn pill(mut self) -> Self {
        self.pill = true;
        self
    }

    /// The side padding in place of the size's, for a button whose leading tile or trailing
    /// mark already gives it room, such as a toolbar's title button.
    pub fn pad_x(mut self, pad: f32) -> Self {
        self.pad_x = Some(pad);
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    pub fn on_click_rc(mut self, handler: Handler) -> Self {
        self.on_click = Some(handler);
        self
    }
}

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let pointer = use_pointer(self.id.clone(), window, cx);
        let (hovered, hover, hover_color, press) = if self.inert {
            (false, 0.0, 0.0, 0.0)
        } else {
            let state = pointer.read(cx);
            (
                state.hovered(),
                state.hover(),
                state.hover_color(),
                state.press(),
            )
        };
        let enabled = !self.disabled;
        let flat = self.kind == ButtonKind::Ghost;
        let icon_only =
            self.size == ButtonSize::Icon || (self.label.is_none() && self.icon.is_some());

        // `text-xs` and the rem paddings are on the 13px root: a flat button pads `px-2.5` or
        // `px-3` inside its 1px transparent border, and a texture one `!px-4`. Every button is
        // round-ended (`!rounded-full`), as the toolbars draw theirs, so buttons and fields
        // share one shape across the apps.
        let (height, font_size, line_height, pad_x) = match self.size {
            ButtonSize::Sm if flat => (28.0, tw(3.0), rem(), tw(2.5) + 1.0),
            ButtonSize::Md if flat => (32.0, 13.0, 19.5, tw(3.0) + 1.0),
            ButtonSize::Sm => (28.0, tw(3.0), rem(), tw(4.0)),
            ButtonSize::Md => (32.0, 13.0, 19.5, tw(4.0)),
            ButtonSize::Icon => (28.0, 13.0, 19.5, 0.0),
        };
        let pad_x = self.pad_x.unwrap_or(pad_x);
        let radius = height / 2.0;

        let mut root = div()
            .id(self.id.clone())
            // Tests find a button by its id (a no-op outside gpui's test support).
            .debug_selector({
                let id = self.id.clone();
                move || id.to_string()
            })
            // A double click on a button inside a drag region is the button's.
            .window_drag_control()
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(px(tw(1.5)))
            .h(px(height))
            .rounded(px(radius))
            .text_size(px(font_size))
            .line_height(px(line_height))
            .font_weight(FontWeight::MEDIUM)
            .whitespace_nowrap();
        root = if icon_only {
            root.w(px(height))
        } else {
            root.px(px(pad_x))
        };
        if self.full_width {
            root = root.w_full();
        }

        // The face (texture) or the round fill (flat), pulled in by the press.
        let inset = px(press);
        let ink = if flat {
            let rest = if enabled {
                theme.color_text_secondary
            } else {
                theme.color_text_disabled
            };
            let fill = mix(theme.color_surface_hover, theme.color_surface_active, press);
            root = root.child(
                div()
                    .absolute()
                    .top(inset)
                    .left(inset)
                    .right(inset)
                    .bottom(inset)
                    .rounded(px(radius))
                    .bg(fade(fill, hover.max(press))),
            );
            mix(rest, theme.color_text_primary, hover_color)
        } else {
            let tone = tone(self.kind, self.disabled, &theme);
            let engaged = enabled && (self.selected || press > 0.5);
            let (top, bottom) = if engaged {
                (tone.face.active_top, tone.face.active_bottom)
            } else if enabled && hovered {
                (tone.face.hover_top, tone.face.hover_bottom)
            } else {
                (tone.face.top, tone.face.bottom)
            };
            root = root.shadow(material::control_drop(&theme));
            root = match tone.frame {
                Some(frame) => root.bg(frame),
                None => root.bg(material::face(tone.outer.0, tone.outer.1)),
            };
            let mut face = div()
                .absolute()
                .top(inset)
                .left(inset)
                .right(inset)
                .bottom(inset)
                .rounded(px(radius))
                .bg(material::face(top, bottom));
            if !icon_only {
                face = face.child(material::edge(Edge::Strong, radius, &theme));
            }
            root = root.child(face);
            if icon_only {
                root = root.child(material::rim_fade(height / 2.0, &theme));
            }
            tone.face.ink
        };

        root = root.text_color(ink);
        if let Some(name) = self.icon {
            root = root.child(icon_colored(name, self.icon_size, ink));
        }
        if let Some(leading) = self.leading {
            root = root.child(leading);
        }
        if !icon_only && let Some(label) = self.label {
            root = root.child(div().relative().child(label));
        }
        if let Some(trailing) = self.trailing {
            root = root.child(trailing);
        }
        if let Some(text) = self.tooltip {
            root = root.tooltip(tooltip(text));
        }
        if self.inert {
            return root;
        }
        root = if enabled {
            root.cursor_pointer()
        } else {
            root.cursor_not_allowed()
        };
        let root = track(root, &pointer, enabled);
        match self.on_click.filter(|_| enabled) {
            Some(handler) => root.on_click(move |_, window, cx| handler(window, cx)),
            None => root,
        }
    }
}

/// A labeled button (the call shape the views use).
pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    kind: ButtonKind,
    on_click: Handler,
    _cx: &App,
) -> Button {
    Button::new(id, kind).label(label).on_click_rc(on_click)
}
