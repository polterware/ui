//! Polterware's shared GPUI controls and the compositions its apps repeat.
//!
//! Nothing here knows an app: what an app adds (its status words, its routes, its data)
//! comes in by parameter or callback. Call [`init`] once when the app starts.
#![allow(dead_code)]

pub use polterware_assets as assets;
pub use polterware_motion as motion;
pub use polterware_theme as theme;
pub use polterware_window as window;
pub(crate) use polterware_window::window_drag;

#[cfg(feature = "bots")]
pub mod bots;
pub mod filter;
pub mod patterns;
pub mod sort;

pub mod button;
pub mod card;
pub mod chip;
pub mod data_table;
pub mod ellipsis;
pub mod filter_popover;
pub mod hang;
pub mod hover_card;
pub mod icon_button;
pub mod icon_tile;
pub mod icons;
pub mod kbd;
pub mod material;
pub mod menu;
pub mod pointer;
pub mod popover;
pub mod progress_bar;
pub mod radio_group;
pub mod scroll;
pub mod segment_display;
pub mod select;
pub mod status_dot;
pub mod switch;
#[cfg(test)]
mod test_support;
pub mod text_input;
pub mod toast;
pub mod tooltip;

pub use button::{Button, ButtonKind, ButtonSize, button};
pub use card::card;
pub use chip::{chip, chip_colors};
pub use data_table::{
    Align, Column, DataTable, RowCx, SortDirection, fade_text, mono, next_sort, primary_cell, sans,
    table_height, tabular, text_width,
};
pub use ellipsis::{Ellipsis, ellipsis, fade};
pub use filter_popover::{FilterField, FilterPopover, FiltersChanged};
pub use hover_card::{HoverBuilder, HoverCard};
pub use icon_button::icon_button;
pub use icon_tile::{IconTile, TileTone};
pub use icons::{Icon, IconAssets, icon, icon_colored, icon_rotated, icon_stroked};
pub use kbd::kbd;
pub use material::{
    Edge, control_drop, drop, edge, face, float_drop, floating_panel, raised_drop, raised_panel,
    rim_fade,
};
pub use menu::{Menu, MenuEntry};
pub use patterns::{
    CollapsibleSection, PageWidth, RailGroup, RailPresence, SectionBody, SectionClip, SidebarItem,
    SidebarSectionLabel, WIDE_MEASURE, rail_card, rail_group, rail_page_body, rail_page_header,
    rail_properties, rail_property, rail_toggle_label, toolbar,
};
#[cfg(feature = "bots")]
pub use patterns::{MascotMood, page_empty, page_empty_with};
pub use pointer::{
    PointerState, PointerTiming, track, track_with_hover, use_pointer, use_pointer_timed,
};
pub use popover::{Popover, PopoverPanel};
pub use progress_bar::{progress_bar, progress_bar_sm};
pub use radio_group::{RadioOption, radio_group};
pub use scroll::{
    GUTTER as SCROLLBAR_GUTTER, ListScroll, ScrollAxis, ScrollGeometry, ScrollSource, ScrollState,
    ScrollbarState, page_content, page_fill, page_fill_bar, page_inner_height, scroll_area,
    scrollbar, use_scroll,
};
pub use segment_display::{GlyphKind, segment_display, segment_glyph};
pub use select::select_trigger;
pub use status_dot::status_dot;
pub use switch::{Switch, switch_row};
pub use text_input::{FieldSize, TextInput};
pub use toast::{Toast, ToastTone, toast_stack};
pub use tooltip::{TooltipView, tooltip};

use std::rc::Rc;

use gpui::{App, KeyBinding, Window, actions};
use theme::Theme;

/// A callback a control calls with the window and the app: a click, a dismissal.
pub type Handler = Rc<dyn Fn(&mut Window, &mut App)>;
/// A callback that also gets a value: a switch's state, a hover, an option's index, an id.
pub type ValueHandler<T> = Rc<dyn Fn(T, &mut Window, &mut App)>;

actions!(polterware_ui, [Dismiss]);

/// Sets the controls up for an app: the bundled fonts, the dark theme unless the app set its
/// own, and the key bindings of text fields, popovers, and menus. Install [`IconAssets`]
/// with `Application::with_assets`, and set the root size with
/// [`theme::set_root_rem`] and the status words with [`theme::set_status_tones`] before.
pub fn init(cx: &mut App) {
    assets::install_fonts(cx);
    if !cx.has_global::<Theme>() {
        cx.set_global(Theme::dark());
    }
    register_key_bindings(cx);
}

/// The controls' key bindings; [`init`] installs them.
pub fn register_key_bindings(cx: &mut App) {
    text_input::register_key_bindings(cx);
    cx.bind_keys([
        KeyBinding::new("escape", Dismiss, Some("Popover")),
        KeyBinding::new("escape", Dismiss, Some("Menu")),
    ]);
}
