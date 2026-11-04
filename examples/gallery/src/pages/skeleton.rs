//! `skeleton.docs.tsx`: a few bones, then `PageSkeleton` in each layout, a list of rows,
//! and a paragraph, as the pages show them while they load.

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::{
    SkeletonLayout, bone, line_bone, list_skeleton, page_skeleton, skeleton_tone, text_skeleton,
};

use super::super::Example;

fn bones(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let tone = skeleton_tone(window, cx);
    vec![
        div()
            .w(px(320.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(bone(tone, cx).h(px(16.0)).w(px(112.0)))
            .child(line_bone(tone, 0.9, 12.0))
            .child(line_bone(tone, 0.6, 12.0))
            .child(bone(tone, cx).size(px(28.0)).rounded_full())
            .into_any_element(),
    ]
}

fn layout(layout: SkeletonLayout) -> Vec<AnyElement> {
    vec![
        div()
            .w(px(640.0))
            .h(px(320.0))
            .child(page_skeleton(layout))
            .into_any_element(),
    ]
}

fn page(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    layout(SkeletonLayout::Page)
}

fn table(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    layout(SkeletonLayout::Table)
}

fn canvas(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    layout(SkeletonLayout::Canvas)
}

fn detail(_: &mut Window, _: &mut App) -> Vec<AnyElement> {
    layout(SkeletonLayout::Detail)
}

fn list(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![
        div()
            .w(px(280.0))
            .child(list_skeleton(5, 32.0, window, cx))
            .into_any_element(),
    ]
}

fn text(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![
        div()
            .w(px(480.0))
            .child(text_skeleton(6, 12.0, window, cx))
            .into_any_element(),
    ]
}

pub fn examples() -> Vec<Example> {
    let example = |title, height, render| Example {
        slug: "skeleton",
        title,
        height,
        render,
    };
    vec![
        example("Bones", 160.0, bones),
        example("Page", 360.0, page),
        example("Table", 360.0, table),
        example("Canvas", 360.0, canvas),
        example("Detail", 360.0, detail),
        example("List", 300.0, list),
        example("Text", 200.0, text),
    ]
}
