//! The compositions every Polterware app repeats: the page toolbar, the rail page and its
//! rail, sidebar rows, collapsible sections, empty pages, the main surface, and the frames of
//! Commander and the Composer, and the Composer's property pills.

pub mod collapsible_section;
pub mod commander;
pub mod composer;
pub mod jump_to_end;
#[cfg(feature = "bots")]
pub mod page_empty;
pub mod property_pills;
pub mod rail;
pub mod rail_page;
pub mod scroll_fade;
pub mod settings_dialog;
pub mod settings_section;
pub mod shell;
pub mod sidebar_item;
pub mod toolbar;
pub mod usage_cards;

pub use collapsible_section::{CollapsibleSection, SectionBody, SectionClip};
pub use commander::{
    COMMANDER_GROUP_GAP, COMMANDER_HEADING_H, COMMANDER_HEIGHT, COMMANDER_INSET,
    COMMANDER_LIST_BOTTOM, COMMANDER_LIST_TOP, COMMANDER_ROW_H, COMMANDER_WIDTH, Commander,
    CommanderEvent, CommanderGroup, CommanderItem, CommanderMark, commander_panel, commander_size,
    fuzzy_score,
};
pub use composer::{
    COMPOSER_MAX_HEIGHT, COMPOSER_WIDTH, ComposerCard, composer_actions, composer_card,
    composer_field, composer_footer,
};
pub use jump_to_end::{JUMP_TO_END_REST, jump_to_end};
#[cfg(feature = "bots")]
pub use page_empty::{MascotMood, list_empty, page_empty, page_empty_with};
pub use property_pills::{
    PICKER_WIDTH, Property, PropertyOption, PropertyPicker, PropertyPickerEvent, PropertyPills,
    PropertyPillsEvent,
};
pub use rail::{rail_card, rail_group};
pub use rail_page::{
    PageWidth, RailGroup, RailPresence, WIDE_MEASURE, rail_page_body, rail_page_body_under,
    rail_page_header, rail_properties, rail_property, rail_section, rail_toggle_label,
};
pub use scroll_fade::{
    FadeEdges, FadeScroll, SCROLL_FADE, fade_item, fade_scroll, toolbar_trail, trailing_toolbar,
};
pub use settings_dialog::{
    SETTINGS_HEIGHT, SETTINGS_WIDTH, SettingsDialog, SettingsNavItem, SettingsSelect,
    settings_dialog, settings_size,
};
pub use settings_section::{SETTINGS_SECTION_GAP, SettingsSection, settings_section};
pub use shell::{AnchorSlot, main_surface, modal_layer, window_anchor, window_layer};
pub use sidebar_item::{SidebarItem, SidebarSectionLabel};
pub use toolbar::{toolbar, toolbar_center};
pub use usage_cards::{StatTile, UsageCard, UsageMeter, card_grid, grid_columns};
