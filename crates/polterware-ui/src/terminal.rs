//! Terminal: an interactive terminal an app feeds with a program's output and hears typing,
//! pastes, and size changes back from, as events.
//!
//! It looks like Vitals' Electron `TerminalView` (xterm.js 6 with its DOM renderer and the fit
//! addon): 13px `--font-mono` text in `--color-text-primary` on `--color-surface`, the block
//! cursor in the text color with the character under it in the surface color (an outline
//! while the terminal is not focused), the selection in `--color-surface-selected` with its
//! text in the text color, and xterm.js's default palette: its 16 ANSI colors, the 256-color
//! cube and gray ramp, and 24-bit color as sent; bold text in the first eight colors takes the
//! bright one. The grid is the fit addon's: whole cells from the top left, with the scrollbar's
//! gutter kept at the right, where the classic scrollbar ([`crate::scroll::scrollbar`]) shows
//! once there is scrollback. The cursor blinks while the terminal is focused and rests solid
//! once nothing has happened for a while, so an idle terminal asks for no frames.
//!
//! gpui-kit has no terminal; this follows its state-entity shape (a [`TerminalView`] entity
//! the app holds, [`TerminalEvent`]s out) and its `Input`'s focus, IME, and clipboard
//! handling. The emulation is `alacritty_terminal`'s (as in Zed's GPUI terminal): the grid,
//! scrollback (1000 lines, xterm.js's default), the alternate screen, selection, and the
//! modes a program sets. Keys are encoded as xterm.js's `Keyboard.ts` does on macOS with
//! Option composing characters (`macOptionIsMeta` off), so dead keys and input methods reach
//! the program as text. A drag selects, a double click selects a word, a triple click a line,
//! and Shift extends; Command-C copies, Command-V pastes (bracketed when the program asks),
//! and Command-A selects everything. While a program reads the mouse, clicks, drags, and the
//! wheel go to it (Shift keeps them for selecting); otherwise the wheel scrolls back, or moves
//! the cursor keys on the alternate screen. Typing scrolls back to the bottom.

use std::cell::{Cell as SharedCell, RefCell};
use std::ops::Range;
use std::rc::Rc;
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event as TermEvent, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point as GridPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::{
    Config, Osc52, Term, TermMode, point_to_viewport, viewport_to_point,
};
use alacritty_terminal::vte::ansi::{Color as AnsiColor, CursorShape, NamedColor, Processor, Rgb};
use gpui::{
    App, BorderStyle, Bounds, ClipboardItem, ContentMask, Context, CursorStyle, DispatchPhase,
    Element, ElementId, ElementInputHandler, Entity, EntityInputHandler, EventEmitter, FocusHandle,
    Focusable, Font, FontStyle, FontWeight, GlobalElementId, Hsla, InspectorElementId, IntoElement,
    KeyBinding, KeyDownEvent, Keystroke, LayoutId, Modifiers, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, Render, Rgba, ScrollWheelEvent, ShapedLine,
    SharedString, StrikethroughStyle, Style, Subscription, TextRun, UTF16Selection, UnderlineStyle,
    Window, actions, canvas, div, fill, font, outline, point, prelude::*, px, relative, size,
};

use crate::scroll::{GUTTER, ScrollAxis, ScrollGeometry, ScrollSource, scrollbar};
use crate::theme::Theme;

actions!(terminal, [Copy, Paste, SelectAll]);

/// The largest grid side a terminal reports, as Vitals' shell channel accepts.
pub const MAX_DIMENSION: u16 = 500;
/// The fit addon's narrowest grid.
const MIN_COLUMNS: u16 = 2;
/// Below this many pixels on a side the box is still being laid out, and the size stays.
const MIN_FIT: f32 = 16.0;
/// xterm.js's default scrollback.
pub const DEFAULT_SCROLLBACK: usize = 1000;
/// `TerminalView.tsx`'s `fontSize`.
const FONT_SIZE: f32 = 13.0;
/// Half of the DOM renderer's one-second blink.
const BLINK_HALF: Duration = Duration::from_millis(500);
/// How long the cursor blinks after the last keystroke, output, or focus.
const BLINK_IDLE: Duration = Duration::from_secs(10);

pub fn register_key_bindings(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-c", Copy, Some("Terminal")),
        KeyBinding::new("cmd-v", Paste, Some("Terminal")),
        KeyBinding::new("cmd-a", SelectAll, Some("Terminal")),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        KeyBinding::new("ctrl-shift-c", Copy, Some("Terminal")),
        KeyBinding::new("ctrl-shift-v", Paste, Some("Terminal")),
    ]);
}

/// A grid size in cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
}

impl TerminalSize {
    /// A size inside `1..=MAX_DIMENSION` on both sides.
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols: cols.clamp(1, MAX_DIMENSION),
            rows: rows.clamp(1, MAX_DIMENSION),
        }
    }
}

impl Dimensions for TerminalSize {
    fn total_lines(&self) -> usize {
        self.rows as usize
    }

    fn screen_lines(&self) -> usize {
        self.rows as usize
    }

    fn columns(&self) -> usize {
        self.cols as usize
    }
}

/// What a terminal tells its app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalEvent {
    /// Bytes for the program: keys, a paste, mouse reports, and the terminal's own answers to
    /// the program's queries.
    Input(Vec<u8>),
    /// The grid was laid out at a new size.
    Resize(TerminalSize),
    /// The program set the title (empty when it reset it).
    Title(String),
}

/// The colors a terminal draws with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalPalette {
    pub foreground: Rgba,
    pub background: Rgba,
    pub cursor: Rgba,
    /// The character under a block cursor.
    pub cursor_accent: Rgba,
    pub selection: Rgba,
    pub selection_foreground: Rgba,
    /// Black, red, green, yellow, blue, magenta, cyan, white, then their bright versions.
    pub ansi: [Rgba; 16],
}

/// xterm.js's `DEFAULT_ANSI_COLORS` (the Tango palette).
const XTERM_ANSI: [u32; 16] = [
    0x2e3436, 0xcc0000, 0x4e9a06, 0xc4a000, 0x3465a4, 0x75507b, 0x06989a, 0xd3d7cf, 0x555753,
    0xef2929, 0x8ae234, 0xfce94f, 0x729fcf, 0xad7fa8, 0x34e2e2, 0xeeeeec,
];

/// xterm's color cube steps.
const CUBE: [u8; 6] = [0x00, 0x5f, 0x87, 0xaf, 0xd7, 0xff];

impl TerminalPalette {
    /// `TerminalView.tsx`'s `terminalTheme()` on the theme's tokens, over xterm.js's ANSI
    /// colors.
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            foreground: theme.color_text_primary,
            background: theme.color_surface,
            cursor: theme.color_text_primary,
            cursor_accent: theme.color_surface,
            selection: theme.color_surface_selected,
            selection_foreground: theme.color_text_primary,
            ansi: XTERM_ANSI.map(gpui::rgb),
        }
    }

    /// One of the 256 indexed colors: the 16 ANSI colors, the 6×6×6 cube, the 24 grays.
    pub fn indexed(&self, index: u8) -> Rgba {
        match index {
            0..=15 => self.ansi[index as usize],
            16..=231 => {
                let i = index - 16;
                rgba(Rgb {
                    r: CUBE[(i / 36) as usize],
                    g: CUBE[((i / 6) % 6) as usize],
                    b: CUBE[(i % 6) as usize],
                })
            }
            _ => {
                let level = 8 + (index - 232) * 10;
                rgba(Rgb {
                    r: level,
                    g: level,
                    b: level,
                })
            }
        }
    }

    fn named(&self, name: NamedColor) -> Rgba {
        let index = name as usize;
        match name {
            NamedColor::Foreground | NamedColor::BrightForeground | NamedColor::DimForeground => {
                self.foreground
            }
            NamedColor::Background => self.background,
            NamedColor::Cursor => self.cursor,
            _ if index < 16 => self.ansi[index],
            // The dim colors are the base ones; `Flags::DIM` halves them as they are drawn.
            _ => self.ansi[(index - NamedColor::DimBlack as usize).min(7)],
        }
    }
}

fn rgba(rgb: Rgb) -> Rgba {
    Rgba {
        r: rgb.r as f32 / 255.0,
        g: rgb.g as f32 / 255.0,
        b: rgb.b as f32 / 255.0,
        a: 1.0,
    }
}

fn to_rgb(color: Rgba) -> Rgb {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgb {
        r: channel(color.r),
        g: channel(color.g),
        b: channel(color.b),
    }
}

/// What the emulator asks of its host, collected while bytes are parsed.
#[derive(Clone, Default)]
struct Listener(Rc<RefCell<Vec<TermEvent>>>);

impl EventListener for Listener {
    fn send_event(&self, event: TermEvent) {
        self.0.borrow_mut().push(event);
    }
}

/// One cell's size, from the font.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CellMetrics {
    width: f32,
    height: f32,
}

/// The scrollback as the scrollbar reads it, in pixels from the oldest line, and the drag it
/// asks for, which the next frame applies.
#[derive(Clone, Default)]
struct ScrollLink {
    reading: Rc<SharedCell<(f32, f32)>>,
    requested: Rc<SharedCell<Option<f32>>>,
}

impl ScrollSource for ScrollLink {
    fn scrolled(&self, _: ScrollAxis) -> f32 {
        self.reading.get().0
    }

    fn max_scroll(&self, _: ScrollAxis) -> f32 {
        self.reading.get().1
    }

    fn scroll_to(&self, _: ScrollAxis, scrolled: f32) {
        let (_, max) = self.reading.get();
        self.reading.set((scrolled, max));
        self.requested.set(Some(scrolled));
    }
}

/// A press the terminal is following outside its box: a selection, or a button reported to
/// the program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Drag {
    Select,
    Report(u8),
}

/// The terminal's state: the emulator, its grid's size, the selection, and the input method's
/// composition. Feed it a program's output with [`TerminalView::feed`]; it emits
/// [`TerminalEvent`]s.
pub struct TerminalView {
    focus_handle: FocusHandle,
    term: Term<Listener>,
    parser: Processor,
    listener: Listener,
    palette: Option<TerminalPalette>,
    font_family: SharedString,
    font_size: f32,
    blink: bool,
    size: Option<TerminalSize>,
    metrics: Option<CellMetrics>,
    /// The grid's top left corner in the window and its box's height, from the last frame.
    origin: Point<Pixels>,
    height: f32,
    /// The input method's composition, drawn at the cursor until it commits.
    marked: Option<String>,
    drag: Option<Drag>,
    /// The cell the last motion report named.
    reported_cell: Option<(usize, usize)>,
    /// Wheel travel short of a whole line.
    scroll_carry: f32,
    scroll: ScrollLink,
    cursor_on: bool,
    blink_epoch: u64,
    last_activity: Instant,
    sync_pending: bool,
    _subscriptions: Vec<Subscription>,
}

impl TerminalView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let theme = *Theme::global(cx);
        let focus_handle = cx.focus_handle();
        let listener = Listener::default();
        let term = Term::new(
            term_config(DEFAULT_SCROLLBACK),
            &TerminalSize::new(80, 24),
            listener.clone(),
        );
        let subscriptions = vec![
            cx.on_focus(&focus_handle, window, |this, _, cx| {
                this.focus_changed(true, cx)
            }),
            cx.on_blur(&focus_handle, window, |this, _, cx| {
                this.focus_changed(false, cx)
            }),
        ];
        Self {
            focus_handle,
            term,
            parser: Processor::new(),
            listener,
            palette: None,
            font_family: theme.font_mono.into(),
            font_size: FONT_SIZE,
            blink: true,
            size: None,
            metrics: None,
            origin: Point::default(),
            height: 0.0,
            marked: None,
            drag: None,
            reported_cell: None,
            scroll_carry: 0.0,
            scroll: ScrollLink::default(),
            cursor_on: true,
            blink_epoch: 0,
            last_activity: Instant::now(),
            sync_pending: false,
            _subscriptions: subscriptions,
        }
    }

    /// Lines kept above the screen.
    pub fn scrollback(mut self, lines: usize) -> Self {
        self.term.set_options(term_config(lines));
        self
    }

    pub fn font_family(mut self, family: impl Into<SharedString>) -> Self {
        self.font_family = family.into();
        self
    }

    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    /// The colors, in place of the theme's.
    pub fn palette(mut self, palette: TerminalPalette) -> Self {
        self.palette = Some(palette);
        self
    }

    /// Whether the cursor blinks while focused (on by default, as in Vitals' Electron app).
    pub fn cursor_blink(mut self, blink: bool) -> Self {
        self.blink = blink;
        self
    }

    /// The grid's size, once it has been laid out.
    pub fn size(&self) -> Option<TerminalSize> {
        self.size
    }

    /// The selected text, if any, without the blank lines a selection can end on.
    pub fn selected_text(&self) -> Option<String> {
        let text = self.term.selection_to_string()?;
        let text = text.trim_end_matches('\n');
        (!text.is_empty()).then(|| text.to_string())
    }

    /// The visible rows' text, trailing blanks trimmed.
    pub fn screen_lines(&self) -> Vec<String> {
        let grid = self.term.grid();
        let offset = grid.display_offset() as i32;
        (0..grid.screen_lines())
            .map(|row| {
                let line = &grid[Line(row as i32 - offset)];
                let mut text = String::new();
                for column in 0..grid.columns() {
                    let cell = &line[Column(column)];
                    if !cell
                        .flags
                        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                    {
                        text.push(cell.c);
                    }
                }
                text.truncate(text.trim_end().len());
                text
            })
            .collect()
    }

    /// Writes a program's output.
    pub fn feed(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        if bytes.is_empty() {
            return;
        }
        self.parser.advance(&mut self.term, bytes);
        self.after_output(cx);
    }

    pub fn focus(&self, window: &mut Window) {
        self.focus_handle.focus(window);
    }

    fn after_output(&mut self, cx: &mut Context<Self>) {
        self.answer_queries(cx);
        self.schedule_sync(cx);
        self.wake_cursor(cx);
        cx.notify();
    }

    fn palette_or_theme(&self, cx: &App) -> TerminalPalette {
        self.palette
            .unwrap_or_else(|| TerminalPalette::from_theme(Theme::global(cx)))
    }

    /// Answers what the emulator asked while parsing: replies to queries, the title.
    fn answer_queries(&mut self, cx: &mut Context<Self>) {
        let events: Vec<TermEvent> = self.listener.0.borrow_mut().drain(..).collect();
        if events.is_empty() {
            return;
        }
        let palette = self.palette_or_theme(cx);
        for event in events {
            match event {
                TermEvent::PtyWrite(text) => cx.emit(TerminalEvent::Input(text.into_bytes())),
                TermEvent::Title(title) => cx.emit(TerminalEvent::Title(title)),
                TermEvent::ResetTitle => cx.emit(TerminalEvent::Title(String::new())),
                TermEvent::ColorRequest(index, format) => {
                    let color = self.color_at(index, &palette);
                    cx.emit(TerminalEvent::Input(format(to_rgb(color)).into_bytes()));
                }
                TermEvent::TextAreaSizeRequest(format) => {
                    let size = self.window_size();
                    cx.emit(TerminalEvent::Input(format(size).into_bytes()));
                }
                _ => {}
            }
        }
    }

    /// A synchronized update (DEC mode 2026) that never ends is drawn once its timeout passes.
    fn schedule_sync(&mut self, cx: &mut Context<Self>) {
        if self.sync_pending || self.parser.sync_bytes_count() == 0 {
            return;
        }
        let Some(deadline) = self.parser.sync_timeout().sync_timeout() else {
            return;
        };
        self.sync_pending = true;
        let wait = deadline.saturating_duration_since(Instant::now());
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, |this, cx| {
                this.sync_pending = false;
                if this.parser.sync_bytes_count() == 0 {
                    return;
                }
                let due = this
                    .parser
                    .sync_timeout()
                    .sync_timeout()
                    .is_none_or(|deadline| deadline <= Instant::now());
                if due {
                    this.parser.stop_sync(&mut this.term);
                    this.after_output(cx);
                } else {
                    this.schedule_sync(cx);
                }
            });
        })
        .detach();
    }

    /// Shows the cursor and blinks it again from the start, for [`BLINK_IDLE`].
    fn wake_cursor(&mut self, cx: &mut Context<Self>) {
        self.cursor_on = true;
        self.last_activity = Instant::now();
        if !self.blink || !self.term.is_focused {
            return;
        }
        self.blink_epoch += 1;
        let epoch = self.blink_epoch;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(BLINK_HALF).await;
                let blinking = this
                    .update(cx, |this, cx| {
                        if this.blink_epoch != epoch {
                            return false;
                        }
                        if !this.term.is_focused || this.last_activity.elapsed() >= BLINK_IDLE {
                            if !this.cursor_on {
                                this.cursor_on = true;
                                cx.notify();
                            }
                            return false;
                        }
                        this.cursor_on = !this.cursor_on;
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !blinking {
                    break;
                }
            }
        })
        .detach();
    }

    fn focus_changed(&mut self, focused: bool, cx: &mut Context<Self>) {
        self.term.is_focused = focused;
        if self.term.mode().contains(TermMode::FOCUS_IN_OUT) {
            let report: &[u8] = if focused { b"\x1b[I" } else { b"\x1b[O" };
            cx.emit(TerminalEvent::Input(report.to_vec()));
        }
        self.wake_cursor(cx);
        cx.notify();
    }

    fn color_at(&self, index: usize, palette: &TerminalPalette) -> Rgba {
        if let Some(rgb) = self.term.colors()[index.min(alacritty_terminal::term::color::COUNT - 1)]
        {
            return rgba(rgb);
        }
        match index {
            0..=255 => palette.indexed(index as u8),
            257 => palette.background,
            258 => palette.cursor,
            _ => palette.foreground,
        }
    }

    fn window_size(&self) -> WindowSize {
        let size = self.size.unwrap_or(TerminalSize::new(80, 24));
        let metrics = self.metrics.unwrap_or(CellMetrics {
            width: 8.0,
            height: 16.0,
        });
        WindowSize {
            num_lines: size.rows,
            num_cols: size.cols,
            cell_width: metrics.width.round() as u16,
            cell_height: metrics.height.round() as u16,
        }
    }

    /// Typing, a paste, or a report for the program: back to the bottom, the selection
    /// dropped, the cursor awake.
    fn send_user(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        if bytes.is_empty() {
            return;
        }
        if self.term.grid().display_offset() != 0 {
            self.term.scroll_display(Scroll::Bottom);
        }
        self.term.selection = None;
        self.wake_cursor(cx);
        cx.emit(TerminalEvent::Input(bytes));
        cx.notify();
    }

    /// Fits the grid to `bounds` (the box less the scrollbar's gutter), as the fit addon does.
    fn lay_out(&mut self, bounds: Bounds<Pixels>, metrics: CellMetrics, cx: &mut Context<Self>) {
        self.origin = bounds.origin;
        self.height = f32::from(bounds.size.height);
        self.metrics = Some(metrics);
        if let Some(scrolled) = self.scroll.requested.take() {
            let history = self.term.history_size() as f32;
            let target = (history - (scrolled / metrics.height).round()).clamp(0.0, history);
            let delta = target as i32 - self.term.grid().display_offset() as i32;
            if delta != 0 {
                self.term.scroll_display(Scroll::Delta(delta));
            }
        }
        let width = f32::from(bounds.size.width);
        if width < MIN_FIT || self.height < MIN_FIT {
            return;
        }
        let cols = ((width / metrics.width).floor() as u16).clamp(MIN_COLUMNS, MAX_DIMENSION);
        let rows = ((self.height / metrics.height).floor() as u16).clamp(1, MAX_DIMENSION);
        let size = TerminalSize { cols, rows };
        if self.size == Some(size) {
            return;
        }
        self.size = Some(size);
        self.term.resize(size);
        cx.emit(TerminalEvent::Resize(size));
    }

    /// The viewport cell under `position`, clamped to the grid, and the half of it.
    fn viewport_cell(&self, position: Point<Pixels>) -> Option<(usize, usize, Side)> {
        let metrics = self.metrics?;
        let size = self.size?;
        let x = f32::from(position.x - self.origin.x);
        let y = f32::from(position.y - self.origin.y);
        let column = (x / metrics.width).max(0.0);
        let col = (column as usize).min(size.cols as usize - 1);
        let side = if x >= size.cols as f32 * metrics.width || column.fract() >= 0.5 {
            Side::Right
        } else {
            Side::Left
        };
        let row = ((y / metrics.height).max(0.0) as usize).min(size.rows as usize - 1);
        Some((row, col, side))
    }

    fn grid_point(&self, position: Point<Pixels>) -> Option<(GridPoint, Side)> {
        let (row, col, side) = self.viewport_cell(position)?;
        let point = viewport_to_point(
            self.term.grid().display_offset(),
            GridPoint::new(row, Column(col)),
        );
        Some((point, side))
    }

    fn reports_mouse(&self, modifiers: &Modifiers) -> bool {
        self.term.mode().intersects(TermMode::MOUSE_MODE) && !modifiers.shift
    }

    fn mouse_report(
        &self,
        code: u8,
        pressed: bool,
        position: Point<Pixels>,
        modifiers: &Modifiers,
    ) -> Option<Vec<u8>> {
        let (row, col, _) = self.viewport_cell(position)?;
        mouse_report(*self.term.mode(), code, pressed, row, col, modifiers)
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window);
        if self.reports_mouse(&event.modifiers) {
            let Some(code) = button_code(event.button) else {
                return;
            };
            if let Some(report) = self.mouse_report(code, true, event.position, &event.modifiers) {
                self.drag = Some(Drag::Report(code));
                cx.emit(TerminalEvent::Input(report));
                cx.notify();
            }
            return;
        }
        if event.button != MouseButton::Left {
            return;
        }
        let Some((point, side)) = self.grid_point(event.position) else {
            return;
        };
        let kind = match event.click_count {
            2 => SelectionType::Semantic,
            count if count >= 3 => SelectionType::Lines,
            _ => SelectionType::Simple,
        };
        match self.term.selection.as_mut() {
            Some(selection) if event.modifiers.shift && kind == SelectionType::Simple => {
                selection.update(point, side);
            }
            _ => self.term.selection = Some(Selection::new(kind, point, side)),
        }
        self.drag = Some(Drag::Select);
        cx.notify();
    }

    fn drag_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        match self.drag {
            Some(Drag::Select) => {
                // Past the top or the bottom, the view scrolls a line a move.
                if let Some(metrics) = self.metrics {
                    let y = f32::from(event.position.y - self.origin.y);
                    let bottom = self.size.map_or(0.0, |size| size.rows as f32) * metrics.height;
                    if y < 0.0 {
                        self.term.scroll_display(Scroll::Delta(1));
                    } else if y > bottom {
                        self.term.scroll_display(Scroll::Delta(-1));
                    }
                }
                if let Some((point, side)) = self.grid_point(event.position)
                    && let Some(selection) = self.term.selection.as_mut()
                {
                    selection.update(point, side);
                    cx.notify();
                }
            }
            Some(Drag::Report(code)) => {
                if !self
                    .term
                    .mode()
                    .intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION)
                {
                    return;
                }
                let Some((row, col, _)) = self.viewport_cell(event.position) else {
                    return;
                };
                if self.reported_cell == Some((row, col)) {
                    return;
                }
                self.reported_cell = Some((row, col));
                if let Some(report) =
                    self.mouse_report(code + 32, true, event.position, &event.modifiers)
                {
                    cx.emit(TerminalEvent::Input(report));
                }
            }
            None => {}
        }
    }

    fn drag_end(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        match self.drag.take() {
            Some(Drag::Select) => {
                if self
                    .term
                    .selection
                    .as_ref()
                    .is_some_and(|selection| selection.is_empty())
                {
                    self.term.selection = None;
                }
                cx.notify();
            }
            Some(Drag::Report(code)) => {
                self.reported_cell = None;
                if let Some(report) =
                    self.mouse_report(code, false, event.position, &event.modifiers)
                {
                    cx.emit(TerminalEvent::Input(report));
                }
            }
            None => {}
        }
    }

    /// Motion with no button held, for a program that tracks every move (mode 1003).
    fn hover_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if self.drag.is_some()
            || event.pressed_button.is_some()
            || !self.term.mode().contains(TermMode::MOUSE_MOTION)
            || event.modifiers.shift
        {
            return;
        }
        let Some((row, col, _)) = self.viewport_cell(event.position) else {
            return;
        };
        if self.reported_cell == Some((row, col)) {
            return;
        }
        self.reported_cell = Some((row, col));
        if let Some(report) = self.mouse_report(35, true, event.position, &event.modifiers) {
            cx.emit(TerminalEvent::Input(report));
        }
    }

    fn scroll_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let Some(metrics) = self.metrics else {
            return;
        };
        self.scroll_carry += f32::from(event.delta.pixel_delta(px(metrics.height)).y);
        let lines = (self.scroll_carry / metrics.height).trunc();
        if lines == 0.0 {
            return;
        }
        self.scroll_carry -= lines * metrics.height;
        let count = lines.abs() as usize;
        let up = lines > 0.0;
        let mode = *self.term.mode();
        if self.reports_mouse(&event.modifiers) {
            let code = if up { 64 } else { 65 };
            let mut bytes = Vec::new();
            for _ in 0..count.min(10) {
                if let Some(report) =
                    self.mouse_report(code, true, event.position, &event.modifiers)
                {
                    bytes.extend(report);
                }
            }
            if !bytes.is_empty() {
                cx.emit(TerminalEvent::Input(bytes));
            }
        } else if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL)
            && !event.modifiers.shift
        {
            let key: &[u8] = match (up, mode.contains(TermMode::APP_CURSOR)) {
                (true, true) => b"\x1bOA",
                (true, false) => b"\x1b[A",
                (false, true) => b"\x1bOB",
                (false, false) => b"\x1b[B",
            };
            cx.emit(TerminalEvent::Input(key.repeat(count)));
        } else {
            let delta = if up { count as i32 } else { -(count as i32) };
            self.term.scroll_display(Scroll::Delta(delta));
            cx.notify();
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let app_cursor = self.term.mode().contains(TermMode::APP_CURSOR);
        match encode_key(keystroke, app_cursor) {
            Some(KeyInput::Bytes(bytes)) => {
                cx.stop_propagation();
                self.send_user(bytes, cx);
            }
            Some(KeyInput::PageUp) => {
                cx.stop_propagation();
                self.term.scroll_display(Scroll::PageUp);
                cx.notify();
            }
            Some(KeyInput::PageDown) => {
                cx.stop_propagation();
                self.term.scroll_display(Scroll::PageDown);
                cx.notify();
            }
            None => {
                // A held key repeats here: the input method would open the accent menu.
                if event.is_held
                    && self.marked.is_none()
                    && !keystroke.modifiers.control
                    && !keystroke.modifiers.platform
                    && let Some(text) = keystroke.key_char.as_ref()
                {
                    cx.stop_propagation();
                    self.send_user(text.as_bytes().to_vec(), cx);
                }
            }
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let bracketed = self.term.mode().contains(TermMode::BRACKETED_PASTE);
        self.send_user(paste_bytes(&text, bracketed), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        let start = GridPoint::new(self.term.topmost_line(), Column(0));
        let end = GridPoint::new(self.term.bottommost_line(), self.term.last_column());
        let mut selection = Selection::new(SelectionType::Simple, start, Side::Left);
        selection.update(end, Side::Right);
        self.term.selection = Some(selection);
        cx.notify();
    }

    /// The cursor's cell in the window, where the input method puts its candidates.
    fn cursor_bounds(&self) -> Option<Bounds<Pixels>> {
        let metrics = self.metrics?;
        let cursor = self.term.grid().cursor.point;
        let row = cursor.line.0 + self.term.grid().display_offset() as i32;
        Some(Bounds::new(
            point(
                self.origin.x + px(cursor.column.0 as f32 * metrics.width),
                self.origin.y + px(row as f32 * metrics.height),
            ),
            size(px(metrics.width), px(metrics.height)),
        ))
    }

    fn cell_colors(
        &self,
        cell: &Cell,
        colors: &Colors,
        palette: &TerminalPalette,
    ) -> (Rgba, Option<Rgba>) {
        let flags = cell.flags;
        let fg = match cell.fg {
            // xterm.js's `drawBoldTextInBrightColors`.
            AnsiColor::Named(name) if flags.contains(Flags::BOLD) && (name as usize) < 8 => {
                AnsiColor::Named(name.to_bright())
            }
            AnsiColor::Indexed(index) if flags.contains(Flags::BOLD) && index < 8 => {
                AnsiColor::Indexed(index + 8)
            }
            color => color,
        };
        let mut fg = resolve(fg, colors, palette);
        let mut bg = match cell.bg {
            AnsiColor::Named(NamedColor::Background) => None,
            color => Some(resolve(color, colors, palette)),
        };
        if flags.contains(Flags::INVERSE) {
            let inverted = bg.unwrap_or(palette.background);
            bg = Some(fg);
            fg = inverted;
        }
        if flags.contains(Flags::DIM) {
            fg.a *= 0.5;
        }
        (fg, bg)
    }

    /// Everything a frame draws: background runs, text runs, and the cursor.
    fn plan(
        &self,
        bounds: Bounds<Pixels>,
        metrics: CellMetrics,
        focused: bool,
        palette: TerminalPalette,
        base: &Font,
        window: &mut Window,
    ) -> Plan {
        let font_size = px(self.font_size);
        let content = self.term.renderable_content();
        let offset = content.display_offset;
        let cursor = content.cursor;
        let cursor_cell = point_to_viewport(offset, cursor.point)
            .filter(|_| cursor.shape != CursorShape::Hidden && self.marked.is_none());
        let cursor_drawn = cursor_cell.filter(|_| self.cursor_on || !focused);
        let block = focused && cursor.shape == CursorShape::Block;
        let origin = bounds.origin;
        let at = |row: usize, col: usize| {
            point(
                origin.x + px(col as f32 * metrics.width),
                origin.y + px(row as f32 * metrics.height),
            )
        };

        let mut plan = Plan {
            background: palette.background,
            rects: Vec::new(),
            lines: Vec::new(),
            cursor: None,
            line_height: px(metrics.height),
        };
        let mut fill_run: Option<(usize, usize, usize, Rgba)> = None;
        let mut text_run: Option<TextBatch> = None;
        let mut cursor_wide = false;
        let flush_fill = |run: &mut Option<(usize, usize, usize, Rgba)>, plan: &mut Plan| {
            if let Some((row, start, end, color)) = run.take() {
                plan.rects.push((
                    Bounds::new(
                        at(row, start),
                        size(px((end - start) as f32 * metrics.width), px(metrics.height)),
                    ),
                    color,
                ));
            }
        };
        let flush_text = |run: &mut Option<TextBatch>, plan: &mut Plan, window: &mut Window| {
            if let Some(batch) = run.take() {
                let mut text = batch.text;
                if batch.style.underline.is_none() && batch.style.strike.is_none() {
                    text.truncate(text.trim_end().len());
                }
                if text.is_empty() {
                    return;
                }
                let run = TextRun {
                    len: text.len(),
                    font: Font {
                        weight: if batch.style.bold {
                            FontWeight::BOLD
                        } else {
                            FontWeight::NORMAL
                        },
                        style: if batch.style.italic {
                            FontStyle::Italic
                        } else {
                            FontStyle::Normal
                        },
                        ..base.clone()
                    },
                    color: batch.style.color.into(),
                    background_color: None,
                    underline: batch.style.underline,
                    strikethrough: batch.style.strike,
                };
                let line = window
                    .text_system()
                    .shape_line(text.into(), font_size, &[run], None);
                plan.lines.push((at(batch.row, batch.col), line));
            }
        };

        let mut current_row = usize::MAX;
        for indexed in content.display_iter {
            let Some(cell_at) = point_to_viewport(offset, indexed.point) else {
                continue;
            };
            let (row, col) = (cell_at.line, cell_at.column.0);
            if row != current_row {
                flush_fill(&mut fill_run, &mut plan);
                flush_text(&mut text_run, &mut plan, window);
                current_row = row;
            }
            let cell = indexed.cell;
            let flags = cell.flags;
            let (mut fg, mut bg) = self.cell_colors(cell, content.colors, &palette);
            if content
                .selection
                .is_some_and(|selection| selection.contains(indexed.point))
            {
                bg = Some(palette.selection);
                fg = palette.selection_foreground;
            }
            let under_cursor = cursor_drawn == Some(cell_at)
                || (cursor_wide
                    && cursor_drawn.is_some_and(|c| c.line == row && c.column.0 + 1 == col));
            if cursor_drawn == Some(cell_at) {
                cursor_wide = flags.contains(Flags::WIDE_CHAR);
            }
            if block && under_cursor {
                bg = Some(palette.cursor);
                fg = palette.cursor_accent;
            }

            match (bg, fill_run.as_mut()) {
                (Some(color), Some(run)) if run.3 == color && run.2 == col => run.2 = col + 1,
                (Some(color), _) => {
                    flush_fill(&mut fill_run, &mut plan);
                    fill_run = Some((row, col, col + 1, color));
                }
                (None, _) => flush_fill(&mut fill_run, &mut plan),
            }

            if flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                continue;
            }
            if flags.contains(Flags::HIDDEN) {
                flush_text(&mut text_run, &mut plan, window);
                continue;
            }
            let style = TextStyleKey {
                color: fg,
                bold: flags.contains(Flags::BOLD),
                italic: flags.contains(Flags::ITALIC),
                underline: flags
                    .intersects(Flags::ALL_UNDERLINES)
                    .then_some(UnderlineStyle {
                        thickness: px(1.0),
                        color: Some(Hsla::from(fg)),
                        wavy: flags.contains(Flags::UNDERCURL),
                    }),
                strike: flags
                    .contains(Flags::STRIKEOUT)
                    .then_some(StrikethroughStyle {
                        thickness: px(1.0),
                        color: Some(Hsla::from(fg)),
                    }),
            };
            let ascii = cell.c.is_ascii() && cell.zerowidth().is_none();
            match text_run.as_mut() {
                Some(batch)
                    if ascii
                        && batch.ascii
                        && batch.style == style
                        && batch.col + batch.cells == col =>
                {
                    batch.text.push(cell.c);
                    batch.cells += 1;
                }
                _ => {
                    flush_text(&mut text_run, &mut plan, window);
                    let mut text = String::from(cell.c);
                    for mark in cell.zerowidth().into_iter().flatten() {
                        text.push(*mark);
                    }
                    text_run = Some(TextBatch {
                        row,
                        col,
                        cells: 1,
                        ascii,
                        text,
                        style,
                    });
                }
            }
            // Anything past ASCII sits alone at its own cell, whatever its glyph's advance.
            if !ascii {
                flush_text(&mut text_run, &mut plan, window);
            }
        }
        flush_fill(&mut fill_run, &mut plan);
        flush_text(&mut text_run, &mut plan, window);

        if let Some(cell) = cursor_drawn {
            let width = metrics.width * if cursor_wide { 2.0 } else { 1.0 };
            let corner = at(cell.line, cell.column.0);
            let shape = if focused {
                cursor.shape
            } else {
                CursorShape::HollowBlock
            };
            plan.cursor = match shape {
                CursorShape::Beam => Some(CursorPaint::Fill(
                    Bounds::new(corner, size(px(1.0), px(metrics.height))),
                    palette.cursor,
                )),
                CursorShape::Underline => Some(CursorPaint::Fill(
                    Bounds::new(
                        point(corner.x, corner.y + px(metrics.height - 1.0)),
                        size(px(width), px(1.0)),
                    ),
                    palette.cursor,
                )),
                CursorShape::HollowBlock => Some(CursorPaint::Outline(
                    Bounds::new(corner, size(px(width), px(metrics.height))),
                    palette.cursor,
                )),
                // The block is drawn with its cell; a hidden cursor is not drawn.
                CursorShape::Block | CursorShape::Hidden => None,
            };
        }

        if let Some(marked) = self.marked.as_ref().filter(|text| !text.is_empty())
            && let Some(cell) = point_to_viewport(offset, cursor.point)
        {
            let run = TextRun {
                len: marked.len(),
                font: base.clone(),
                color: palette.foreground.into(),
                background_color: None,
                underline: Some(UnderlineStyle {
                    thickness: px(1.0),
                    color: Some(palette.foreground.into()),
                    wavy: false,
                }),
                strikethrough: None,
            };
            let line =
                window
                    .text_system()
                    .shape_line(marked.clone().into(), font_size, &[run], None);
            let corner = at(cell.line, cell.column.0);
            plan.rects.push((
                Bounds::new(
                    corner,
                    size(line.width.max(px(metrics.width)), px(metrics.height)),
                ),
                palette.background,
            ));
            plan.lines.push((corner, line));
        }
        plan
    }
}

fn term_config(scrollback: usize) -> Config {
    Config {
        scrolling_history: scrollback,
        // A program on the far end does not reach the clipboard (xterm.js has no OSC 52).
        osc52: Osc52::Disabled,
        ..Config::default()
    }
}

fn resolve(color: AnsiColor, colors: &Colors, palette: &TerminalPalette) -> Rgba {
    match color {
        AnsiColor::Spec(rgb) => rgba(rgb),
        AnsiColor::Indexed(index) => colors[index as usize]
            .map(rgba)
            .unwrap_or_else(|| palette.indexed(index)),
        AnsiColor::Named(name) => colors[name]
            .map(rgba)
            .unwrap_or_else(|| palette.named(name)),
    }
}

/// The xterm button number of a press.
fn button_code(button: MouseButton) -> Option<u8> {
    match button {
        MouseButton::Left => Some(0),
        MouseButton::Middle => Some(1),
        MouseButton::Right => Some(2),
        _ => None,
    }
}

/// A mouse report in the encoding the program chose: SGR (mode 1006), UTF-8 (1005), or the
/// classic one, which cannot name a cell past 223.
fn mouse_report(
    mode: TermMode,
    code: u8,
    pressed: bool,
    row: usize,
    col: usize,
    modifiers: &Modifiers,
) -> Option<Vec<u8>> {
    let mut code = code;
    if modifiers.shift {
        code += 4;
    }
    if modifiers.alt {
        code += 8;
    }
    if modifiers.control {
        code += 16;
    }
    let (x, y) = (col + 1, row + 1);
    if mode.contains(TermMode::SGR_MOUSE) {
        let end = if pressed { 'M' } else { 'm' };
        return Some(format!("\x1b[<{code};{x};{y}{end}").into_bytes());
    }
    // The classic encodings say a release as button 3.
    let code = if pressed { code } else { (code & !0b11) | 3 };
    let mut out = b"\x1b[M".to_vec();
    out.push(32 + code);
    for value in [x, y] {
        if mode.contains(TermMode::UTF8_MOUSE) {
            let ch = char::from_u32(32 + value as u32).filter(|_| value <= 2015)?;
            let mut buf = [0; 4];
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        } else {
            out.push(u8::try_from(32 + value).ok()?);
        }
    }
    Some(out)
}

/// What a key does in the terminal, when it is not plain text for the input method.
#[derive(Debug, PartialEq, Eq)]
enum KeyInput {
    Bytes(Vec<u8>),
    PageUp,
    PageDown,
}

/// xterm.js's `evaluateKeyboardEvent` on macOS (`macOptionIsMeta` off): the bytes a key
/// sends, or `None` for text the input method types (letters, Option's characters) and for
/// Command's shortcuts.
fn encode_key(keystroke: &Keystroke, app_cursor: bool) -> Option<KeyInput> {
    let m = &keystroke.modifiers;
    let mods = u8::from(m.shift)
        | (u8::from(m.alt) << 1)
        | (u8::from(m.control) << 2)
        | (u8::from(m.platform) << 3);
    let bytes = |text: &str| Some(KeyInput::Bytes(text.as_bytes().to_vec()));
    let esc_alt = |text: &str| {
        if m.alt {
            bytes(&format!("\x1b{text}"))
        } else {
            bytes(text)
        }
    };
    let cursor_key = |end: char| {
        if mods != 0 {
            let sequence = format!("\x1b[1;{}{end}", mods + 1);
            // Option-Left and Option-Right move by word, as macOS shells expect.
            match sequence.as_str() {
                "\x1b[1;3D" => bytes("\x1bb"),
                "\x1b[1;3C" => bytes("\x1bf"),
                _ => bytes(&sequence),
            }
        } else if app_cursor {
            bytes(&format!("\x1bO{end}"))
        } else {
            bytes(&format!("\x1b[{end}"))
        }
    };
    let tilde = |number: u8| {
        if mods != 0 {
            bytes(&format!("\x1b[{number};{}~", mods + 1))
        } else {
            bytes(&format!("\x1b[{number}~"))
        }
    };
    match keystroke.key.as_str() {
        "backspace" => esc_alt(if m.control { "\x08" } else { "\x7f" }),
        "tab" if m.shift => bytes("\x1b[Z"),
        "tab" => bytes("\t"),
        "enter" => esc_alt("\r"),
        "escape" => esc_alt("\x1b"),
        "left" | "right" | "up" | "down" if m.platform => None,
        "left" => cursor_key('D'),
        "right" => cursor_key('C'),
        "up" => cursor_key('A'),
        "down" => cursor_key('B'),
        "home" | "end" => {
            let end = if keystroke.key == "home" { 'H' } else { 'F' };
            if mods != 0 {
                bytes(&format!("\x1b[1;{}{end}", mods + 1))
            } else if app_cursor {
                bytes(&format!("\x1bO{end}"))
            } else {
                bytes(&format!("\x1b[{end}"))
            }
        }
        "insert" if !m.shift && !m.control => bytes("\x1b[2~"),
        "insert" => None,
        "delete" => tilde(3),
        "pageup" if m.shift => Some(KeyInput::PageUp),
        "pagedown" if m.shift => Some(KeyInput::PageDown),
        "pageup" => tilde(5),
        "pagedown" => tilde(6),
        "f1" | "f2" | "f3" | "f4" => {
            let end = match keystroke.key.as_str() {
                "f1" => 'P',
                "f2" => 'Q',
                "f3" => 'R',
                _ => 'S',
            };
            if mods != 0 {
                bytes(&format!("\x1b[1;{}{end}", mods + 1))
            } else {
                bytes(&format!("\x1bO{end}"))
            }
        }
        "f5" => tilde(15),
        "f6" => tilde(17),
        "f7" => tilde(18),
        "f8" => tilde(19),
        "f9" => tilde(20),
        "f10" => tilde(21),
        "f11" => tilde(23),
        "f12" => tilde(24),
        key if m.control && !m.alt && !m.platform => {
            if m.shift {
                // Control with a shifted key: ^_ and ^@.
                return match keystroke.key_char.as_deref() {
                    Some("_") => bytes("\x1f"),
                    Some("@") => bytes("\0"),
                    _ => match key {
                        "-" => bytes("\x1f"),
                        "2" => bytes("\0"),
                        _ => None,
                    },
                };
            }
            let mut chars = key.chars();
            let (Some(ch), None) = (chars.next(), chars.next()) else {
                return if key == "space" { bytes("\0") } else { None };
            };
            let code = match ch {
                'a'..='z' => ch as u8 - b'a' + 1,
                '3'..='7' => ch as u8 - b'3' + 0x1b,
                '8' => 0x7f,
                '[' => 0x1b,
                '\\' => 0x1c,
                ']' => 0x1d,
                '/' => 0x1f,
                '@' => 0,
                _ => return None,
            };
            Some(KeyInput::Bytes(vec![code]))
        }
        _ => None,
    }
}

/// A paste as the program reads it: line breaks as Return, and wrapped in the bracketed
/// paste markers when the program asks for them, with escapes taken out so the text cannot
/// end the paste early.
fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    let text = text.replace("\r\n", "\r").replace('\n', "\r");
    if bracketed {
        format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', "")).into_bytes()
    } else {
        text.into_bytes()
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TextStyleKey {
    color: Rgba,
    bold: bool,
    italic: bool,
    underline: Option<UnderlineStyle>,
    strike: Option<StrikethroughStyle>,
}

/// Cells of one style in a row, shaped as one line.
struct TextBatch {
    row: usize,
    col: usize,
    cells: usize,
    ascii: bool,
    text: String,
    style: TextStyleKey,
}

enum CursorPaint {
    Fill(Bounds<Pixels>, Rgba),
    Outline(Bounds<Pixels>, Rgba),
}

struct Plan {
    background: Rgba,
    rects: Vec<(Bounds<Pixels>, Rgba)>,
    lines: Vec<(Point<Pixels>, ShapedLine)>,
    cursor: Option<CursorPaint>,
    line_height: Pixels,
}

impl EventEmitter<TerminalEvent> for TerminalView {}

impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette_or_theme(cx);
        let history = self.term.history_size();
        let line_height = self.metrics.map_or(0.0, |metrics| metrics.height);
        let rows = self.size.map_or(0, |size| size.rows as usize);
        let scrolled = (history - self.term.grid().display_offset()) as f32 * line_height;
        self.scroll
            .reading
            .set((scrolled, history as f32 * line_height));
        let mouse_mode = self.term.mode().intersects(TermMode::MOUSE_MODE);
        let entity = cx.entity();

        let mut root = div()
            .id(("terminal", entity.entity_id()))
            .key_context("Terminal")
            .track_focus(&self.focus_handle)
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(palette.background)
            .cursor(if mouse_mode {
                CursorStyle::Arrow
            } else {
                CursorStyle::IBeam
            })
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::select_all))
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(|this, event, _, cx| this.hover_move(event, cx)))
            .on_scroll_wheel(cx.listener(|this, event, _, cx| this.scroll_wheel(event, cx)))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .bottom_0()
                    .right(px(GUTTER))
                    .child(TerminalElement {
                        view: entity.clone(),
                    }),
            );
        if history > 0 && rows > 0 && line_height > 0.0 {
            let viewport = rows as f32 * line_height;
            root = root.child(
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .bottom_0()
                    .w(px(GUTTER))
                    .child(scrollbar(
                        ("terminal-scrollbar", entity.entity_id()),
                        &self.scroll,
                        ScrollAxis::Vertical,
                        ScrollGeometry {
                            track: self.height,
                            viewport,
                            content: viewport + history as f32 * line_height,
                        },
                        window,
                        cx,
                    )),
            );
        }
        if self.drag.is_some() {
            let (moves, ups) = (entity.clone(), entity);
            root = root.child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, _| {
                        window.on_mouse_event({
                            let view = moves.clone();
                            move |event: &MouseMoveEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble {
                                    view.update(cx, |view, cx| view.drag_move(event, cx));
                                }
                            }
                        });
                        window.on_mouse_event({
                            let view = ups.clone();
                            move |event: &MouseUpEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble {
                                    view.update(cx, |view, cx| view.drag_end(event, cx));
                                }
                            }
                        });
                    },
                )
                .absolute()
                .size_0(),
            );
        }
        root
    }
}

impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let marked = self.marked.as_ref()?;
        let utf16: Vec<u16> = marked.encode_utf16().collect();
        let range = range.start.min(utf16.len())..range.end.min(utf16.len());
        *adjusted_range = Some(range.clone());
        Some(String::from_utf16_lossy(&utf16[range]))
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let end = self
            .marked
            .as_ref()
            .map_or(0, |text| text.encode_utf16().count());
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked
            .as_ref()
            .map(|text| 0..text.encode_utf16().count())
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked = None;
        self.send_user(text.as_bytes().to_vec(), cx);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        new_text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked = (!new_text.is_empty()).then(|| new_text.to_string());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        self.cursor_bounds()
    }

    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

/// Lays the grid out in its box and draws it.
struct TerminalElement {
    view: Entity<TerminalView>,
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = Plan;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let (family, font_size, palette) = {
            let view = self.view.read(cx);
            (
                view.font_family.clone(),
                view.font_size,
                view.palette_or_theme(cx),
            )
        };
        let base = font(family);
        let font_size = px(font_size);
        let text_system = window.text_system();
        let font_id = text_system.resolve_font(&base);
        let width = text_system
            .advance(font_id, font_size, 'm')
            .map(|advance| f32::from(advance.width))
            .unwrap_or(f32::from(font_size) * 0.6);
        let ascent = f32::from(text_system.ascent(font_id, font_size));
        let descent = f32::from(text_system.descent(font_id, font_size)).abs();
        // A whole number of device pixels a row, so colored rows meet without seams.
        let scale = window.scale_factor().max(1.0);
        let height = ((ascent + descent) * scale).round().max(1.0) / scale;
        let metrics = CellMetrics {
            width: width.max(1.0),
            height,
        };
        self.view
            .update(cx, |view, cx| view.lay_out(bounds, metrics, cx));
        let focused = {
            let view = self.view.read(cx);
            view.focus_handle.is_focused(window) && window.is_window_active()
        };
        let view = self.view.read(cx);
        view.plan(bounds, metrics, focused, palette, &base, window)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        plan: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.paint_quad(fill(bounds, plan.background));
            for (rect, color) in &plan.rects {
                window.paint_quad(fill(*rect, *color));
            }
            for (origin, line) in &plan.lines {
                let _ = line.paint(*origin, plan.line_height, window, cx);
            }
            match plan.cursor {
                Some(CursorPaint::Fill(rect, color)) => window.paint_quad(fill(rect, color)),
                Some(CursorPaint::Outline(rect, color)) => {
                    window.paint_quad(outline(rect, color, BorderStyle::Solid));
                }
                None => {}
            }
        });
        let focus_handle = self.view.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.view.clone()),
            cx,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{
        AppContext as _, ClipboardItem, Entity, Keystroke, Modifiers, TestAppContext,
        VisualTestContext, div, point, prelude::*, px, size,
    };

    use super::*;
    use crate::test_support::{draw, open};

    fn key(text: &str) -> Keystroke {
        Keystroke::parse(text).unwrap()
    }

    fn sent(text: &str, app_cursor: bool) -> Option<Vec<u8>> {
        match encode_key(&key(text), app_cursor) {
            Some(KeyInput::Bytes(bytes)) => Some(bytes),
            _ => None,
        }
    }

    #[test]
    fn keys_encode_as_xterm_js_does_on_macos() {
        assert_eq!(sent("enter", false).unwrap(), b"\r");
        assert_eq!(sent("backspace", false).unwrap(), b"\x7f");
        assert_eq!(sent("alt-backspace", false).unwrap(), b"\x1b\x7f");
        assert_eq!(sent("tab", false).unwrap(), b"\t");
        assert_eq!(sent("shift-tab", false).unwrap(), b"\x1b[Z");
        assert_eq!(sent("escape", false).unwrap(), b"\x1b");
        assert_eq!(sent("up", false).unwrap(), b"\x1b[A");
        assert_eq!(sent("up", true).unwrap(), b"\x1bOA");
        assert_eq!(sent("shift-right", false).unwrap(), b"\x1b[1;2C");
        assert_eq!(sent("ctrl-left", false).unwrap(), b"\x1b[1;5D");
        assert_eq!(sent("alt-left", false).unwrap(), b"\x1bb");
        assert_eq!(sent("alt-right", false).unwrap(), b"\x1bf");
        assert_eq!(sent("cmd-left", false), None);
        assert_eq!(sent("home", true).unwrap(), b"\x1bOH");
        assert_eq!(sent("delete", false).unwrap(), b"\x1b[3~");
        assert_eq!(sent("pageup", false).unwrap(), b"\x1b[5~");
        assert_eq!(
            encode_key(&key("shift-pageup"), false),
            Some(KeyInput::PageUp)
        );
        assert_eq!(sent("f1", false).unwrap(), b"\x1bOP");
        assert_eq!(sent("f5", false).unwrap(), b"\x1b[15~");
        assert_eq!(sent("shift-f12", false).unwrap(), b"\x1b[24;2~");
        assert_eq!(sent("ctrl-c", false).unwrap(), b"\x03");
        assert_eq!(sent("ctrl-a", false).unwrap(), b"\x01");
        assert_eq!(sent("ctrl-[", false).unwrap(), b"\x1b");
        assert_eq!(sent("ctrl-space", false).unwrap(), b"\0");
        // Text, Option's characters, and Command's shortcuts are not the terminal's to send.
        assert_eq!(sent("a", false), None);
        assert_eq!(sent("alt-e", false), None);
        assert_eq!(sent("cmd-k", false), None);
    }

    #[test]
    fn pastes_bracket_and_end_lines_with_return() {
        assert_eq!(paste_bytes("a\nb\r\nc", false), b"a\rb\rc");
        assert_eq!(
            paste_bytes("x\x1b[201~y", true),
            b"\x1b[200~x[201~y\x1b[201~"
        );
    }

    #[test]
    fn mouse_reports_follow_the_programs_encoding() {
        let none = Modifiers::default();
        let sgr = TermMode::SGR_MOUSE | TermMode::MOUSE_REPORT_CLICK;
        assert_eq!(
            mouse_report(sgr, 0, true, 4, 9, &none).unwrap(),
            b"\x1b[<0;10;5M"
        );
        assert_eq!(
            mouse_report(sgr, 0, false, 4, 9, &none).unwrap(),
            b"\x1b[<0;10;5m"
        );
        let classic = TermMode::MOUSE_REPORT_CLICK;
        assert_eq!(
            mouse_report(classic, 0, true, 0, 0, &none).unwrap(),
            b"\x1b[M !!"
        );
        assert_eq!(
            mouse_report(classic, 2, false, 0, 0, &none).unwrap(),
            b"\x1b[M#!!"
        );
        assert_eq!(mouse_report(classic, 0, true, 0, 300, &none), None);
    }

    #[test]
    fn the_palette_is_xterms() {
        let palette = TerminalPalette::from_theme(&Theme::dark());
        assert_eq!(palette.indexed(1), gpui::rgb(0xcc0000));
        assert_eq!(palette.indexed(16), gpui::rgb(0x000000));
        assert_eq!(palette.indexed(196), gpui::rgb(0xff0000));
        assert_eq!(palette.indexed(231), gpui::rgb(0xffffff));
        assert_eq!(palette.indexed(232), gpui::rgb(0x080808));
        assert_eq!(palette.indexed(255), gpui::rgb(0xeeeeee));
    }

    struct Fixture {
        terminal: Entity<TerminalView>,
        events: Rc<RefCell<Vec<TerminalEvent>>>,
        cx: VisualTestContext,
    }

    impl Fixture {
        fn input(&self) -> Vec<u8> {
            self.events
                .borrow_mut()
                .drain(..)
                .filter_map(|event| match event {
                    TerminalEvent::Input(bytes) => Some(bytes),
                    _ => None,
                })
                .flatten()
                .collect()
        }

        fn feed(&mut self, bytes: &[u8]) {
            let terminal = self.terminal.clone();
            self.cx
                .update(|_, cx| terminal.update(cx, |terminal, cx| terminal.feed(bytes, cx)));
            draw(&mut self.cx);
        }

        fn read<R>(&mut self, read: impl FnOnce(&TerminalView) -> R) -> R {
            let terminal = self.terminal.clone();
            self.cx.update(|_, cx| read(terminal.read(cx)))
        }

        /// The window position `across` of the way into a viewport cell, halfway down it.
        fn cell(&mut self, row: usize, col: usize, across: f32) -> gpui::Point<Pixels> {
            self.read(|terminal| {
                let metrics = terminal.metrics.unwrap();
                point(
                    terminal.origin.x + px((col as f32 + across) * metrics.width),
                    terminal.origin.y + px((row as f32 + 0.5) * metrics.height),
                )
            })
        }

        fn press(&mut self, at: gpui::Point<Pixels>, click_count: usize) {
            self.cx.simulate_event(MouseDownEvent {
                button: MouseButton::Left,
                position: at,
                modifiers: Modifiers::default(),
                click_count,
                first_mouse: false,
            });
            draw(&mut self.cx);
        }

        fn release(&mut self, at: gpui::Point<Pixels>) {
            self.cx
                .simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
            draw(&mut self.cx);
        }
    }

    fn terminal(cx: &mut TestAppContext) -> Fixture {
        let slot: Rc<RefCell<Option<Entity<TerminalView>>>> = Rc::default();
        let events: Rc<RefCell<Vec<TerminalEvent>>> = Rc::default();
        let (made, sink) = (slot.clone(), events.clone());
        let mut cx = open(cx, size(640.0, 360.0), move |window, cx| {
            let terminal = made
                .borrow_mut()
                .get_or_insert_with(|| {
                    let terminal = cx.new(|cx| TerminalView::new(window, cx));
                    let sink = sink.clone();
                    cx.subscribe(&terminal, move |_, _, event: &TerminalEvent, _| {
                        sink.borrow_mut().push(event.clone());
                    })
                    .detach();
                    terminal
                })
                .clone();
            div().size_full().child(terminal).into_any_element()
        });
        draw(&mut cx);
        let terminal = slot.borrow().clone().unwrap();
        cx.update(|window, cx| terminal.read(cx).focus(window));
        draw(&mut cx);
        Fixture {
            terminal,
            events,
            cx,
        }
    }

    #[gpui::test]
    fn the_grid_fits_the_box_and_reports_its_size(cx: &mut TestAppContext) {
        let mut fixture = terminal(cx);
        let (size, metrics) = fixture.read(|terminal| (terminal.size(), terminal.metrics));
        let (size, metrics) = (size.unwrap(), metrics.unwrap());
        assert_eq!(
            size.cols,
            (((640.0 - GUTTER) / metrics.width).floor() as u16).min(MAX_DIMENSION)
        );
        assert_eq!(
            size.rows,
            ((360.0 / metrics.height).floor() as u16).min(MAX_DIMENSION)
        );
        assert!(
            fixture
                .events
                .borrow()
                .contains(&TerminalEvent::Resize(size))
        );
    }

    #[gpui::test]
    fn output_lands_on_the_grid_and_queries_are_answered(cx: &mut TestAppContext) {
        let mut fixture = terminal(cx);
        fixture.input();
        fixture.feed(b"hello\r\n\x1b[31mred\x1b[0m \x1b[38;5;196mcube\x1b[38;2;1;2;3m true");
        let lines = fixture.read(|terminal| terminal.screen_lines());
        assert_eq!(lines[0], "hello");
        assert_eq!(lines[1], "red cube true");
        let (red, cube, truecolor) = fixture.read(|terminal| {
            let grid = terminal.term.grid();
            (
                grid[Line(1)][Column(0)].fg,
                grid[Line(1)][Column(4)].fg,
                grid[Line(1)][Column(10)].fg,
            )
        });
        assert_eq!(red, AnsiColor::Named(NamedColor::Red));
        assert_eq!(cube, AnsiColor::Indexed(196));
        assert_eq!(truecolor, AnsiColor::Spec(Rgb { r: 1, g: 2, b: 3 }));
        // The cursor position report, which shells and full-screen programs ask for.
        fixture.feed(b"\x1b[6n");
        assert_eq!(fixture.input(), b"\x1b[2;14R");
    }

    #[gpui::test]
    fn typing_sends_text_keys_and_control_codes(cx: &mut TestAppContext) {
        let mut fixture = terminal(cx);
        fixture.input();
        fixture.cx.simulate_input("ls");
        fixture.cx.simulate_keystrokes("enter ctrl-c up");
        assert_eq!(fixture.input(), b"ls\r\x03\x1b[A");
        // Application cursor keys, as a full-screen program sets them.
        fixture.feed(b"\x1b[?1h");
        fixture.cx.simulate_keystrokes("up");
        assert_eq!(fixture.input(), b"\x1bOA");
    }

    #[gpui::test]
    fn a_double_click_selects_a_word_and_command_c_copies_it(cx: &mut TestAppContext) {
        let mut fixture = terminal(cx);
        fixture.feed(b"hello world");
        let at = fixture.cell(0, 8, 0.5);
        fixture.press(at, 1);
        fixture.release(at);
        assert_eq!(fixture.read(|terminal| terminal.selected_text()), None);
        fixture.press(at, 2);
        fixture.release(at);
        assert_eq!(
            fixture.read(|terminal| terminal.selected_text()),
            Some("world".to_string())
        );
        fixture.cx.simulate_keystrokes("cmd-c");
        assert_eq!(
            fixture
                .cx
                .read_from_clipboard()
                .and_then(|item| item.text()),
            Some("world".to_string())
        );
        fixture.cx.simulate_keystrokes("cmd-a");
        assert_eq!(
            fixture.read(|terminal| terminal.selected_text()),
            Some("hello world".to_string())
        );
    }

    #[gpui::test]
    fn a_drag_selects_across_cells(cx: &mut TestAppContext) {
        let mut fixture = terminal(cx);
        fixture.feed(b"one two three");
        let (from, to) = (fixture.cell(0, 0, 0.25), fixture.cell(0, 5, 0.75));
        fixture.press(from, 1);
        fixture
            .cx
            .simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
        fixture.release(to);
        assert_eq!(
            fixture.read(|terminal| terminal.selected_text()),
            Some("one tw".to_string())
        );
        // Typing drops the selection.
        fixture.cx.simulate_input("x");
        assert_eq!(fixture.read(|terminal| terminal.selected_text()), None);
    }

    #[gpui::test]
    fn command_v_pastes_bracketed_when_the_program_asks(cx: &mut TestAppContext) {
        let mut fixture = terminal(cx);
        fixture.input();
        fixture
            .cx
            .write_to_clipboard(ClipboardItem::new_string("echo hi\n".into()));
        fixture.cx.simulate_keystrokes("cmd-v");
        assert_eq!(fixture.input(), b"echo hi\r");
        fixture.feed(b"\x1b[?2004h");
        fixture.cx.simulate_keystrokes("cmd-v");
        assert_eq!(fixture.input(), b"\x1b[200~echo hi\r\x1b[201~");
    }

    #[gpui::test]
    fn the_wheel_scrolls_back_and_typing_returns_to_the_bottom(cx: &mut TestAppContext) {
        let mut fixture = terminal(cx);
        let rows = fixture.read(|terminal| terminal.size().unwrap().rows as usize);
        let output: String = (0..rows + 10).map(|n| format!("line {n}\r\n")).collect();
        fixture.feed(output.as_bytes());
        let at = fixture.cell(0, 0, 0.5);
        fixture.cx.simulate_event(ScrollWheelEvent {
            position: at,
            delta: gpui::ScrollDelta::Lines(point(0.0, 3.0)),
            modifiers: Modifiers::default(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        draw(&mut fixture.cx);
        assert_eq!(
            fixture.read(|terminal| terminal.term.grid().display_offset()),
            3
        );
        fixture.cx.simulate_input("x");
        assert_eq!(
            fixture.read(|terminal| terminal.term.grid().display_offset()),
            0
        );
    }

    #[gpui::test]
    fn input_method_text_reaches_the_program_once_it_commits(cx: &mut TestAppContext) {
        let mut fixture = terminal(cx);
        fixture.input();
        let terminal = fixture.terminal.clone();
        fixture.cx.update(|window, cx| {
            terminal.update(cx, |terminal, cx| {
                terminal.replace_and_mark_text_in_range(None, "´", None, window, cx);
            })
        });
        assert_eq!(fixture.input(), b"");
        fixture.cx.update(|window, cx| {
            terminal.update(cx, |terminal, cx| {
                terminal.replace_text_in_range(None, "á", window, cx);
            })
        });
        assert_eq!(fixture.input(), "á".as_bytes());
    }
}
