//! The dashboard's grid, from the old Solid Vitals app's `.vitals-metrics`, `card-layout.ts`,
//! `card-reorder.ts`, and the design system's `drag-motion.ts`.
//!
//! Cards are whole cells (`w × h` of a 260px-minimum column and a 192px row, 12px apart),
//! packed in order like icons on a home screen, as `packOrder` does: anchored cards keep their
//! cell while it is free, and every other card takes the first free spot scanning left to
//! right, then top to bottom. A card learns its width from [`dashboard_measure`] and
//! [`card_width`] before it builds its body.
//!
//! Cards reorder as `createCardReorder` reorders them. A press on a card's header (or its
//! grip, [`CardGrip`]) that travels 4px lifts the card: it floats above every surface with
//! `0 20px 56px --color-shadow-float` and a 1px `--color-shadow-subtle` ring and follows the
//! pointer 1:1, and the free cells show as quiet card-shaped wells (`.vitals-drop-slot`:
//! `--color-surface-subtle` at `--radius-card`). Resting 120ms on a side of another card
//! places the held card on that side (the top side takes that card's spot), pushing whatever
//! it would cover straight down, so the grid grows downward; resting over a free cell,
//! below every card included, anchors it there. The side just dropped on stays quiet until
//! the pointer leaves that card, so cards never flip-flop. The cards that move glide to their
//! cells on the moderate spring (0.16s, FLIP), and on release the held card glides from where
//! it is drawn to its cell and lands. Near the scroller's top or bottom (48px, under its
//! toolbar) the page scrolls 14px a frame, once the pointer has been out of that zone. Escape
//! puts every card back where the drag found them. With its grip focused, the arrow keys
//! step a card over its neighbor, or one cell into free space. GPUI cannot scale an element,
//! so the held card's 1.03 lift is its shadow alone.
//!
//! It behaves like gpui-kit's `GridLayout`: the app owns the layout ([`DashboardLayout`]:
//! the order cards flow in and the cells they were dropped on) and the hidden cards, and saves
//! them; the grid packs and draws them, keeps the gesture's working layout while the pointer
//! is down, and reports the layout a drop, a keyboard step, or nothing at all (a cancel)
//! leaves through [`DashboardGrid::on_change`]. A press takes focus so Escape reaches the
//! grid, the dragged card paints last, above the cards it displaces, and every other card
//! eases into its cells.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, App, BoxShadow, Context, CursorStyle, DispatchPhase, ElementId, Entity,
    FocusHandle, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, ScrollHandle, SharedString, Window, canvas, deferred, div, point,
    prelude::*, px, svg,
};

use crate::icons::Icon;
use crate::material::drop;
use crate::motion::{SpringTier, Tween};
use crate::theme::Theme;

/// `CELL_MIN_W`: the narrowest a column gets.
pub const DASHBOARD_CELL_MIN: f32 = 260.0;
/// `ROW_H`: one row is one small card.
pub const DASHBOARD_ROW: f32 = 192.0;
/// `GAP`, both ways.
pub const DASHBOARD_GAP: f32 = 12.0;
/// How long the pointer rests over a target before the cards reflow (`DWELL_MS`).
pub const DASHBOARD_DWELL: Duration = Duration::from_millis(120);
/// Pointer travel before a press on a header becomes a drag (`DEAD_ZONE`).
const DEAD_ZONE: f32 = 4.0;
/// The scroller's edge zones and how far a frame scrolls in one (`EDGE`, `EDGE_STEP`).
const EDGE: f32 = 48.0;
const EDGE_STEP: f32 = 14.0;
/// Where a lifted card paints among the deferred layers: over the page and its menus' anchors,
/// under modals.
const LIFT_PRIORITY: usize = 50;

/// A card's place on the grid, in cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Cell {
    pub x: u16,
    pub y: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl Rect {
    fn overlaps(&self, other: &Rect) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }

    /// Whether the cell at `x`, `y` is one of this card's.
    pub fn contains(&self, x: i32, y: i32) -> bool {
        let (left, top) = (i32::from(self.x), i32::from(self.y));
        x >= left && x < left + i32::from(self.w) && y >= top && y < top + i32::from(self.h)
    }
}

/// The layout an app keeps for a grid: the order its cards flow in, and the cells cards were
/// dropped on, which they keep instead of flowing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DashboardLayout {
    pub order: Vec<SharedString>,
    pub anchors: HashMap<SharedString, Cell>,
}

impl DashboardLayout {
    /// `order`, with no anchors.
    pub fn new<S: Into<SharedString>>(order: impl IntoIterator<Item = S>) -> Self {
        Self {
            order: order.into_iter().map(Into::into).collect(),
            anchors: HashMap::new(),
        }
    }
}

/// The side of a card the pointer is over (`DropSide`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DropSide {
    Top,
    Bottom,
    Left,
    Right,
}

/// A card's box in pixels, inside the grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardBox {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl CardBox {
    fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.left
            && x <= self.left + self.width
            && y >= self.top
            && y <= self.top + self.height
    }
}

/// How many columns of at least [`DASHBOARD_CELL_MIN`] fit `width`: one at least (`colsFor`).
pub fn dashboard_columns(width: f32) -> u16 {
    if !(width.is_finite() && width > 0.0) {
        return 1;
    }
    ((width + DASHBOARD_GAP) / (DASHBOARD_CELL_MIN + DASHBOARD_GAP))
        .floor()
        .clamp(1.0, 64.0) as u16
}

/// One column's width on a grid `width` wide with `columns` columns.
fn cell_width(columns: u16, width: f32) -> f32 {
    let columns = columns.max(1);
    ((width - DASHBOARD_GAP * f32::from(columns - 1)) / f32::from(columns)).max(0.0)
}

/// How wide a card `span` columns wide draws on a grid `width` wide with `columns` columns.
pub fn card_width(span: u16, columns: u16, width: f32) -> f32 {
    let span = span.clamp(1, columns.max(1));
    (cell_width(columns, width) * f32::from(span) + DASHBOARD_GAP * f32::from(span - 1)).max(0.0)
}

/// The pixel box of `rect` inside the grid, with columns `cell` wide (`rectBox`).
pub fn card_box(rect: Rect, cell: f32) -> CardBox {
    CardBox {
        left: f32::from(rect.x) * (cell + DASHBOARD_GAP),
        top: f32::from(rect.y) * (DASHBOARD_ROW + DASHBOARD_GAP),
        width: f32::from(rect.w) * cell + f32::from(rect.w.saturating_sub(1)) * DASHBOARD_GAP,
        height: f32::from(rect.h) * DASHBOARD_ROW
            + f32::from(rect.h.saturating_sub(1)) * DASHBOARD_GAP,
    }
}

/// Packs `cards` (an id and its size in cells, `w` clamped to the columns) in order: anchored
/// cards first, each at its anchor (clamped into the columns) while the spot is free, then the
/// others in the first free spot left to right, top to bottom.
pub fn pack_cards<K: Clone + Eq + Hash>(
    cards: &[(K, u16, u16)],
    columns: u16,
    anchors: &HashMap<K, Cell>,
) -> HashMap<K, Rect> {
    let columns = columns.max(1);
    let mut placed: Vec<Rect> = Vec::new();
    let mut out = HashMap::new();
    let mut flowing = Vec::new();
    for (id, w, h) in cards {
        let (w, h) = ((*w).clamp(1, columns), (*h).max(1));
        let Some(anchor) = anchors.get(id) else {
            flowing.push((id.clone(), w, h));
            continue;
        };
        let rect = Rect {
            x: anchor.x.min(columns - w),
            y: anchor.y,
            w,
            h,
        };
        if placed.iter().any(|other| other.overlaps(&rect)) {
            flowing.push((id.clone(), w, h));
            continue;
        }
        placed.push(rect);
        out.insert(id.clone(), rect);
    }
    for (id, w, h) in flowing {
        let rect = first_free(&placed, w, h, columns);
        placed.push(rect);
        out.insert(id, rect);
    }
    out
}

fn first_free(placed: &[Rect], w: u16, h: u16, columns: u16) -> Rect {
    let mut y = 0;
    loop {
        let free = (0..=columns.saturating_sub(w))
            .map(|x| Rect { x, y, w, h })
            .find(|candidate| !placed.iter().any(|other| other.overlaps(candidate)));
        if let Some(rect) = free {
            return rect;
        }
        y += 1;
    }
}

/// The rows a packing takes.
pub fn packed_rows<K>(rects: &HashMap<K, Rect>) -> u16 {
    rects
        .values()
        .map(|rect| rect.y + rect.h)
        .max()
        .unwrap_or(0)
}

/// Which side of `card` the point `x`, `y` falls on: the axis where it sits furthest from
/// the center wins (`dropSide`).
pub fn drop_side(card: CardBox, x: f32, y: f32) -> DropSide {
    let half_w = if card.width > 0.0 {
        card.width / 2.0
    } else {
        1.0
    };
    let half_h = if card.height > 0.0 {
        card.height / 2.0
    } else {
        1.0
    };
    let u = (x - (card.left + card.width / 2.0)) / half_w;
    let v = (y - (card.top + card.height / 2.0)) / half_h;
    if u.abs() > v.abs() {
        if u < 0.0 {
            DropSide::Left
        } else {
            DropSide::Right
        }
    } else if v < 0.0 {
        DropSide::Top
    } else {
        DropSide::Bottom
    }
}

/// The card whose cells include `x`, `y`, if any (`cellAt`).
pub fn cell_at<K: Clone>(rects: &HashMap<K, Rect>, x: i32, y: i32) -> Option<K> {
    rects
        .iter()
        .find(|(_, rect)| rect.contains(x, y))
        .map(|(id, _)| id.clone())
}

/// Where `id`, `size` cells, would sit dropped with its top-left on `x`, `y`: the cell clamped
/// so the card fits the columns, or `None` when another card covers any of that spot
/// (`freeSpot`).
pub fn free_spot<K: Eq + Hash>(
    rects: &HashMap<K, Rect>,
    id: &K,
    size: (u16, u16),
    columns: u16,
    x: i32,
    y: i32,
) -> Option<Cell> {
    let rect = clamped(size, columns, x, y);
    let covered = rects
        .iter()
        .any(|(other, placed)| other != id && placed.overlaps(&rect));
    (!covered).then_some(Cell {
        x: rect.x,
        y: rect.y,
    })
}

fn clamped((w, h): (u16, u16), columns: u16, x: i32, y: i32) -> Rect {
    let columns = columns.max(1);
    let w = w.clamp(1, columns);
    let max_x = i32::from(columns - w);
    Rect {
        x: x.clamp(0, max_x) as u16,
        y: y.clamp(0, i32::from(u16::MAX)) as u16,
        w,
        h: h.max(1),
    }
}

/// The free cells while `id` is held, down to the bottom of the other cards plus the held
/// card's height, so there is always room below everything; the held card's own cells count
/// as free (`freeCells`).
pub fn free_cells<K: Clone + Eq + Hash>(
    rects: &HashMap<K, Rect>,
    id: &K,
    held_height: u16,
    columns: u16,
) -> Vec<Cell> {
    let bottom = rects
        .iter()
        .filter(|(other, _)| *other != id)
        .map(|(_, rect)| rect.y + rect.h)
        .max()
        .unwrap_or(0)
        + held_height;
    let mut cells = Vec::new();
    for y in 0..bottom {
        for x in 0..columns.max(1) {
            match cell_at(rects, i32::from(x), i32::from(y)) {
                Some(owner) if &owner != id => {}
                _ => cells.push(Cell { x, y }),
            }
        }
    }
    cells
}

/// Puts `id` (`size` cells) with its top-left on `x`, `y` and pushes every card it would cover
/// straight down, cascading. Returns anchors pinning every card of `rects` where it now
/// sits; the `anchors` of cards that are not in `rects` (hidden ones) are kept
/// (`placeWithPush`).
pub fn place_with_push<K: Clone + Eq + Hash>(
    rects: &HashMap<K, Rect>,
    id: &K,
    size: (u16, u16),
    columns: u16,
    (x, y): (i32, i32),
    anchors: &HashMap<K, Cell>,
) -> HashMap<K, Cell> {
    let held = clamped(size, columns, x, y);
    let mut others: Vec<(K, Rect)> = rects
        .iter()
        .filter(|(other, _)| *other != id)
        .map(|(other, rect)| (other.clone(), *rect))
        .collect();
    others.sort_by_key(|(_, rect)| (rect.y, rect.x));
    let mut placed = vec![held];
    let mut next = anchors.clone();
    next.insert(
        id.clone(),
        Cell {
            x: held.x,
            y: held.y,
        },
    );
    for (other, mut rect) in others {
        let mut moved = true;
        while moved {
            moved = false;
            for fixed in &placed {
                if fixed.overlaps(&rect) {
                    rect.y = fixed.y + fixed.h;
                    moved = true;
                }
            }
        }
        placed.push(rect);
        next.insert(
            other,
            Cell {
                x: rect.x,
                y: rect.y,
            },
        );
    }
    next
}

/// Empty cells with a card somewhere below them in the same column.
fn holes(placed: &[Rect], columns: u16) -> u32 {
    (0..columns)
        .map(|x| {
            let filled = placed
                .iter()
                .filter(|rect| x >= rect.x && x < rect.x + rect.w);
            let (bottom, covered) = filled.fold((0u32, 0u32), |(bottom, covered), rect| {
                (
                    bottom.max(u32::from(rect.y + rect.h)),
                    covered + u32::from(rect.h),
                )
            });
            bottom - covered
        })
        .sum()
}

struct Arrange {
    sizes: Vec<(u16, u16)>,
    columns: u16,
    floor: u16,
    used: Vec<bool>,
    order: Vec<usize>,
    placed: Vec<Rect>,
    best: Option<(Vec<usize>, u16, u32)>,
}

impl Arrange {
    /// Tries the unused cards in turn; returns whether nothing can beat the best found.
    fn visit(&mut self, bottom: u16) -> bool {
        if self
            .best
            .as_ref()
            .is_some_and(|(_, best, _)| bottom > *best)
        {
            return false;
        }
        if self.order.len() == self.sizes.len() {
            let holes = holes(&self.placed, self.columns);
            let better = self
                .best
                .as_ref()
                .is_none_or(|(_, best_bottom, best_holes)| {
                    bottom < *best_bottom || holes < *best_holes
                });
            if better {
                self.best = Some((self.order.clone(), bottom, holes));
            }
            return self
                .best
                .as_ref()
                .is_some_and(|(_, bottom, holes)| *bottom == self.floor && *holes == 0);
        }
        // Cards of one size lay out the same, so only the first unused one of each is tried.
        let mut tried = HashSet::new();
        for index in 0..self.sizes.len() {
            if self.used[index] || !tried.insert(self.sizes[index]) {
                continue;
            }
            let (w, h) = self.sizes[index];
            let rect = first_free(&self.placed, w, h, self.columns);
            self.used[index] = true;
            self.order.push(index);
            self.placed.push(rect);
            let done = self.visit(bottom.max(rect.y + rect.h));
            self.placed.pop();
            self.order.pop();
            self.used[index] = false;
            if done {
                return true;
            }
        }
        false
    }
}

/// The flow order that packs `cards` best on `columns`: the lowest grid, then the fewest
/// holes; orders closest to the given one are tried first and only a strictly better one
/// replaces it, so a tie keeps the given order (`autoArrange`).
pub fn auto_arrange<K: Clone>(cards: &[(K, u16, u16)], columns: u16) -> Vec<K> {
    let columns = columns.max(1);
    let sizes: Vec<(u16, u16)> = cards
        .iter()
        .map(|(_, w, h)| ((*w).clamp(1, columns), (*h).max(1)))
        .collect();
    let area: u32 = sizes
        .iter()
        .map(|(w, h)| u32::from(*w) * u32::from(*h))
        .sum();
    let mut search = Arrange {
        floor: area.div_ceil(u32::from(columns)) as u16,
        used: vec![false; sizes.len()],
        sizes,
        columns,
        order: Vec::new(),
        placed: Vec::new(),
        best: None,
    };
    search.visit(0);
    match search.best {
        Some((order, _, _)) => order
            .into_iter()
            .map(|index| cards[index].0.clone())
            .collect(),
        None => cards.iter().map(|(id, _, _)| id.clone()).collect(),
    }
}

/// A drop target: a side of a card, or a free cell.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    Side(SharedString, DropSide),
    Free(Cell),
}

/// An offset from a card's cell that springs back to it.
#[derive(Clone, Copy, Debug)]
struct Glide {
    x: Tween,
    y: Tween,
}

impl Glide {
    /// From `dx`, `dy` to the cell, on the moderate spring.
    fn from(dx: f32, dy: f32) -> Self {
        let moderate = SpringTier::Moderate;
        let mut x = Tween::new(dx);
        let mut y = Tween::new(dy);
        x.animate_to(0.0, moderate.duration(), moderate.curve());
        y.animate_to(0.0, moderate.duration(), moderate.curve());
        Self { x, y }
    }

    fn offset(&self) -> (f32, f32) {
        (self.x.value(), self.y.value())
    }

    fn moving(&self) -> bool {
        self.x.is_animating() || self.y.is_animating()
    }
}

/// A press on a card that has not travelled far enough to lift it.
struct Press {
    key: SharedString,
    at: Point<Pixels>,
}

struct Drag {
    key: SharedString,
    pointer: Point<Pixels>,
    /// The pointer's place on the card, from its top-left, when it lifted.
    grab: Point<Pixels>,
    /// The layout when it lifted, for Escape.
    start: DashboardLayout,
    /// The layout the drag has made so far.
    layout: DashboardLayout,
    /// The target the pointer rests on, and the dwell timer's turn.
    pending: Option<Target>,
    /// The side just dropped on, quiet until the pointer leaves its card.
    blocked: Option<(SharedString, Target)>,
    /// Each edge scrolls only once the pointer has been out of its zone.
    armed: (bool, bool),
    lift: Tween,
}

/// The card gliding from where it was dropped to its cell, still above the others.
struct Landing {
    key: SharedString,
    /// Where it was drawn when released, in window coordinates, until the next frame turns
    /// it into a glide.
    from: Option<Point<Pixels>>,
    glide: Option<Glide>,
    lift: Tween,
}

/// What a grid keeps between frames: what it measured and drew, the glides, and the gesture.
pub struct GridState {
    width: Option<f32>,
    /// The grid's top-left in the window, and the scroll offset then.
    origin: Option<(Point<Pixels>, Point<Pixels>)>,
    columns: u16,
    sizes: HashMap<SharedString, (u16, u16)>,
    rects: HashMap<SharedString, Rect>,
    /// The app's layout, as the last frame got it.
    layout: DashboardLayout,
    glides: HashMap<SharedString, Glide>,
    press: Option<Press>,
    drag: Option<Drag>,
    landing: Option<Landing>,
    dwell: u64,
    focus: FocusHandle,
    restore: Option<FocusHandle>,
    grips: HashMap<SharedString, FocusHandle>,
    /// The last move came from the keyboard, so a focused grip shows it.
    keyboard: bool,
    on_change: Option<LayoutHandler>,
    scroll: Option<(ScrollHandle, f32)>,
}

/// Hears the layout a drop or a keyboard step leaves.
pub type LayoutHandler = Rc<dyn Fn(DashboardLayout, &mut Window, &mut App)>;

impl GridState {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            width: None,
            origin: None,
            columns: 0,
            sizes: HashMap::new(),
            rects: HashMap::new(),
            layout: DashboardLayout::default(),
            glides: HashMap::new(),
            press: None,
            drag: None,
            landing: None,
            dwell: 0,
            focus: cx.focus_handle(),
            restore: None,
            grips: HashMap::new(),
            keyboard: false,
            on_change: None,
            scroll: None,
        }
    }

    /// The card being dragged.
    pub fn dragging(&self) -> Option<&SharedString> {
        self.drag.as_ref().map(|drag| &drag.key)
    }

    /// The layout drawn now: the gesture's while one runs.
    pub fn drawn_layout(&self) -> &DashboardLayout {
        self.drag.as_ref().map_or(&self.layout, |drag| &drag.layout)
    }

    fn cell(&self) -> Option<f32> {
        self.width.map(|width| cell_width(self.columns, width))
    }

    /// The grid's top-left in the window now, the scroll since it was measured included.
    fn grid_origin(&self) -> Option<Point<Pixels>> {
        let (origin, seen) = self.origin?;
        let now = self
            .scroll
            .as_ref()
            .map_or(seen, |(handle, _)| handle.offset());
        Some(origin + (now - seen))
    }

    /// The top-left of `key`'s cell in the window.
    fn home(&self, key: &SharedString) -> Option<Point<Pixels>> {
        let card = card_box(*self.rects.get(key)?, self.cell()?);
        Some(self.grid_origin()? + point(px(card.left), px(card.top)))
    }

    fn grip_focus(&mut self, key: &SharedString, cx: &mut App) -> FocusHandle {
        self.grips
            .entry(key.clone())
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone()
    }

    fn press(
        &mut self,
        key: SharedString,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.on_change.is_none() || self.drag.is_some() || !self.rects.contains_key(&key) {
            return;
        }
        self.keyboard = false;
        self.press = Some(Press { key, at });
        cx.notify();
        window.refresh();
    }

    fn pointer_moved(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.pressed_button != Some(MouseButton::Left) {
            // The release happened where no listener heard it.
            self.release(window, cx);
            return;
        }
        if let Some(press) = &self.press {
            let travel = event.position - press.at;
            let distance = f32::from(travel.x).hypot(f32::from(travel.y));
            if distance < DEAD_ZONE {
                return;
            }
            self.lift(window, cx);
        }
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        drag.pointer = event.position;
        self.retarget(window, cx);
        cx.notify();
        window.refresh();
    }

    /// The press became a drag.
    fn lift(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(press) = self.press.take() else {
            return;
        };
        let Some(home) = self.home(&press.key) else {
            return;
        };
        let fast = SpringTier::Fast;
        let mut lift = Tween::new(0.0);
        lift.animate_to(1.0, fast.duration(), fast.curve());
        self.glides.remove(&press.key);
        self.landing = None;
        self.drag = Some(Drag {
            key: press.key,
            pointer: press.at,
            grab: press.at - home,
            start: self.layout.clone(),
            layout: self.layout.clone(),
            pending: None,
            blocked: None,
            armed: (false, false),
            lift,
        });
        self.restore = window.focused(cx);
        self.focus.focus(window);
    }

    /// The target under the pointer, and the card it is over (`probe`).
    fn probe(&self) -> (Option<Target>, Option<SharedString>) {
        let (Some(drag), Some(origin), Some(cell)) = (&self.drag, self.grid_origin(), self.cell())
        else {
            return (None, None);
        };
        let at = drag.pointer - origin;
        let (x, y) = (f32::from(at.x), f32::from(at.y));
        // The held card's own cells count as free space.
        for (other, rect) in &self.rects {
            if other == &drag.key {
                continue;
            }
            let card = card_box(*rect, cell);
            if card.contains(x, y) {
                return (
                    Some(Target::Side(other.clone(), drop_side(card, x, y))),
                    Some(other.clone()),
                );
            }
        }
        // Off every card, the free cell under the held card's top-left.
        let drawn = at - drag.grab;
        let column = ((f32::from(drawn.x) + cell / 2.0) / (cell + DASHBOARD_GAP)).floor() as i32;
        let row = ((f32::from(drawn.y) + DASHBOARD_ROW / 2.0) / (DASHBOARD_ROW + DASHBOARD_GAP))
            .floor() as i32;
        let size = self.sizes.get(&drag.key).copied().unwrap_or((1, 1));
        let spot = (column >= 0 && column < i32::from(self.columns) && row >= 0)
            .then(|| free_spot(&self.rects, &drag.key, size, self.columns, column, row))
            .flatten();
        (spot.map(Target::Free), None)
    }

    /// Waits on the target under the pointer, once it changes.
    fn retarget(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (target, over) = self.probe();
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        if drag
            .blocked
            .as_ref()
            .is_some_and(|(card, _)| over.as_ref() != Some(card))
        {
            drag.blocked = None;
        }
        let next = target.filter(|target| {
            drag.blocked
                .as_ref()
                .is_none_or(|(_, blocked)| blocked != target)
        });
        if next == drag.pending {
            return;
        }
        drag.pending = next;
        self.dwell += 1;
        if drag.pending.is_none() {
            return;
        }
        let turn = self.dwell;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(DASHBOARD_DWELL).await;
            let _ = this.update_in(cx, |state, window, cx| {
                if state.dwell == turn {
                    state.commit(window, cx);
                }
            });
        })
        .detach();
    }

    /// The pointer rested on its target: the cards make room.
    fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.drag.as_mut().and_then(|drag| drag.pending.take()) else {
            return;
        };
        let Some(drag) = self.drag.as_ref() else {
            return;
        };
        let next = self.insert_at(&drag.key, &target, &drag.layout);
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        drag.blocked = match &target {
            Target::Side(card, _) => Some((card.clone(), target.clone())),
            Target::Free(_) => None,
        };
        if next != drag.layout {
            drag.layout = next;
            cx.notify();
            window.refresh();
        }
    }

    /// `layout` with `key` placed at `target` (`insertAt`).
    fn insert_at(
        &self,
        key: &SharedString,
        target: &Target,
        layout: &DashboardLayout,
    ) -> DashboardLayout {
        let mut next = layout.clone();
        match target {
            Target::Free(cell) => {
                next.anchors.insert(key.clone(), *cell);
            }
            Target::Side(card, side) => {
                let Some(rect) = self.rects.get(card) else {
                    return next;
                };
                let size = self.sizes.get(key).copied().unwrap_or((1, 1));
                let (x, y, w, h) = (
                    i32::from(rect.x),
                    i32::from(rect.y),
                    i32::from(rect.w),
                    i32::from(rect.h),
                );
                let at = match side {
                    DropSide::Top => (x, y),
                    DropSide::Bottom => (x, y + h),
                    DropSide::Left => (x - i32::from(size.0), y),
                    DropSide::Right => (x + w, y),
                };
                next.anchors =
                    place_with_push(&self.rects, key, size, self.columns, at, &layout.anchors);
            }
        }
        next
    }

    /// The pointer let go: a press is a click, a drag drops.
    fn release(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.press.take().is_some() {
            cx.notify();
            return;
        }
        self.finish(false, window, cx);
    }

    /// Ends the drag, keeping its layout or, `cancelled`, the one it started from.
    fn finish(&mut self, cancelled: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        self.dwell += 1;
        let moderate = SpringTier::Moderate;
        let mut lift = Tween::new(drag.lift.value());
        lift.animate_to(0.0, moderate.duration(), moderate.curve());
        self.landing = Some(Landing {
            key: drag.key.clone(),
            from: Some(drag.pointer - drag.grab),
            glide: None,
            lift,
        });
        if let Some(restore) = self.restore.take()
            && self.focus.is_focused(window)
        {
            restore.focus(window);
        }
        let layout = if cancelled { drag.start } else { drag.layout };
        if layout != self.layout {
            self.layout = layout.clone();
            if let Some(handler) = self.on_change.clone() {
                handler(layout, window, cx);
            }
        }
        cx.notify();
        window.refresh();
    }

    /// Moves `key` toward `side` from the keyboard (`onGripKeyDown`).
    fn step(
        &mut self,
        key: &SharedString,
        side: DropSide,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.drag.is_some() {
            return;
        }
        let Some(rect) = self.rects.get(key).copied() else {
            return;
        };
        let (x, y, w, h) = (
            i32::from(rect.x),
            i32::from(rect.y),
            i32::from(rect.w),
            i32::from(rect.h),
        );
        let (probe, step) = match side {
            DropSide::Left => ((x - 1, y), (x - 1, y)),
            DropSide::Right => ((x + w, y), (x + 1, y)),
            DropSide::Top => ((x, y - 1), (x, y - 1)),
            DropSide::Bottom => ((x, y + h), (x, y + 1)),
        };
        // Next to a card, step over it; otherwise slide one cell into free space.
        let mut target =
            cell_at(&self.rects, probe.0, probe.1).map(|card| Target::Side(card, side));
        if target.is_none() && step.0 >= 0 && step.0 + w <= i32::from(self.columns) && step.1 >= 0 {
            let size = self.sizes.get(key).copied().unwrap_or((1, 1));
            target =
                free_spot(&self.rects, key, size, self.columns, step.0, step.1).map(Target::Free);
        }
        let Some(target) = target else {
            return;
        };
        let next = self.insert_at(key, &target, &self.layout);
        if next == self.layout {
            return;
        }
        self.keyboard = true;
        self.layout = next.clone();
        if let Some(handler) = self.on_change.clone() {
            handler(next, window, cx);
        }
        cx.notify();
    }

    /// Scrolls while the held card is near the scroller's top or bottom (`pump`). Returns
    /// whether it scrolled.
    fn edge_scroll(&mut self) -> bool {
        let (Some(drag), Some((handle, inset))) = (self.drag.as_mut(), self.scroll.as_ref()) else {
            return false;
        };
        let bounds = handle.bounds();
        let top = f32::from(bounds.top()) + inset;
        let bottom = f32::from(bounds.bottom());
        let y = f32::from(drag.pointer.y);
        if y >= top + EDGE {
            drag.armed.0 = true;
        }
        if y <= bottom - EDGE {
            drag.armed.1 = true;
        }
        let direction = if drag.armed.0 && y < top + EDGE {
            -1.0
        } else if drag.armed.1 && y > bottom - EDGE {
            1.0
        } else {
            return false;
        };
        let offset = handle.offset();
        let max = f32::from(handle.max_offset().height);
        let next = (f32::from(offset.y) - direction * EDGE_STEP).clamp(-max, 0.0);
        if (next - f32::from(offset.y)).abs() < 0.01 {
            return false;
        }
        handle.set_offset(point(offset.x, px(next)));
        true
    }
}

/// A card on the grid: its key in the layout, its size in cells, and the card.
pub struct GridCard {
    key: SharedString,
    size: (u16, u16),
    element: AnyElement,
}

impl GridCard {
    pub fn new(key: impl Into<SharedString>, size: (u16, u16), card: impl IntoElement) -> Self {
        Self {
            key: key.into(),
            size,
            element: card.into_any_element(),
        }
    }
}

fn grid_state(id: ElementId, window: &mut Window, cx: &mut App) -> Entity<GridState> {
    window.use_keyed_state(id, cx, |_, cx| GridState::new(cx))
}

/// The column count and the width of the [`dashboard_grid`] named `id`, as its last frame
/// measured them (`guess` columns and no width before its first), so a card can lay its
/// body out for its width (see [`card_width`]).
pub fn dashboard_measure(
    id: impl Into<ElementId>,
    guess: u16,
    window: &mut Window,
    cx: &mut App,
) -> (u16, Option<f32>) {
    let width = grid_state(id.into(), window, cx).read(cx).width;
    (width.map_or(guess.max(1), dashboard_columns), width)
}

/// What lets a [`DashboardCard`](super::DashboardCard) of the [`dashboard_grid`] named `grid`
/// be dragged by its header: its grip (`.vitals-card-grip`).
#[derive(Clone)]
pub struct CardGrip {
    state: Entity<GridState>,
    key: SharedString,
    focus: FocusHandle,
}

/// The grip of the card `key` on the [`dashboard_grid`] named `grid`.
pub fn dashboard_grip(
    grid: impl Into<ElementId>,
    key: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut App,
) -> CardGrip {
    let key = key.into();
    let state = grid_state(grid.into(), window, cx);
    let focus = state.update(cx, |state, cx| state.grip_focus(&key, cx));
    CardGrip { state, key, focus }
}

impl CardGrip {
    /// A press on the header or the grip, which may become a drag.
    pub(crate) fn on_press(&self) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        let (state, key) = (self.state.clone(), self.key.clone());
        move |event, window, cx| {
            state.update(cx, |state, cx| {
                state.press(key.clone(), event.position, window, cx);
            });
        }
    }

    /// A release over the header or the grip, for a press no window listener heard end (it
    /// came before the next frame).
    pub(crate) fn on_release(&self) -> impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static {
        let state = self.state.clone();
        move |_, window, cx| {
            if state.read(cx).press.is_some() {
                state.update(cx, |state, cx| state.release(window, cx));
            }
        }
    }

    /// `.vitals-card-grip`: a 24px button at `--radius-item` with the 14px drag mark in muted
    /// ink, filled with `--color-surface-hover` in primary ink under the pointer and while it
    /// has the keyboard's focus.
    pub(crate) fn element(&self, window: &Window, cx: &App) -> AnyElement {
        let theme = *Theme::global(cx);
        let lit = self.state.read(cx).keyboard && self.focus.is_focused(window);
        let group = SharedString::from(format!("card-grip-{}", self.key));
        let (state, key) = (self.state.clone(), self.key.clone());
        div()
            .id("card-grip")
            .group(group.clone())
            .track_focus(&self.focus)
            .flex_none()
            .size(px(24.0))
            .rounded(px(theme.radius_item))
            .flex()
            .items_center()
            .justify_center()
            .cursor(CursorStyle::OpenHand)
            .when(lit, |grip| grip.bg(theme.color_surface_hover))
            .hover(|grip| grip.bg(theme.color_surface_hover))
            .on_mouse_down(MouseButton::Left, {
                let (press, focus) = (self.on_press(), self.focus.clone());
                move |event, window, cx| {
                    cx.stop_propagation();
                    // A button takes focus as it is pressed, so the arrow keys move its card.
                    focus.focus(window);
                    press(event, window, cx);
                }
            })
            .on_mouse_up(MouseButton::Left, self.on_release())
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                let side = match event.keystroke.key.as_str() {
                    "left" => DropSide::Left,
                    "right" => DropSide::Right,
                    "up" => DropSide::Top,
                    "down" => DropSide::Bottom,
                    _ => return,
                };
                cx.stop_propagation();
                state.update(cx, |state, cx| state.step(&key, side, window, cx));
            })
            .child(
                svg()
                    .path(Icon::DragDropVerticalIcon.asset_path())
                    .size(px(14.0))
                    .flex_none()
                    .text_color(if lit {
                        theme.color_text_primary
                    } else {
                        theme.color_text_muted
                    })
                    .group_hover(group, |icon| icon.text_color(theme.color_text_primary)),
            )
            .into_any_element()
    }
}

/// The grid named `id`: see the module's docs. Build it with [`DashboardGrid::build`] while
/// the view renders, where [`dashboard_measure`] and [`dashboard_grip`] find the same state.
pub fn dashboard_grid(id: impl Into<ElementId>) -> DashboardGrid {
    DashboardGrid {
        id: id.into(),
        cards: Vec::new(),
        layout: None,
        guess: 3,
        on_change: None,
        scroll: None,
    }
}

pub struct DashboardGrid {
    id: ElementId,
    cards: Vec<GridCard>,
    layout: Option<DashboardLayout>,
    guess: u16,
    on_change: Option<LayoutHandler>,
    scroll: Option<(ScrollHandle, f32)>,
}

/// What one card draws as this frame.
struct Placement {
    rect: Rect,
    offset: (f32, f32),
    /// The lift shadow's share while it floats over the others.
    lift: Option<f32>,
}

impl DashboardGrid {
    pub fn cards(mut self, cards: impl IntoIterator<Item = GridCard>) -> Self {
        self.cards.extend(cards);
        self
    }

    /// The order and anchors to pack by; the cards' own order, unanchored, by default. Cards
    /// the order does not name flow after the ones it does.
    pub fn layout(mut self, layout: &DashboardLayout) -> Self {
        self.layout = Some(layout.clone());
        self
    }

    /// The columns to lay out by before the first frame measures the width.
    pub fn guess(mut self, columns: u16) -> Self {
        self.guess = columns.max(1);
        self
    }

    /// Makes the cards with a grip draggable, and hears the layout each drop or keyboard
    /// step leaves.
    pub fn on_change(mut self, handler: LayoutHandler) -> Self {
        self.on_change = Some(handler);
        self
    }

    /// The scroller the grid sits in, which a held card scrolls near its edges, `top` the
    /// height its toolbar covers.
    pub fn scroll(mut self, handle: ScrollHandle, top: f32) -> Self {
        self.scroll = Some((handle, top));
        self
    }

    /// Lays the frame out: packs the cards, starts the glides of the ones that moved, and
    /// floats the held one.
    fn place(
        state: &mut GridState,
        cards: &[(SharedString, (u16, u16))],
        window: &mut Window,
    ) -> (HashMap<SharedString, Placement>, Vec<Cell>, u16, bool) {
        let columns = state.width.map_or(state.columns.max(1), dashboard_columns);
        let previous_columns = state.columns;
        state.columns = columns;
        let sizes: HashMap<SharedString, (u16, u16)> = cards
            .iter()
            .map(|(key, (w, h))| (key.clone(), ((*w).clamp(1, columns), (*h).max(1))))
            .collect();
        let layout = state.drawn_layout().clone();
        let mut order: Vec<SharedString> = layout
            .order
            .iter()
            .filter(|key| sizes.contains_key(*key))
            .cloned()
            .collect();
        for (key, _) in cards {
            if !order.contains(key) {
                order.push(key.clone());
            }
        }
        let packing: Vec<(SharedString, u16, u16)> = order
            .iter()
            .map(|key| {
                let (w, h) = sizes[key];
                (key.clone(), w, h)
            })
            .collect();
        let rects = pack_cards(&packing, columns, &layout.anchors);

        // The cards that moved glide from where they were drawn; a new width, new columns, or
        // a card shown or hidden lays everything out at once.
        let cell = state.cell();
        let same_cards = state.rects.len() == rects.len()
            && rects.keys().all(|key| state.rects.contains_key(key));
        let glide = same_cards && previous_columns == columns && cell.is_some();
        let held = state.dragging().cloned();
        let landing = state.landing.as_ref().map(|landing| landing.key.clone());
        if glide && let Some(cell) = cell {
            for (key, rect) in &rects {
                let Some(old) = state.rects.get(key) else {
                    continue;
                };
                if old == rect || Some(key) == held.as_ref() || Some(key) == landing.as_ref() {
                    continue;
                }
                let (dx, dy) = state
                    .glides
                    .get(key)
                    .map_or((0.0, 0.0), |glide| glide.offset());
                let (from, to) = (card_box(*old, cell), card_box(*rect, cell));
                state.glides.insert(
                    key.clone(),
                    Glide::from(from.left + dx - to.left, from.top + dy - to.top),
                );
            }
        } else {
            state.glides.clear();
        }
        state
            .glides
            .retain(|key, glide| glide.moving() && rects.contains_key(key));
        state.rects = rects;
        state.sizes = sizes;

        // The dropped card turns where it was drawn into a glide to its cell.
        if let Some(landing) = state.landing.as_mut()
            && let Some(from) = landing.from.take()
        {
            let home = state.rects.get(&landing.key).copied().and_then(|rect| {
                let card = card_box(rect, state.width.map(|w| cell_width(columns, w))?);
                let (origin, seen) = state.origin?;
                let now = state
                    .scroll
                    .as_ref()
                    .map_or(seen, |(handle, _)| handle.offset());
                Some(origin + (now - seen) + point(px(card.left), px(card.top)))
            });
            match home {
                Some(home) => {
                    let delta = from - home;
                    landing.glide = Some(Glide::from(f32::from(delta.x), f32::from(delta.y)));
                }
                None => state.landing = None,
            }
        }
        if state.landing.as_ref().is_some_and(|landing| {
            !landing.glide.is_some_and(|glide| glide.moving()) && !landing.lift.is_animating()
        }) {
            state.landing = None;
        }

        let mut placements: HashMap<SharedString, Placement> = state
            .rects
            .iter()
            .map(|(key, rect)| {
                let offset = state
                    .glides
                    .get(key)
                    .map_or((0.0, 0.0), |glide| glide.offset());
                (
                    key.clone(),
                    Placement {
                        rect: *rect,
                        offset,
                        lift: None,
                    },
                )
            })
            .collect();
        let mut slots = Vec::new();
        if let Some(drag) = &state.drag
            && let (Some(home), Some(placement)) =
                (state.home(&drag.key), placements.get_mut(&drag.key))
        {
            let at = drag.pointer - drag.grab - home;
            placement.offset = (f32::from(at.x), f32::from(at.y));
            placement.lift = Some(drag.lift.value());
            let held_height = state.sizes.get(&drag.key).map_or(1, |size| size.1);
            slots = free_cells(&state.rects, &drag.key, held_height, columns);
        }
        if let Some(landing) = &state.landing
            && let Some(placement) = placements.get_mut(&landing.key)
        {
            placement.offset = landing.glide.map_or((0.0, 0.0), |glide| glide.offset());
            placement.lift = Some(landing.lift.value());
        }
        let rows = packed_rows(&state.rects)
            .max(slots.iter().map(|cell| cell.y + 1).max().unwrap_or(0))
            .max(1);
        let animating = !state.glides.is_empty()
            || state.landing.is_some()
            || state
                .drag
                .as_ref()
                .is_some_and(|drag| drag.lift.is_animating());
        if animating {
            window.request_animation_frame();
        }
        (placements, slots, rows, state.drag.is_some())
    }

    pub fn build(self, window: &mut Window, cx: &mut App) -> AnyElement {
        let theme = *Theme::global(cx);
        let state = grid_state(self.id.clone(), window, cx);
        let keys: Vec<(SharedString, (u16, u16))> = self
            .cards
            .iter()
            .map(|card| (card.key.clone(), card.size))
            .collect();
        let layout = self
            .layout
            .unwrap_or_else(|| DashboardLayout::new(keys.iter().map(|(key, _)| key.clone())));
        let guess = self.guess;
        let (on_change, scroll) = (self.on_change, self.scroll);
        let (placements, slots, rows, dragging) = state.update(cx, |state, cx| {
            if state.columns == 0 {
                state.columns = guess;
            }
            state.on_change = on_change;
            state.scroll = scroll;
            if state.drag.is_none() {
                state.layout = layout;
            }
            if state.edge_scroll() {
                window.request_animation_frame();
                state.retarget(window, cx);
            }
            Self::place(state, &keys, window)
        });
        let columns = state.read(cx).columns;
        let height =
            f32::from(rows) * DASHBOARD_ROW + f32::from(rows.saturating_sub(1)) * DASHBOARD_GAP;
        let line = |at: u16| i16::try_from(at + 1).unwrap_or(i16::MAX);

        let probe_state = state.clone();
        let paint_state = state.clone();
        let probe = canvas(
            move |bounds, window, cx| {
                let seen = f32::from(bounds.size.width);
                let changed = probe_state.update(cx, |state, _| {
                    let scrolled = state
                        .scroll
                        .as_ref()
                        .map_or(Point::default(), |(handle, _)| handle.offset());
                    state.origin = Some((bounds.origin, scrolled));
                    let changed = state.width.is_none_or(|width| (width - seen).abs() > 0.5);
                    if changed {
                        state.width = Some(seen);
                    }
                    changed
                });
                if changed {
                    window.refresh();
                }
            },
            move |_, _, window, cx| {
                let (active, dragging) = {
                    let state = paint_state.read(cx);
                    (
                        state.press.is_some() || state.drag.is_some(),
                        state.drag.is_some(),
                    )
                };
                if dragging {
                    window.set_window_cursor_style(CursorStyle::ClosedHand);
                }
                if !active {
                    return;
                }
                let moves = paint_state.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let dragging = moves.read(cx).drag.is_some();
                    moves.update(cx, |state, cx| state.pointer_moved(event, window, cx));
                    if dragging {
                        cx.stop_propagation();
                    }
                });
                let ups = paint_state.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                        return;
                    }
                    let dragging = ups.read(cx).drag.is_some();
                    ups.update(cx, |state, cx| state.release(window, cx));
                    if dragging {
                        cx.stop_propagation();
                    }
                });
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .w_full()
        .h_0();

        // Explicit grid lines place each card, so the cells need no measured width.
        let mut grid = div()
            .w_full()
            .h(px(height))
            .grid()
            .grid_cols(columns)
            .grid_rows(rows)
            .gap(px(DASHBOARD_GAP));
        for cell in slots {
            grid = grid.child(
                div()
                    .col_start(line(cell.x))
                    .col_end(line(cell.x + 1))
                    .row_start(line(cell.y))
                    .row_end(line(cell.y + 1))
                    .rounded(px(theme.radius_card))
                    .bg(theme.color_surface_subtle),
            );
        }
        for card in self.cards {
            let Some(placement) = placements.get(&card.key) else {
                continue;
            };
            let Placement { rect, offset, lift } = *placement;
            let body = match lift {
                Some(share) => div()
                    .size_full()
                    .rounded(px(theme.radius_card))
                    .shadow(lift_shadow(share, &theme))
                    .child(card.element)
                    .into_any_element(),
                None => card.element,
            };
            let item = div()
                .id(ElementId::Name(card.key.clone()))
                .min_w(px(0.0))
                .min_h(px(0.0))
                .col_start(line(rect.x))
                .col_end(line(rect.x + rect.w))
                .row_start(line(rect.y))
                .row_end(line(rect.y + rect.h))
                .relative()
                .left(px(offset.0))
                .top(px(offset.1))
                .child(body);
            // The lifted card paints over every surface, unclipped, as the web app's top layer.
            grid = grid.child(match lift {
                Some(_) => deferred(item)
                    .with_priority(LIFT_PRIORITY)
                    .into_any_element(),
                None => item.into_any_element(),
            });
        }
        let cancel = state.clone();
        let focus = state.read(cx).focus.clone();
        div()
            .relative()
            .w_full()
            .track_focus(&focus)
            .when(dragging, |grid| grid.cursor(CursorStyle::ClosedHand))
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && cancel.read(cx).drag.is_some() {
                    cx.stop_propagation();
                    cancel.update(cx, |state, cx| state.finish(true, window, cx));
                }
            })
            .child(probe)
            .child(grid)
            .into_any_element()
    }
}

/// `.vitals-section[data-lifted]`'s shadow, at `share` of its strength.
fn lift_shadow(share: f32, theme: &Theme) -> Vec<BoxShadow> {
    let share = share.clamp(0.0, 1.0);
    let mut float = theme.color_shadow_float;
    float.a *= share;
    let mut ring = theme.color_shadow_subtle;
    ring.a *= share;
    vec![
        drop(20.0, 56.0, float),
        BoxShadow {
            color: ring.into(),
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: px(1.0),
        },
    ]
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use gpui::{Modifiers, TestAppContext, VisualTestContext, size};

    use super::*;
    use crate::patterns::DashboardCard;
    use crate::test_support::{draw, open};

    #[test]
    fn a_card_spans_its_cells_and_the_gaps_between() {
        // Three 260px columns with two 12px gaps.
        assert_eq!(card_width(1, 3, 804.0), 260.0);
        assert_eq!(card_width(2, 3, 804.0), 532.0);
        // Wider than the grid: as wide as it.
        assert_eq!(card_width(4, 3, 804.0), 804.0);
        let card = card_box(
            Rect {
                x: 1,
                y: 1,
                w: 2,
                h: 1,
            },
            260.0,
        );
        assert_eq!((card.left, card.top, card.width), (272.0, 204.0, 532.0));
    }

    const CARDS: [(&str, u16, u16); 8] = [
        ("cpu", 1, 2),
        ("memory", 1, 1),
        ("network", 1, 2),
        ("diskio", 2, 1),
        ("pressure", 1, 1),
        ("disk", 2, 1),
        ("thermal", 1, 1),
        ("processes", 2, 2),
    ];

    fn at(rects: &HashMap<&str, Rect>, id: &str) -> (u16, u16, u16, u16) {
        let rect = rects[id];
        (rect.x, rect.y, rect.w, rect.h)
    }

    fn no_overlap<K>(rects: &HashMap<K, Rect>) -> bool {
        rects
            .values()
            .all(|a| rects.values().filter(|b| *b != a).all(|b| !a.overlaps(b)))
    }

    #[test]
    fn cards_flow_into_the_first_free_spot() {
        let rects = pack_cards(&CARDS, 2, &HashMap::new());
        assert_eq!(at(&rects, "cpu"), (0, 0, 1, 2));
        assert_eq!(at(&rects, "memory"), (1, 0, 1, 1));
        assert_eq!(at(&rects, "network"), (1, 1, 1, 2));
        assert_eq!(at(&rects, "diskio"), (0, 3, 2, 1));
        // The hole under the CPU card takes the next card that fits.
        assert_eq!(at(&rects, "pressure"), (0, 2, 1, 1));
        assert_eq!(packed_rows(&rects), 8);
        assert!(no_overlap(&rects));
    }

    #[test]
    fn one_column_stacks_and_narrows_the_wide_cards() {
        let rects = pack_cards(&CARDS, 1, &HashMap::new());
        assert_eq!(at(&rects, "diskio"), (0, 5, 1, 1));
        assert_eq!(packed_rows(&rects), 11);
    }

    #[test]
    fn anchors_hold_their_spot_while_it_is_free() {
        let anchors = HashMap::from([
            ("thermal", Cell { x: 3, y: 0 }),
            ("memory", Cell { x: 3, y: 0 }),
        ]);
        let rects = pack_cards(&CARDS, 4, &anchors);
        assert_eq!(
            at(&rects, "memory"),
            (3, 0, 1, 1),
            "the first anchor wins the cell"
        );
        assert_ne!((rects["thermal"].x, rects["thermal"].y), (3, 0));
        // An anchor past the columns comes back inside them.
        let far = HashMap::from([("disk", Cell { x: 9, y: 0 })]);
        assert_eq!(at(&pack_cards(&CARDS, 3, &far), "disk"), (1, 0, 2, 1));
    }

    #[test]
    fn columns_fit_the_width() {
        assert_eq!(dashboard_columns(0.0), 1);
        assert_eq!(dashboard_columns(531.0), 1);
        assert_eq!(dashboard_columns(532.0), 2);
        assert_eq!(dashboard_columns(1100.0), 4);
    }

    #[test]
    fn the_side_is_the_axis_furthest_from_the_center() {
        let card = CardBox {
            left: 0.0,
            top: 0.0,
            width: 200.0,
            height: 100.0,
        };
        assert_eq!(drop_side(card, 10.0, 50.0), DropSide::Left);
        assert_eq!(drop_side(card, 190.0, 50.0), DropSide::Right);
        assert_eq!(drop_side(card, 100.0, 5.0), DropSide::Top);
        assert_eq!(drop_side(card, 100.0, 95.0), DropSide::Bottom);
        // Relative to the half sizes: 30% across beats 20% down.
        assert_eq!(drop_side(card, 130.0, 60.0), DropSide::Right);
    }

    fn rects_of(cards: &[(&'static str, u16, u16)], columns: u16) -> HashMap<&'static str, Rect> {
        pack_cards(cards, columns, &HashMap::new())
    }

    #[test]
    fn free_spots_and_cells_skip_the_other_cards() {
        let rects = rects_of(&CARDS, 2);
        assert_eq!(cell_at(&rects, 0, 1), Some("cpu"));
        assert_eq!(cell_at(&rects, 0, -1), None);
        // Memory may go back to its own cell, but not onto the CPU card.
        assert_eq!(
            free_spot(&rects, &"memory", (1, 1), 2, 1, 0),
            Some(Cell { x: 1, y: 0 })
        );
        assert_eq!(free_spot(&rects, &"memory", (1, 1), 2, 0, 0), None);
        // Past the right edge comes back in; below everything is free.
        assert_eq!(
            free_spot(&rects, &"memory", (1, 1), 2, 7, 8),
            Some(Cell { x: 1, y: 8 })
        );
        let free = free_cells(&rects, &"memory", 1, 2);
        assert!(free.contains(&Cell { x: 1, y: 0 }), "its own cell");
        assert!(!free.contains(&Cell { x: 0, y: 0 }));
        // The grid's eight rows plus one for the held card.
        assert!(free.iter().all(|cell| cell.y < 9));
        assert!(free.contains(&Cell { x: 0, y: 8 }));
    }

    #[test]
    fn a_drop_pushes_what_it_covers_straight_down() {
        let rects = rects_of(&CARDS, 2);
        // Processes (2 × 2) dropped on the top-left: everything under it moves down 2 rows,
        // cascading, and keeps its column.
        let anchors = place_with_push(&rects, &"processes", (2, 2), 2, (0, 0), &HashMap::new());
        let next = pack_cards(&CARDS, 2, &anchors);
        assert_eq!(at(&next, "processes"), (0, 0, 2, 2));
        assert_eq!(at(&next, "cpu"), (0, 2, 1, 2));
        assert_eq!(at(&next, "memory"), (1, 2, 1, 1));
        assert!(no_overlap(&next));
        assert_eq!(packed_rows(&next), 8, "the grid is as tall as before");
        // Every shown card is pinned, and a hidden card's anchor is kept.
        let hidden = HashMap::from([("gpu", Cell { x: 1, y: 9 })]);
        let kept = place_with_push(&rects, &"memory", (1, 1), 2, (-1, 0), &hidden);
        assert_eq!(kept["gpu"], Cell { x: 1, y: 9 });
        assert_eq!(kept["memory"], Cell { x: 0, y: 0 }, "clamped into the grid");
        assert_eq!(kept.len(), 9);
    }

    #[test]
    fn auto_arrange_packs_without_holes() {
        let order = auto_arrange(&CARDS, 2);
        let sized: Vec<(&str, u16, u16)> = order
            .iter()
            .map(|id| *CARDS.iter().find(|(card, _, _)| card == id).unwrap())
            .collect();
        let rects = pack_cards(&sized, 2, &HashMap::new());
        let placed: Vec<Rect> = rects.values().copied().collect();
        assert_eq!(holes(&placed, 2), 0);
        // 15 cells on 2 columns: 8 rows is the floor.
        assert_eq!(packed_rows(&rects), 8);
        // Cards that already pack perfectly keep their order.
        let even = [("a", 1, 1), ("b", 1, 1)];
        assert_eq!(auto_arrange(&even, 2), vec!["a", "b"]);
    }

    /// Two columns 300px wide with the 12px gap: column 1 starts at 312.
    const WIDTH: f32 = 612.0;

    struct Harness {
        layout: Rc<RefCell<DashboardLayout>>,
        changes: Rc<RefCell<Vec<DashboardLayout>>>,
        state: Rc<RefCell<Option<Entity<GridState>>>>,
        cx: VisualTestContext,
    }

    fn harness(cx: &mut TestAppContext) -> Harness {
        let layout = Rc::new(RefCell::new(DashboardLayout::new(["a", "b", "c"])));
        let changes = Rc::new(RefCell::new(Vec::new()));
        let state = Rc::new(RefCell::new(None));
        let (shared, heard, seen) = (layout.clone(), changes.clone(), state.clone());
        let cx = open(cx, size(WIDTH, 900.0), move |window, cx| {
            let cards = ["a", "b", "c"].map(|key| {
                let grip = dashboard_grip("grid", key, window, cx);
                *seen.borrow_mut() = Some(grip.state.clone());
                GridCard::new(
                    key,
                    (1, 1),
                    DashboardCard::new(SharedString::from(format!("card-{key}")), key).grip(grip),
                )
            });
            let (shared, heard) = (shared.clone(), heard.clone());
            let layout = shared.borrow().clone();
            div()
                .w(px(WIDTH))
                .child(
                    dashboard_grid("grid")
                        .guess(2)
                        .layout(&layout)
                        .cards(cards)
                        .on_change(Rc::new(move |next, _, _| {
                            *shared.borrow_mut() = next.clone();
                            heard.borrow_mut().push(next);
                        }))
                        .build(window, cx),
                )
                .into_any_element()
        });
        let mut harness = Harness {
            layout,
            changes,
            state,
            cx,
        };
        // The first frame measures the width; the second lays out by it.
        draw(&mut harness.cx);
        draw(&mut harness.cx);
        harness
    }

    fn dragged(h: &mut Harness) -> Option<SharedString> {
        let state = h.state.borrow().clone().expect("the grid drew");
        h.cx.update(|_, cx| state.read(cx).dragging().cloned())
    }

    fn rest(cx: &mut VisualTestContext) {
        cx.executor().advance_clock(DASHBOARD_DWELL * 2);
        cx.run_until_parked();
        draw(cx);
    }

    /// The header of the card in `column`, `row`.
    fn header(column: f32, row: f32) -> Point<Pixels> {
        point(
            px(80.0 + column * 312.0),
            px(20.0 + row * (DASHBOARD_ROW + DASHBOARD_GAP)),
        )
    }

    fn press(cx: &mut VisualTestContext, at: Point<Pixels>) {
        cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
        draw(cx);
    }

    fn move_to(cx: &mut VisualTestContext, to: Point<Pixels>) {
        cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
        draw(cx);
    }

    #[gpui::test]
    fn dropping_a_card_on_a_side_moves_it_there(cx: &mut TestAppContext) {
        let mut h = harness(cx);
        // a (0, 0), b (1, 0), c (0, 1). Lift c and rest it on b's bottom half.
        press(&mut h.cx, header(0.0, 1.0));
        move_to(&mut h.cx, header(0.0, 1.0) + point(px(2.0), px(0.0)));
        assert_eq!(
            dragged(&mut h),
            None,
            "inside the dead zone it stays a press"
        );
        let over_b = point(px(312.0 + 150.0), px(DASHBOARD_ROW - 10.0));
        move_to(&mut h.cx, over_b);
        assert_eq!(dragged(&mut h), Some("c".into()));
        rest(&mut h.cx);
        assert!(
            h.changes.borrow().is_empty(),
            "nothing is saved while it is held"
        );
        h.cx.simulate_mouse_up(over_b, MouseButton::Left, Modifiers::default());
        draw(&mut h.cx);

        assert_eq!(dragged(&mut h), None);
        assert_eq!(h.changes.borrow().len(), 1);
        let layout = h.layout.borrow().clone();
        assert_eq!(
            layout.anchors[&SharedString::from("c")],
            Cell { x: 1, y: 1 }
        );
        assert_eq!(
            layout.anchors[&SharedString::from("a")],
            Cell { x: 0, y: 0 }
        );
        assert_eq!(
            layout.anchors[&SharedString::from("b")],
            Cell { x: 1, y: 0 }
        );
    }

    #[gpui::test]
    fn escape_puts_the_cards_back(cx: &mut TestAppContext) {
        let mut h = harness(cx);
        press(&mut h.cx, header(0.0, 1.0));
        let over_a = point(px(150.0), px(10.0));
        move_to(&mut h.cx, over_a);
        rest(&mut h.cx);
        // Resting on a's top took a's spot and pushed it down, for now.
        assert_eq!(dragged(&mut h), Some("c".into()));
        h.cx.simulate_keystrokes("escape");
        draw(&mut h.cx);
        assert_eq!(dragged(&mut h), None);
        h.cx.simulate_mouse_up(over_a, MouseButton::Left, Modifiers::default());
        draw(&mut h.cx);
        assert!(h.changes.borrow().is_empty());
        assert!(h.layout.borrow().anchors.is_empty());
    }

    #[gpui::test]
    fn arrow_keys_on_the_grip_step_the_card(cx: &mut TestAppContext) {
        let mut h = harness(cx);
        // The grip sits at the header's end: 16px padding, 24px wide.
        let grip = point(px(300.0 - 16.0 - 12.0), px(22.0));
        h.cx.simulate_click(grip, Modifiers::default());
        draw(&mut h.cx);
        assert_eq!(dragged(&mut h), None, "a click is not a drag");
        // a steps over b to the right.
        h.cx.simulate_keystrokes("right");
        draw(&mut h.cx);
        let layout = h.layout.borrow().clone();
        assert_eq!(h.changes.borrow().len(), 1);
        assert_eq!(
            layout.anchors[&SharedString::from("a")],
            Cell { x: 1, y: 0 }
        );
        assert_eq!(
            layout.anchors[&SharedString::from("b")],
            Cell { x: 1, y: 1 }
        );
    }
}
