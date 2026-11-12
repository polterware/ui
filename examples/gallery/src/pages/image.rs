//! `ImageThumb`, `ImageCard`, and `ImageViewer`: a thumbnail with its caption, a gallery
//! card with its title and meta line, and the viewer a click opens with its actions.

use gpui::{AnyElement, App, ImageFormat, IntoElement, Window, div, prelude::*, px};
use polterware_ui::{Icon, ImageAction, ImageCard, ImageData, ImageThumb, ImageViewer};

use super::super::Example;

const SAMPLE: &[u8] = include_bytes!("../../assets/sample.png");

fn sample() -> ImageData {
    ImageData::from_bytes(ImageFormat::Png, SAMPLE.to_vec())
}

fn thumbnails(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    vec![
        div()
            .flex()
            .gap(px(12.0))
            .child(
                ImageThumb::new("image-thumb", sample())
                    .caption("chart.png")
                    .on_click(|_, _| {}),
            )
            .child(ImageThumb::new("image-thumb-plain", sample()).size(64.0))
            .into_any_element(),
    ]
}

fn viewer(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    vec![
        ImageViewer::new("image-viewer", "chart.png", sample())
            .natural(256.0, 256.0)
            .caption("PNG · 256×256")
            .action(ImageAction::new(
                "image-viewer-reveal",
                Icon::Folder,
                "Reveal in Finder",
                |_, _| {},
            ))
            .action(ImageAction::new(
                "image-viewer-copy",
                Icon::Copy,
                "Copy",
                |_, _| {},
            ))
            .on_close(|_, _| {})
            .into_any_element(),
    ]
}

fn cards(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    vec![
        div()
            .flex()
            .gap(px(16.0))
            .child(
                div().w(px(240.0)).child(
                    ImageCard::new("gallery-card-1", sample(), "Draw a futuristic city")
                        .thumb_height(140.0)
                        .meta(
                            div()
                                .text_size(px(12.0))
                                .text_color(gpui::rgb(0x888888))
                                .child("Grok · 2h ago"),
                        )
                        .on_click(|_, _| {}),
                ),
            )
            .child(
                div().w(px(240.0)).child(
                    ImageCard::new("gallery-card-2", sample(), "App logo icon")
                        .thumb_height(140.0)
                        .meta(
                            div()
                                .text_size(px(12.0))
                                .text_color(gpui::rgb(0x888888))
                                .child("Codex · 1d ago"),
                        )
                        .hover_progress(1.0),
                ),
            )
            .into_any_element(),
    ]
}

pub fn examples() -> Vec<Example> {
    vec![
        Example {
            slug: "image-card",
            title: "A gallery card with title and meta",
            height: 260.0,
            render: cards,
        },
        Example {
            slug: "image-thumb",
            title: "A thumbnail, with and without its caption",
            height: 200.0,
            render: thumbnails,
        },
        Example {
            slug: "image-viewer",
            title: "The viewer, with its actions",
            height: 320.0,
            render: viewer,
        },
    ]
}
