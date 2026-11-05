//! `Board` (`work-board.tsx`, `work-board.css`, `work-board-drag.ts`, and the design system's
//! `drag-motion.ts`): columns of cards that drag between them, as Polterware's Work board
//! shows a milestone's cards by stage.
//!
//! The board (`.pw-work-board`) scrolls both ways, 16px 12px in, its columns 12px apart and
//! at least 200px wide, sharing the rest. A column (`.pw-work-column`) is a lane 8px in that
//! hugs its cards, 8px apart: its header (4px in) holds a 20px [`IconTile`] with a 12px mark,
//! the 12px medium secondary label 8px after it, and the count in 12px muted tabular figures
//! at its end; an empty column says "No cards" (12px muted, 4px 8px in), and its last row is
//! the 32px dashed pill (`.pw-work-column__add`: a 1px dashed `--color-border` ring, muted
//! ink, `--color-surface-hover` and primary ink under the pointer) that adds a card there,
//! titled "Add card to <label>". A card (`.pw-work-card`) is `--color-surface-subtle` at
//! `--radius-card` with the control's rings (`--field-rings`): its head, 12px in, holds the
//! 13px medium title in secondary ink (primary under the pointer) over its 11px mono caption
//! in muted ink, 2px apart, and a footer may close it (the Work board's Tasks Morph).
//!
//! A press on a card's head that travels 4px (`DRAG_DEAD_ZONE`) lifts it: it follows the
//! pointer 1:1 above every surface with `0 8px 24px --color-shadow` over its rings, and the
//! column under the pointer (the nearest one between columns, none outside the board)
//! fills with `--color-surface-active`. Near the board's side edges (48px) it scrolls 14px a
//! frame under the held card. Releasing over another column moves the card there at once and
//! hands the move to [`Board::on_move`]; the card glides from where it was dropped into its
//! slot on the moderate spring. The board keeps the card in that column until the app's
//! cards show it there, or until the app settles the move ([`BoardMove::settle`]): a move
//! that was cancelled or failed glides back to the column the cards name. Releasing over its
//! own column, outside the board, or pressing Escape glides it home. The click that ends a
//! drag does not open the card. GPUI cannot scale an element, so the held card's 1.03 lift
//! is its shadow alone, and the hover lights the title under the pointer without the
//! travelling marker of `createFluidHover`.
//!
//! It behaves like gpui-kit's `Sortable` across lists: the app owns which column each card
//! is in and writes the move; the board draws the gesture, the move it is waiting on, and
//! the glides.

use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, Context, DispatchPhase, ElementId, Entity, FocusHandle, FontWeight,
    GlobalElementId, InspectorElementId, IntoElement, KeyDownEvent, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollHandle, SharedString,
    Window, canvas, deferred, div, point, prelude::*, px,
};

use crate::Handler;
use crate::icon_tile::IconTile;
use crate::icons::{Icon, icon};
use crate::material::{Edge, control_drop, drop, edge};
use crate::motion::{SpringTier, Tween};
use crate::theme::Theme;
use crate::tooltip::tooltip;

/// A column's narrowest width (`minmax(200px, 1fr)`).
pub const BOARD_COLUMN_MIN: f32 = 200.0;
/// The room between columns, and the board's padding (16px down, 12px across).
pub const BOARD_GAP: f32 = 12.0;
const PAD_Y: f32 = 16.0;
const PAD_X: f32 = 12.0;
/// Pointer travel before a press becomes a drag (`DRAG_DEAD_ZONE`).
const DEAD_ZONE: f32 = 4.0;
/// The side edges that scroll the board, and how far a frame scrolls (`EDGE`, `EDGE_STEP`).
const EDGE: f32 = 48.0;
const EDGE_STEP: f32 = 14.0;
/// Where a held or landing card paints among the deferred layers: over the page, under
/// popovers and modals.
const LIFT_PRIORITY: usize = 50;

/// The column nearest `x` (`columnAt`): the one under it, else the closest edge.
pub fn column_at(spans: &[(SharedString, f32, f32)], x: f32) -> Option<SharedString> {
    let mut best = None;
    let mut best_distance = f32::INFINITY;
    for (key, left, right) in spans {
        let distance = if x < *left {
            left - x
        } else if x > *right {
            x - right
        } else {
            0.0
        };
        if distance < best_distance {
            best = Some(key.clone());
            best_distance = distance;
        }
    }
    best
}

/// A card on the board: its key, its title and caption, an optional footer, what opening it
/// does, and its place in the app's order, which the columns keep.
pub struct BoardCard {
    key: SharedString,
    title: SharedString,
    caption: SharedString,
    footer: Option<AnyElement>,
    on_activate: Option<Handler>,
    order: usize,
}

impl BoardCard {
    pub fn new(
        key: impl Into<SharedString>,
        title: impl Into<SharedString>,
        caption: impl Into<SharedString>,
        order: usize,
    ) -> Self {
        Self {
            key: key.into(),
            title: title.into(),
            caption: caption.into(),
            footer: None,
            on_activate: None,
            order,
        }
    }

    /// The row that closes the card, outside the head a drag starts from.
    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    /// A click on the head (not the one that ends a drag).
    pub fn on_activate(mut self, handler: Handler) -> Self {
        self.on_activate = Some(handler);
        self
    }
}

/// A column: its key, its label and tile, its cards, and the add pill when it has one.
pub struct BoardColumn {
    key: SharedString,
    label: SharedString,
    tile: IconTile,
    cards: Vec<BoardCard>,
    on_add: Option<Handler>,
}

impl BoardColumn {
    pub fn new(
        key: impl Into<SharedString>,
        label: impl Into<SharedString>,
        tile: IconTile,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            tile,
            cards: Vec::new(),
            on_add: None,
        }
    }

    pub fn cards(mut self, cards: impl IntoIterator<Item = BoardCard>) -> Self {
        self.cards.extend(cards);
        self
    }

    /// The dashed pill that adds a card to this column.
    pub fn on_add(mut self, handler: Handler) -> Self {
        self.on_add = Some(handler);
        self
    }
}

/// A drop on another column, handed to [`Board::on_move`].
pub struct BoardMove {
    pub card: SharedString,
    pub column: SharedString,
    state: Entity<BoardState>,
}

impl BoardMove {
    /// Ends the wait on this move: `moved` once it is written, else (cancelled or failed) the
    /// card glides back to the column the app's cards name.
    pub fn settle(&self, moved: bool, cx: &mut App) {
        let card = self.card.clone();
        self.state.update(cx, |state, cx| {
            state.pending.remove(&card);
            if !moved && let Some(drawn) = state.drawn.get(&card) {
                state.landing = Some(Landing::from(card, drawn.origin, 1.0));
            }
            cx.notify();
        });
    }
}

/// Hears a drop on another column.
pub type MoveHandler = Rc<dyn Fn(BoardMove, &mut Window, &mut App)>;

/// An offset that springs back to zero on the moderate spring.
#[derive(Clone, Copy, Debug)]
struct Glide {
    x: Tween,
    y: Tween,
}

impl Glide {
    fn from(offset: Point<Pixels>) -> Self {
        let moderate = SpringTier::Moderate;
        let mut x = Tween::new(f32::from(offset.x));
        let mut y = Tween::new(f32::from(offset.y));
        x.animate_to(0.0, moderate.duration(), moderate.curve());
        y.animate_to(0.0, moderate.duration(), moderate.curve());
        Self { x, y }
    }

    fn offset(&self) -> Point<Pixels> {
        point(px(self.x.value()), px(self.y.value()))
    }

    fn moving(&self) -> bool {
        self.x.is_animating() || self.y.is_animating()
    }
}

/// A press on a card's head that has not travelled far enough to lift it.
struct Press {
    card: SharedString,
    at: Point<Pixels>,
}

struct Drag {
    card: SharedString,
    pointer: Point<Pixels>,
    /// The pointer's place on the card, from its top-left, when it lifted.
    grab: Point<Pixels>,
    /// The column under the pointer.
    target: Option<SharedString>,
    lift: Tween,
}

/// A card gliding into its slot after a drop, a cancel, or a refused move, above the others.
struct Landing {
    card: SharedString,
    /// Where it was drawn, in window coordinates, until its slot is known and it glides.
    from: Option<Point<Pixels>>,
    glide: Option<Glide>,
    lift: Tween,
}

impl Landing {
    fn from(card: SharedString, from: Point<Pixels>, lifted: f32) -> Self {
        let moderate = SpringTier::Moderate;
        let mut lift = Tween::new(lifted);
        lift.animate_to(0.0, moderate.duration(), moderate.curve());
        Self {
            card,
            from: Some(from),
            glide: None,
            lift,
        }
    }

    fn done(&self) -> bool {
        self.from.is_none()
            && !self.glide.is_some_and(|glide| glide.moving())
            && !self.lift.is_animating()
    }
}

/// What a board keeps between frames: where its columns and cards were drawn, the gesture,
/// the moves it waits on, and the glide.
pub struct BoardState {
    scroll: ScrollHandle,
    focus: FocusHandle,
    restore: Option<FocusHandle>,
    /// Each column's bounds in the window, as the last frame drew them.
    columns: Vec<(SharedString, Bounds<Pixels>)>,
    /// Each card's bounds as drawn, offsets included.
    drawn: HashMap<SharedString, Bounds<Pixels>>,
    /// The column each card is shown in while its move is written.
    pending: HashMap<SharedString, SharedString>,
    /// The column the app's cards name for each card.
    homes: HashMap<SharedString, SharedString>,
    press: Option<Press>,
    drag: Option<Drag>,
    landing: Option<Landing>,
    on_move: Option<MoveHandler>,
}

impl BoardState {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            restore: None,
            columns: Vec::new(),
            drawn: HashMap::new(),
            pending: HashMap::new(),
            homes: HashMap::new(),
            press: None,
            drag: None,
            landing: None,
            on_move: None,
        }
    }

    /// The card being dragged.
    pub fn dragging(&self) -> Option<&SharedString> {
        self.drag.as_ref().map(|drag| &drag.card)
    }

    /// The column a held card would drop into.
    pub fn target(&self) -> Option<&SharedString> {
        self.drag.as_ref().and_then(|drag| drag.target.as_ref())
    }

    fn press(&mut self, card: SharedString, at: Point<Pixels>, cx: &mut Context<Self>) {
        if self.on_move.is_none() || self.drag.is_some() {
            return;
        }
        self.press = Some(Press { card, at });
        cx.notify();
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
            if f32::from(travel.x).hypot(f32::from(travel.y)) < DEAD_ZONE {
                return;
            }
            self.lift(window, cx);
        }
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        drag.pointer = event.position;
        self.retarget();
        cx.notify();
    }

    /// The press became a drag.
    fn lift(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(press) = self.press.take() else {
            return;
        };
        let Some(drawn) = self.drawn.get(&press.card) else {
            return;
        };
        let fast = SpringTier::Fast;
        let mut lift = Tween::new(0.0);
        lift.animate_to(1.0, fast.duration(), fast.curve());
        if self
            .landing
            .as_ref()
            .is_some_and(|landing| landing.card == press.card)
        {
            self.landing = None;
        }
        self.drag = Some(Drag {
            grab: press.at - drawn.origin,
            card: press.card,
            pointer: press.at,
            target: None,
            lift,
        });
        self.retarget();
        self.restore = window.focused(cx);
        self.focus.focus(window);
    }

    /// The column under the held card's pointer, none outside the board.
    fn retarget(&mut self) {
        let bounds = self.scroll.bounds();
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        drag.target = bounds
            .contains(&drag.pointer)
            .then(|| {
                let spans: Vec<(SharedString, f32, f32)> = self
                    .columns
                    .iter()
                    .map(|(key, bounds)| {
                        (
                            key.clone(),
                            f32::from(bounds.left()),
                            f32::from(bounds.right()),
                        )
                    })
                    .collect();
                column_at(&spans, f32::from(drag.pointer.x))
            })
            .flatten();
    }

    fn release(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.press.take().is_some() {
            cx.notify();
            return;
        }
        self.finish(false, window, cx);
    }

    /// Ends the drag: a drop on another column moves the card there and hands the move on;
    /// anything else glides it home.
    fn finish(&mut self, cancelled: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        if let Some(restore) = self.restore.take()
            && self.focus.is_focused(window)
        {
            restore.focus(window);
        }
        self.landing = Some(Landing::from(
            drag.card.clone(),
            drag.pointer - drag.grab,
            drag.lift.value(),
        ));
        let shown = self
            .pending
            .get(&drag.card)
            .or(self.homes.get(&drag.card))
            .cloned();
        if !cancelled
            && let Some(column) = drag.target
            && Some(&column) != shown.as_ref()
            && let Some(handler) = self.on_move.clone()
        {
            self.pending.insert(drag.card.clone(), column.clone());
            let state = cx.entity();
            // After this update, so the handler may settle the move at once.
            window.defer(cx, move |window, cx| {
                handler(
                    BoardMove {
                        card: drag.card,
                        column,
                        state,
                    },
                    window,
                    cx,
                );
            });
        }
        cx.notify();
    }

    /// Scrolls while the held card is near a side edge (`pump`). Returns whether it scrolled.
    fn edge_scroll(&mut self) -> bool {
        let Some(drag) = self.drag.as_ref() else {
            return false;
        };
        let bounds = self.scroll.bounds();
        let x = f32::from(drag.pointer.x);
        if !bounds.contains(&drag.pointer) {
            return false;
        }
        let direction = if x < f32::from(bounds.left()) + EDGE {
            -1.0
        } else if x > f32::from(bounds.right()) - EDGE {
            1.0
        } else {
            return false;
        };
        let offset = self.scroll.offset();
        let max = f32::from(self.scroll.max_offset().width);
        let next = (f32::from(offset.x) - direction * EDGE_STEP).clamp(-max, 0.0);
        if (next - f32::from(offset.x)).abs() < 0.01 {
            return false;
        }
        self.scroll.set_offset(point(px(next), offset.y));
        true
    }

    /// Where card `key` draws, from where its slot is: under the pointer while held, along its
    /// glide while landing, else in its slot. Records where it drew.
    fn place(
        &mut self,
        key: &SharedString,
        slot: Bounds<Pixels>,
        window: &mut Window,
    ) -> Point<Pixels> {
        let offset = match (&self.drag, &mut self.landing) {
            (Some(drag), _) if &drag.card == key => drag.pointer - drag.grab - slot.origin,
            (_, Some(landing)) if &landing.card == key => {
                if let Some(from) = landing.from.take() {
                    landing.glide = Some(Glide::from(from - slot.origin));
                }
                let glide = landing.glide.as_ref();
                if glide.is_some_and(|glide| glide.moving()) || landing.lift.is_animating() {
                    window.request_animation_frame();
                }
                glide.map_or(Point::default(), |glide| glide.offset())
            }
            _ => Point::default(),
        };
        self.drawn.insert(
            key.clone(),
            Bounds {
                origin: slot.origin + offset,
                size: slot.size,
            },
        );
        offset
    }

    /// How lifted card `key` is, 0 to 1, when it paints above the others.
    fn lifted(&self, key: &SharedString) -> Option<f32> {
        match (&self.drag, &self.landing) {
            (Some(drag), _) if &drag.card == key => Some(drag.lift.value()),
            (_, Some(landing)) if &landing.card == key => Some(landing.lift.value()),
            _ => None,
        }
    }
}

fn board_state(id: ElementId, window: &mut Window, cx: &mut App) -> Entity<BoardState> {
    window.use_keyed_state(id, cx, |_, cx| BoardState::new(cx))
}

/// The board named `id`: see the module's docs. Build it with [`Board::build`].
pub fn board(id: impl Into<ElementId>) -> Board {
    Board {
        id: id.into(),
        columns: Vec::new(),
        on_move: None,
    }
}

pub struct Board {
    id: ElementId,
    columns: Vec<BoardColumn>,
    on_move: Option<MoveHandler>,
}

impl Board {
    pub fn column(mut self, column: BoardColumn) -> Self {
        self.columns.push(column);
        self
    }

    pub fn columns(mut self, columns: impl IntoIterator<Item = BoardColumn>) -> Self {
        self.columns.extend(columns);
        self
    }

    /// Makes cards draggable between columns; hears each drop on another column.
    pub fn on_move(mut self, handler: MoveHandler) -> Self {
        self.on_move = Some(handler);
        self
    }

    pub fn build(self, window: &mut Window, cx: &mut App) -> AnyElement {
        let theme = *Theme::global(cx);
        let state = board_state(self.id.clone(), window, cx);

        // The app's columns, with the cards whose move is written shown where they were dropped.
        let mut homes = HashMap::new();
        let mut columns: Vec<(BoardColumn, Vec<BoardCard>)> = Vec::new();
        let mut moving: Vec<BoardCard> = Vec::new();
        let pending = state.read(cx).pending.clone();
        for mut column in self.columns {
            let cards = std::mem::take(&mut column.cards);
            let mut kept = Vec::new();
            for card in cards {
                homes.insert(card.key.clone(), column.key.clone());
                match pending.get(&card.key) {
                    Some(target) if *target != column.key => moving.push(card),
                    _ => kept.push(card),
                }
            }
            columns.push((column, kept));
        }
        for card in moving {
            let target = pending.get(&card.key).cloned();
            if let Some((_, cards)) = columns
                .iter_mut()
                .find(|(column, _)| Some(&column.key) == target.as_ref())
            {
                cards.push(card);
            }
        }
        for (_, cards) in &mut columns {
            cards.sort_by_key(|card| card.order);
        }

        let on_move = self.on_move;
        let (scroll, focus, target) = state.update(cx, |state, _| {
            state.on_move = on_move;
            // A move the app's cards now show, or a card that left, is no longer waited on.
            state
                .pending
                .retain(|card, target| homes.get(card).is_some_and(|home| home != target));
            state.homes = homes;
            state.columns.clear();
            if state.landing.as_ref().is_some_and(Landing::done) {
                state.landing = None;
            }
            if state.edge_scroll() {
                window.request_animation_frame();
                state.retarget();
            }
            if state
                .drag
                .as_ref()
                .is_some_and(|drag| drag.lift.is_animating())
            {
                window.request_animation_frame();
            }
            (
                state.scroll.clone(),
                state.focus.clone(),
                state.target().cloned(),
            )
        });

        let listen = state.clone();
        let listeners = canvas(
            |_, _, _| {},
            move |_, _, window, cx| {
                let active = {
                    let state = listen.read(cx);
                    state.press.is_some() || state.drag.is_some()
                };
                if !active {
                    return;
                }
                let moves = listen.clone();
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
                let ups = listen.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                        return;
                    }
                    let dragging = ups.read(cx).drag.is_some();
                    ups.update(cx, |state, cx| state.release(window, cx));
                    // The click that ends a drag must not open the card.
                    if dragging {
                        cx.stop_propagation();
                    }
                });
            },
        )
        .absolute()
        .size_0();

        let mut row = div()
            .flex()
            .items_start()
            .gap(px(BOARD_GAP))
            .px(px(PAD_X))
            .py(px(PAD_Y))
            .w_full()
            .min_w(px(columns.len() as f32 * BOARD_COLUMN_MIN
                + columns.len().saturating_sub(1) as f32 * BOARD_GAP
                + 2.0 * PAD_X));
        for (column, cards) in columns {
            row = row.child(column_element(
                &state,
                column,
                cards,
                target.as_ref(),
                &theme,
                cx,
            ));
        }

        let cancel = state.clone();
        div()
            .id(self.id)
            .relative()
            .size_full()
            .min_h(px(0.0))
            .overflow_scroll()
            .track_scroll(&scroll)
            .track_focus(&focus)
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && cancel.read(cx).drag.is_some() {
                    cx.stop_propagation();
                    cancel.update(cx, |state, cx| state.finish(true, window, cx));
                }
            })
            .child(listeners)
            .child(row)
            .into_any_element()
    }
}

/// `.pw-work-column`: its header, its cards or "No cards", and the add pill.
fn column_element(
    state: &Entity<BoardState>,
    column: BoardColumn,
    cards: Vec<BoardCard>,
    target: Option<&SharedString>,
    theme: &Theme,
    cx: &App,
) -> AnyElement {
    let lit = target == Some(&column.key);
    let measure = state.clone();
    let key = column.key.clone();
    let probe = canvas(
        move |bounds, _, cx| {
            measure.update(cx, |state, _| state.columns.push((key.clone(), bounds)));
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0();
    let header = div()
        .flex()
        .items_center()
        .justify_between()
        .px(px(4.0))
        .child(
            div()
                .flex()
                .min_w(px(0.0))
                .items_center()
                .gap(px(8.0))
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.color_text_secondary)
                .child(column.tile.size(20.0, 12.0))
                .child(column.label.clone()),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(theme.color_text_muted)
                .child(cards.len().to_string()),
        );
    let mut body = div().flex().flex_col().gap(px(8.0));
    if cards.is_empty() {
        body = body.child(
            div()
                .px(px(4.0))
                .py(px(8.0))
                .text_size(px(12.0))
                .text_color(theme.color_text_muted)
                .child("No cards"),
        );
    }
    for card in cards {
        body = body.child(card_element(state, card, theme, cx));
    }
    if let Some(add) = column.on_add {
        let label: SharedString = format!("Add card to {}", column.label).into();
        body = body.child(
            div()
                .id(ElementId::Name(format!("board-add-{}", column.key).into()))
                .group("board-add")
                .h(px(32.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .border_dashed()
                .border_color(theme.color_border)
                .text_color(theme.color_text_muted)
                .cursor_pointer()
                .hover(|pill| {
                    pill.bg(theme.color_surface_hover)
                        .text_color(theme.color_text_primary)
                })
                .tooltip(tooltip(label))
                .on_click(move |_, window, cx| add(window, cx))
                .child(icon(Icon::Plus, 14.0, cx)),
        );
    }
    div()
        .relative()
        .flex_1()
        .min_w(px(BOARD_COLUMN_MIN))
        .p(px(8.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .rounded(px(theme.radius_surface))
        .when(lit, |column| column.bg(theme.color_surface_active))
        .child(probe)
        .child(header)
        .child(body)
        .into_any_element()
}

/// `.pw-work-card`: the head a drag starts from, then the footer; above the others while it is
/// held or landing.
fn card_element(
    state: &Entity<BoardState>,
    card: BoardCard,
    theme: &Theme,
    cx: &App,
) -> AnyElement {
    let lifted = state.read(cx).lifted(&card.key);
    let group = SharedString::from(format!("board-card-{}", card.key));
    let press_state = state.clone();
    let press_key = card.key.clone();
    let release_state = state.clone();
    let mut head = div()
        .id(ElementId::Name(
            format!("board-card-head-{}", card.key).into(),
        ))
        .flex()
        .flex_col()
        .items_start()
        .gap(px(2.0))
        .p(px(12.0))
        .rounded(px(theme.radius_card))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, _, cx| {
            press_state.update(cx, |state, cx| {
                state.press(press_key.clone(), event.position, cx)
            });
        })
        .on_mouse_up(MouseButton::Left, move |_, window, cx| {
            // A press no window listener heard end: it came before the next frame.
            if release_state.read(cx).press.is_some() {
                release_state.update(cx, |state, cx| state.release(window, cx));
            }
        })
        .child(
            div()
                .max_w_full()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.color_text_secondary)
                .group_hover(group.clone(), |title| {
                    title.text_color(theme.color_text_primary)
                })
                .child(card.title.clone()),
        )
        .child(
            div()
                .max_w_full()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .font_family(theme.font_mono)
                .text_size(px(11.0))
                .text_color(theme.color_text_muted)
                .child(card.caption.clone()),
        );
    if let Some(activate) = card.on_activate {
        head = head.on_click(move |_, window, cx| activate(window, cx));
    }
    let mut shadow = control_drop(theme);
    if let Some(share) = lifted {
        let mut lift = theme.color_shadow;
        lift.a *= share.clamp(0.0, 1.0);
        shadow.push(drop(8.0, 24.0, lift));
    }
    let surface = div()
        .relative()
        .group(group)
        .flex()
        .flex_col()
        .rounded(px(theme.radius_card))
        .bg(theme.color_surface_subtle)
        .shadow(shadow)
        .child(head)
        .children(card.footer)
        .child(edge(Edge::Strong, theme.radius_card, theme));
    let shift = Shift {
        child: surface.into_any_element(),
        state: state.clone(),
        key: card.key.clone(),
    };
    match lifted {
        Some(_) => deferred(shift)
            .with_priority(LIFT_PRIORITY)
            .into_any_element(),
        None => shift.into_any_element(),
    }
}

/// A card in its slot whose drawing moves by the offset the board decides once the slot is
/// known: under the pointer while held, along its glide while landing.
struct Shift {
    child: AnyElement,
    state: Entity<BoardState>,
    key: SharedString,
}

impl IntoElement for Shift {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Shift {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let key = self.key.clone();
        let offset = self
            .state
            .update(cx, |state, _| state.place(&key, bounds, window));
        window.with_element_offset(offset, |window| self.child.prepaint(window, cx));
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Modifiers, TestAppContext, VisualTestContext};

    use super::*;

    #[test]
    fn the_nearest_column_takes_the_drop() {
        let spans = vec![
            (SharedString::from("a"), 0.0, 100.0),
            (SharedString::from("b"), 112.0, 212.0),
        ];
        assert_eq!(column_at(&spans, 50.0), Some("a".into()));
        assert_eq!(column_at(&spans, 105.0), Some("a".into()));
        assert_eq!(column_at(&spans, 108.0), Some("b".into()));
        assert_eq!(column_at(&spans, 400.0), Some("b".into()));
        assert_eq!(column_at(&[], 10.0), None);
    }

    struct Fixture {
        columns: Vec<(&'static str, Vec<&'static str>)>,
        moves: Rc<std::cell::RefCell<Vec<(String, String)>>>,
        settle: Option<bool>,
        state: Option<Entity<BoardState>>,
    }

    impl Render for Fixture {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.state = Some(board_state(ElementId::Name("board".into()), window, cx));
            let moves = self.moves.clone();
            let settle = self.settle;
            let mut board = board("board").on_move(Rc::new(move |change: BoardMove, _, cx| {
                moves
                    .borrow_mut()
                    .push((change.card.to_string(), change.column.to_string()));
                if let Some(moved) = settle {
                    change.settle(moved, cx);
                }
            }));
            for (key, cards) in &self.columns {
                board = board.column(
                    BoardColumn::new(*key, *key, IconTile::new(Icon::Check)).cards(
                        cards
                            .iter()
                            .enumerate()
                            .map(|(order, card)| BoardCard::new(*card, *card, *card, order)),
                    ),
                );
            }
            div().size(px(1200.0)).child(board.build(window, cx))
        }
    }

    fn open(
        cx: &mut TestAppContext,
        settle: Option<bool>,
    ) -> (Entity<Fixture>, &mut VisualTestContext) {
        cx.update(|cx| cx.set_global(Theme::dark()));
        let (view, cx) = cx.add_window_view(|_, _| Fixture {
            columns: vec![
                ("shape", vec!["one", "two"]),
                ("plan", vec![]),
                ("build", vec![]),
            ],
            moves: Rc::default(),
            settle,
            state: None,
        });
        cx.run_until_parked();
        (view, cx)
    }

    fn state(view: &Entity<Fixture>, cx: &mut VisualTestContext) -> Entity<BoardState> {
        view.read_with(cx, |view, _| view.state.clone().expect("board drawn"))
    }

    fn column_center(
        view: &Entity<Fixture>,
        cx: &mut VisualTestContext,
        key: &str,
    ) -> Point<Pixels> {
        let state = state(view, cx);
        state.read_with(cx, |state, _| {
            let (_, bounds) = state
                .columns
                .iter()
                .find(|(column, _)| column.as_ref() == key)
                .expect("column drawn");
            bounds.center()
        })
    }

    fn card_head(view: &Entity<Fixture>, cx: &mut VisualTestContext, key: &str) -> Point<Pixels> {
        let state = state(view, cx);
        state.read_with(cx, |state, _| {
            let bounds = *state.drawn.get(key).expect("card drawn");
            point(bounds.left() + px(20.0), bounds.top() + px(12.0))
        })
    }

    #[gpui::test]
    fn a_drag_past_the_dead_zone_drops_the_card_on_another_column(cx: &mut TestAppContext) {
        let (view, cx) = open(cx, None);
        let from = card_head(&view, cx, "one");
        let to = column_center(&view, cx, "plan");
        cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        cx.simulate_mouse_move(
            from + point(px(2.0), px(0.0)),
            MouseButton::Left,
            Modifiers::none(),
        );
        cx.run_until_parked();
        assert!(
            state(&view, cx).read_with(cx, |state, _| state.drag.is_none()),
            "inside the dead zone"
        );
        cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            state(&view, cx).read_with(cx, |state, _| state.target().cloned()),
            Some("plan".into())
        );
        cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        let moves = view.read_with(cx, |view, _| view.moves.borrow().clone());
        assert_eq!(moves, vec![("one".to_string(), "plan".to_string())]);
        // It waits in the column it was dropped on.
        let pending = state(&view, cx).read_with(cx, |state, _| state.pending.get("one").cloned());
        assert_eq!(pending, Some("plan".into()));
    }

    #[gpui::test]
    fn a_refused_move_goes_back_and_escape_cancels(cx: &mut TestAppContext) {
        let (view, cx) = open(cx, Some(false));
        let from = card_head(&view, cx, "two");
        let to = column_center(&view, cx, "build");
        cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        assert_eq!(view.read_with(cx, |view, _| view.moves.borrow().len()), 1);
        let waiting = state(&view, cx).read_with(cx, |state, _| state.pending.is_empty());
        assert!(waiting, "a refused move is not waited on");

        let from = card_head(&view, cx, "one");
        cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        cx.simulate_keystrokes("escape");
        cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |view, _| view.moves.borrow().len()),
            1,
            "Escape drops nothing"
        );
    }
}
