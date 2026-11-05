//! `TreeView` with `branches compact` (`tree-view.tsx`, `tree-view.css`): a small, flat file
//! tree, every row led by a texture key where curved guides land.
//!
//! Rows (`.pw-tree__row`) are 28px ghost lines, `4px 6px` in, 6px between an optional leading
//! tile, the label (12px secondary, cut with an ellipsis), and an optional trailing slot (11px
//! muted, pulled 4px into the line) at its end; hover fills `--color-surface-hover` and the
//! selected row `--color-surface-selected` with primary ink. Each row sits 8px after its key
//! (`.pw-tree__key`), 16px and centered on the row's 14px middle: a round texture face with a
//! 10px `+` while the branch is closed and `−` while it is open, or a 6px texture dot on a leaf.
//! Rows stack 2px apart, and a branch's children sit 2px under it, 22px in. An open key drops
//! a 1px muted rail, and each child draws an elbow from it into its own key, rounded on
//! `--radius-item`; the children clip the elbows at their top, so the rail reads as one line.
//!
//! A key toggles its branch; a row selects (`expandOnClick={false}`). Branches open and close
//! like the shared collapse (`createBranchGrowth`): the children grow from 0 to their height on
//! the moderate spring and fold on its quicker exit tween, easing in, fading in with the spring
//! and out early, easing out. The first paint snaps. The tree takes the focus on a press, and
//! the arrow keys move the selection as Ark's tree does: Up and Down to the row before or after,
//! Right opens a branch or steps into it, Left closes it or steps out to the parent, Home and
//! End to the first and last rows. The host owns which branches are open and which row is
//! selected; nothing animates while nothing changes.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, ElementId, Entity, FocusHandle, IntoElement, KeyDownEvent,
    SharedString, Window, div, prelude::*, px,
};

use crate::icons::{Icon, icon_colored};
use crate::material::{Edge, control_drop, edge, face};
use crate::motion::{Curve, SpringTier, Tween};
use crate::theme::{Ease, Theme};

/// `--pw-tree-key`, `--pw-tree-indent`, `--pw-tree-row-mid`, and `--pw-tree-gap` of
/// `.pw-tree--compact`.
const KEY: f32 = 16.0;
const INDENT: f32 = 22.0;
const ROW_MID: f32 = 14.0;
const GAP: f32 = 2.0;
/// `min-height: 28px` on a compact row.
const ROW: f32 = 28.0;
/// The key's glyph and a leaf's dot.
const KEY_ICON: f32 = 10.0;
const DOT: f32 = 6.0;
/// Each elbow runs this far up, so the children's clip cuts it at their top.
const ELBOW_REACH: f32 = 4000.0;
const EASE_IN: Ease = Ease(0.42, 0.0, 1.0, 1.0);
const EASE_OUT: Ease = Ease(0.0, 0.0, 0.58, 1.0);

/// One row and the rows under it.
pub struct TreeNode {
    pub id: SharedString,
    pub label: SharedString,
    pub icon: Option<AnyElement>,
    pub trailing: Option<AnyElement>,
    /// The trailing slot shows only while the row's line is hovered.
    pub trailing_on_hover: bool,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            trailing: None,
            trailing_on_hover: false,
            children: Vec::new(),
        }
    }

    /// The tile before the label.
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    /// The slot at the row's end, such as an add button shown while the row is hovered.
    pub fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }

    /// The trailing slot only while the row's line is hovered, as Structure's Add child shows
    /// (`.pw-tree__line:hover .pw-structure-line__add`).
    pub fn trailing_on_hover(mut self) -> Self {
        self.trailing_on_hover = true;
        self
    }

    pub fn children(mut self, children: Vec<TreeNode>) -> Self {
        self.children = children;
        self
    }
}

/// A row picked: its id.
pub type SelectHandler = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;
/// A branch toggled: its id, and whether it opens.
pub type ToggleHandler = Rc<dyn Fn(SharedString, bool, &mut Window, &mut App)>;

/// One branch's growth: how far open its children are, and how much they show.
struct Growth {
    open: bool,
    progress: Tween,
    fade: Tween,
}

/// What a tree keeps between frames: its focus and each branch's growth.
pub struct TreeState {
    focus: FocusHandle,
    growth: HashMap<SharedString, Growth>,
}

impl TreeState {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            growth: HashMap::new(),
        }
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// Follows the branches the host opens: a branch seen for the first time snaps.
    fn follow(&mut self, id: &SharedString, open: bool) {
        let growth = self.growth.entry(id.clone()).or_insert_with(|| {
            let at = if open { 1.0 } else { 0.0 };
            Growth {
                open,
                progress: Tween::new(at),
                fade: Tween::new(at),
            }
        });
        if growth.open == open {
            return;
        }
        growth.open = open;
        let tier = SpringTier::Moderate;
        if open {
            growth
                .progress
                .animate_to(1.0, tier.duration(), tier.curve());
            growth.fade.animate_to(1.0, tier.duration(), tier.curve());
        } else {
            growth
                .progress
                .animate_to(0.0, tier.exit_duration(), Curve::Ease(EASE_IN));
            growth
                .fade
                .animate_to(0.0, tier.exit_duration(), Curve::Ease(EASE_OUT));
        }
    }

    fn animating(&self) -> bool {
        self.growth
            .values()
            .any(|growth| growth.progress.is_animating() || growth.fade.is_animating())
    }
}

pub struct TreeView {
    id: SharedString,
    nodes: Vec<TreeNode>,
    expanded: HashSet<SharedString>,
    selected: Option<SharedString>,
    on_select: Option<SelectHandler>,
    on_toggle: Option<ToggleHandler>,
}

/// A tree of `nodes`, its state kept under `id`.
pub fn tree_view(id: impl Into<SharedString>, nodes: Vec<TreeNode>) -> TreeView {
    TreeView {
        id: id.into(),
        nodes,
        expanded: HashSet::new(),
        selected: None,
        on_select: None,
        on_toggle: None,
    }
}

/// A row the keys step through: its id, its parent's, and whether it is an open or a closed
/// branch.
struct Visible {
    id: SharedString,
    parent: Option<SharedString>,
    branch: bool,
    open: bool,
    first_child: Option<SharedString>,
}

fn visible_rows(
    nodes: &[TreeNode],
    parent: Option<&SharedString>,
    expanded: &HashSet<SharedString>,
    out: &mut Vec<Visible>,
) {
    for node in nodes {
        let branch = !node.children.is_empty();
        let open = branch && expanded.contains(&node.id);
        out.push(Visible {
            id: node.id.clone(),
            parent: parent.cloned(),
            branch,
            open,
            first_child: node.children.first().map(|child| child.id.clone()),
        });
        if open {
            visible_rows(&node.children, Some(&node.id), expanded, out);
        }
    }
}

/// The height of `children` laid out open as `expanded` holds them.
fn children_height(children: &[TreeNode], expanded: &HashSet<SharedString>) -> f32 {
    let rows: f32 = children
        .iter()
        .map(|child| {
            let open = !child.children.is_empty() && expanded.contains(&child.id);
            ROW + if open {
                GAP + children_height(&child.children, expanded)
            } else {
                0.0
            }
        })
        .sum();
    rows + GAP * children.len().saturating_sub(1) as f32
}

/// What drawing a row needs besides the node.
struct Draw<'a> {
    state: &'a Entity<TreeState>,
    expanded: &'a HashSet<SharedString>,
    selected: Option<&'a SharedString>,
    on_select: Option<SelectHandler>,
    on_toggle: Option<ToggleHandler>,
    theme: Theme,
}

impl TreeView {
    /// The open branches.
    pub fn expanded(mut self, ids: impl IntoIterator<Item = SharedString>) -> Self {
        self.expanded = ids.into_iter().collect();
        self
    }

    pub fn selected(mut self, id: Option<SharedString>) -> Self {
        self.selected = id;
        self
    }

    /// A press on a row, or a key that moves the selection.
    pub fn on_select(mut self, handler: SelectHandler) -> Self {
        self.on_select = Some(handler);
        self
    }

    /// A press on a key, or Right and Left on a branch.
    pub fn on_toggle(mut self, handler: ToggleHandler) -> Self {
        self.on_toggle = Some(handler);
        self
    }

    pub fn build(self, window: &mut Window, cx: &mut App) -> AnyElement {
        let theme = *Theme::global(cx);
        let state = window.use_keyed_state(
            ElementId::Name(format!("{}-tree", self.id).into()),
            cx,
            |_, cx| TreeState::new(cx),
        );
        // Each branch follows the host's open set before it is drawn.
        fn follow(nodes: &[TreeNode], expanded: &HashSet<SharedString>, state: &mut TreeState) {
            for node in nodes.iter().filter(|node| !node.children.is_empty()) {
                state.follow(&node.id, expanded.contains(&node.id));
                follow(&node.children, expanded, state);
            }
        }
        state.update(cx, |state, _| follow(&self.nodes, &self.expanded, state));
        if state.read(cx).animating() {
            window.request_animation_frame();
        }
        let mut order = Vec::new();
        visible_rows(&self.nodes, None, &self.expanded, &mut order);
        let focus = state.read(cx).focus.clone();
        let keys = {
            let (on_select, on_toggle) = (self.on_select.clone(), self.on_toggle.clone());
            let selected = self.selected.clone();
            move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                let at = selected
                    .as_ref()
                    .and_then(|id| order.iter().position(|row| &row.id == id));
                let select = |id: SharedString, window: &mut Window, cx: &mut App| {
                    if let Some(on_select) = &on_select {
                        on_select(id, window, cx);
                    }
                };
                let toggle = |id: SharedString, open: bool, window: &mut Window, cx: &mut App| {
                    if let Some(on_toggle) = &on_toggle {
                        on_toggle(id, open, window, cx);
                    }
                };
                let key = event.keystroke.key.as_str();
                if event.keystroke.modifiers.modified() {
                    return;
                }
                let target = match (key, at) {
                    ("down", Some(at)) => order.get(at + 1).map(|row| row.id.clone()),
                    ("down", None) | ("home", _) => order.first().map(|row| row.id.clone()),
                    ("up", Some(at)) => at.checked_sub(1).map(|at| order[at].id.clone()),
                    ("up", None) | ("end", _) => order.last().map(|row| row.id.clone()),
                    ("right", Some(at)) => {
                        let row = &order[at];
                        if row.branch && !row.open {
                            cx.stop_propagation();
                            return toggle(row.id.clone(), true, window, cx);
                        }
                        row.first_child.clone().filter(|_| row.open)
                    }
                    ("left", Some(at)) => {
                        let row = &order[at];
                        if row.open {
                            cx.stop_propagation();
                            return toggle(row.id.clone(), false, window, cx);
                        }
                        row.parent.clone()
                    }
                    _ => return,
                };
                cx.stop_propagation();
                if let Some(target) = target {
                    select(target, window, cx);
                }
            }
        };
        let draw = Draw {
            state: &state,
            expanded: &self.expanded,
            selected: self.selected.as_ref(),
            on_select: self.on_select.clone(),
            on_toggle: self.on_toggle.clone(),
            theme,
        };
        let rows: Vec<AnyElement> = self
            .nodes
            .into_iter()
            .map(|node| draw.node(node, false, cx))
            .collect();
        div()
            .id(ElementId::Name(self.id.clone()))
            .track_focus(&focus)
            .flex()
            .flex_col()
            .gap(px(GAP))
            .min_w(px(0.0))
            .text_size(px(12.0))
            .text_color(theme.color_text_secondary)
            .on_key_down(keys)
            .children(rows)
            .into_any_element()
    }
}

impl Draw<'_> {
    fn node(&self, node: TreeNode, nested: bool, cx: &App) -> AnyElement {
        let theme = &self.theme;
        let branch = !node.children.is_empty();
        let open = branch && self.expanded.contains(&node.id);
        let selected = self.selected == Some(&node.id);
        let id = node.id.clone();
        let key = if branch {
            let toggle = self.on_toggle.clone();
            let toggled = id.clone();
            let face_set = theme.face_neutral;
            div()
                .id(SharedString::from(format!("tree-key-{id}")))
                .relative()
                .flex_none()
                .size(px(KEY))
                .mt(px(ROW_MID - KEY / 2.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .cursor_pointer()
                .bg(face(face_set.top, face_set.bottom))
                .shadow(control_drop(theme))
                .hover(move |key| key.bg(face(face_set.hover_top, face_set.hover_bottom)))
                .active(move |key| key.bg(face(face_set.active_top, face_set.active_bottom)))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    if let Some(toggle) = &toggle {
                        toggle(toggled.clone(), !open, window, cx);
                    }
                })
                .child(icon_colored(
                    if open {
                        Icon::MinusSignIcon
                    } else {
                        Icon::Plus
                    },
                    KEY_ICON,
                    face_set.ink,
                ))
                .child(edge(Edge::Strong, KEY / 2.0, theme))
                .into_any_element()
        } else {
            let face_set = theme.face_neutral;
            div()
                .flex_none()
                .size(px(KEY))
                .mt(px(ROW_MID - KEY / 2.0))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .relative()
                        .size(px(DOT))
                        .rounded_full()
                        .bg(face(face_set.top, face_set.bottom))
                        .shadow(control_drop(theme))
                        .child(edge(Edge::Strong, DOT / 2.0, theme)),
                )
                .into_any_element()
        };
        let select = self.on_select.clone();
        let picked = id.clone();
        let focus = self.state.read(cx).focus.clone();
        let line_group: SharedString = format!("tree-line-{id}").into();
        let hover_only = node.trailing_on_hover;
        let row = div()
            .id(SharedString::from(format!("tree-row-{id}")))
            .flex_1()
            .min_w(px(0.0))
            .h(px(ROW))
            .px(px(6.0))
            .py(px(4.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .rounded(px(theme.radius_item))
            .cursor_pointer()
            .when(selected, |row| {
                row.bg(theme.color_surface_selected)
                    .text_color(theme.color_text_primary)
            })
            .when(!selected, |row| {
                let hover = theme.color_surface_hover;
                row.hover(move |row| row.bg(hover))
            })
            .on_click(move |_, window, cx| {
                window.focus(&focus);
                if let Some(select) = &select {
                    select(picked.clone(), window, cx);
                }
            })
            .children(node.icon)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(node.label),
            )
            .children(node.trailing.map(|trailing| {
                let group = line_group.clone();
                div()
                    .flex_none()
                    .ml_auto()
                    .my(px(-4.0))
                    .text_size(px(11.0))
                    .text_color(theme.color_text_muted)
                    .when(hover_only, |slot| {
                        slot.invisible().group_hover(group, |slot| slot.visible())
                    })
                    .child(trailing)
            }));
        let line = div()
            .group(line_group)
            .relative()
            .flex()
            .items_start()
            .gap(px(8.0))
            .min_w(px(0.0))
            .child(key)
            .child(row)
            // The open key's rail down to the children's top.
            .when(open, |line| {
                line.child(
                    div()
                        .absolute()
                        .left(px(KEY / 2.0 - 0.5))
                        .top(px(ROW_MID + KEY / 2.0))
                        .w(px(1.0))
                        .h(px(ROW - ROW_MID - KEY / 2.0 + GAP))
                        .bg(theme.color_text_muted),
                )
            })
            // The elbow from the parent's rail into this key.
            .when(nested, |line| {
                line.child(
                    div()
                        .absolute()
                        .left(px(KEY / 2.0 - 0.5 - INDENT))
                        .top(px(ROW_MID - ELBOW_REACH))
                        .w(px(INDENT - KEY / 2.0 + 0.5))
                        .h(px(ELBOW_REACH))
                        .border_l_1()
                        .border_b_1()
                        .border_color(theme.color_text_muted)
                        .rounded_bl(px(theme.radius_item)),
                )
            });
        if !branch {
            return line.into_any_element();
        }
        let (progress, fade, moving) = self
            .state
            .read(cx)
            .growth
            .get(&id)
            .map(|growth| {
                (
                    growth.progress.value(),
                    growth.fade.value(),
                    growth.progress.is_animating(),
                )
            })
            .unwrap_or((if open { 1.0 } else { 0.0 }, 1.0, false));
        let shows = open || moving;
        let mut branch_el = div().flex().flex_col().min_w(px(0.0)).child(line);
        if shows {
            let full = children_height(&node.children, self.expanded);
            let rows: Vec<AnyElement> = node
                .children
                .into_iter()
                .map(|child| self.node(child, true, cx))
                .collect();
            branch_el = branch_el.child(
                div()
                    .relative()
                    .mt(px(GAP * progress.clamp(0.0, 1.0)))
                    .pl(px(INDENT))
                    .flex()
                    .flex_col()
                    .gap(px(GAP))
                    .overflow_hidden()
                    .when(moving, |children| {
                        children.h(px((full * progress).max(0.0)))
                    })
                    .opacity(fade.clamp(0.0, 1.0))
                    .children(rows),
            );
        }
        branch_el.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use gpui::{Modifiers, MouseButton, TestAppContext, VisualTestContext, point, size};

    use super::*;
    use crate::test_support::{draw, open};

    fn nodes() -> Vec<TreeNode> {
        vec![TreeNode::new("project", "Project").children(vec![
                TreeNode::new("billing", "Billing")
                    .children(vec![TreeNode::new("invoices", "Invoices")]),
                TreeNode::new("identity", "Identity"),
            ])]
    }

    #[test]
    fn the_keys_step_through_the_open_rows_only() {
        let expanded: HashSet<SharedString> = ["project".into()].into_iter().collect();
        let mut rows = Vec::new();
        visible_rows(&nodes(), None, &expanded, &mut rows);
        let ids: Vec<&str> = rows.iter().map(|row| row.id.as_ref()).collect();
        assert_eq!(ids, ["project", "billing", "identity"]);
        assert_eq!(
            rows[1].parent.as_ref().map(|id| id.as_ref()),
            Some("project")
        );
        assert!(rows[1].branch && !rows[1].open);
        // Two rows 2px apart; Billing open adds its gap and its row.
        assert_eq!(
            children_height(&nodes()[0].children, &expanded),
            2.0 * ROW + GAP
        );
        let both: HashSet<SharedString> =
            ["project".into(), "billing".into()].into_iter().collect();
        assert_eq!(
            children_height(&nodes()[0].children, &both),
            3.0 * ROW + 2.0 * GAP
        );
    }

    struct Host {
        expanded: Rc<RefCell<Vec<SharedString>>>,
        selected: Rc<RefCell<Option<SharedString>>>,
    }

    impl Host {
        fn picked(&self) -> Option<String> {
            self.selected.borrow().as_ref().map(|id| id.to_string())
        }
    }

    fn opened(cx: &mut TestAppContext) -> (Host, VisualTestContext) {
        let expanded = Rc::new(RefCell::new(vec![SharedString::from("project")]));
        let selected = Rc::new(RefCell::new(Some(SharedString::from("project"))));
        let host = Host {
            expanded: expanded.clone(),
            selected: selected.clone(),
        };
        let cx = open(cx, size(400.0, 300.0), move |window, cx| {
            let (toggled, picked) = (expanded.clone(), selected.clone());
            tree_view("test", nodes())
                .expanded(expanded.borrow().clone())
                .selected(selected.borrow().clone())
                .on_toggle(Rc::new(move |id, open, window, _| {
                    let mut ids = toggled.borrow_mut();
                    ids.retain(|kept| *kept != id);
                    if open {
                        ids.push(id);
                    }
                    window.refresh();
                }))
                .on_select(Rc::new(move |id, window, _| {
                    *picked.borrow_mut() = Some(id);
                    window.refresh();
                }))
                .build(window, cx)
        });
        (host, cx)
    }

    #[gpui::test]
    fn arrows_walk_the_rows_and_open_and_close_branches(cx: &mut TestAppContext) {
        let (host, mut cx) = opened(cx);
        draw(&mut cx);
        // A press on the first row gives the tree the focus.
        cx.simulate_mouse_down(
            point(px(60.0), px(14.0)),
            MouseButton::Left,
            Modifiers::none(),
        );
        cx.simulate_mouse_up(
            point(px(60.0), px(14.0)),
            MouseButton::Left,
            Modifiers::none(),
        );
        draw(&mut cx);
        cx.simulate_keystrokes("down");
        draw(&mut cx);
        assert_eq!(host.picked().as_deref(), Some("billing"));
        cx.simulate_keystrokes("right");
        draw(&mut cx);
        assert!(host.expanded.borrow().iter().any(|id| id == "billing"));
        cx.simulate_keystrokes("right");
        draw(&mut cx);
        assert_eq!(host.picked().as_deref(), Some("invoices"));
        cx.simulate_keystrokes("left");
        draw(&mut cx);
        assert_eq!(host.picked().as_deref(), Some("billing"));
        cx.simulate_keystrokes("left");
        draw(&mut cx);
        assert!(!host.expanded.borrow().iter().any(|id| id == "billing"));
        cx.simulate_keystrokes("end");
        draw(&mut cx);
        assert_eq!(host.picked().as_deref(), Some("identity"));
    }
}
