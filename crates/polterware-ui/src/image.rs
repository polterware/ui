//! Images: a thumbnail and the viewer a click opens, as a page shows a picture it did not
//! draw itself.
//!
//! [`ImageThumb`] is a square thumbnail on a raised face with a quiet edge, the picture
//! cropped to fill it (`object-fit: cover`), an optional caption under it in 11px muted ink,
//! and a click that asks the host to open it. [`ImageCard`] is a full gallery tile: the picture
//! thumbnail sitting 1px inside the card's border and clipped to its inner radius, over a
//! calm footer with generous padding holding a title and meta line. [`ImageViewer`] is the modal
//! that shows it whole: a floating card over a [`modal_layer`](super::modal_layer), the file's
//! name as its title, the picture fitted inside 86% of the window's width and 72% of its height
//! at the size the host says it has, a caption under it, and the host's actions (Reveal, Copy)
//! at the end of the footer, with Close at the header's end.
//!
//! The web has no Polterware counterpart for these; the card and viewer behave like gpui-kit's
//! counterparts: the app owns whether they are displayed, what clicks do, and what the actions
//! do, and the components only draw and report clicks. GPUI clips to rectangles, so a picture
//! in a rounded frame keeps its own square corners; the frame insets it 1px, which keeps
//! them inside the edge at the sizes these controls draw.
//!
//! The picture comes in as an [`ImageData`]: a file on disk (read through the asset source
//! when it first draws), bytes already in memory (as a copy from the clipboard holds them),
//! or a frame the host decoded. Everything else about it (its name, its size in pixels, what
//! its buttons do) is the host's.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, App, Bounds, Context, CursorStyle, ElementId, Entity, Image, ImageFormat,
    ImageSource, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit,
    Pixels, Point, Render, RenderImage, RenderOnce, ScrollDelta, ScrollWheelEvent, SharedString,
    Window, canvas, div, img, point, prelude::*, px,
};

use crate::Handler;
use crate::button::{Button, ButtonKind, ButtonSize};
use crate::canvas::{canvas_dock, dock_group, dock_separator, zoom_level};
use crate::ellipsis::ellipsis;
use crate::icons::Icon;
use crate::material::floating_panel;
use crate::theme::Theme;
use crate::tooltip::tooltip;

use crate::patterns::shell::modal_layer;

/// The share of the window the viewer's picture may take, leaving room for its frame.
const MAX_WIDTH_SHARE: f32 = 0.86;
const MAX_HEIGHT_SHARE: f32 = 0.72;
/// A thumbnail's default side.
pub const THUMB_SIZE: f32 = 96.0;

/// What an image control draws.
#[derive(Clone, Debug)]
pub enum ImageData {
    /// A file on disk, read through the asset source when it first draws.
    Path(PathBuf),
    /// Bytes already in memory, and the format they are in.
    Bytes {
        format: ImageFormat,
        bytes: Arc<Vec<u8>>,
    },
    /// A frame the host decoded itself.
    Render(Arc<RenderImage>),
}

impl ImageData {
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        ImageData::Path(path.into())
    }

    pub fn from_bytes(format: ImageFormat, bytes: impl Into<Vec<u8>>) -> Self {
        ImageData::Bytes {
            format,
            bytes: Arc::new(bytes.into()),
        }
    }

    pub fn from_render(image: Arc<RenderImage>) -> Self {
        ImageData::Render(image)
    }

    fn source(&self) -> ImageSource {
        match self {
            ImageData::Path(path) => ImageSource::from(path.clone()),
            ImageData::Bytes { format, bytes } => {
                ImageSource::from(Arc::new(Image::from_bytes(*format, (**bytes).clone())))
            }
            ImageData::Render(image) => ImageSource::from(image.clone()),
        }
    }
}

/// The size a picture of `natural` takes inside `max`, keeping its shape and never growing
/// past its own pixels.
pub fn fit_size(natural: (f32, f32), max: (f32, f32)) -> (f32, f32) {
    let (width, height) = natural;
    if width <= 0.0 || height <= 0.0 {
        return (max.0.max(0.0), max.1.max(0.0));
    }
    let scale = (max.0 / width).min(max.1 / height).min(1.0);
    ((width * scale).max(1.0), (height * scale).max(1.0))
}

/// A square thumbnail of a picture, with an optional caption under it.
#[derive(IntoElement)]
pub struct ImageThumb {
    id: ElementId,
    image: ImageData,
    size: f32,
    caption: Option<SharedString>,
    on_click: Option<Handler>,
}

impl ImageThumb {
    pub fn new(id: impl Into<ElementId>, image: ImageData) -> Self {
        Self {
            id: id.into(),
            image,
            size: THUMB_SIZE,
            caption: None,
            on_click: None,
        }
    }

    /// The thumbnail's side in pixels; the picture fills it, cropped.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// A line under the picture, cut with `…` to the thumbnail's width.
    pub fn caption(mut self, caption: impl Into<SharedString>) -> Self {
        self.caption = Some(caption.into());
        self
    }

    /// A click on the thumbnail; its caption doubles as the tooltip.
    pub fn on_click(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for ImageThumb {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let size = self.size.max(1.0);
        let mut frame = div()
            .id(self.id)
            .flex_none()
            .w(px(size))
            .h(px(size))
            .p(px(1.0))
            .rounded(px(theme.radius_md))
            .bg(theme.color_surface_raised)
            .border_1()
            .border_color(theme.color_border_subtle)
            .overflow_hidden()
            .child(
                img(self.image.source())
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            );
        if let Some(caption) = self.caption.clone() {
            frame = frame.tooltip(tooltip(caption));
        }
        if let Some(on_click) = self.on_click {
            frame = frame
                .cursor_pointer()
                .hover(|style| style.border_color(theme.color_border_strong))
                .on_click(move |_, window, cx| on_click(window, cx));
        }
        let caption = self.caption.map(|caption| {
            div()
                .w(px(size))
                .mt(px(4.0))
                .text_size(px(11.0))
                .text_color(theme.color_text_muted)
                .child(ellipsis(caption))
        });
        div()
            .flex_none()
            .flex()
            .flex_col()
            .child(frame)
            .children(caption)
    }
}

/// An image card: a picture thumbnail filling the top of a card inside its border,
/// clipped to the card's inner radius (`radius_card - 1.0`), over a calm footer holding
/// a title line and optional meta line.
///
/// The look is the design system's `Card` (`.card-panel`, [`card`](crate::card)) with a picture
/// thumbnail cropped to fill it (`object-fit: cover`) and generous padding (`12px-14px`) in its
/// footer. The thumbnail sits 1px inside the card's top and side borders, clipped to
/// `radius_card - 1.0`, ensuring the card's 1px border reads all the way round without cut
/// corners. It behaves like gpui-kit's `Card`: the host supplies the image, title, meta slot,
/// and click handler.
#[derive(IntoElement)]
pub struct ImageCard {
    id: ElementId,
    image: ImageData,
    thumb_height: f32,
    title: SharedString,
    meta: Option<AnyElement>,
    on_click: Option<Handler>,
    hover_progress: f32,
}

impl ImageCard {
    pub fn new(id: impl Into<ElementId>, image: ImageData, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            image,
            thumb_height: 150.0,
            title: title.into(),
            meta: None,
            on_click: None,
            hover_progress: 0.0,
        }
    }

    /// The thumbnail's height in pixels; the picture fills it cropped to cover.
    pub fn thumb_height(mut self, height: f32) -> Self {
        self.thumb_height = height.max(1.0);
        self
    }

    /// The footer's secondary line: small bot avatar, model name, relative date, etc.
    pub fn meta(mut self, meta: impl IntoElement) -> Self {
        self.meta = Some(meta.into_any_element());
        self
    }

    /// A click on the card.
    pub fn on_click(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    /// Progress of the card's hover animation (0.0 at rest, 1.0 fully hovered), driving a subtle
    /// lift and border tint.
    pub fn hover_progress(mut self, progress: f32) -> Self {
        self.hover_progress = progress;
        self
    }
}

impl RenderOnce for ImageCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let inner_radius = (theme.radius_card - 1.0).max(0.0);

        let border_color = if self.hover_progress > 0.001 {
            blend_color(
                theme.color_border_subtle,
                theme.color_border_strong,
                self.hover_progress,
            )
        } else {
            theme.color_border_subtle
        };

        let mut card = div()
            .id(self.id.clone())
            .relative()
            .flex()
            .flex_col()
            .rounded(px(theme.radius_card))
            .border_1()
            .border_color(border_color)
            .bg(theme.color_surface_subtle);

        if let Some(on_click) = self.on_click {
            card = card
                .cursor_pointer()
                .hover(|style| style.border_color(theme.color_border_strong))
                .on_click(move |_, window, cx| on_click(window, cx));
        }

        let thumb = div()
            .w_full()
            .h(px(self.thumb_height))
            .flex_none()
            .bg(theme.color_surface)
            .rounded_tl(px(inner_radius))
            .rounded_tr(px(inner_radius))
            .overflow_hidden()
            .child(
                img(self.image.source())
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            );

        let title_line = div()
            .text_size(px(theme.font_size_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(theme.color_text_primary)
            .child(ellipsis(self.title));

        let footer = div()
            .flex()
            .flex_none()
            .flex_col()
            .gap(px(4.0))
            .px(px(13.0))
            .py(px(12.0))
            .child(title_line)
            .children(self.meta);

        card.child(thumb).child(footer)
    }
}

fn blend_color(a: gpui::Rgba, b: gpui::Rgba, t: f32) -> gpui::Rgba {
    let t = t.clamp(0.0, 1.0);
    gpui::Rgba {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

/// One button in a viewer's footer, such as Reveal in Finder or Copy.
#[derive(Clone)]
pub struct ImageAction {
    pub id: SharedString,
    pub icon: Icon,
    pub label: SharedString,
    pub handler: Handler,
    /// The button's kind: Secondary unless set, Danger for a destructive one such as Delete.
    pub kind: ButtonKind,
}

impl ImageAction {
    pub fn new(
        id: impl Into<SharedString>,
        icon: Icon,
        label: impl Into<SharedString>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            icon,
            label: label.into(),
            handler: Rc::new(handler),
            kind: ButtonKind::Secondary,
        }
    }

    /// Draws the button as `kind`, such as Danger or Primary.
    pub fn kind(mut self, kind: ButtonKind) -> Self {
        self.kind = kind;
        self
    }
}

/// The modal that shows one picture whole, with its name, its caption, and the host's
/// actions.
#[derive(IntoElement)]
pub struct ImageViewer {
    id: SharedString,
    image: ImageData,
    title: SharedString,
    caption: Option<SharedString>,
    details: Option<AnyElement>,
    natural: (f32, f32),
    actions: Vec<ImageAction>,
    presence: f32,
    interactive: bool,
    on_close: Option<Handler>,
}

impl ImageViewer {
    pub fn new(
        id: impl Into<SharedString>,
        title: impl Into<SharedString>,
        image: ImageData,
    ) -> Self {
        Self {
            id: id.into(),
            image,
            title: title.into(),
            caption: None,
            details: None,
            natural: (0.0, 0.0),
            actions: Vec::new(),
            presence: 1.0,
            interactive: true,
            on_close: None,
        }
    }

    /// The picture's own size in pixels, which the viewer fits to the window.
    pub fn natural(mut self, width: f32, height: f32) -> Self {
        self.natural = (width, height);
        self
    }

    /// A line under the picture, such as its format, size, and pixels.
    pub fn caption(mut self, caption: impl Into<SharedString>) -> Self {
        self.caption = Some(caption.into());
        self
    }

    /// The host's own content between the picture and the footer, such as the prompt that
    /// made it and where it came from.
    pub fn details(mut self, details: impl IntoElement) -> Self {
        self.details = Some(details.into_any_element());
        self
    }

    /// A button at the end of the footer.
    pub fn action(mut self, action: ImageAction) -> Self {
        self.actions.push(action);
        self
    }

    /// How far in the modal is drawn, from 0 (leaving) to 1 (open).
    pub fn presence(mut self, presence: f32) -> Self {
        self.presence = presence;
        self
    }

    /// Whether the scrim takes the pointer and closes it.
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    pub fn on_close(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for ImageViewer {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let viewport = window.viewport_size();
        let max = (
            f32::from(viewport.width) * MAX_WIDTH_SHARE,
            f32::from(viewport.height) * MAX_HEIGHT_SHARE,
        );
        let (width, height) = fit_size(self.natural, max);
        let title = div()
            .flex_1()
            .min_w(px(0.0))
            .text_size(px(13.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(theme.color_text_primary)
            .child(ellipsis(self.title));
        let mut header = div().flex().items_center().gap(px(8.0)).child(title);
        if let Some(on_close) = self.on_close.clone() {
            header = header.child(
                Button::new(
                    SharedString::from(format!("{}-close", self.id)),
                    ButtonKind::Ghost,
                )
                .size(ButtonSize::Icon)
                .icon(Icon::XIcon)
                .tooltip("Close")
                .on_click_rc(on_close),
            );
        }
        let picture = div()
            .w(px(width))
            .h(px(height))
            .p(px(1.0))
            .flex_none()
            .rounded(px(theme.radius_md))
            .bg(theme.color_surface_subtle)
            .overflow_hidden()
            .child(
                img(self.image.source())
                    .size_full()
                    .object_fit(ObjectFit::Contain),
            );
        let caption = self.caption.map(|caption| {
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(11.0))
                .text_color(theme.color_text_muted)
                .child(ellipsis(caption))
        });
        let actions: Vec<AnyElement> = self
            .actions
            .into_iter()
            .map(|action| {
                Button::new(action.id, action.kind)
                    .size(ButtonSize::Sm)
                    .icon(action.icon)
                    .label(action.label)
                    .on_click_rc(action.handler)
                    .into_any_element()
            })
            .collect();
        let footer = (!actions.is_empty() || caption.is_some()).then(|| {
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .children(caption)
                .child(div().flex().flex_none().gap(px(6.0)).children(actions))
        });
        let card = floating_panel(
            div()
                .relative()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .p(px(12.0))
                .bg(theme.color_surface_raised),
            theme.radius_dialog,
            &theme,
        )
        .child(header)
        .child(picture)
        .children(self.details)
        .children(footer);
        fn ignore(_: &mut Window, _: &mut App) {}
        let dismiss: Handler = self.on_close.unwrap_or_else(|| Rc::new(ignore));
        modal_layer(self.presence, self.interactive, dismiss, card, &theme)
    }
}

/// The smallest zoom an image can be scaled to (10%).
pub const MIN_ZOOM: f32 = 0.10;
/// The largest zoom an image can be scaled to (800%).
pub const MAX_ZOOM: f32 = 8.0;

/// Clamps zoom between 10% and 800%, falling back to 1.0 for NaN values.
pub fn clamp_zoom(zoom: f32) -> f32 {
    if zoom.is_nan() {
        1.0
    } else {
        zoom.clamp(MIN_ZOOM, MAX_ZOOM)
    }
}

/// Fits a picture of `(natural_w, natural_h)` inside a `(viewport_w, viewport_h)` container,
/// leaving `padding` on each side, capped at 1.0 (never upscaled by default).
pub fn calculate_fit_zoom(
    natural_w: f32,
    natural_h: f32,
    viewport_w: f32,
    viewport_h: f32,
    padding: f32,
) -> f32 {
    if natural_w <= 0.0 || natural_h <= 0.0 || viewport_w <= 0.0 || viewport_h <= 0.0 {
        return 1.0;
    }
    let room_w = (viewport_w - padding * 2.0).max(1.0);
    let room_h = (viewport_h - padding * 2.0).max(1.0);
    clamp_zoom((room_w / natural_w).min(room_h / natural_h).min(1.0))
}

/// Calculates the `(x, y)` top-left position to center an image of `(natural_w, natural_h)`
/// at `zoom` within a `(viewport_w, viewport_h)` container.
pub fn calculate_fit_position(
    natural_w: f32,
    natural_h: f32,
    viewport_w: f32,
    viewport_h: f32,
    zoom: f32,
) -> (f32, f32) {
    let x = (viewport_w - natural_w * zoom) / 2.0;
    let y = (viewport_h - natural_h * zoom) / 2.0;
    (x, y)
}

/// Where an image sits in its view: element-local pixels for top-left position and zoom scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageViewport {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

impl Default for ImageViewport {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        }
    }
}

impl ImageViewport {
    pub fn new(x: f32, y: f32, zoom: f32) -> Self {
        Self {
            x,
            y,
            zoom: clamp_zoom(zoom),
        }
    }

    /// Zooms keeping the point on the image under `focus` (in local pixels) fixed on screen.
    pub fn zoom_at(self, focus: Point<Pixels>, new_zoom: f32) -> Self {
        let new_zoom = clamp_zoom(new_zoom);
        let fx = f32::from(focus.x);
        let fy = f32::from(focus.y);
        let img_x = (fx - self.x) / self.zoom;
        let img_y = (fy - self.y) / self.zoom;
        Self {
            zoom: new_zoom,
            x: fx - img_x * new_zoom,
            y: fy - img_y * new_zoom,
        }
    }

    /// Shifts the image by `(dx, dy)` local pixels.
    pub fn pan_by(self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            ..self
        }
    }
}

/// An interactive viewer with pan, zoom around the pointer, double-click fit toggle,
/// and a floating dock for zoom controls.
pub struct ZoomableImage {
    id: SharedString,
    image: ImageData,
    natural: Option<(f32, f32)>,
    fit_padding: f32,
    viewport: Option<ImageViewport>,
    bounds: Option<Bounds<Pixels>>,
    fit_pending: bool,
    press: Option<ImagePress>,
    dragging: bool,
    debug_selector: Option<SharedString>,
}

#[derive(Clone, Copy, Debug)]
struct ImagePress {
    origin: Point<Pixels>,
    viewport: ImageViewport,
}

impl ZoomableImage {
    pub fn new(id: impl Into<SharedString>, image: ImageData) -> Self {
        Self {
            id: id.into(),
            image,
            natural: None,
            fit_padding: 24.0,
            viewport: None,
            bounds: None,
            fit_pending: true,
            press: None,
            dragging: false,
            debug_selector: None,
        }
    }

    /// The picture's natural resolution in pixels.
    pub fn natural(mut self, width: f32, height: f32) -> Self {
        if width > 0.0 && height > 0.0 {
            self.natural = Some((width, height));
        }
        self
    }

    pub fn fit_padding(mut self, padding: f32) -> Self {
        self.fit_padding = padding.max(0.0);
        self
    }

    pub fn debug_selector(mut self, selector: impl Into<SharedString>) -> Self {
        self.debug_selector = Some(selector.into());
        self
    }

    pub fn viewport(&self) -> Option<ImageViewport> {
        self.viewport
    }

    fn resolved_natural(&self) -> (f32, f32) {
        if let Some((w, h)) = self.natural
            && w > 0.0
            && h > 0.0
        {
            return (w, h);
        }
        if let Some(bounds) = self.bounds {
            let bw = f32::from(bounds.size.width);
            let bh = f32::from(bounds.size.height);
            if bw > 0.0 && bh > 0.0 {
                return (bw, bh);
            }
        }
        (512.0, 512.0)
    }

    fn calculate_fit_zoom(&self) -> f32 {
        let (nat_w, nat_h) = self.resolved_natural();
        let (vw, vh) = match self.bounds {
            Some(b) => (f32::from(b.size.width), f32::from(b.size.height)),
            None => (nat_w, nat_h),
        };
        calculate_fit_zoom(nat_w, nat_h, vw, vh, self.fit_padding)
    }

    pub fn fit(&mut self, cx: &mut Context<Self>) {
        let (nat_w, nat_h) = self.resolved_natural();
        let Some(bounds) = self.bounds else {
            self.fit_pending = true;
            cx.notify();
            return;
        };
        let vw = f32::from(bounds.size.width);
        let vh = f32::from(bounds.size.height);
        let zoom = calculate_fit_zoom(nat_w, nat_h, vw, vh, self.fit_padding);
        let (x, y) = calculate_fit_position(nat_w, nat_h, vw, vh, zoom);
        self.viewport = Some(ImageViewport { x, y, zoom });
        self.fit_pending = false;
        cx.notify();
    }

    pub fn zoom_to_100(&mut self, cx: &mut Context<Self>) {
        let (nat_w, nat_h) = self.resolved_natural();
        let (vw, vh) = match self.bounds {
            Some(b) => (f32::from(b.size.width), f32::from(b.size.height)),
            None => (nat_w, nat_h),
        };
        let center = point(px(vw / 2.0), px(vh / 2.0));
        self.zoom_at(center, 1.0, cx);
    }

    pub fn zoom_at(&mut self, focus: Point<Pixels>, zoom: f32, cx: &mut Context<Self>) {
        let vp = self.viewport.unwrap_or_default();
        self.viewport = Some(vp.zoom_at(focus, zoom));
        cx.notify();
    }

    pub fn zoom_in(&mut self, cx: &mut Context<Self>) {
        let vp = self.viewport.unwrap_or_default();
        let (vw, vh) = match self.bounds {
            Some(b) => (f32::from(b.size.width), f32::from(b.size.height)),
            None => (500.0, 500.0),
        };
        let center = point(px(vw / 2.0), px(vh / 2.0));
        self.zoom_at(center, vp.zoom * 1.25, cx);
    }

    pub fn zoom_out(&mut self, cx: &mut Context<Self>) {
        let vp = self.viewport.unwrap_or_default();
        let (vw, vh) = match self.bounds {
            Some(b) => (f32::from(b.size.width), f32::from(b.size.height)),
            None => (500.0, 500.0),
        };
        let center = point(px(vw / 2.0), px(vh / 2.0));
        self.zoom_at(center, vp.zoom / 1.25, cx);
    }

    pub fn toggle_fit_100(&mut self, local_focus: Option<Point<Pixels>>, cx: &mut Context<Self>) {
        let fit_zoom = self.calculate_fit_zoom();
        let current_zoom = self.viewport.map_or(fit_zoom, |v| v.zoom);
        if (current_zoom - fit_zoom).abs() < 0.02 {
            let target = if (fit_zoom - 1.0).abs() < 0.02 {
                2.0
            } else {
                1.0
            };
            let (vw, vh) = match self.bounds {
                Some(b) => (f32::from(b.size.width), f32::from(b.size.height)),
                None => (500.0, 500.0),
            };
            let focus = local_focus.unwrap_or_else(|| point(px(vw / 2.0), px(vh / 2.0)));
            self.zoom_at(focus, target, cx);
        } else {
            self.fit(cx);
        }
    }

    fn ensure_viewport(&mut self, bounds: Bounds<Pixels>) {
        if self.viewport.is_none() || self.fit_pending {
            let (nat_w, nat_h) = self.resolved_natural();
            let vw = f32::from(bounds.size.width);
            let vh = f32::from(bounds.size.height);
            let zoom = calculate_fit_zoom(nat_w, nat_h, vw, vh, self.fit_padding);
            let (x, y) = calculate_fit_position(nat_w, nat_h, vw, vh, zoom);
            self.viewport = Some(ImageViewport { x, y, zoom });
            self.fit_pending = false;
        }
    }

    fn local_point(&self, window_point: Point<Pixels>) -> Option<Point<Pixels>> {
        let bounds = self.bounds?;
        Some(point(
            window_point.x - bounds.origin.x,
            window_point.y - bounds.origin.y,
        ))
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(local) = self.local_point(event.position) else {
            return;
        };
        if event.click_count == 2 {
            self.toggle_fit_100(Some(local), cx);
            return;
        }
        let vp = self.viewport.unwrap_or_default();
        self.press = Some(ImagePress {
            origin: local,
            viewport: vp,
        });
        self.dragging = false;
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(press) = self.press else {
            return;
        };
        let Some(local) = self.local_point(event.position) else {
            return;
        };
        let dx = f32::from(local.x - press.origin.x);
        let dy = f32::from(local.y - press.origin.y);
        if !self.dragging && (dx.hypot(dy) > 2.0) {
            self.dragging = true;
        }
        if self.dragging {
            self.viewport = Some(press.viewport.pan_by(dx, dy));
            cx.notify();
        }
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, cx: &mut Context<Self>) {
        self.press = None;
        if self.dragging {
            self.dragging = false;
            cx.notify();
        }
    }

    fn scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(local) = self.local_point(event.position) else {
            return;
        };
        let Some(vp) = self.viewport else {
            return;
        };
        let (dx, dy) = match event.delta {
            ScrollDelta::Pixels(delta) => (f32::from(delta.x), f32::from(delta.y)),
            ScrollDelta::Lines(delta) => (delta.x * 24.0, delta.y * 24.0),
        };
        if event.modifiers.secondary()
            || event.modifiers.control
            || matches!(event.delta, ScrollDelta::Lines(_))
        {
            let zoom = vp.zoom * (-dy * 0.0025).exp();
            self.viewport = Some(vp.zoom_at(local, zoom));
        } else if dx != 0.0 || dy != 0.0 {
            self.viewport = Some(vp.pan_by(-dx, -dy));
        }
        cx.stop_propagation();
        cx.notify();
    }
}

impl Render for ZoomableImage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (nat_w, nat_h) = self.resolved_natural();
        let entity: Entity<Self> = cx.entity();
        let vp = self.viewport.unwrap_or_default();
        let fit_zoom = self.calculate_fit_zoom();

        let cursor = if self.dragging {
            CursorStyle::ClosedHand
        } else if vp.zoom > fit_zoom + 0.02 {
            CursorStyle::OpenHand
        } else {
            CursorStyle::Arrow
        };

        let entity_zoom_out = entity.clone();
        let entity_reset = entity.clone();
        let entity_zoom_in = entity.clone();
        let entity_fit = entity.clone();

        let dock = canvas_dock(cx)
            .child(
                dock_group()
                    .child(
                        Button::new("image-zoom-out", ButtonKind::Ghost)
                            .size(ButtonSize::Icon)
                            .icon(Icon::MinusSignIcon)
                            .tooltip("Zoom out")
                            .on_click(move |_, cx| {
                                entity_zoom_out.update(cx, |this, cx| this.zoom_out(cx));
                            }),
                    )
                    .child(
                        div()
                            .id("image-zoom-reset")
                            .cursor_pointer()
                            .tooltip(tooltip("Reset to 100%"))
                            .on_click(move |_, _, cx| {
                                entity_reset.update(cx, |this, cx| this.zoom_to_100(cx));
                            })
                            .child(zoom_level(vp.zoom, cx)),
                    )
                    .child(
                        Button::new("image-zoom-in", ButtonKind::Ghost)
                            .size(ButtonSize::Icon)
                            .icon(Icon::Plus)
                            .tooltip("Zoom in")
                            .on_click(move |_, cx| {
                                entity_zoom_in.update(cx, |this, cx| this.zoom_in(cx));
                            }),
                    ),
            )
            .child(dock_separator(cx))
            .child(
                dock_group().child(
                    Button::new("image-zoom-fit", ButtonKind::Ghost)
                        .size(ButtonSize::Icon)
                        .icon(Icon::Maximize2)
                        .tooltip("Fit to view")
                        .on_click(move |_, cx| {
                            entity_fit.update(cx, |this, cx| this.fit(cx));
                        }),
                ),
            );

        let img_w = (nat_w * vp.zoom).max(1.0);
        let img_h = (nat_h * vp.zoom).max(1.0);

        let image_element = div()
            .absolute()
            .left(px(vp.x))
            .top(px(vp.y))
            .w(px(img_w))
            .h(px(img_h))
            .child(
                img(self.image.source())
                    .size_full()
                    .object_fit(ObjectFit::Fill),
            );

        let probe = canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                let changed = entity.update(cx, |view, _| {
                    let changed = view.bounds.is_none_or(|b| b.size != bounds.size);
                    view.bounds = Some(bounds);
                    if changed {
                        view.ensure_viewport(bounds);
                    }
                    changed
                });
                // The fit is known only once the view is measured: draw again with it.
                if changed {
                    window.refresh();
                }
            },
        )
        .absolute()
        .size_full();

        let mut root = div()
            .id(self.id.clone())
            .size_full()
            .relative()
            .overflow_hidden()
            .cursor(cursor)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event: &MouseDownEvent, _, cx| view.mouse_down(event, cx)),
            )
            .on_mouse_move(
                cx.listener(|view, event: &MouseMoveEvent, _, cx| view.mouse_move(event, cx)),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, event: &MouseUpEvent, _, cx| view.mouse_up(event, cx)),
            )
            .on_scroll_wheel(cx.listener(Self::scroll))
            .child(probe)
            .child(image_element)
            .child(
                div()
                    .absolute()
                    .bottom(px(16.0))
                    .right(px(16.0))
                    .child(dock),
            );

        if let Some(selector) = self.debug_selector.clone() {
            root = root.debug_selector(move || selector.to_string());
        }

        root
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::{TestAppContext, size};

    use super::*;
    use crate::test_support::{draw, open};

    #[test]
    fn clamp_zoom_clamps_boundaries_and_recovers_from_nan() {
        assert_eq!(clamp_zoom(0.0), MIN_ZOOM);
        assert_eq!(clamp_zoom(0.05), MIN_ZOOM);
        assert_eq!(clamp_zoom(0.10), MIN_ZOOM);
        assert_eq!(clamp_zoom(1.0), 1.0);
        assert_eq!(clamp_zoom(8.0), MAX_ZOOM);
        assert_eq!(clamp_zoom(12.0), MAX_ZOOM);
        assert_eq!(clamp_zoom(f32::NAN), 1.0);
        assert_eq!(clamp_zoom(f32::INFINITY), MAX_ZOOM);
        assert_eq!(clamp_zoom(f32::NEG_INFINITY), MIN_ZOOM);
    }

    #[test]
    fn calculate_fit_zoom_scales_down_and_respects_padding() {
        // Image larger than viewport scales down to fit within room:
        let zoom = calculate_fit_zoom(1000.0, 1000.0, 500.0, 500.0, 0.0);
        assert_eq!(zoom, 0.5);

        // With padding 25px: room is 500 - 50 = 450px -> 450 / 1000 = 0.45:
        let zoom_pad = calculate_fit_zoom(1000.0, 1000.0, 500.0, 500.0, 25.0);
        assert_eq!(zoom_pad, 0.45);

        // Image taller than viewport constrains on height:
        let zoom_tall = calculate_fit_zoom(100.0, 1000.0, 500.0, 500.0, 0.0);
        assert_eq!(zoom_tall, 0.5);

        // Image wider than viewport constrains on width:
        let zoom_wide = calculate_fit_zoom(1000.0, 100.0, 500.0, 500.0, 0.0);
        assert_eq!(zoom_wide, 0.5);

        // Small image is capped at 1.0 (never upscaled by default):
        let zoom_small = calculate_fit_zoom(100.0, 100.0, 500.0, 500.0, 0.0);
        assert_eq!(zoom_small, 1.0);

        // Invalid dimensions fall back to 1.0:
        assert_eq!(calculate_fit_zoom(0.0, 100.0, 500.0, 500.0, 0.0), 1.0);
        assert_eq!(calculate_fit_zoom(100.0, 0.0, 500.0, 500.0, 0.0), 1.0);
        assert_eq!(calculate_fit_zoom(100.0, 100.0, 0.0, 500.0, 0.0), 1.0);
    }

    #[test]
    fn calculate_fit_position_centers_image() {
        let (x, y) = calculate_fit_position(1000.0, 600.0, 800.0, 600.0, 0.5);
        // Width at 0.5 is 500px: left is (800 - 500) / 2 = 150px:
        assert_eq!(x, 150.0);
        // Height at 0.5 is 300px: top is (600 - 300) / 2 = 150px:
        assert_eq!(y, 150.0);

        // Center of the image coincides with center of the viewport:
        let center_x = x + (1000.0 * 0.5) / 2.0;
        let center_y = y + (600.0 * 0.5) / 2.0;
        assert_eq!(center_x, 400.0);
        assert_eq!(center_y, 300.0);
    }

    #[test]
    fn image_viewport_zoom_at_preserves_focus_point() {
        let vp = ImageViewport::new(10.0, 20.0, 1.0);
        let focus = point(px(100.0), px(100.0));
        let zoomed = vp.zoom_at(focus, 2.0);

        assert_eq!(zoomed.zoom, 2.0);
        // Before zoom: image point at focus: (100 - 10) / 1.0 = 90
        // After zoom: 90 * 2.0 + new_x = 100 -> new_x = 100 - 180 = -80
        assert_eq!(zoomed.x, -80.0);
        // Before zoom: image point at focus: (100 - 20) / 1.0 = 80
        // After zoom: 80 * 2.0 + new_y = 100 -> new_y = 100 - 160 = -60
        assert_eq!(zoomed.y, -60.0);
    }

    #[test]
    fn image_viewport_pan_by_moves_by_exact_pixels() {
        let vp = ImageViewport::new(10.0, 20.0, 1.5);
        let panned = vp.pan_by(15.0, -5.0);
        assert_eq!(panned.x, 25.0);
        assert_eq!(panned.y, 15.0);
        assert_eq!(panned.zoom, 1.5);
    }

    #[test]
    fn a_picture_fits_inside_its_box_keeping_its_shape() {
        assert_eq!(fit_size((1024.0, 512.0), (500.0, 500.0)), (500.0, 250.0));
        assert_eq!(fit_size((100.0, 400.0), (500.0, 200.0)), (50.0, 200.0));
        // A picture smaller than its box is not blown up.
        assert_eq!(fit_size((64.0, 64.0), (500.0, 500.0)), (64.0, 64.0));
        // An unknown size fills the box, so a viewer still draws something.
        assert_eq!(fit_size((0.0, 0.0), (500.0, 200.0)), (500.0, 200.0));
    }

    #[gpui::test]
    fn a_thumbnail_reports_its_click(cx: &mut TestAppContext) {
        let clicked = Rc::new(Cell::new(0));
        let count = clicked.clone();
        let mut cx = open(cx, size(400.0, 300.0), move |_, _| {
            let count = count.clone();
            div()
                .debug_selector(|| "thumb".to_string())
                .child(
                    ImageThumb::new("thumb", ImageData::from_path("/tmp/chart.png"))
                        .size(80.0)
                        .caption("chart.png")
                        .on_click(move |_, _| count.set(count.get() + 1)),
                )
                .into_any_element()
        });
        assert!(
            cx.debug_bounds("thumb").is_some(),
            "the thumbnail is on screen"
        );
        cx.simulate_click(gpui::point(px(40.0), px(40.0)), gpui::Modifiers::none());
        draw(&mut cx);
        assert_eq!(clicked.get(), 1);
    }

    #[gpui::test]
    fn a_viewer_reports_its_actions_and_close(cx: &mut TestAppContext) {
        let (closed, revealed) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
        let (close, reveal) = (closed.clone(), revealed.clone());
        let mut cx = open(cx, size(800.0, 600.0), move |_, _| {
            let (close, reveal) = (close.clone(), reveal.clone());
            ImageViewer::new(
                "viewer",
                "chart.png",
                ImageData::from_path("/tmp/chart.png"),
            )
            .natural(1024.0, 768.0)
            .caption("PNG · 1024×768")
            .details(
                div()
                    .debug_selector(|| "viewer-details".to_string())
                    .child("a bar chart"),
            )
            .action(ImageAction::new(
                "viewer-reveal",
                Icon::Folder,
                "Reveal in Finder",
                move |_, _| reveal.set(reveal.get() + 1),
            ))
            .on_close(move |_, _| close.set(close.get() + 1))
            .into_any_element()
        });
        assert!(
            cx.debug_bounds("viewer-details").is_some(),
            "the host's details are drawn"
        );
        for id in ["viewer-reveal", "viewer-close"] {
            let bounds = cx.debug_bounds(id).unwrap_or_else(|| panic!("no {id}"));
            cx.simulate_click(bounds.center(), gpui::Modifiers::none());
            draw(&mut cx);
        }
        assert_eq!(revealed.get(), 1);
        assert_eq!(closed.get(), 1);
    }

    #[gpui::test]
    fn a_card_reports_its_click(cx: &mut TestAppContext) {
        let clicked = Rc::new(Cell::new(0));
        let count = clicked.clone();
        let mut cx = open(cx, size(400.0, 300.0), move |_, _| {
            let count = count.clone();
            div()
                .debug_selector(|| "card".to_string())
                .child(
                    ImageCard::new("card", ImageData::from_path("/tmp/chart.png"), "My Chart")
                        .thumb_height(100.0)
                        .meta(div().child("Grok · 2h ago"))
                        .on_click(move |_, _| count.set(count.get() + 1)),
                )
                .into_any_element()
        });
        assert!(cx.debug_bounds("card").is_some(), "the card is on screen");
        cx.simulate_click(gpui::point(px(40.0), px(40.0)), gpui::Modifiers::none());
        draw(&mut cx);
        assert_eq!(clicked.get(), 1);
    }
}
