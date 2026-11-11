//! `ImageThumb` and `ImageViewer`: a thumbnail with its caption, a row of them as a run's
//! reply shows its pictures, and the viewer a click opens with its actions.

use gpui::{AnyElement, App, ImageFormat, IntoElement, Window, div, prelude::*, px};
use polterware_ui::{Icon, ImageAction, ImageData, ImageThumb, ImageViewer};

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

pub fn examples() -> Vec<Example> {
    vec![
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
