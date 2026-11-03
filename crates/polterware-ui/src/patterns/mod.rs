//! The compositions every Polterware app repeats: the page toolbar, the rail page and its
//! rail, sidebar rows, collapsible sections, empty pages, the main surface, and the frames of
//! Commander and the Composer.

pub mod collapsible_section;
pub mod commander;
pub mod composer;
#[cfg(feature = "bots")]
pub mod page_empty;
pub mod rail;
pub mod rail_page;
pub mod shell;
pub mod sidebar_item;
pub mod toolbar;

pub use collapsible_section::{CollapsibleSection, SectionBody, SectionClip};
pub use commander::{
    COMMANDER_HEIGHT, COMMANDER_INSET, COMMANDER_WIDTH, commander_panel, commander_size,
};
pub use composer::{COMPOSER_MAX_HEIGHT, COMPOSER_WIDTH, composer_actions, composer_card};
#[cfg(feature = "bots")]
pub use page_empty::{MascotMood, page_empty, page_empty_with};
pub use rail::{rail_card, rail_group};
pub use rail_page::{
    PageWidth, RailGroup, RailPresence, WIDE_MEASURE, rail_page_body, rail_page_header,
    rail_properties, rail_property, rail_toggle_label,
};
pub use shell::{main_surface, modal_layer};
pub use sidebar_item::{SidebarItem, SidebarSectionLabel};
pub use toolbar::toolbar;
