//! `page_under`: a page whose content scrolls under its toolbar, as the run page's transcript
//! does. The toolbar floats on its trail, the rows run up beneath it, and the end fades into
//! the surface while there is more below. The scrolls are at rest in the captures; wheel
//! them to see the rows go soft under the toolbar.

use gpui::{AnyElement, App, IntoElement, Window, div, prelude::*, px};
use polterware_ui::theme::Theme;
use polterware_ui::{
    Button, ButtonKind, Column, DataTable, Icon, page_under, page_under_inset, scroll_area_under,
    toolbar,
};

use super::super::Example;

const ROWS: usize = 40;

/// The page's surface, as tall as the example.
fn surface(content: AnyElement, cx: &App) -> AnyElement {
    let theme = Theme::global(cx);
    div()
        .h(px(260.0))
        .w_full()
        .flex()
        .flex_col()
        .rounded(px(theme.radius_surface))
        .bg(theme.color_surface)
        .overflow_hidden()
        .child(content)
        .into_any_element()
}

fn bar(cx: &App) -> AnyElement {
    toolbar(
        div(),
        Button::new("page-under-filter", ButtonKind::Secondary)
            .icon(Icon::ListFilter)
            .label("Filter"),
        cx,
    )
    .into_any_element()
}

fn table(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let id = "page-under-table";
    let top = page_under_inset(id, false, theme.space_2, window, cx);
    let rows = DataTable::new(
        "page-under-rows",
        vec![Column::new("Model").fixed(), Column::new("Score").end()],
        ROWS,
        |row, _, _| {
            vec![
                div()
                    .child(format!("model-{:02}", row.index + 1))
                    .into_any_element(),
                div()
                    .child(format!("{}", 90 - row.index))
                    .into_any_element(),
            ]
        },
    )
    .inset_top(top)
    .fades(theme.color_surface);
    let page = page_under(id, bar(cx), None, theme.space_2, rows, window, cx);
    vec![surface(page, cx)]
}

fn scroll(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let theme = *Theme::global(cx);
    let id = "page-under-scroll";
    let top = page_under_inset(id, false, theme.space_2, window, cx);
    let lines = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(theme.space_2))
        .text_size(px(theme.font_size_sm))
        .text_color(theme.color_text_secondary)
        .children((1..=ROWS).map(|line| div().child(format!("Line {line} of the page"))));
    let content = scroll_area_under("page-under-lines", top, lines, window, cx);
    let page = page_under(id, bar(cx), None, theme.space_2, content, window, cx);
    vec![surface(page, cx)]
}

pub fn examples() -> Vec<Example> {
    let example = |title, render| Example {
        slug: "page-under",
        title,
        height: 300.0,
        render,
    };
    vec![
        example("Table under the toolbar", table),
        example("Scroll under the toolbar", scroll),
    ]
}
