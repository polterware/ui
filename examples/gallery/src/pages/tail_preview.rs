//! `TailPreview`: the end of a transcript in a frame of a fixed height. A live one fades at
//! both ends, a finished one at its top, and one with no lines says what it waits for.

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::theme::Theme;
use polterware_ui::{TailLine, TailTone, tail_preview};

use super::super::Example;

const WIDTH: f32 = 360.0;
const HEIGHT: f32 = 164.0;

fn lines() -> Vec<TailLine> {
    [
        ("Read crates/core/src/lib.rs", TailTone::Muted),
        ("Ran cargo test -p core", TailTone::Muted),
        ("running 14 tests", TailTone::Normal),
        (
            "test routing::picks_the_first_tier ... ok",
            TailTone::Normal,
        ),
        ("test routing::skips_a_full_quota ... ok", TailTone::Normal),
        ("test runner::ends_a_cancelled_run ... ok", TailTone::Normal),
        ("Edited crates/core/src/runner.rs", TailTone::Muted),
        (
            "The runner now ends a cancelled run before it reads its",
            TailTone::Normal,
        ),
        (
            "last event, so the run page never shows a stale phase and the status chip agrees.",
            TailTone::Normal,
        ),
        ("Ran cargo test -p core", TailTone::Muted),
        ("test result: ok. 14 passed; 0 failed", TailTone::Normal),
        (
            "All tests pass; the change is in the worktree.",
            TailTone::Normal,
        ),
    ]
    .into_iter()
    .map(|(text, tone)| TailLine::new(text).tone(tone))
    .collect()
}

fn frame(preview: impl IntoElement, cx: &App) -> AnyElement {
    let theme = Theme::global(cx);
    div()
        .w(px(WIDTH))
        .p(px(14.0))
        .rounded(px(theme.radius_card))
        .bg(theme.color_surface_subtle)
        .border_1()
        .border_color(theme.color_border_subtle)
        .child(preview)
        .into_any_element()
}

fn live(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let surface = Theme::global(cx).color_surface_subtle;
    vec![frame(
        tail_preview(lines(), HEIGHT, surface)
            .live(true)
            .placeholder("Waiting for output…"),
        cx,
    )]
}

fn finished(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let surface = Theme::global(cx).color_surface_subtle;
    vec![frame(tail_preview(lines(), HEIGHT, surface), cx)]
}

fn empty(_: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let surface = Theme::global(cx).color_surface_subtle;
    vec![frame(
        tail_preview(Vec::new(), HEIGHT, surface)
            .live(true)
            .placeholder("Waiting for output…"),
        cx,
    )]
}

pub fn examples() -> Vec<Example> {
    vec![
        Example {
            slug: "tail-preview",
            title: "A live tail, fading at both ends",
            height: 220.0,
            render: live,
        },
        Example {
            slug: "tail-preview-finished",
            title: "A finished tail, fading at its top",
            height: 220.0,
            render: finished,
        },
        Example {
            slug: "tail-preview-empty",
            title: "No output yet",
            height: 220.0,
            render: empty,
        },
    ]
}
