//! The gallery's pages, one module per Solid docs entry, in the Solid gallery's order.

use super::Example;

mod button;
mod card;
mod collapsible_section;
mod commander;
mod composer;
mod filter_popover;
mod icon_tile;
mod keyboard_key;
mod page_under;
mod progress_bar;
mod radio_group;
mod rail_properties;
mod segment_display;
mod select;
mod settings_dialog;
mod skeleton;
mod status;
mod switch;
mod tooltip;
mod usage_cards;

pub fn all() -> Vec<Example> {
    let mut examples = Vec::new();
    examples.extend(button::examples());
    examples.extend(radio_group::examples());
    examples.extend(select::examples());
    examples.extend(filter_popover::examples());
    examples.extend(page_under::examples());
    examples.extend(switch::examples());
    examples.extend(progress_bar::examples());
    examples.extend(skeleton::examples());
    examples.extend(segment_display::examples());
    examples.extend(status::examples());
    examples.extend(card::examples());
    examples.extend(collapsible_section::examples());
    examples.extend(icon_tile::examples());
    examples.extend(keyboard_key::examples());
    examples.extend(tooltip::examples());
    examples.extend(commander::examples());
    examples.extend(composer::examples());
    examples.extend(rail_properties::examples());
    examples.extend(settings_dialog::examples());
    examples.extend(usage_cards::examples());
    examples
}
