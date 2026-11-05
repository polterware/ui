//! The component gallery: every shared control in every variant and state, in the order and
//! with the content of the Solid gallery (`apps/gallery`), each example in a box like its
//! `.docs-preview` (920px wide, 24px padding, 16px gaps, #242424, radius 14).
//!
//! `cargo run --release -p polterware-gallery` opens it. With `GALLERY_SHOTS=<dir>` it draws one
//! example at a time, saves `<dir>/<slug>/<nn>.png` at the display's scale, and quits;
//! `GALLERY_ONLY=button,switch` limits the run to those pages. With `GALLERY_SOLID=<dir>` as
//! well, each capture takes the number the Solid gallery's `<dir>/<slug>/index.json` gives its
//! title and is compared with that `<dir>/<slug>/<nn>.png` (taken at 2x); one line per example
//! prints the share of pixels whose largest channel differs by
//! more than 24, and the mean of each pixel's largest channel difference.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui::{
    AnyElement, App, AppContext as _, Application, Bounds, Context, FontWeight, IntoElement,
    Render, SharedString, Window, WindowBackgroundAppearance, WindowBounds, WindowKind,
    WindowOptions, div, point, prelude::*, px, relative, size,
};
use polterware_ui::IconAssets;
use polterware_ui::theme::{Theme, white};

mod hot;
mod pages;

/// One gallery example: its page slug, its title, its box height, and its content.
pub struct Example {
    pub slug: &'static str,
    pub title: &'static str,
    pub height: f32,
    pub render: fn(&mut Window, &mut App) -> Vec<AnyElement>,
}

const BOX_WIDTH: f32 = 920.0;

fn preview(example: &Example, window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = *Theme::global(cx);
    div()
        .w(px(BOX_WIDTH))
        .h(px(example.height))
        .flex_none()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(16.0))
        .p(px(24.0))
        .bg(theme.color_surface_subtle)
        .border_1()
        .border_color(white(0.043))
        .rounded(px(theme.radius_card))
        .font_family(theme.font_sans)
        .text_size(px(16.0))
        .line_height(px(24.0))
        .text_color(theme.color_text_secondary)
        .children((example.render)(window, cx))
        .into_any_element()
}

struct Gallery {
    examples: Vec<Example>,
    /// `GALLERY_ONLY`'s pages, kept so a hot patch can rebuild the list with its new code.
    only: Option<Vec<String>>,
    /// The one example drawn while capturing.
    current: Option<usize>,
}

fn examples(only: &Option<Vec<String>>) -> Vec<Example> {
    pages::all()
        .into_iter()
        .filter(|example| {
            only.as_ref()
                .is_none_or(|only| only.iter().any(|slug| slug == example.slug))
        })
        .collect()
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        hot::call(|| self.draw(window, cx))
    }
}

impl Gallery {
    fn draw(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        // The list holds function pointers taken at startup; under hot reload it is rebuilt
        // here so each example's render is the patched one.
        if cfg!(feature = "hot-reload") && self.current.is_none() {
            self.examples = examples(&self.only);
        }
        let theme = *Theme::global(cx);
        let root = div()
            .size_full()
            .bg(theme.color_app_background)
            .font_family(theme.font_sans)
            .text_color(theme.color_text_primary);
        if let Some(index) = self.current {
            return root
                .child(preview(&self.examples[index], window, cx))
                .into_any_element();
        }
        let mut list = div()
            .id("gallery")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(24.0))
            .p(px(40.0));
        let mut last_slug = "";
        for example in &self.examples {
            if example.slug != last_slug {
                last_slug = example.slug;
                list = list.child(
                    div()
                        .pt(px(16.0))
                        .text_size(px(22.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(SharedString::from(example.slug)),
                );
            }
            list = list
                .child(
                    div()
                        .text_size(px(15.0))
                        .line_height(relative(1.5))
                        .font_weight(FontWeight::MEDIUM)
                        .child(example.title),
                )
                .child(preview(example, window, cx));
        }
        root.child(list).into_any_element()
    }
}

fn main() {
    let shots = std::env::var_os("GALLERY_SHOTS")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from);
    let only: Option<Vec<String>> = std::env::var("GALLERY_ONLY")
        .ok()
        .map(|list| list.split(',').map(str::to_owned).collect());
    Application::new()
        .with_assets(IconAssets)
        .run(move |cx: &mut App| {
            // Dispatch's 13px root, so the controls draw as they do in the apps.
            polterware_ui::theme::set_root_rem(13.0);
            polterware_ui::init(cx);
            hot::init(cx);
            let examples = examples(&only);
            let capturing = shots.is_some();
            let window_size = if capturing {
                size(px(BOX_WIDTH), px(520.0))
            } else {
                size(px(1040.0), px(860.0))
            };
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    window_size,
                    cx,
                ))),
                kind: WindowKind::Normal,
                window_background: WindowBackgroundAppearance::Opaque,
                titlebar: if capturing {
                    None
                } else {
                    Some(gpui::TitlebarOptions {
                        title: Some("Polterware UI gallery".into()),
                        appears_transparent: false,
                        traffic_light_position: Some(point(px(12.0), px(12.0))),
                    })
                },
                ..Default::default()
            };
            let window = cx
                .open_window(options, |_, cx| {
                    cx.new(|_| Gallery {
                        examples,
                        only,
                        current: None,
                    })
                })
                .expect("gallery window");
            cx.activate(true);
            if let Some(dir) = shots {
                let solid = std::env::var_os("GALLERY_SOLID")
                    .filter(|dir| !dir.is_empty())
                    .map(PathBuf::from);
                capture_all(window, dir, solid, cx);
            }
        });
}

fn capture_all(
    window: gpui::WindowHandle<Gallery>,
    dir: PathBuf,
    solid: Option<PathBuf>,
    cx: &mut App,
) {
    cx.spawn(async move |cx| {
        let count = window
            .update(cx, |gallery, _, _| gallery.examples.len())
            .unwrap_or(0);
        cx.background_executor()
            .timer(Duration::from_millis(800))
            .await;
        let mut per_slug: std::collections::HashMap<&'static str, usize> = Default::default();
        for index in 0..count {
            let (slug, title, height) = window
                .update(cx, |gallery, _, cx| {
                    gallery.current = Some(index);
                    cx.notify();
                    let example = &gallery.examples[index];
                    (example.slug, example.title, example.height)
                })
                .unwrap();
            cx.background_executor()
                .timer(Duration::from_millis(350))
                .await;
            let number = per_slug.entry(slug).or_default();
            let index = solid
                .as_ref()
                .and_then(|solid| solid_index(solid, slug, title))
                .unwrap_or(*number);
            *number += 1;
            let folder = dir.join(slug);
            let _ = std::fs::create_dir_all(&folder);
            let name = format!("{index:02}.png");
            let path = folder.join(&name);
            let _ = window.update(cx, |_, window, _| {
                let scale = window.scale_factor();
                let crop = (
                    0,
                    0,
                    (BOX_WIDTH * scale).round() as u32,
                    (height * scale).round() as u32,
                );
                if let Err(err) =
                    polterware_ui::window::capture::save_window_png(window, &path, Some(crop))
                {
                    eprintln!("{}: {err}", path.display());
                }
            });
            if let Some(solid) = solid.as_ref() {
                match compare(&path, &solid.join(slug).join(&name)) {
                    Ok(metric) => println!("{slug}/{name}\t{title}\t{metric}"),
                    Err(err) => println!("{slug}/{name}\t{title}\t{err}"),
                }
            }
        }
        let _ = cx.update(|cx| cx.quit());
    })
    .detach();
}

/// Where the Solid gallery numbered the example of that title, from its `index.json`, so a
/// page that leaves examples out still pairs each capture with its own.
fn solid_index(solid: &Path, slug: &str, title: &str) -> Option<usize> {
    let text = std::fs::read_to_string(solid.join(slug).join("index.json")).ok()?;
    let entries: Vec<serde_json::Value> = serde_json::from_str(&text).ok()?;
    entries
        .iter()
        .find(|entry| entry["title"].as_str() == Some(title))
        .and_then(|entry| entry["i"].as_u64())
        .map(|index| index as usize)
}

/// The share of pixels whose largest channel differs by more than 24 (a pixel only one image
/// covers counts as differing, at 255), and the mean of each pixel's largest difference.
fn compare(gpui: &Path, solid: &Path) -> Result<String, String> {
    let load = |path: &Path| {
        image::open(path)
            .map(|image| image.to_rgba8())
            .map_err(|err| format!("{}: {err}", path.display()))
    };
    let a = load(gpui)?;
    let b = load(solid)?;
    let (w, h) = (a.width().max(b.width()), a.height().max(b.height()));
    let (mut differing, mut sum) = (0u64, 0f64);
    for y in 0..h {
        for x in 0..w {
            let inside = |image: &image::RgbaImage| x < image.width() && y < image.height();
            let largest = if inside(&a) && inside(&b) {
                let (p, q) = (a.get_pixel(x, y), b.get_pixel(x, y));
                (0..3)
                    .map(|i| (p[i] as i32 - q[i] as i32).abs())
                    .max()
                    .unwrap_or(0)
            } else {
                255
            };
            if largest > 24 {
                differing += 1;
            }
            sum += largest as f64;
        }
    }
    let total = (w as u64 * h as u64).max(1);
    Ok(format!(
        "{}x{} vs {}x{}\t{:.2}%\t{:.2}",
        a.width(),
        a.height(),
        b.width(),
        b.height(),
        100.0 * differing as f64 / total as f64,
        sum / total as f64
    ))
}
