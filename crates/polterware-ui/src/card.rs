//! `Card` (`.card-panel`): the raised host surface, `--color-surface-subtle` with the quiet
//! edge and the raised drop at `--radius-card`.

use gpui::{App, Div, div, prelude::*};

use super::material::raised_panel;
use crate::theme::Theme;

pub fn card(cx: &App) -> Div {
    let theme = Theme::global(cx);
    raised_panel(
        div().relative().bg(theme.color_surface_subtle),
        theme.radius_card,
        theme,
    )
}
