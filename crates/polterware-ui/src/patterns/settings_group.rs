//! The design system's `SettingsSection` (`settings-dialog.tsx`): a titled group of settings
//! rows. The title is 12px medium muted ink over its optional 11.5px muted description,
//! `px-1` in, with an optional action at the heading's end; the rows sit on one [`card`]
//! under it, `gap-2` lower, `p-4` in and `gap-4` apart. A page stacks its groups `gap-6`
//! apart ([`settings_group_gap`]). Its rows are [`settings_row`](super::settings_row)s.
//!
//! [`settings_section`](super::settings_section) is the other form, with the title on the
//! card beside an icon tile.

use gpui::{
    AnyElement, App, FontWeight, IntoElement, ParentElement, RenderOnce, SharedString, Window, div,
    prelude::*, px,
};

use crate::card::card;
use crate::theme::{Theme, tw};

/// The room between a settings page's groups (`gap-6`).
pub fn settings_group_gap() -> f32 {
    tw(6.0)
}

/// A group titled `title`.
pub fn settings_group(title: impl Into<SharedString>) -> SettingsGroup {
    SettingsGroup {
        title: title.into(),
        description: None,
        action: None,
        children: Vec::new(),
    }
}

#[derive(IntoElement)]
pub struct SettingsGroup {
    title: SharedString,
    description: Option<SharedString>,
    action: Option<AnyElement>,
    children: Vec<AnyElement>,
}

impl SettingsGroup {
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        let description = description.into();
        self.description = (!description.is_empty()).then_some(description);
        self
    }

    /// A control at the heading's end, such as a refresh button.
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }
}

impl ParentElement for SettingsGroup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for SettingsGroup {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let anchor = self.title.clone();
        let heading = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(tw(2.0)))
            .px(px(tw(1.0)))
            .child(
                div()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(18.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.color_text_muted)
                            .child(self.title),
                    )
                    .children(self.description.map(|description| {
                        div()
                            .mt(px(tw(0.5)))
                            .text_size(px(11.5))
                            .line_height(px(17.25))
                            .text_color(theme.color_text_muted)
                            .child(description)
                    })),
            )
            .children(
                self.action
                    .map(|action| div().flex_none().my(px(-tw(2.0))).child(action)),
            );
        super::settings_dialog::settings_anchor(
            anchor,
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(tw(2.0)))
                .child(heading)
                .child(
                    card(cx)
                        .w_full()
                        .p(px(tw(4.0)))
                        .flex()
                        .flex_col()
                        .gap(px(tw(4.0)))
                        .children(self.children),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, div, prelude::*, px, size};

    use super::settings_group;
    use crate::patterns::settings_row;
    use crate::test_support::{draw, open};
    use crate::theme::tw;

    #[gpui::test]
    fn the_title_sits_over_the_card_and_the_rows_pad_13px_in(cx: &mut TestAppContext) {
        let mut cx = open(cx, size(640.0, 400.0), |_, _| {
            div()
                .size_full()
                .child(
                    settings_group("Sessions").child(
                        div()
                            .debug_selector(|| "group-row".into())
                            .child(settings_row("Confirm").description("Ask first.")),
                    ),
                )
                .into_any_element()
        });
        draw(&mut cx);
        let row = cx.debug_bounds("group-row").expect("the row draws");
        // The 18px title, `gap-2`, then the card's `p-4`.
        assert_eq!(row.origin.y, px(18.0 + tw(2.0) + tw(4.0)));
        assert_eq!(row.origin.x, px(tw(4.0)));
        // The label's 18.75px line and the description's 17.25px, `mt-0.5` apart (laid out
        // on whole pixels).
        let text = 18.75 + tw(0.5) + 17.25;
        assert!((f32::from(row.size.height) - text).abs() <= 1.0);
    }
}
