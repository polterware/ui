//! `tree-view.tsx` with `branches compact`: a small tree whose keys open and close the
//! branches, curved guides from each open key into its children, and a selection the arrow
//! keys move once the tree has the focus.

use std::rc::Rc;

use gpui::{AnyElement, App, Context, SharedString, Window, div, prelude::*, px};
use polterware_ui::patterns::{TreeNode, tree_view};
use polterware_ui::{Icon, IconTile};

use super::super::Example;

struct Tree {
    expanded: Vec<SharedString>,
    selected: Option<SharedString>,
}

/// Structure's 20px level tile (`LevelTile`).
fn tile(icon: Icon) -> IconTile {
    IconTile::new(icon).size(20.0, 12.0)
}

fn leaf(id: &'static str, label: &'static str) -> TreeNode {
    TreeNode::new(id, label).icon(tile(Icon::Blockchain01Icon))
}

fn nodes() -> Vec<TreeNode> {
    vec![
        TreeNode::new("project", "Polterware")
            .icon(tile(Icon::House))
            .children(vec![
                TreeNode::new("billing", "Billing")
                    .icon(tile(Icon::Boxes))
                    .children(vec![
                        leaf("invoices", "Invoices"),
                        leaf("payouts", "Payouts"),
                        leaf("ledger", "Ledger"),
                    ]),
                TreeNode::new("identity", "Identity")
                    .icon(tile(Icon::Boxes))
                    .children(vec![leaf("sessions", "Sessions"), leaf("tokens", "Tokens")]),
                leaf("updater", "Updater"),
            ]),
    ]
}

fn compact(window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
    let state = window.use_keyed_state("tree-compact", cx, |_, _: &mut Context<Tree>| Tree {
        expanded: vec!["project".into(), "billing".into()],
        selected: Some("invoices".into()),
    });
    let (expanded, selected) = {
        let tree = state.read(cx);
        (tree.expanded.clone(), tree.selected.clone())
    };
    let (toggles, picks) = (state.clone(), state.clone());
    let tree = tree_view("gallery-tree", nodes())
        .expanded(expanded)
        .selected(selected)
        .on_toggle(Rc::new(move |id, open, _, cx| {
            toggles.update(cx, |tree, cx| {
                tree.expanded.retain(|kept| *kept != id);
                if open {
                    tree.expanded.push(id);
                }
                cx.notify();
            });
        }))
        .on_select(Rc::new(move |id, _, cx| {
            picks.update(cx, |tree, cx| {
                tree.selected = Some(id);
                cx.notify();
            });
        }))
        .build(window, cx);
    vec![div().w(px(280.0)).child(tree).into_any_element()]
}

pub fn examples() -> Vec<Example> {
    vec![Example {
        slug: "tree-view",
        title: "Compact branches",
        height: 300.0,
        render: compact,
    }]
}
