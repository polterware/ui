//! The compositions every Polterware app repeats: the page toolbar, the rail page and its
//! rail, a surface's archive, sidebar rows, collapsible sections, empty pages, the main surface, the frames of
//! Commander and the Composer and the Composer whole, the Composer's property pills,
//! confirmations, settings rows, context menus, the metrics dashboard's cards, readings, and
//! packed grid, a board of cards that drag between columns, a quick look at a record, a title
//! bar of tabs, a compact tree, prompt cards for an agent, and a start
//! page.

pub mod archived_list;
pub mod board;
pub mod chat_bubble;
pub mod collapsible_section;
pub mod commander;
pub mod composer;
pub mod composer_form;
pub mod confirm;
pub mod context_menu;
pub mod dashboard;
pub mod dashboard_grid;
pub mod detail_panel;
pub mod dialog;
pub mod document_page;
pub mod jump_to_end;
pub mod message_composer;
pub mod morph;
#[cfg(feature = "bots")]
pub mod page_empty;
pub mod peek;
pub mod prompt_cards;
pub mod property_pills;
pub mod rail;
pub mod rail_page;
pub mod resizable_panel;
pub mod scroll_fade;
pub mod selection_toolbar;
pub mod settings_dialog;
pub mod settings_group;
pub mod settings_row;
pub mod settings_section;
pub mod shell;
pub mod sidebar_item;
#[cfg(feature = "bots")]
pub mod start_page;
pub mod tag_picker;
pub mod title_tabs;
pub mod toolbar;
pub mod tree_view;
pub mod usage_cards;

pub use archived_list::{ArchivedItem, ArchivedList, ArchivedListEvent};
pub use board::{
    BOARD_COLUMN_MIN, BOARD_GAP, Board, BoardCard, BoardColumn, BoardMove, BoardState, MoveHandler,
    board, column_at,
};
pub use chat_bubble::{ChatBubble, collapsed_text};
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
pub use composer_form::{Composer, ComposerDraft, ComposerEvent, ComposerSpec};
pub use confirm::{CancelAction, ConfirmAction, ConfirmDialog};
pub use context_menu::{ContextMenu, menu_height, place as place_menu};
pub use dashboard::{DashboardCard, Level, Meter, PickRow, Reading, headline, meters, readings};
pub use dashboard_grid::{
    CardBox, CardGrip, Cell, DASHBOARD_CELL_MIN, DASHBOARD_DWELL, DASHBOARD_GAP, DASHBOARD_ROW,
    DashboardGrid, DashboardLayout, DropSide, GridCard, GridState, LayoutHandler, Rect,
    auto_arrange, card_box, card_width, cell_at, dashboard_columns, dashboard_grid, dashboard_grip,
    dashboard_measure, drop_side, free_cells, free_spot, pack_cards, packed_rows, place_with_push,
};
pub use detail_panel::{
    DETAIL_PANEL_INSET, DETAIL_PANEL_WIDTH, DetailPanel, detail_numbered_list, detail_properties,
    detail_property, detail_section, detail_section_with_aside, detail_text,
};
pub use dialog::{
    DIALOG_SCALE_FROM, Dialog, DialogPose, DialogPresence, DialogSize, Field, dialog, field,
    form_stack,
};
pub use document_page::{
    DOCUMENT_COLUMN, DocumentPage, RAIL_BUTTON_RADIUS, document_footer, document_group,
    document_group_body, document_group_with, document_title, rail_select,
};
pub use jump_to_end::{JUMP_TO_END_REST, jump_to_end};
pub use message_composer::{
    MESSAGE_COMPOSER_MAX_HEIGHT, MESSAGE_COMPOSER_RADIUS, MessageComposer, message_composer,
    message_input,
};
pub use morph::{MORPH_WIDTH, Morph, MorphAlign};
#[cfg(feature = "bots")]
pub use page_empty::{MascotMood, list_empty, page_empty, page_empty_action, page_empty_with};
pub use peek::{Peek, peek, peek_chip, peek_section};
pub use prompt_cards::{
    PickHandler as PromptPickHandler, PromptCard, prompt_card_grid, template_card_grid,
};
pub use property_pills::{
    PICKER_WIDTH, Property, PropertyOption, PropertyPicker, PropertyPickerEvent, PropertyPills,
    PropertyPillsEvent, PropertyTokenEvent,
};
pub use rail::{rail_card, rail_group};
pub use rail_page::{
    PageWidth, RailGroup, RailPresence, RailState, WIDE_MEASURE, rail_page_body,
    rail_page_body_flush, rail_page_body_under, rail_page_end_pad, rail_page_header,
    rail_properties, rail_property, rail_section, rail_toggle_label,
};
pub use resizable_panel::{COLLAPSE_THRESHOLD, PanelSide, ResizablePanel, resizable_panel};
pub use scroll_fade::{
    FadeEdges, FadeScroll, SCROLL_FADE, fade_item, fade_scroll, toolbar_trail, trailing_toolbar,
};
pub use selection_toolbar::selection_toolbar;
pub use settings_dialog::{
    SETTINGS_HEIGHT, SETTINGS_WIDTH, SettingsDialog, SettingsNavItem, SettingsSearchEntry,
    SettingsSearchOpen, SettingsSelect, scroll_to_settings_section, search_settings,
    settings_anchor, settings_dialog, settings_size,
};
pub use settings_group::{SettingsGroup, settings_group, settings_group_gap};
pub use settings_row::{SettingsRow, settings_row};
pub use settings_section::{SETTINGS_SECTION_GAP, SettingsSection, settings_section};
pub use shell::{AnchorSlot, main_surface, modal_layer, window_anchor, window_layer};
pub use sidebar_item::{SidebarItem, SidebarSectionLabel};
#[cfg(feature = "bots")]
pub use start_page::{MoreHandler, StartRow, start_page, start_section};
pub use tag_picker::{TagPicker, TagPickerEvent, TagRow, shown_tags, tag_rows};
pub use title_tabs::{
    TITLEBAR_TRAFFIC_INSET, TitleTab, title_add, title_card, title_divider, title_spacer,
    title_strip,
};
pub use toolbar::{toolbar, toolbar_center};
pub use tree_view::{
    SelectHandler as TreeSelectHandler, ToggleHandler as TreeToggleHandler, TreeNode, TreeState,
    TreeView, tree_view,
};
pub use usage_cards::{StatTile, UsageCard, UsageMeter, card_grid, grid_columns};
