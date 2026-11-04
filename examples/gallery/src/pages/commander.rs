//! Commander: the palette open on grouped commands, each on its tile with a hint, one of them
//! disabled; and a query that matches nothing.

use gpui::{AnyElement, App, Context, Entity, IntoElement, Window};
use polterware_ui::{Commander, CommanderGroup, CommanderItem, Icon, TileTone};

use super::super::Example;

fn groups() -> Vec<CommanderGroup> {
    vec![
        CommanderGroup::new("Navigation")
            .item(CommanderItem::new("nav.inbox", "Inbox").tile(Icon::Inbox, TileTone::Knowledge))
            .item(
                CommanderItem::new("nav.activity", "Activity")
                    .tile(Icon::Activity, TileTone::Neutral)
                    .hint("G A"),
            )
            .item(
                CommanderItem::new("nav.settings", "Settings")
                    .tile(Icon::Settings, TileTone::Neutral)
                    .hint("⌘,"),
            ),
        CommanderGroup::new("Actions")
            .item(
                CommanderItem::new("action.reload", "Reload")
                    .tile(Icon::RefreshCw, TileTone::Execution)
                    .hint("⌘R"),
            )
            .item(
                CommanderItem::new("action.sync", "Sync now")
                    .tile(Icon::Activity, TileTone::Execution)
                    .disabled(true),
            ),
    ]
}

fn held(
    id: &'static str,
    query: &'static str,
    window: &mut Window,
    cx: &mut App,
) -> Entity<Commander> {
    window.use_keyed_state(id, cx, move |window, cx: &mut Context<Commander>| {
        let mut commander = Commander::new(window, cx);
        commander.set_placeholder("Go to a page or run a command…", cx);
        commander.set_groups(groups(), cx);
        commander.set_open(true, window, cx);
        if !query.is_empty() {
            commander.set_query(query, cx);
        }
        commander
    })
}

fn default(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![held("commander-default", "", window, cx).into_any_element()]
}

fn empty(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    vec![held("commander-empty", "zzz", window, cx).into_any_element()]
}

pub fn examples() -> Vec<Example> {
    let example = |title, render| Example {
        slug: "commander",
        title,
        height: 470.0,
        render,
    };
    vec![
        example("Grouped commands", default),
        example("No matches", empty),
    ]
}
