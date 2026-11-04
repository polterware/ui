//! One section of a settings page, as a card: an [`IconTile`] (28px, its 16px mark in the
//! section's tone), the 14px semibold title and its 12.5px secondary description beside it,
//! an optional action at the right, and the section's rows below, 12px under the heading,
//! all 16px in on [`card`]. A page stacks its sections 16px apart.

use gpui::{
    AnyElement, App, FontWeight, IntoElement, ParentElement, RenderOnce, SharedString, Window, div,
    prelude::*, px,
};

use crate::card::card;
use crate::icon_tile::{IconTile, TileTone};
use crate::icons::Icon;
use crate::theme::Theme;

/// The room between a page's sections.
pub const SETTINGS_SECTION_GAP: f32 = 16.0;

const PAD: f32 = 16.0;
const TILE: f32 = 28.0;
const MARK: f32 = 16.0;

/// A section titled `title`.
pub fn settings_section(title: impl Into<SharedString>) -> SettingsSection {
    SettingsSection {
        title: title.into(),
        description: None,
        icon: None,
        tone: TileTone::Neutral,
        action: None,
        children: Vec::new(),
    }
}

#[derive(IntoElement)]
pub struct SettingsSection {
    title: SharedString,
    description: Option<SharedString>,
    icon: Option<Icon>,
    tone: TileTone,
    action: Option<AnyElement>,
    children: Vec<AnyElement>,
}

impl SettingsSection {
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        let description = description.into();
        self.description = (!description.is_empty()).then_some(description);
        self
    }

    /// The section's mark, on a tile of `tone`.
    pub fn icon(mut self, icon: Icon, tone: TileTone) -> Self {
        self.icon = Some(icon);
        self.tone = tone;
        self
    }

    /// A control at the heading's right, such as Install for all.
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }
}

impl ParentElement for SettingsSection {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for SettingsSection {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let text = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.color_text_primary)
                    .child(self.title),
            )
            .children(self.description.map(|description| {
                div()
                    .text_size(px(12.5))
                    .line_height(px(18.0))
                    .text_color(theme.color_text_secondary)
                    .child(description)
            }));
        let heading = div()
            .w_full()
            .flex()
            .items_start()
            .gap(px(12.0))
            .children(
                self.icon
                    .map(|icon| IconTile::new(icon).size(TILE, MARK).tone(self.tone)),
            )
            .child(text)
            .children(self.action.map(|action| div().flex_none().child(action)));
        let body = (!self.children.is_empty())
            .then(|| div().w_full().flex().flex_col().children(self.children));
        card(cx)
            .w_full()
            .p(px(PAD))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(heading)
            .children(body)
    }
}
