//! `ResizableSidebar` (`panels/resizable.tsx`, `resizable.css`): a side panel beside the main
//! area, resized by a 12px handle centered on its inner edge (the column-resize cursor, no
//! visible line). The width stays between its minimum and maximum; released at or under the
//! collapse threshold (104px) the panel collapses, and dragging the handle again opens it.
//! `set_open(false)` hides the panel and its handle without forgetting its width (the
//! canvas's inspector opens only for a selection).
//!
//! Opening, closing, and collapsing move the width over `--duration-fast` (120ms) on the
//! standard ease (`transition: flex-basis`); a drag follows the pointer at once. Nothing
//! animates while nothing changes. The width lasts while the panel does (`storageKey` keeps
//! it across launches in the Electron Desktop).

use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, AppContext as _, Context, CursorStyle, DispatchPhase, Entity, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Window, canvas, div, prelude::*, px,
};

use crate::theme::Theme;

/// `DEFAULT_COLLAPSE_THRESHOLD`.
pub const COLLAPSE_THRESHOLD: f32 = 104.0;
/// The handle's width, centered on the edge.
const HANDLE: f32 = 12.0;

/// Which side of the main area the panel sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelSide {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug)]
struct Drag {
    from_x: f32,
    from_width: f32,
}

#[derive(Clone, Copy, Debug)]
struct Tween {
    from: f32,
    started: Instant,
}

pub struct ResizablePanel {
    side: PanelSide,
    /// The expanded width, kept while the panel is collapsed or hidden.
    width: f32,
    min: f32,
    max: f32,
    collapsible: bool,
    collapsed: bool,
    open: bool,
    drag: Option<Drag>,
    /// The width a drag shows, which may sit at zero under the threshold.
    live: Option<f32>,
    tween: Option<Tween>,
    /// The width drawn last frame, which a transition starts from.
    drawn: f32,
}

impl ResizablePanel {
    /// A panel of `width` (kept between `min` and `max`) on `side`.
    pub fn new(side: PanelSide, width: f32, min: f32, max: f32) -> Self {
        let width = width.clamp(min, max);
        Self {
            side,
            width,
            min,
            max,
            collapsible: true,
            collapsed: false,
            open: true,
            drag: None,
            live: None,
            tween: None,
            drawn: width,
        }
    }

    /// `collapsible={false}`: dragging under the threshold stops at the minimum.
    pub fn collapsible(mut self, collapsible: bool) -> Self {
        self.collapsible = collapsible;
        self
    }

    /// Starts hidden (`open={false}`), drawn at zero until opened.
    pub fn hidden(mut self) -> Self {
        self.open = false;
        self.drawn = 0.0;
        self
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    /// Shows or hides the panel, moving its width over 120ms.
    pub fn set_open(&mut self, open: bool) {
        if self.open != open {
            self.open = open;
            self.start_tween();
        }
    }

    fn target(&self) -> f32 {
        if !self.open || self.collapsed {
            0.0
        } else {
            self.width
        }
    }

    fn start_tween(&mut self) {
        self.tween = Some(Tween {
            from: self.drawn,
            started: Instant::now(),
        });
    }

    /// The width to draw now; asks for the next frame while a transition runs.
    fn current(&mut self, window: &mut Window, cx: &App) -> f32 {
        if let Some(live) = self.live {
            return live;
        }
        let target = self.target();
        let Some(tween) = self.tween else {
            return target;
        };
        let theme = Theme::global(cx);
        let length = Duration::from_secs_f32(theme.duration_fast);
        let elapsed = tween.started.elapsed();
        if elapsed >= length {
            self.tween = None;
            return target;
        }
        window.request_animation_frame();
        let t = theme
            .ease_standard
            .at(elapsed.as_secs_f32() / length.as_secs_f32());
        tween.from + (target - tween.from) * t
    }

    fn dragged(&mut self, x: f32) {
        let Some(drag) = self.drag else { return };
        let travel = x - drag.from_x;
        let travel = if self.side == PanelSide::Left {
            travel
        } else {
            -travel
        };
        let wanted = (drag.from_width + travel).min(self.max);
        let live = if self.collapsible && wanted <= COLLAPSE_THRESHOLD {
            0.0
        } else {
            wanted.max(self.min)
        };
        self.live = Some(live);
    }

    fn released(&mut self) {
        self.drag = None;
        if let Some(live) = self.live.take() {
            if live <= 0.0 {
                self.collapsed = true;
            } else {
                self.collapsed = false;
                self.width = live.clamp(self.min, self.max);
            }
            self.drawn = live;
        }
    }

    /// Lays the panel holding `panel` beside `main`, its handle between them.
    pub fn render(
        &mut self,
        this: &Entity<Self>,
        panel: AnyElement,
        main: AnyElement,
        window: &mut Window,
        cx: &App,
    ) -> AnyElement {
        let width = self.current(window, cx);
        self.drawn = width;
        let side = self.side;
        let hidden = !self.open && self.tween.is_none();
        // The content keeps its full width and slides under the clip as the panel narrows.
        let content_width = self.width.max(self.min);
        let shell = div()
            .relative()
            .flex_none()
            .h_full()
            .w(px(width))
            .overflow_hidden()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .w(px(content_width))
                    .map(|content| match side {
                        PanelSide::Left => content.left_0(),
                        PanelSide::Right => content.right_0(),
                    })
                    .when(width <= 0.5, |content| content.invisible())
                    .child(panel),
            );
        let handle = (!hidden).then(|| {
            let start = this.clone();
            div()
                .id(match side {
                    PanelSide::Left => "resizable-panel-handle-left",
                    PanelSide::Right => "resizable-panel-handle-right",
                })
                .relative()
                .flex_none()
                .h_full()
                .w(px(HANDLE))
                .mx(px(-HANDLE / 2.0))
                .when(self.collapsed && self.live.is_none(), |handle| {
                    handle.left(px(match side {
                        PanelSide::Left => HANDLE / 2.0,
                        PanelSide::Right => -HANDLE / 2.0,
                    }))
                })
                .cursor(CursorStyle::ResizeLeftRight)
                .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    start.update(cx, |panel, cx| {
                        panel.drag = Some(Drag {
                            from_x: f32::from(event.position.x),
                            from_width: if panel.collapsed { 0.0 } else { panel.width },
                        });
                        panel.tween = None;
                        cx.notify();
                    });
                })
        });
        let tracker = self.drag.is_some().then(|| {
            let moves = this.clone();
            let ups = this.clone();
            canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    window.on_mouse_event({
                        let panel = moves.clone();
                        move |event: &MouseMoveEvent, phase, _, cx| {
                            if phase != DispatchPhase::Bubble {
                                return;
                            }
                            panel.update(cx, |panel, cx| {
                                if event.pressed_button != Some(MouseButton::Left) {
                                    panel.released();
                                } else {
                                    panel.dragged(f32::from(event.position.x));
                                }
                                cx.notify();
                            });
                        }
                    });
                    window.on_mouse_event({
                        let panel = ups.clone();
                        move |event: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                                panel.update(cx, |panel, cx| {
                                    panel.released();
                                    cx.notify();
                                });
                            }
                        }
                    });
                },
            )
            .absolute()
            .size_0()
        });
        let main = div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .child(main);
        let row = div()
            .relative()
            .flex()
            .size_full()
            .min_h_0()
            .min_w_0()
            .children(tracker);
        match side {
            PanelSide::Left => row.child(shell).children(handle).child(main),
            PanelSide::Right => row.child(main).children(handle).child(shell),
        }
        .into_any_element()
    }
}

/// Builds a panel entity for a view.
pub fn resizable_panel<V>(
    side: PanelSide,
    width: f32,
    min: f32,
    max: f32,
    cx: &mut Context<V>,
) -> Entity<ResizablePanel> {
    cx.new(|_| ResizablePanel::new(side, width, min, max))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_under_the_threshold_collapses_and_keeps_the_width() {
        let mut panel = ResizablePanel::new(PanelSide::Left, 192.0, 168.0, 256.0);
        panel.drag = Some(Drag {
            from_x: 192.0,
            from_width: 192.0,
        });
        panel.dragged(300.0);
        assert_eq!(panel.live, Some(256.0));
        panel.dragged(150.0);
        assert_eq!(panel.live, Some(168.0));
        panel.dragged(90.0);
        assert_eq!(panel.live, Some(0.0));
        panel.released();
        assert!(panel.collapsed);
        assert_eq!(panel.width, 192.0);
    }

    #[test]
    fn a_right_panel_grows_leftward() {
        let mut panel = ResizablePanel::new(PanelSide::Right, 272.0, 256.0, 336.0);
        panel.drag = Some(Drag {
            from_x: 500.0,
            from_width: 272.0,
        });
        panel.dragged(460.0);
        panel.released();
        assert_eq!(panel.width, 312.0);
    }
}
