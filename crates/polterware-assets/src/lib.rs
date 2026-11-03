//! The assets every Polterware app embeds: the Hugeicons Free marks (stroke rounded, MIT)
//! and the Inter and JetBrains Mono faces (SIL OFL), with an [`AssetSource`] for the icons.
//!
//! The icons are the design system's set and stroke (1.5), under its names. `assets/icons`
//! holds the SVGs, generated from `@hugeicons/core-free-icons` with the names and definitions
//! of the design system's `hugeicons.tsx`. The licenses sit beside the files.

use std::borrow::Cow;

use gpui::{App, AssetSource, SharedString};

macro_rules! icons {
    ($($name:ident => $key:literal,)*) => {
        /// Every icon the design system exports, by its design-system name.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Icon {
            $($name,)*
        }

        impl Icon {
            pub const ALL: &'static [Icon] = &[$(Icon::$name,)*];

            /// The asset key, the kebab-case file name in `assets/icons`.
            pub fn key(self) -> &'static str {
                match self {
                    $(Icon::$name => $key,)*
                }
            }
        }

        pub fn svg_for_key(key: &str) -> Option<&'static str> {
            match key {
                $($key => Some(include_str!(concat!("../assets/icons/", $key, ".svg"))),)*
                _ => None,
            }
        }
    };
}

icons! {
    Activity => "activity",
    Activity03Icon => "activity03-icon",
    AddTeam => "add-team",
    Archive => "archive",
    ArrowDataTransferVertical => "arrow-data-transfer-vertical",
    ArrowDown => "arrow-down",
    ArrowUp => "arrow-up",
    Bell => "bell",
    Blocks => "blocks",
    Book03Icon => "book03-icon",
    BookOpen => "book-open",
    BookOpen02Icon => "book-open02-icon",
    Bookshelf01Icon => "bookshelf01-icon",
    Bookmark => "bookmark",
    Blockchain01Icon => "blockchain01-icon",
    Bot => "bot",
    Box => "box",
    Boxes => "boxes",
    Briefcase06Icon => "briefcase06-icon",
    Building2 => "building2",
    CalendarClock => "calendar-clock",
    ChartRelationshipIcon => "chart-relationship-icon",
    XIcon => "x-icon",
    Check => "check",
    ChevronDown => "chevron-down",
    ChevronLeft => "chevron-left",
    ChevronRight => "chevron-right",
    Clock => "clock",
    Cloud => "cloud",
    CloudCog => "cloud-cog",
    Cog => "cog",
    CollapseIcon => "collapse-icon",
    ConnectIcon => "connect-icon",
    Copy => "copy",
    Cpu => "cpu",
    CreditCard => "credit-card",
    Database => "database",
    Delete => "delete",
    Download => "download",
    ExternalLink => "external-link",
    Eye => "eye",
    ExpandIcon => "expand-icon",
    File => "file",
    FileDiff => "file-diff",
    FileText => "file-text",
    FileUp => "file-up",
    FloppyDisk => "floppy-disk",
    FingerAccessIcon => "finger-access-icon",
    Folder => "folder",
    FolderKanban => "folder-kanban",
    Gauge => "gauge",
    GitBranch => "git-branch",
    GitCommit => "git-commit",
    Github => "github",
    AlertDiamond => "alert-diamond",
    GitCompare => "git-compare",
    GitMerge => "git-merge",
    GitPullRequest => "git-pull-request",
    Globe => "globe",
    Globe2 => "globe2",
    Hammer => "hammer",
    Hand => "hand",
    HardDrive => "hard-drive",
    HelpCircle => "help-circle",
    InformationCircle => "information-circle",
    History => "history",
    Home01Icon => "home01-icon",
    House => "house",
    Inbox => "inbox",
    Keyboard => "keyboard",
    KeyRound => "key-round",
    Laptop => "laptop",
    Layers => "layers",
    LayoutDashboard => "layout-dashboard",
    LibraryBig => "library-big",
    Link => "link",
    Link2 => "link2",
    ListChecks => "list-checks",
    ListFilter => "list-filter",
    ListOrdered => "list-ordered",
    MapPin => "map-pin",
    Maximize2 => "maximize2",
    MemoryStick => "memory-stick",
    Monitor => "monitor",
    MousePointer2 => "mouse-pointer2",
    Network => "network",
    Package => "package",
    Pause => "pause",
    Pencil => "pencil",
    Pin => "pin",
    PinOff => "pin-off",
    Play => "play",
    Plug => "plug",
    Plus => "plus",
    RadioButtonIcon => "radio-button-icon",
    Redo2 => "redo2",
    RefreshCw => "refresh-cw",
    Route => "route",
    Scale => "scale",
    SchemeIcon => "scheme-icon",
    ScrollText => "scroll-text",
    Search => "search",
    Server => "server",
    Settings => "settings",
    Shield => "shield",
    ShieldCheck => "shield-check",
    Smartphone => "smartphone",
    Square => "square",
    SquareChevronRight => "square-chevron-right",
    SquareTerminal => "square-terminal",
    StickyNote => "sticky-note",
    Sun => "sun",
    Moon => "moon",
    Stop => "stop",
    Tag => "tag",
    Terminal => "terminal",
    ThumbsDownIcon => "thumbs-down-icon",
    ThumbsUpIcon => "thumbs-up-icon",
    Undo2 => "undo2",
    User => "user",
    Warehouse => "warehouse",
    Webhook => "webhook",
    Workflow => "workflow",
    Wrench01Icon => "wrench01-icon",
    X => "x",
    Zap => "zap",
    MoreHorizontal => "more-horizontal",
    TextBoldIcon => "text-bold-icon",
    TextItalicIcon => "text-italic-icon",
    Heading02Icon => "heading02-icon",
    LeftToRightListBulletIcon => "left-to-right-list-bullet-icon",
    LeftToRightListNumberIcon => "left-to-right-list-number-icon",
    SourceCodeIcon => "source-code-icon",
    QuoteDownIcon => "quote-down-icon",
    MinusSignIcon => "minus-sign-icon",
    SidebarLeftIcon => "sidebar-left-icon",
    SidebarRightIcon => "sidebar-right-icon",
    DragDropVerticalIcon => "drag-drop-vertical-icon",
    Menu01Icon => "menu01-icon",
    ArrowRight02Icon => "arrow-right02-icon",
    ArrowUpRight01Icon => "arrow-up-right01-icon",
    AiBrain01Icon => "ai-brain01-icon",
    AiSearchIcon => "ai-search-icon",
    FirewallIcon => "firewall-icon",
    GridTableIcon => "grid-table-icon",
    InfinityIcon => "infinity-icon",
    Key01Icon => "key01-icon",
    McpServerIcon => "mcp-server-icon",
    RoboticIcon => "robotic-icon",
    SlidersHorizontalIcon => "sliders-horizontal-icon",
    SquareLock02Icon => "square-lock02-icon",
}

/// The names the views used before the icons followed the design system.
#[allow(non_upper_case_globals)]
impl Icon {
    pub const Close: Icon = Icon::X;
    pub const Back: Icon = Icon::ChevronLeft;
    pub const More: Icon = Icon::MoreHorizontal;
    pub const Info: Icon = Icon::InformationCircle;
    pub const Sliders: Icon = Icon::SlidersHorizontalIcon;

    pub fn asset_path(self) -> SharedString {
        SharedString::from(format!("icons/{}.svg", self.key()))
    }

    /// The asset drawn with another stroke width, as `IconTile` sets `stroke-width` on its mark.
    pub fn asset_path_stroked(self, stroke: f32) -> SharedString {
        SharedString::from(format!("icons/{}@{}.svg", self.key(), stroke))
    }
}

/// The embedded icons, at `icons/<key>.svg`. `icons/<key>@<stroke>.svg` asks for the mark
/// with its 1.5 strokes redrawn at that width, as `IconTile` sets `stroke-width` on its mark.
#[derive(Clone, Copy, Debug, Default)]
pub struct IconSource;

impl AssetSource for IconSource {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let Some(key) = path
            .strip_prefix("icons/")
            .and_then(|key| key.strip_suffix(".svg"))
        else {
            return Ok(None);
        };
        if let Some((key, stroke)) = key.split_once('@') {
            return Ok(svg_for_key(key).map(|svg| {
                let width = format!("stroke-width=\"{stroke}\"");
                Cow::Owned(svg.replace("stroke-width=\"1.5\"", &width).into_bytes())
            }));
        }
        Ok(svg_for_key(key).map(|svg| Cow::Borrowed(svg.as_bytes())))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let prefix = path.trim_matches('/');
        Ok(Icon::ALL
            .iter()
            .map(|icon| icon.asset_path())
            .filter(|item| {
                prefix.is_empty() || prefix == "icons" || item.as_ref().starts_with(prefix)
            })
            .collect())
    }
}

/// Inter (the sans) and JetBrains Mono, bundled so text renders on machines that do not
/// have them installed.
pub const FONTS: [&[u8]; 8] = [
    include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Inter-Bold.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-SemiBold.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"),
];

/// Registers the bundled fonts with the text system.
pub fn install_fonts(cx: &App) {
    let fonts: Vec<Cow<'static, [u8]>> = FONTS.iter().map(|bytes| Cow::Borrowed(*bytes)).collect();
    if let Err(err) = cx.text_system().add_fonts(fonts) {
        eprintln!("fonts: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::{Icon, IconSource, svg_for_key};
    use gpui::AssetSource;

    #[test]
    fn every_icon_is_an_embedded_svg() {
        let assets = IconSource;
        for icon in Icon::ALL {
            let bytes = assets
                .load(&icon.asset_path())
                .unwrap()
                .unwrap_or_else(|| panic!("{}", icon.key()));
            assert!(bytes.starts_with(b"<svg"), "{}", icon.key());
            assert!(bytes.windows(6).any(|w| w == b"</svg>"), "{}", icon.key());
            assert!(svg_for_key(icon.key()).is_some());
        }
        assert!(assets.load("icons/missing.svg").unwrap().is_none());
        assert!(assets.load("segments/0.svg").unwrap().is_none());
        let stroked = assets
            .load(&Icon::Bot.asset_path_stroked(2.5))
            .unwrap()
            .unwrap();
        let stroked = String::from_utf8(stroked.into_owned()).unwrap();
        assert!(
            stroked.contains("stroke-width=\"2.5\"") && !stroked.contains("stroke-width=\"1.5\"")
        );
    }
}
