//! The gallery's pages, one module per Solid docs entry, in the Solid gallery's order.

use super::Example;

mod button;
mod card;
mod collapsible_section;
mod icon_tile;
mod keyboard_key;
mod progress_bar;
mod radio_group;
mod segment_display;
mod select;
mod status;
mod switch;

pub fn all() -> Vec<Example> {
    let mut examples = Vec::new();
    examples.extend(button::examples());
    examples.extend(radio_group::examples());
    examples.extend(select::examples());
    examples.extend(switch::examples());
    examples.extend(progress_bar::examples());
    examples.extend(segment_display::examples());
    examples.extend(status::examples());
    examples.extend(card::examples());
    examples.extend(collapsible_section::examples());
    examples.extend(icon_tile::examples());
    examples.extend(keyboard_key::examples());
    examples
}
