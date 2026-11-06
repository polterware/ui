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

pub mod aim;
#[cfg(feature = "bots")]
pub mod bots;
pub mod filter;
pub mod patterns;
pub mod sort;

pub mod button;
pub mod canvas;
pub mod card;
pub mod checkbox;
pub mod chip;
pub mod count_badge;
pub mod data_table;
pub mod echo_ring;
pub mod ellipsis;
pub mod filter_popover;
pub mod hang;
pub mod hover_card;
pub mod icon_button;
pub mod icon_tile;
pub mod icons;
pub mod kbd;
pub mod list_popover;
pub mod markdown;
pub mod markdown_tools;
pub mod material;
pub mod menu;
pub mod number_stepper;
pub mod pointer;
pub mod popover;
pub mod progress_bar;
pub mod radio_group;
pub mod scroll;
pub mod segment_display;
pub mod segmented;
pub mod select;
pub mod skeleton;
pub mod spark_chart;
pub mod status_dot;
pub mod switch;
pub mod tabs;
pub mod terminal;
#[cfg(test)]
mod test_support;
pub mod text_input;
pub mod text_swap;
pub mod texture_group;
pub mod toast;
pub mod tooltip;
pub mod wrap;

pub use button::{Button, ButtonKind, ButtonSize, button};
pub use card::{card, texture_card, texture_compact};
pub use checkbox::{checkbox_mark, checkbox_mark_mixed};
pub use chip::{chip, chip_colors, chip_rounded};
pub use count_badge::{CountBadge, CountTone};
pub use data_table::{
    Align, Column, DataTable, GroupAdd, RowCx, SELECTION_WIDTH, SelectionChange, SortDirection,
    TableGroup, fade_text, mono, next_sort, primary_cell, primary_cell_ink, sans, table_height,
    tabular, text_width,
};
pub use echo_ring::{EchoColors, EchoRing, echo_color};
pub use ellipsis::{Ellipsis, ellipsis, fade};
pub use filter_popover::{FilterField, FilterPopover, FiltersChanged};
pub use hover_card::{HoverBuilder, HoverCard};
pub use icon_button::icon_button;
pub use icon_tile::{IconTile, TileTone};
pub use icons::{Icon, IconAssets, icon, icon_colored, icon_rotated, icon_stroked};
pub use kbd::kbd;
pub use list_popover::{ListPopover, ListRow, RowLeading};
pub use markdown::{MarkdownEditor, MarkdownEditorEvent, markdown_view};
pub use material::{
    Edge, control_drop, drop, edge, face, float_drop, floating_panel, raised_drop, raised_panel,
    rim_fade,
};
pub use menu::{Menu, MenuEntry, MenuItem};
pub use number_stepper::{NumberStepper, StepperChanged};
pub use patterns::ChatBubble;
pub use patterns::{
    CardGrip, Cell, CollapsibleSection, Commander, CommanderEvent, CommanderGroup, CommanderItem,
    CommanderMark, ConfirmDialog, ContextMenu, DashboardCard, DashboardGrid, DashboardLayout,
    Dialog, DialogPose, DialogPresence, DialogSize, Field, GridCard, Level, Meter, PageWidth,
    PickRow, Property, PropertyOption, PropertyPicker, PropertyPickerEvent, PropertyPills,
    PropertyPillsEvent, RailGroup, RailPresence, Reading, Rect, SectionBody, SectionClip,
    SettingsGroup, SettingsRow, SidebarItem, SidebarSectionLabel, StatTile, UsageCard, UsageMeter,
    WIDE_MEASURE, card_grid, card_width, dashboard_columns, dashboard_grid, dashboard_grip,
    dashboard_measure, dialog, field, form_stack, grid_columns, headline, meters, pack_cards,
    packed_rows, rail_card, rail_group, rail_page_body, rail_page_body_flush, rail_page_body_under,
    rail_page_end_pad, rail_page_header, rail_properties, rail_property, rail_section,
    rail_toggle_label, readings, settings_group, settings_row, toolbar, toolbar_center,
};
pub use patterns::{
    DOCUMENT_COLUMN, DocumentPage, RAIL_BUTTON_RADIUS, TagPicker, TagPickerEvent, document_footer,
    document_group, document_group_body, document_title,
};
#[cfg(feature = "bots")]
pub use patterns::{MascotMood, list_empty, page_empty, page_empty_action, page_empty_with};
pub use pointer::{
    PointerState, PointerTiming, track, track_with_hover, use_pointer, use_pointer_timed,
};
pub use popover::{Popover, PopoverPanel};
pub use progress_bar::{bar_legend, progress_bar, progress_bar_sm, stacked_bar};
pub use radio_group::{RadioOption, radio_group, radio_mark};
pub use scroll::{
    GUTTER as SCROLLBAR_GUTTER, ListScroll, ScrollAxis, ScrollGeometry, ScrollSource, ScrollState,
    ScrollbarState, page_content, page_fill, page_fill_bar, page_inner_height, page_under,
    page_under_inset, scroll_area, scroll_area_under, scrollbar, use_scroll,
};
pub use segment_display::{GlyphKind, segment_display, segment_glyph};
pub use segmented::SegmentedControl;
pub use select::select_trigger;
pub use skeleton::{
    PageSkeleton, SkeletonLayout, bone, line_bone, list_skeleton, page_skeleton, skeleton_tone,
    text_skeleton,
};
pub use spark_chart::{SPARK_HEIGHT, SparkChart, compact_tick, value_axis};
pub use status_dot::{status_dot, status_dot_label};
pub use switch::{Switch, switch_card, switch_row};
pub use tabs::Tabs;
pub use terminal::{TerminalEvent, TerminalPalette, TerminalSize, TerminalView};
pub use text_input::{FieldSize, TextInput};
pub use text_swap::{TextSwap, text_swap};
pub use texture_group::{TextureButtonGroup, TextureGroupItem};
pub use toast::{Toast, ToastTone, toast_lifetime, toast_stack};
pub use tooltip::{
    RichTooltipView, TooltipView, rich_tooltip, tooltip, tooltip_card, tooltip_heading,
    tooltip_note, tooltip_row,
};
pub use wrap::{SpanStyle, WrapText, wrap_text};

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
    // The tests lay out the classic scrollbar whatever the machine running them prefers.
    #[cfg(test)]
    window::scroller::set_overlay_scrollers(false);
    assets::install_fonts(cx);
    if !cx.has_global::<Theme>() {
        cx.set_global(Theme::dark());
    }
    register_key_bindings(cx);
}

/// The controls' key bindings; [`init`] installs them.
pub fn register_key_bindings(cx: &mut App) {
    text_input::register_key_bindings(cx);
    patterns::commander::register_key_bindings(cx);
    patterns::property_pills::register_key_bindings(cx);
    patterns::confirm::register_key_bindings(cx);
    menu::register_key_bindings(cx);
    terminal::register_key_bindings(cx);
    cx.bind_keys([
        KeyBinding::new("escape", Dismiss, Some("Popover")),
        KeyBinding::new("escape", Dismiss, Some("Menu")),
        KeyBinding::new("escape", Dismiss, Some("Composer")),
    ]);
}
