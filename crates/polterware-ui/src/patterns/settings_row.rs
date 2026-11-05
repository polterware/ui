//! `SettingsRow` (`settings-dialog.tsx`): a setting's label in 12.5px primary ink with its
//! optional 11.5px muted description `mt-0.5` under it, at least `min-w-40` (130px) wide, and its controls
//! at the row's end, `gap-2` apart and `gap-x-4` from the text
//! (`flex flex-wrap items-center justify-between`). When the row is too narrow for both, the
//! controls wrap under the text, `gap-y-2` lower. The row is as tall as its text or its
//! controls; a [`settings_group`](super::settings_group)'s card sets its rows `gap-4` apart.
//!
//! [`SettingsRow::icon`] is the settings feature's row for a list entry (`settings-section.tsx`):
//! a 24px [`IconTile`] with its 14px mark leads the row, `gap-3` before it.

use gpui::{AnyElement, App, IntoElement, SharedString, Window, div, prelude::*, px};

use crate::icon_tile::{IconTile, TileTone};
use crate::icons::Icon;
use crate::theme::{Theme, tw};

#[derive(IntoElement)]
pub struct SettingsRow {
    label: SharedString,
    description: Option<SharedString>,
    icon: Option<(Icon, TileTone)>,
    controls: Vec<AnyElement>,
}

/// A row labelled `label`.
pub fn settings_row(label: impl Into<SharedString>) -> SettingsRow {
    SettingsRow {
        label: label.into(),
        description: None,
        icon: None,
        controls: Vec::new(),
    }
}

impl SettingsRow {
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        let description = description.into();
        self.description = (!description.is_empty()).then_some(description);
        self
    }

    /// A tile of `tone` with `icon` leading the row, for a list entry.
    pub fn icon(mut self, icon: Icon, tone: TileTone) -> Self {
        self.icon = Some((icon, tone));
        self
    }
}

impl ParentElement for SettingsRow {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.controls.extend(elements);
    }
}

impl RenderOnce for SettingsRow {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = Theme::global(cx);
        let text = div()
            .flex_1()
            .min_w(px(tw(40.0)))
            .flex()
            .flex_col()
            .child(
                div()
                    .debug_selector(|| "settings-row-label".into())
                    .text_size(px(12.5))
                    .line_height(px(18.75))
                    .text_color(theme.color_text_primary)
                    .child(self.label),
            )
            .children(self.description.map(|description| {
                div()
                    .debug_selector(|| "settings-row-description".into())
                    .mt(px(tw(0.5)))
                    .text_size(px(11.5))
                    .line_height(px(17.25))
                    .text_color(theme.color_text_muted)
                    .child(description)
            }));
        let row = div()
            .w_full()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_x(px(tw(4.0)))
            .gap_y(px(tw(2.0)))
            .child(text)
            .when(!self.controls.is_empty(), |row| {
                row.child(
                    div()
                        .flex_none()
                        .max_w_full()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(tw(2.0)))
                        .children(self.controls),
                )
            });
        match self.icon {
            None => row.into_any_element(),
            Some((icon, tone)) => div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(tw(3.0)))
                .child(
                    div()
                        .flex_none()
                        .child(IconTile::new(icon).size(24.0, 14.0).tone(tone)),
                )
                .child(div().flex_1().min_w(px(0.0)).child(row))
                .into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_description_draws_no_line() {
        assert!(settings_row("Unit").description("").description.is_none());
        let row = settings_row("Unit").description("Bytes or bits");
        assert_eq!(
            row.description.as_ref().map(|text| text.as_ref()),
            Some("Bytes or bits")
        );
    }
}
