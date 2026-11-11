//! Images: a thumbnail and the viewer a click opens, as a page shows a picture it did not
//! draw itself.
//!
//! [`ImageThumb`] is a square thumbnail on a raised face with a quiet edge, the picture
//! cropped to fill it (`object-fit: cover`), an optional caption under it in 11px muted ink,
//! and a click that asks the host to open it. [`ImageViewer`] is the modal that shows it
//! whole: a floating card over a [`modal_layer`](super::modal_layer), the file's name as its
//! title, the picture fitted inside 86% of the window's width and 72% of its height at the
//! size the host says it has, a caption under it, and the host's actions (Reveal, Copy) at the
//! end of the footer, with Close at the header's end.
//!
//! The web has no Polterware counterpart for either; the viewer behaves like gpui-kit's
//! `ImageView` dialog: the app owns whether it is open, what Close does, and what the actions
//! do, and the viewer only draws and reports clicks. GPUI clips to rectangles, so a picture
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
    AnyElement, App, ElementId, Image, ImageFormat, ImageSource, IntoElement, ObjectFit,
    RenderImage, RenderOnce, SharedString, Window, div, img, prelude::*, px,
};

use crate::Handler;
use crate::button::{Button, ButtonKind, ButtonSize};
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

/// One button in a viewer's footer, such as Reveal in Finder or Copy.
#[derive(Clone)]
pub struct ImageAction {
    pub id: SharedString,
    pub icon: Icon,
    pub label: SharedString,
    pub handler: Handler,
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
        }
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
                Button::new(action.id, ButtonKind::Secondary)
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
        .children(footer);
        fn ignore(_: &mut Window, _: &mut App) {}
        let dismiss: Handler = self.on_close.unwrap_or_else(|| Rc::new(ignore));
        modal_layer(self.presence, self.interactive, dismiss, card, &theme)
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
            .action(ImageAction::new(
                "viewer-reveal",
                Icon::Folder,
                "Reveal in Finder",
                move |_, _| reveal.set(reveal.get() + 1),
            ))
            .on_close(move |_, _| close.set(close.get() + 1))
            .into_any_element()
        });
        for id in ["viewer-reveal", "viewer-close"] {
            let bounds = cx.debug_bounds(id).unwrap_or_else(|| panic!("no {id}"));
            cx.simulate_click(bounds.center(), gpui::Modifiers::none());
            draw(&mut cx);
        }
        assert_eq!(revealed.get(), 1);
        assert_eq!(closed.get(), 1);
    }
}
